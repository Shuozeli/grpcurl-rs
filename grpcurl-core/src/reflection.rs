use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;
use prost::Message;
use prost_reflect::DescriptorPool;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;
use tonic_reflection::pb::v1;
use tonic_reflection::pb::v1::server_reflection_client::ServerReflectionClient as V1Client;
use tonic_reflection::pb::v1alpha;

use crate::descriptor::{self, DescriptorSource, SymbolDescriptor};
use crate::error::{GrpcurlError, Result};

/// Reflection API version: 0=unknown, 1=v1, 2=v1alpha
const VERSION_UNKNOWN: u8 = 0;
const VERSION_V1: u8 = 1;
const VERSION_V1ALPHA: u8 = 2;

/// Descriptor source backed by gRPC server reflection.
///
/// Equivalent to Go's `serverSource` (desc_source.go:205-295).
/// Uses the server reflection API to query for service definitions,
/// symbols, and extensions on demand.
///
/// Implements automatic version negotiation: tries v1 first,
/// falls back to v1alpha on Unimplemented error (matching Go's
/// grpcreflect.NewClientAuto behavior).
///
/// The descriptor pool is lazily populated as symbols are queried.
/// Since prost-reflect descriptors use Arc internally and don't
/// borrow from the pool, a Mutex provides safe interior mutability.
// TODO: Add multi-threaded integration tests to exercise ServerSource from
// concurrent tasks, validating that the auto-derived Send+Sync is sound.
pub struct ServerSource {
    channel: Channel,
    pool: Mutex<DescriptorPool>,
    /// Metadata to attach to reflection requests (-H + --reflect-header).
    metadata: tonic::metadata::MetadataMap,
    /// Max decoding message size for reflection responses, matching --max-msg-sz.
    max_msg_sz: Option<usize>,
    /// Cached reflection API version for avoiding repeated v1/v1alpha negotiation.
    version: AtomicU8,
}

impl ServerSource {
    /// Create a new server reflection source.
    pub fn new(channel: Channel) -> Self {
        ServerSource {
            channel,
            pool: Mutex::new(DescriptorPool::new()),
            metadata: tonic::metadata::MetadataMap::new(),
            max_msg_sz: None,
            version: AtomicU8::new(VERSION_UNKNOWN),
        }
    }

    /// Create a new server reflection source with metadata for reflection requests.
    pub fn with_metadata(channel: Channel, metadata: tonic::metadata::MetadataMap) -> Self {
        ServerSource {
            channel,
            pool: Mutex::new(DescriptorPool::new()),
            metadata,
            max_msg_sz: None,
            version: AtomicU8::new(VERSION_UNKNOWN),
        }
    }

    /// Set the maximum decoding message size for reflection responses.
    /// Matches Go's behavior where --max-msg-sz applies to all gRPC calls
    /// including reflection queries.
    pub fn with_max_msg_sz(mut self, max_msg_sz: Option<i32>) -> Self {
        self.max_msg_sz = max_msg_sz.map(|sz| sz as usize);
        self
    }

    /// Send a reflection request and get the response, with v1/v1alpha auto-negotiation.
    /// Caches the discovered version to avoid repeated negotiation overhead.
    async fn reflect(
        &self,
        message_request: v1::server_reflection_request::MessageRequest,
    ) -> Result<v1::server_reflection_response::MessageResponse> {
        let cached = self.version.load(Ordering::Relaxed);
        match cached {
            VERSION_V1 => return self.reflect_v1(message_request).await,
            VERSION_V1ALPHA => return self.reflect_v1alpha(message_request).await,
            _ => {}
        }

        // Unknown version: try v1 first, fall back to v1alpha
        match self.reflect_v1(message_request.clone()).await {
            Ok(resp) => {
                self.version.store(VERSION_V1, Ordering::Relaxed);
                Ok(resp)
            }
            Err(e) if is_unimplemented(&e) => {
                let resp = self.reflect_v1alpha(message_request).await?;
                self.version.store(VERSION_V1ALPHA, Ordering::Relaxed);
                Ok(resp)
            }
            Err(e) => Err(e),
        }
    }

    /// Send a v1 reflection request.
    async fn reflect_v1(
        &self,
        message_request: v1::server_reflection_request::MessageRequest,
    ) -> Result<v1::server_reflection_response::MessageResponse> {
        let request = v1::ServerReflectionRequest {
            host: String::new(),
            message_request: Some(message_request),
        };

        let (tx, rx) = mpsc::channel(1);
        tx.send(request)
            .await
            .map_err(|_| GrpcurlError::Other("failed to send reflection request".into()))?;
        drop(tx);

        let mut client = V1Client::new(self.channel.clone());
        if let Some(max_sz) = self.max_msg_sz {
            client = client.max_decoding_message_size(max_sz);
        }
        let mut req = tonic::Request::new(ReceiverStream::new(rx));
        *req.metadata_mut() = self.metadata.clone();
        let response = client
            .server_reflection_info(req)
            .await
            .map_err(map_status_error)?;

        let mut stream = response.into_inner();
        let resp = stream
            .message()
            .await
            .map_err(GrpcurlError::GrpcStatus)?
            .ok_or_else(|| GrpcurlError::Other("empty reflection response stream".into()))?;

        extract_response(resp.message_response)
    }

    /// Send a v1alpha reflection request, converting types as needed.
    async fn reflect_v1alpha(
        &self,
        message_request: v1::server_reflection_request::MessageRequest,
    ) -> Result<v1::server_reflection_response::MessageResponse> {
        let alpha_request = convert_request_to_v1alpha(message_request);

        let (tx, rx) = mpsc::channel(1);
        tx.send(alpha_request)
            .await
            .map_err(|_| GrpcurlError::Other("failed to send reflection request".into()))?;
        drop(tx);

        let mut client =
            v1alpha::server_reflection_client::ServerReflectionClient::new(self.channel.clone());
        if let Some(max_sz) = self.max_msg_sz {
            client = client.max_decoding_message_size(max_sz);
        }
        let mut req = tonic::Request::new(ReceiverStream::new(rx));
        *req.metadata_mut() = self.metadata.clone();
        let response = client
            .server_reflection_info(req)
            .await
            .map_err(map_status_error)?;

        let mut stream = response.into_inner();
        let resp = stream
            .message()
            .await
            .map_err(GrpcurlError::GrpcStatus)?
            .ok_or_else(|| GrpcurlError::Other("empty reflection response stream".into()))?;

        convert_response_from_v1alpha(resp)
    }

    /// Add serialized file descriptor protos to our pool, fetching any
    /// missing dependencies (e.g., well-known types like google/protobuf/any.proto)
    /// from the server via reflection.
    ///
    /// All descriptors from a single reflection response are collected and
    /// added as one `FileDescriptorSet` so that `prost-reflect` can resolve
    /// inter-file dependencies internally.
    async fn add_file_descriptors(&self, serialized_fds: &[Vec<u8>]) -> Result<()> {
        let new_files = {
            let pool = self
                .pool
                .lock()
                .map_err(|_| GrpcurlError::Other("internal lock poisoned".into()))?;
            let mut files = Vec::new();
            for bytes in serialized_fds {
                let fdp =
                    prost_types::FileDescriptorProto::decode(bytes.as_slice()).map_err(|e| {
                        GrpcurlError::Proto(format!("failed to decode file descriptor: {e}"))
                    })?;

                let file_name = fdp.name.as_deref().unwrap_or("");
                if pool.get_file_by_name(file_name).is_some() {
                    continue;
                }

                files.push(fdp);
            }
            files
        };

        if new_files.is_empty() {
            return Ok(());
        }

        // Collect missing dependencies that need to be fetched from the server.
        let missing = {
            let pool = self
                .pool
                .lock()
                .map_err(|_| GrpcurlError::Other("internal lock poisoned".into()))?;
            let mut missing_files = Vec::new();
            let new_names: std::collections::HashSet<_> =
                new_files.iter().filter_map(|f| f.name.as_deref()).collect();
            for fdp in &new_files {
                for dep in &fdp.dependency {
                    if pool.get_file_by_name(dep).is_none() && !new_names.contains(dep.as_str()) {
                        missing_files.push(dep.clone());
                    }
                }
            }
            missing_files
        };

        // Fetch missing dependencies from the server (e.g., well-known types).
        for dep_name in missing {
            let msg = v1::server_reflection_request::MessageRequest::FileByFilename(dep_name);
            if let Ok(v1::server_reflection_response::MessageResponse::FileDescriptorResponse(
                fdr,
            )) = self.reflect(msg).await
            {
                // Recursive call to handle transitive dependencies.
                Box::pin(self.add_file_descriptors(&fdr.file_descriptor_proto)).await?;
            }
        }

        // Now add our files with all dependencies resolved.
        let mut pool = self
            .pool
            .lock()
            .map_err(|_| GrpcurlError::Other("internal lock poisoned".into()))?;
        // Re-filter in case recursive calls already added some.
        let final_files: Vec<_> = new_files
            .into_iter()
            .filter(|fdp| {
                let name = fdp.name.as_deref().unwrap_or("");
                pool.get_file_by_name(name).is_none()
            })
            .collect();
        if !final_files.is_empty() {
            let fds = prost_types::FileDescriptorSet {
                file: final_files.clone(),
            };
            match pool.add_file_descriptor_set(fds) {
                Ok(()) => {}
                Err(_) => {
                    // Gracefully handle missing dependencies by adding files one at a time.
                    // Matches Go's AllowMissingFileDescriptors() behavior.
                    for fdp in final_files {
                        let name = fdp.name.clone().unwrap_or_else(|| "<unknown>".into());
                        let single_fds = prost_types::FileDescriptorSet { file: vec![fdp] };
                        if let Err(e) = pool.add_file_descriptor_set(single_fds) {
                            eprintln!("warning: skipping file descriptor {name}: {e}");
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Async: list all services via reflection.
    async fn list_services_async(&self) -> Result<Vec<String>> {
        let msg = v1::server_reflection_request::MessageRequest::ListServices(String::new());
        let resp = self.reflect(msg).await?;

        match resp {
            v1::server_reflection_response::MessageResponse::ListServicesResponse(list) => {
                Ok(list.service.into_iter().map(|s| s.name).collect())
            }
            _ => Err(GrpcurlError::Other(
                "unexpected reflection response for list_services".into(),
            )),
        }
    }

    /// Async: find a symbol via reflection.
    async fn find_symbol_async(&self, name: &str) -> Result<SymbolDescriptor> {
        // Check pool first
        {
            let pool = self
                .pool
                .lock()
                .map_err(|_| GrpcurlError::Other("internal lock poisoned".into()))?;
            if let Ok(sym) = descriptor::find_symbol_in_pool(&pool, name) {
                return Ok(sym);
            }
        }

        // Fetch from server
        let msg =
            v1::server_reflection_request::MessageRequest::FileContainingSymbol(name.to_string());
        let resp = self.reflect(msg).await?;

        if let v1::server_reflection_response::MessageResponse::FileDescriptorResponse(fdr) = resp {
            self.add_file_descriptors(&fdr.file_descriptor_proto)
                .await?;
        }

        let pool = self
            .pool
            .lock()
            .map_err(|_| GrpcurlError::Other("internal lock poisoned".into()))?;
        descriptor::find_symbol_in_pool(&pool, name)
    }

    /// Async: find all extensions for a type via reflection.
    async fn all_extensions_async(
        &self,
        type_name: &str,
    ) -> Result<Vec<prost_reflect::ExtensionDescriptor>> {
        let msg = v1::server_reflection_request::MessageRequest::AllExtensionNumbersOfType(
            type_name.to_string(),
        );
        let resp = self.reflect(msg).await?;

        if let v1::server_reflection_response::MessageResponse::AllExtensionNumbersResponse(
            ext_resp,
        ) = resp
        {
            for ext_num in &ext_resp.extension_number {
                let ext_msg =
                    v1::server_reflection_request::MessageRequest::FileContainingExtension(
                        v1::ExtensionRequest {
                            containing_type: type_name.to_string(),
                            extension_number: *ext_num,
                        },
                    );
                if let Ok(
                    v1::server_reflection_response::MessageResponse::FileDescriptorResponse(fdr),
                ) = self.reflect(ext_msg).await
                {
                    let _ = self.add_file_descriptors(&fdr.file_descriptor_proto).await;
                }
            }
        }

        // Now collect extensions from the pool for the given message type
        let pool = self
            .pool
            .lock()
            .map_err(|_| GrpcurlError::Other("internal lock poisoned".into()))?;
        let exts: Vec<prost_reflect::ExtensionDescriptor> = pool
            .all_extensions()
            .filter(|ext| ext.containing_message().full_name() == type_name)
            .collect();
        Ok(exts)
    }
}

#[async_trait]
impl DescriptorSource for ServerSource {
    async fn list_services(&self) -> Result<Vec<String>> {
        self.list_services_async().await
    }

    async fn find_symbol(&self, fully_qualified_name: &str) -> Result<SymbolDescriptor> {
        self.find_symbol_async(fully_qualified_name).await
    }

    async fn all_extensions_for_type(
        &self,
        type_name: &str,
    ) -> Result<Vec<prost_reflect::ExtensionDescriptor>> {
        self.all_extensions_async(type_name).await
    }

    fn descriptor_pool(&self) -> Option<&DescriptorPool> {
        // Cannot return a reference through a Mutex.
        // Callers that need the pool should use find_symbol() instead.
        None
    }
}

// -- Helper functions ----------------------------------------------------------

fn map_status_error(status: tonic::Status) -> GrpcurlError {
    if status.code() == tonic::Code::Unimplemented {
        GrpcurlError::ReflectionNotSupported
    } else {
        GrpcurlError::GrpcStatus(status)
    }
}

fn is_unimplemented(err: &GrpcurlError) -> bool {
    matches!(err, GrpcurlError::ReflectionNotSupported)
        || matches!(err, GrpcurlError::GrpcStatus(s) if s.code() == tonic::Code::Unimplemented)
}

/// Extract the message from a v1 reflection response, checking for errors.
fn extract_response(
    msg: Option<v1::server_reflection_response::MessageResponse>,
) -> Result<v1::server_reflection_response::MessageResponse> {
    let msg =
        msg.ok_or_else(|| GrpcurlError::Other("reflection response has no message".into()))?;

    if let v1::server_reflection_response::MessageResponse::ErrorResponse(ref err) = msg {
        return Err(GrpcurlError::Other(
            format!(
                "reflection error (code {}): {}",
                err.error_code, err.error_message
            )
            .into(),
        ));
    }

    Ok(msg)
}

// -- Version conversion --------------------------------------------------------

/// Convert a v1 request to v1alpha format.
fn convert_request_to_v1alpha(
    msg: v1::server_reflection_request::MessageRequest,
) -> v1alpha::ServerReflectionRequest {
    use v1::server_reflection_request::MessageRequest;
    let alpha_msg = match msg {
        MessageRequest::FileByFilename(s) => {
            v1alpha::server_reflection_request::MessageRequest::FileByFilename(s)
        }
        MessageRequest::FileContainingSymbol(s) => {
            v1alpha::server_reflection_request::MessageRequest::FileContainingSymbol(s)
        }
        MessageRequest::FileContainingExtension(ext) => {
            v1alpha::server_reflection_request::MessageRequest::FileContainingExtension(
                v1alpha::ExtensionRequest {
                    containing_type: ext.containing_type,
                    extension_number: ext.extension_number,
                },
            )
        }
        MessageRequest::AllExtensionNumbersOfType(s) => {
            v1alpha::server_reflection_request::MessageRequest::AllExtensionNumbersOfType(s)
        }
        MessageRequest::ListServices(s) => {
            v1alpha::server_reflection_request::MessageRequest::ListServices(s)
        }
    };
    v1alpha::ServerReflectionRequest {
        host: String::new(),
        message_request: Some(alpha_msg),
    }
}

/// Convert a v1alpha response to v1 format.
fn convert_response_from_v1alpha(
    resp: v1alpha::ServerReflectionResponse,
) -> Result<v1::server_reflection_response::MessageResponse> {
    use v1alpha::server_reflection_response::MessageResponse;
    let msg = resp
        .message_response
        .ok_or_else(|| GrpcurlError::Other("reflection response has no message".into()))?;

    let v1_msg = match msg {
        MessageResponse::FileDescriptorResponse(fdr) => {
            v1::server_reflection_response::MessageResponse::FileDescriptorResponse(
                v1::FileDescriptorResponse {
                    file_descriptor_proto: fdr.file_descriptor_proto,
                },
            )
        }
        MessageResponse::AllExtensionNumbersResponse(ext) => {
            v1::server_reflection_response::MessageResponse::AllExtensionNumbersResponse(
                v1::ExtensionNumberResponse {
                    base_type_name: ext.base_type_name,
                    extension_number: ext.extension_number,
                },
            )
        }
        MessageResponse::ListServicesResponse(list) => {
            v1::server_reflection_response::MessageResponse::ListServicesResponse(
                v1::ListServiceResponse {
                    service: list
                        .service
                        .into_iter()
                        .map(|s| v1::ServiceResponse { name: s.name })
                        .collect(),
                },
            )
        }
        MessageResponse::ErrorResponse(err) => {
            return Err(GrpcurlError::Other(
                format!(
                    "reflection error (code {}): {}",
                    err.error_code, err.error_message
                )
                .into(),
            ));
        }
    };

    Ok(v1_msg)
}
