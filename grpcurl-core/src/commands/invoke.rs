use http::uri::PathAndQuery;
use prost::Message;
use prost_reflect::DynamicMessage;
use tonic::client::Grpc;
use tonic::metadata::MetadataMap;
use tonic::transport::Channel;

use crate::codec::DynamicCodec;
use crate::descriptor::{self, DescriptorSource, SymbolDescriptor};
use crate::descriptor_text;
use crate::error::GrpcurlError;
use crate::format::{
    self, Format, FormatOptions, JsonRequestParser, ParseError, RequestParser, TextRequestParser,
};
use crate::metadata;

/// Configuration for invoking an RPC method.
///
/// This struct decouples the invocation logic from any CLI framework.
/// The CLI binary builds an `InvokeConfig` from its parsed arguments
/// and passes it to `run_invoke()`.
#[derive(Debug, Clone)]
pub struct InvokeConfig {
    /// The format of request/response data ('json' or 'text').
    pub format: Format,

    /// Emit default values for JSON-encoded responses.
    pub emit_defaults: bool,

    /// Allow unknown fields in JSON input.
    pub allow_unknown_fields: bool,

    /// When a non-zero status is returned, format the error using --format.
    pub format_error: bool,

    /// Data for request contents. "@" means read from stdin.
    pub data: Option<String>,

    /// Additional headers in 'name: value' format (sent with all requests).
    pub headers: Vec<String>,

    /// Additional RPC-only headers in 'name: value' format.
    pub rpc_headers: Vec<String>,

    /// If set, headers may use '${NAME}' syntax to reference env variables.
    pub expand_headers: bool,

    /// Maximum encoded size of a response message, in bytes.
    pub max_msg_sz: Option<i32>,

    /// Verbosity level: 0 = default, 1 = verbose, 2 = very verbose.
    pub verbosity: u8,

    /// File to write a FileDescriptorSet proto to.
    pub protoset_out: Option<String>,

    /// Directory to write generated .proto files to.
    pub proto_out_dir: Option<String>,
}

/// Callback trait for RPC invocation events.
///
/// Equivalent to Go's `InvocationEventHandler` interface.
/// Allows callers to customize how request/response events are handled,
/// enabling testability and library embedding beyond CLI output.
pub trait InvocationEventHandler {
    /// Called when the method descriptor is resolved.
    fn on_resolve_method(&self, _method: &prost_reflect::MethodDescriptor) {}

    /// Called when request headers are about to be sent.
    fn on_send_headers(&self, _md: &MetadataMap) {}

    /// Called when response headers are received.
    fn on_receive_headers(&self, _md: &MetadataMap) {}

    /// Called for each response message received.
    fn on_receive_response(&self, _msg: &DynamicMessage) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    /// Called when response trailers are received.
    fn on_receive_trailers(&self, _status: &tonic::Status, _md: &MetadataMap) {}
}

/// Default event handler that prints to stdout/stderr, matching Go's grpcurl behavior.
pub struct DefaultEventHandler {
    formatter: format::Formatter,
    verbosity: u8,
}

impl DefaultEventHandler {
    pub fn new(formatter: format::Formatter, verbosity: u8) -> Self {
        DefaultEventHandler {
            formatter,
            verbosity,
        }
    }
}

impl InvocationEventHandler for DefaultEventHandler {
    fn on_resolve_method(&self, method: &prost_reflect::MethodDescriptor) {
        if self.verbosity > 0 {
            let sym = SymbolDescriptor::Method(method.clone());
            let txt = descriptor_text::get_descriptor_text(&sym);
            print!("\nResolved method descriptor:\n{txt}\n");
        }
    }

    fn on_send_headers(&self, md: &MetadataMap) {
        if self.verbosity > 0 {
            print!(
                "\nRequest metadata to send:\n{}\n",
                metadata::metadata_to_string(md)
            );
        }
    }

    fn on_receive_headers(&self, md: &MetadataMap) {
        if self.verbosity > 0 {
            let filtered = filter_grpc_internal_headers(md);
            print!(
                "\nResponse headers received:\n{}\n",
                metadata::metadata_to_string(&filtered)
            );
        }
    }

    fn on_receive_response(&self, msg: &DynamicMessage) -> Result<(), Box<dyn std::error::Error>> {
        if self.verbosity > 1 {
            print!("\nEstimated response size: {} bytes\n", msg.encoded_len());
        }
        if self.verbosity > 0 {
            print!("\nResponse contents:\n");
        }
        match (self.formatter.as_ref())(msg) {
            Ok(output) => println!("{output}"),
            Err(e) => {
                eprintln!("Failed to format response message: {e}");
            }
        }
        Ok(())
    }

    fn on_receive_trailers(&self, _status: &tonic::Status, md: &MetadataMap) {
        if self.verbosity > 0 {
            let filtered = filter_grpc_internal_headers(md);
            print!(
                "\nResponse trailers received:\n{}\n",
                metadata::metadata_to_string(&filtered)
            );
        }
    }
}

/// Common context for all RPC invocation types, grouping parameters
/// shared by unary, server-streaming, client-streaming, and bidi calls.
struct InvokeContext<'a> {
    client: &'a mut Grpc<Channel>,
    parser: &'a mut RequestParser,
    request_desc: &'a prost_reflect::MessageDescriptor,
    response_desc: &'a prost_reflect::MessageDescriptor,
    path: PathAndQuery,
    formatter: &'a format::Formatter,
    request_metadata: &'a MetadataMap,
    verbosity: u8,
}

/// Result of an RPC invocation, carrying status and count information
/// back to main for exit code calculation and summary output.
pub struct InvokeResult {
    /// The gRPC status from the response (None if the call failed before getting a status).
    pub status: Option<tonic::Status>,
    /// Number of request messages sent.
    pub num_requests: usize,
    /// Number of response messages received.
    pub num_responses: usize,
}

pub async fn run_invoke(
    config: &InvokeConfig,
    channel: Channel,
    symbol: &str,
    source: &dyn DescriptorSource,
) -> Result<InvokeResult, Box<dyn std::error::Error>> {
    let verbosity = config.verbosity;

    // Resolve the method descriptor
    let method_desc = resolve_method(source, symbol).await?;

    // Export protoset/protos if requested (before RPC, matching Go)
    if let Some(ref protoset_out) = config.protoset_out {
        descriptor::write_protoset(protoset_out, source, &[symbol.to_string()]).await?;
    }
    if let Some(ref proto_out_dir) = config.proto_out_dir {
        descriptor::write_proto_files(proto_out_dir, source, &[symbol.to_string()]).await?;
    }

    // Verbose: print resolved method descriptor (Go sends to stdout)
    if verbosity > 0 {
        let sym = SymbolDescriptor::Method(method_desc.clone());
        let txt = descriptor_text::get_descriptor_text(&sym);
        print!("\nResolved method descriptor:\n{txt}\n");
    }

    let request_desc = method_desc.input();
    let response_desc = method_desc.output();

    // Build format options from config
    let format_options = FormatOptions {
        emit_defaults: config.emit_defaults,
        allow_unknown_fields: config.allow_unknown_fields,
    };

    // Parse request data and create response formatter based on --format flag
    let mut parser = match config.format {
        Format::Json => RequestParser::Json(JsonRequestParser::new(
            config.data.as_deref(),
            &format_options,
        )?),
        Format::Text => RequestParser::Text(TextRequestParser::new(config.data.as_deref())?),
    };

    let formatter = match config.format {
        Format::Json => format::json_formatter(&format_options),
        Format::Text => format::text_formatter(config.verbosity == 0),
    };

    // Build request metadata from headers
    // Combine -H (all requests) + --rpc-header (RPC only)
    let mut all_headers: Vec<String> = config.headers.clone();
    all_headers.extend(config.rpc_headers.clone());

    // Expand environment variables if --expand-headers is set
    if config.expand_headers {
        all_headers = metadata::expand_headers(&all_headers)?;
    }

    let request_metadata = metadata::metadata_from_headers(&all_headers);

    // Verbose: print request metadata (Go sends to stdout)
    if verbosity > 0 {
        print!(
            "\nRequest metadata to send:\n{}\n",
            metadata::metadata_to_string(&request_metadata)
        );
    }

    // Build the gRPC method path: /package.Service/Method
    let service_name = method_desc.parent_service().full_name();
    let method_name = method_desc.name();
    let path: PathAndQuery = format!("/{service_name}/{method_name}")
        .parse()
        .map_err(|e| GrpcurlError::InvalidArgument(format!("invalid method path: {e}")))?;

    // Create the gRPC client with gzip decompression support.
    // Matches Go's `_ "google.golang.org/grpc/encoding/gzip"` import which
    // registers gzip as an available encoding (accept compressed responses).
    let mut grpc_client =
        Grpc::new(channel).accept_compressed(tonic::codec::CompressionEncoding::Gzip);

    // Set max message size if specified
    if let Some(max_sz) = config.max_msg_sz {
        grpc_client = grpc_client.max_decoding_message_size(max_sz as usize);
    }

    // Dispatch based on streaming type
    let is_client_stream = method_desc.is_client_streaming();
    let is_server_stream = method_desc.is_server_streaming();

    let mut ctx = InvokeContext {
        client: &mut grpc_client,
        parser: &mut parser,
        request_desc: &request_desc,
        response_desc: &response_desc,
        path,
        formatter: &formatter,
        request_metadata: &request_metadata,
        verbosity,
    };

    let result = match (is_client_stream, is_server_stream) {
        (false, false) => invoke_unary(&mut ctx).await,
        (false, true) => invoke_server_stream(&mut ctx).await,
        (true, false) => invoke_client_stream(&mut ctx).await,
        (true, true) => invoke_bidi_stream(&mut ctx).await,
    };

    // Handle gRPC status errors: convert to InvokeResult instead of propagating.
    // When verbose, show any trailers attached to the error status (matching Go
    // which shows headers/trailers even on error responses).
    match result {
        Ok(invoke_result) => Ok(invoke_result),
        Err(e) => match extract_grpc_status(e) {
            Ok(status) => {
                if config.verbosity > 0 {
                    print_response_trailers(status.metadata(), config.verbosity);
                }
                Ok(InvokeResult {
                    status: Some(status),
                    num_requests: parser.num_requests().max(1),
                    num_responses: 0,
                })
            }
            Err(e) => Err(e),
        },
    }
}

/// Build a tonic Request with metadata attached.
fn build_request<T>(msg: T, md: &MetadataMap) -> tonic::Request<T> {
    let mut req = tonic::Request::new(msg);
    *req.metadata_mut() = md.clone();
    req
}

/// Filter out gRPC pseudo-headers from metadata for display.
///
/// tonic includes grpc-status, grpc-message, and grpc-encoding in response
/// metadata for unary calls. These are internal gRPC headers, not
/// user-visible response headers. Go's gRPC library separates these into
/// trailers, so we filter them out of the displayed headers to match.
fn filter_grpc_internal_headers(md: &MetadataMap) -> MetadataMap {
    let mut filtered = MetadataMap::new();
    for kv in md.iter() {
        match kv {
            tonic::metadata::KeyAndValueRef::Ascii(key, value) => {
                let k = key.as_str();
                if k == "grpc-status" || k == "grpc-message" || k == "grpc-encoding" {
                    continue;
                }
                filtered.append(key.clone(), value.clone());
            }
            tonic::metadata::KeyAndValueRef::Binary(key, value) => {
                filtered.append_bin(key.clone(), value.clone());
            }
        }
    }
    filtered
}

/// Print response headers in verbose mode (Go sends to stdout).
fn print_response_headers(md: &MetadataMap, verbosity: u8) {
    if verbosity > 0 {
        let filtered = filter_grpc_internal_headers(md);
        print!(
            "\nResponse headers received:\n{}\n",
            metadata::metadata_to_string(&filtered)
        );
    }
}

/// Print response trailers in verbose mode (Go sends to stdout).
fn print_response_trailers(md: &MetadataMap, verbosity: u8) {
    if verbosity > 0 {
        let filtered = filter_grpc_internal_headers(md);
        print!(
            "\nResponse trailers received:\n{}\n",
            metadata::metadata_to_string(&filtered)
        );
    }
}

/// Print a single response message with appropriate verbose headers.
/// Go sends all of this to stdout (h.Out), errors to stderr.
fn print_response(
    msg: &DynamicMessage,
    formatter: &format::Formatter,
    verbosity: u8,
    response_num: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if verbosity > 1 {
        print!("\nEstimated response size: {} bytes\n", msg.encoded_len());
    }
    if verbosity > 0 {
        print!("\nResponse contents:\n");
    }
    match (formatter)(msg) {
        Ok(output) => println!("{output}"),
        Err(e) => {
            eprintln!("Failed to format response message {response_num}: {e}");
        }
    }
    Ok(())
}

/// Invoke a unary RPC: single request, single response.
async fn invoke_unary(
    ctx: &mut InvokeContext<'_>,
) -> Result<InvokeResult, Box<dyn std::error::Error>> {
    let request_msg = match ctx.parser.next(ctx.request_desc) {
        Ok(msg) => msg,
        Err(ParseError::Eof) => DynamicMessage::new(ctx.request_desc.clone()),
        Err(ParseError::Error(e)) => return Err(e.into()),
    };

    // Reject extra messages: unary RPCs must have exactly 0 or 1 request messages
    match ctx.parser.next(ctx.request_desc) {
        Ok(_) => {
            return Err(format!(
                "method {:?} is a unary RPC, but request data contained more than 1 message",
                ctx.path.path()
            )
            .into());
        }
        Err(ParseError::Error(e)) => return Err(e.into()),
        Err(ParseError::Eof) => {} // expected
    }

    let num_requests = ctx.parser.num_requests();

    let codec = DynamicCodec::new(ctx.request_desc.clone(), ctx.response_desc.clone());
    ctx.client
        .ready()
        .await
        .map_err(|e| GrpcurlError::Other(format!("service not ready: {e}").into()))?;

    let path = std::mem::replace(&mut ctx.path, PathAndQuery::from_static("/"));
    let response = ctx
        .client
        .unary(
            build_request(request_msg, ctx.request_metadata),
            path,
            codec,
        )
        .await?;

    // For unary RPCs, tonic merges headers and trailers into response.metadata().
    // We filter out gRPC pseudo-headers for the "headers" display, and show the
    // full metadata as "trailers" (matching Go's behavior where the trailers
    // contain the real metadata from the HEADERS frame after the body).
    print_response_headers(response.metadata(), ctx.verbosity);

    // Response body
    print_response(response.get_ref(), ctx.formatter, ctx.verbosity, 1)?;

    // Show trailers (same metadata, since tonic merges them for unary)
    print_response_trailers(response.metadata(), ctx.verbosity);

    Ok(InvokeResult {
        status: Some(tonic::Status::ok("")),
        num_requests,
        num_responses: 1,
    })
}

/// Invoke a server-streaming RPC: single request, stream of responses.
async fn invoke_server_stream(
    ctx: &mut InvokeContext<'_>,
) -> Result<InvokeResult, Box<dyn std::error::Error>> {
    let request_msg = match ctx.parser.next(ctx.request_desc) {
        Ok(msg) => msg,
        Err(ParseError::Eof) => DynamicMessage::new(ctx.request_desc.clone()),
        Err(ParseError::Error(e)) => return Err(e.into()),
    };

    // Reject extra messages: server-streaming RPCs must have exactly 0 or 1 request messages
    match ctx.parser.next(ctx.request_desc) {
        Ok(_) => {
            return Err(format!(
                "method {:?} is a server-streaming RPC, but request data contained more than 1 message",
                ctx.path.path()
            ).into());
        }
        Err(ParseError::Error(e)) => return Err(e.into()),
        Err(ParseError::Eof) => {} // expected
    }

    let num_requests = ctx.parser.num_requests();

    let codec = DynamicCodec::new(ctx.request_desc.clone(), ctx.response_desc.clone());
    ctx.client
        .ready()
        .await
        .map_err(|e| GrpcurlError::Other(format!("service not ready: {e}").into()))?;

    let path = std::mem::replace(&mut ctx.path, PathAndQuery::from_static("/"));
    let response = ctx
        .client
        .server_streaming(
            build_request(request_msg, ctx.request_metadata),
            path,
            codec,
        )
        .await?;

    // Response headers from the initial frame
    print_response_headers(response.metadata(), ctx.verbosity);

    let mut stream = response.into_inner();
    let mut num_responses = 0;
    while let Some(msg) = stream.message().await? {
        num_responses += 1;
        print_response(&msg, ctx.formatter, ctx.verbosity, num_responses)?;
    }

    // Response trailers (available after stream ends)
    if let Some(trailers) = stream.trailers().await? {
        print_response_trailers(&trailers, ctx.verbosity);
    } else if ctx.verbosity > 0 {
        let empty = MetadataMap::new();
        print_response_trailers(&empty, ctx.verbosity);
    }

    Ok(InvokeResult {
        status: Some(tonic::Status::ok("")),
        num_requests,
        num_responses,
    })
}

/// Collect all request messages from the parser, with empty-input default.
fn collect_all_messages(
    parser: &mut RequestParser,
    request_desc: &prost_reflect::MessageDescriptor,
) -> Result<Vec<DynamicMessage>, Box<dyn std::error::Error>> {
    let mut messages = Vec::new();
    loop {
        match parser.next(request_desc) {
            Ok(msg) => messages.push(msg),
            Err(ParseError::Eof) => break,
            Err(ParseError::Error(e)) => return Err(e.into()),
        }
    }
    Ok(messages)
}

/// Invoke a client-streaming RPC: stream of requests, single response.
async fn invoke_client_stream(
    ctx: &mut InvokeContext<'_>,
) -> Result<InvokeResult, Box<dyn std::error::Error>> {
    let messages = collect_all_messages(ctx.parser, ctx.request_desc)?;
    let num_requests = ctx.parser.num_requests();
    let request_stream = tokio_stream::iter(messages);

    let codec = DynamicCodec::new(ctx.request_desc.clone(), ctx.response_desc.clone());
    ctx.client
        .ready()
        .await
        .map_err(|e| GrpcurlError::Other(format!("service not ready: {e}").into()))?;

    let path = std::mem::replace(&mut ctx.path, PathAndQuery::from_static("/"));
    let response = ctx
        .client
        .client_streaming(
            build_request(request_stream, ctx.request_metadata),
            path,
            codec,
        )
        .await?;

    // For client-streaming with unary response, same trailer behavior as unary
    print_response_headers(response.metadata(), ctx.verbosity);

    // Response body
    print_response(response.get_ref(), ctx.formatter, ctx.verbosity, 1)?;

    // Show trailers (same metadata, since tonic merges them for unary response)
    print_response_trailers(response.metadata(), ctx.verbosity);

    Ok(InvokeResult {
        status: Some(tonic::Status::ok("")),
        num_requests,
        num_responses: 1,
    })
}

/// Invoke a bidirectional streaming RPC: stream of requests, stream of responses.
///
/// Uses a channel-based approach to send requests concurrently with receiving
/// responses, matching Go's goroutine-based concurrent send/receive pattern.
async fn invoke_bidi_stream(
    ctx: &mut InvokeContext<'_>,
) -> Result<InvokeResult, Box<dyn std::error::Error>> {
    let messages = collect_all_messages(ctx.parser, ctx.request_desc)?;
    let num_requests = ctx.parser.num_requests();

    // Use a channel so messages are fed concurrently with response reading.
    // This matches Go's pattern where a goroutine sends messages while the
    // main goroutine reads responses.
    let (tx, rx) = tokio::sync::mpsc::channel::<DynamicMessage>(16);
    let send_handle = tokio::spawn(async move {
        for msg in messages {
            if tx.send(msg).await.is_err() {
                break; // receiver dropped (server closed stream)
            }
        }
        // tx drops here, signaling end-of-stream
    });

    let request_stream = tokio_stream::wrappers::ReceiverStream::new(rx);

    let codec = DynamicCodec::new(ctx.request_desc.clone(), ctx.response_desc.clone());
    ctx.client
        .ready()
        .await
        .map_err(|e| GrpcurlError::Other(format!("service not ready: {e}").into()))?;

    let path = std::mem::replace(&mut ctx.path, PathAndQuery::from_static("/"));
    let response = ctx
        .client
        .streaming(
            build_request(request_stream, ctx.request_metadata),
            path,
            codec,
        )
        .await?;

    // Response headers from the initial frame
    print_response_headers(response.metadata(), ctx.verbosity);

    let mut stream = response.into_inner();
    let mut num_responses = 0;
    while let Some(msg) = stream.message().await? {
        num_responses += 1;
        print_response(&msg, ctx.formatter, ctx.verbosity, num_responses)?;
    }

    // Wait for sender to finish (should already be done by now)
    let _ = send_handle.await;

    // Response trailers
    if let Some(trailers) = stream.trailers().await? {
        print_response_trailers(&trailers, ctx.verbosity);
    } else if ctx.verbosity > 0 {
        let empty = MetadataMap::new();
        print_response_trailers(&empty, ctx.verbosity);
    }

    Ok(InvokeResult {
        status: Some(tonic::Status::ok("")),
        num_requests,
        num_responses,
    })
}

/// Extract a gRPC status from a boxed error, if it contains one.
fn extract_grpc_status(
    err: Box<dyn std::error::Error>,
) -> Result<tonic::Status, Box<dyn std::error::Error>> {
    // Try downcasting to tonic::Status
    let err = match err.downcast::<tonic::Status>() {
        Ok(status) => return Ok(*status),
        Err(err) => err,
    };
    // Try downcasting to GrpcurlError
    match err.downcast::<GrpcurlError>() {
        Ok(grpc_err) => {
            if let GrpcurlError::GrpcStatus(status) = *grpc_err {
                Ok(status)
            } else {
                Err(grpc_err)
            }
        }
        Err(err) => Err(err),
    }
}

/// Resolve a fully-qualified method name to a MethodDescriptor.
///
/// Accepts both "package.Service/Method" and "package.Service.Method" formats.
/// Matches Go's approach: resolve the service first, then find the method within it.
async fn resolve_method(
    source: &dyn DescriptorSource,
    symbol: &str,
) -> Result<prost_reflect::MethodDescriptor, Box<dyn std::error::Error>> {
    // Split into service and method parts
    // "package.Service/Method" or "package.Service.Method"
    let (service_name, method_name) = if let Some(slash_pos) = symbol.rfind('/') {
        (&symbol[..slash_pos], &symbol[slash_pos + 1..])
    } else if let Some(dot_pos) = symbol.rfind('.') {
        (&symbol[..dot_pos], &symbol[dot_pos + 1..])
    } else {
        return Err(Box::new(GrpcurlError::InvalidArgument(format!(
            "method name must be in the form 'Service/Method' or 'Service.Method': {symbol}"
        ))));
    };

    // Resolve the service
    let desc = source.find_symbol(service_name).await?;
    let svc = desc.as_service().ok_or_else(|| {
        GrpcurlError::InvalidArgument(format!("\"{service_name}\" is not a service"))
    })?;

    // Find the method within the service
    let method = svc.methods().find(|m| m.name() == method_name).ok_or_else(
        || -> Box<dyn std::error::Error> {
            format!("service \"{service_name}\" does not include a method named \"{method_name}\"")
                .into()
        },
    )?;

    Ok(method)
}
