use std::pin::Pin;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};

use crate::pb;

/// Metadata key: values echoed back as response headers.
/// Format: "key: value" (parsed like grpcurl headers).
const METADATA_REPLY_HEADERS: &str = "reply-with-headers";

/// Metadata key: values echoed back as response trailers.
const METADATA_REPLY_TRAILERS: &str = "reply-with-trailers";

/// Metadata key: if present and non-zero, return this gRPC status code immediately.
const METADATA_FAIL_EARLY: &str = "fail-early";

/// Metadata key: if present and non-zero, return this gRPC status code after processing.
const METADATA_FAIL_LATE: &str = "fail-late";

/// Parsed metadata directives from an incoming request.
struct MetadataDirectives {
    reply_headers: Vec<(String, String)>,
    reply_trailers: Vec<(String, String)>,
    fail_early: Option<tonic::Code>,
    fail_late: Option<tonic::Code>,
}

fn parse_header_value(val: &str) -> (String, String) {
    if let Some(pos) = val.find(':') {
        let key = val[..pos].trim().to_lowercase();
        let value = val[pos + 1..].trim().to_string();
        (key, value)
    } else {
        (val.trim().to_lowercase(), String::new())
    }
}

fn parse_code(val: &str) -> Option<tonic::Code> {
    if let Ok(n) = val.parse::<i32>() {
        if n != 0 {
            Some(code_from_i32(n))
        } else {
            None
        }
    } else {
        None
    }
}

fn code_from_i32(n: i32) -> tonic::Code {
    match n {
        0 => tonic::Code::Ok,
        1 => tonic::Code::Cancelled,
        2 => tonic::Code::Unknown,
        3 => tonic::Code::InvalidArgument,
        4 => tonic::Code::DeadlineExceeded,
        5 => tonic::Code::NotFound,
        6 => tonic::Code::AlreadyExists,
        7 => tonic::Code::PermissionDenied,
        8 => tonic::Code::ResourceExhausted,
        9 => tonic::Code::FailedPrecondition,
        10 => tonic::Code::Aborted,
        11 => tonic::Code::OutOfRange,
        12 => tonic::Code::Unimplemented,
        13 => tonic::Code::Internal,
        14 => tonic::Code::Unavailable,
        15 => tonic::Code::DataLoss,
        16 => tonic::Code::Unauthenticated,
        _ => tonic::Code::Unknown,
    }
}

fn extract_metadata<T>(req: &Request<T>) -> MetadataDirectives {
    let md = req.metadata();

    let reply_headers: Vec<(String, String)> = md
        .get_all(METADATA_REPLY_HEADERS)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(parse_header_value)
        .collect();

    let reply_trailers: Vec<(String, String)> = md
        .get_all(METADATA_REPLY_TRAILERS)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(parse_header_value)
        .collect();

    let fail_early = md
        .get(METADATA_FAIL_EARLY)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_code);

    let fail_late = md
        .get(METADATA_FAIL_LATE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_code);

    MetadataDirectives {
        reply_headers,
        reply_trailers,
        fail_early,
        fail_late,
    }
}

fn extract_metadata_from_streaming<T>(req: &Request<Streaming<T>>) -> MetadataDirectives {
    let md = req.metadata();

    let reply_headers: Vec<(String, String)> = md
        .get_all(METADATA_REPLY_HEADERS)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(parse_header_value)
        .collect();

    let reply_trailers: Vec<(String, String)> = md
        .get_all(METADATA_REPLY_TRAILERS)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(parse_header_value)
        .collect();

    let fail_early = md
        .get(METADATA_FAIL_EARLY)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_code);

    let fail_late = md
        .get(METADATA_FAIL_LATE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_code);

    MetadataDirectives {
        reply_headers,
        reply_trailers,
        fail_early,
        fail_late,
    }
}

fn apply_headers(directives: &MetadataDirectives) -> tonic::metadata::MetadataMap {
    let mut map = tonic::metadata::MetadataMap::new();
    for (k, v) in &directives.reply_headers {
        if let (Ok(key), Ok(val)) = (
            k.parse::<tonic::metadata::MetadataKey<tonic::metadata::Ascii>>(),
            v.parse::<tonic::metadata::MetadataValue<tonic::metadata::Ascii>>(),
        ) {
            map.insert(key, val);
        }
    }
    map
}

fn apply_trailers(directives: &MetadataDirectives) -> tonic::metadata::MetadataMap {
    let mut map = tonic::metadata::MetadataMap::new();
    for (k, v) in &directives.reply_trailers {
        if let (Ok(key), Ok(val)) = (
            k.parse::<tonic::metadata::MetadataKey<tonic::metadata::Ascii>>(),
            v.parse::<tonic::metadata::MetadataValue<tonic::metadata::Ascii>>(),
        ) {
            map.insert(key, val);
        }
    }
    map
}

type ResponseStream =
    Pin<Box<dyn Stream<Item = Result<pb::StreamingOutputCallResponse, Status>> + Send>>;

pub struct TestServiceImpl;

#[tonic::async_trait]
impl pb::test_service_server::TestService for TestServiceImpl {
    async fn empty_call(&self, request: Request<pb::Empty>) -> Result<Response<pb::Empty>, Status> {
        let directives = extract_metadata(&request);

        if let Some(code) = directives.fail_early {
            return Err(Status::new(code, "fail"));
        }
        if let Some(code) = directives.fail_late {
            return Err(Status::new(code, "fail"));
        }

        let mut response = Response::new(pb::Empty {});
        let headers = apply_headers(&directives);
        for kv in headers.iter() {
            match kv {
                tonic::metadata::KeyAndValueRef::Ascii(k, v) => {
                    response.metadata_mut().insert(k, v.clone());
                }
                tonic::metadata::KeyAndValueRef::Binary(k, v) => {
                    response.metadata_mut().insert_bin(k, v.clone());
                }
            }
        }
        // Trailers are set via extensions for unary RPCs in tonic
        let trailers = apply_trailers(&directives);
        if !trailers.is_empty() {
            response.extensions_mut().insert(trailers);
        }
        Ok(response)
    }

    async fn unary_call(
        &self,
        request: Request<pb::SimpleRequest>,
    ) -> Result<Response<pb::SimpleResponse>, Status> {
        let directives = extract_metadata(&request);
        let req = request.into_inner();

        if let Some(code) = directives.fail_early {
            return Err(Status::new(code, "fail"));
        }

        // Check if the request asks for a specific status
        if let Some(ref status) = req.response_status {
            if status.code != 0 {
                let code = code_from_i32(status.code);
                return Err(Status::new(code, &status.message));
            }
        }

        if let Some(code) = directives.fail_late {
            return Err(Status::new(code, "fail"));
        }

        let response = pb::SimpleResponse {
            payload: req.payload,
            username: String::new(),
            oauth_scope: String::new(),
        };

        let mut resp = Response::new(response);
        let headers = apply_headers(&directives);
        for kv in headers.iter() {
            match kv {
                tonic::metadata::KeyAndValueRef::Ascii(k, v) => {
                    resp.metadata_mut().insert(k, v.clone());
                }
                tonic::metadata::KeyAndValueRef::Binary(k, v) => {
                    resp.metadata_mut().insert_bin(k, v.clone());
                }
            }
        }
        Ok(resp)
    }

    type StreamingOutputCallStream = ResponseStream;

    async fn streaming_output_call(
        &self,
        request: Request<pb::StreamingOutputCallRequest>,
    ) -> Result<Response<Self::StreamingOutputCallStream>, Status> {
        let directives = extract_metadata(&request);
        let req = request.into_inner();

        if let Some(code) = directives.fail_early {
            return Err(Status::new(code, "fail"));
        }

        let fail_late = directives.fail_late;
        let response_type = req.response_type;
        let params = req.response_parameters;

        let (tx, rx) = mpsc::channel(32);

        tokio::spawn(async move {
            for param in &params {
                let sz = param.size as usize;
                let mut buf = vec![0u8; sz];
                for (i, byte) in buf.iter_mut().enumerate() {
                    *byte = i as u8;
                }

                let delay_micros = param.interval_us as u64;
                if delay_micros > 0 {
                    tokio::time::sleep(Duration::from_micros(delay_micros)).await;
                }

                let resp = pb::StreamingOutputCallResponse {
                    payload: Some(pb::Payload {
                        r#type: response_type,
                        body: buf,
                    }),
                };
                if tx.send(Ok(resp)).await.is_err() {
                    return;
                }
            }

            if let Some(code) = fail_late {
                let _ = tx.send(Err(Status::new(code, "fail"))).await;
            }
        });

        let stream = ReceiverStream::new(rx);
        let mut resp = Response::new(Box::pin(stream) as Self::StreamingOutputCallStream);

        let headers = apply_headers(&directives);
        for kv in headers.iter() {
            match kv {
                tonic::metadata::KeyAndValueRef::Ascii(k, v) => {
                    resp.metadata_mut().insert(k, v.clone());
                }
                tonic::metadata::KeyAndValueRef::Binary(k, v) => {
                    resp.metadata_mut().insert_bin(k, v.clone());
                }
            }
        }
        Ok(resp)
    }

    async fn streaming_input_call(
        &self,
        request: Request<Streaming<pb::StreamingInputCallRequest>>,
    ) -> Result<Response<pb::StreamingInputCallResponse>, Status> {
        let directives = extract_metadata_from_streaming(&request);
        let mut stream = request.into_inner();

        if let Some(code) = directives.fail_early {
            return Err(Status::new(code, "fail"));
        }

        let mut total_size: i32 = 0;
        while let Some(msg) = stream.next().await {
            let msg = msg?;
            if let Some(ref payload) = msg.payload {
                total_size += payload.body.len() as i32;
            }
        }

        if let Some(code) = directives.fail_late {
            return Err(Status::new(code, "fail"));
        }

        let resp = pb::StreamingInputCallResponse {
            aggregated_payload_size: total_size,
        };
        let mut response = Response::new(resp);
        let headers = apply_headers(&directives);
        for kv in headers.iter() {
            match kv {
                tonic::metadata::KeyAndValueRef::Ascii(k, v) => {
                    response.metadata_mut().insert(k, v.clone());
                }
                tonic::metadata::KeyAndValueRef::Binary(k, v) => {
                    response.metadata_mut().insert_bin(k, v.clone());
                }
            }
        }
        Ok(response)
    }

    type FullDuplexCallStream = ResponseStream;

    async fn full_duplex_call(
        &self,
        request: Request<Streaming<pb::StreamingOutputCallRequest>>,
    ) -> Result<Response<Self::FullDuplexCallStream>, Status> {
        let directives = extract_metadata_from_streaming(&request);
        let mut in_stream = request.into_inner();

        if let Some(code) = directives.fail_early {
            return Err(Status::new(code, "fail"));
        }

        let fail_late = directives.fail_late;
        let (tx, rx) = mpsc::channel(32);

        tokio::spawn(async move {
            while let Some(result) = in_stream.next().await {
                match result {
                    Ok(req) => {
                        for param in &req.response_parameters {
                            let sz = param.size as usize;
                            let mut buf = vec![0u8; sz];
                            for (i, byte) in buf.iter_mut().enumerate() {
                                *byte = i as u8;
                            }

                            let resp = pb::StreamingOutputCallResponse {
                                payload: Some(pb::Payload {
                                    r#type: req.response_type,
                                    body: buf,
                                }),
                            };
                            if tx.send(Ok(resp)).await.is_err() {
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        return;
                    }
                }
            }

            if let Some(code) = fail_late {
                let _ = tx.send(Err(Status::new(code, "fail"))).await;
            }
        });

        let stream = ReceiverStream::new(rx);
        let mut resp = Response::new(Box::pin(stream) as Self::FullDuplexCallStream);

        let headers = apply_headers(&directives);
        for kv in headers.iter() {
            match kv {
                tonic::metadata::KeyAndValueRef::Ascii(k, v) => {
                    resp.metadata_mut().insert(k, v.clone());
                }
                tonic::metadata::KeyAndValueRef::Binary(k, v) => {
                    resp.metadata_mut().insert_bin(k, v.clone());
                }
            }
        }
        Ok(resp)
    }

    type HalfDuplexCallStream = ResponseStream;

    async fn half_duplex_call(
        &self,
        request: Request<Streaming<pb::StreamingOutputCallRequest>>,
    ) -> Result<Response<Self::HalfDuplexCallStream>, Status> {
        let directives = extract_metadata_from_streaming(&request);
        let mut in_stream = request.into_inner();

        if let Some(code) = directives.fail_early {
            return Err(Status::new(code, "fail"));
        }

        // Buffer all requests first
        let mut reqs = Vec::new();
        while let Some(result) = in_stream.next().await {
            let req = result?;
            reqs.push(req);
        }

        let fail_late = directives.fail_late;
        let (tx, rx) = mpsc::channel(32);

        tokio::spawn(async move {
            for req in &reqs {
                let resp = pb::StreamingOutputCallResponse {
                    payload: req.payload.clone(),
                };
                if tx.send(Ok(resp)).await.is_err() {
                    return;
                }
            }

            if let Some(code) = fail_late {
                let _ = tx.send(Err(Status::new(code, "fail"))).await;
            }
        });

        let stream = ReceiverStream::new(rx);
        let mut resp = Response::new(Box::pin(stream) as Self::HalfDuplexCallStream);

        let headers = apply_headers(&directives);
        for kv in headers.iter() {
            match kv {
                tonic::metadata::KeyAndValueRef::Ascii(k, v) => {
                    resp.metadata_mut().insert(k, v.clone());
                }
                tonic::metadata::KeyAndValueRef::Binary(k, v) => {
                    resp.metadata_mut().insert_bin(k, v.clone());
                }
            }
        }
        Ok(resp)
    }
}

/// ComplexService echoes back the request.
pub struct ComplexServiceImpl;

#[tonic::async_trait]
impl pb::complex_service_server::ComplexService for ComplexServiceImpl {
    async fn get_complex(
        &self,
        request: Request<pb::ComplexMessage>,
    ) -> Result<Response<pb::ComplexMessage>, Status> {
        Ok(Response::new(request.into_inner()))
    }

    async fn get_well_known(
        &self,
        request: Request<pb::WellKnownTypesMessage>,
    ) -> Result<Response<pb::WellKnownTypesMessage>, Status> {
        Ok(Response::new(request.into_inner()))
    }
}
