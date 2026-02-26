use std::fmt;

/// All error types produced by the grpcurl library.
///
/// Maps to the Go codebase's error types:
/// - `notFoundError` (invoke.go:382)
/// - `ErrReflectionNotSupported` (desc_source.go)
/// - Various ad-hoc errors wrapped in `fmt.Errorf`
#[derive(Debug)]
pub enum GrpcurlError {
    /// The requested symbol (service, method, message, etc.) was not found.
    /// Equivalent to Go's `notFoundError` and `grpcreflect.IsElementNotFoundError`.
    NotFound(String),

    /// The server does not support the gRPC reflection API.
    /// Equivalent to Go's `ErrReflectionNotSupported`.
    ReflectionNotSupported,

    /// An invalid argument was provided (e.g., malformed method name).
    InvalidArgument(String),

    /// An I/O error (file read, network, etc.).
    Io(std::io::Error),

    /// A protobuf encoding/decoding error.
    Proto(String),

    /// A gRPC status error from the server.
    GrpcStatus(tonic::Status),

    /// Any other error.
    Other(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for GrpcurlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GrpcurlError::NotFound(name) => write!(f, "Symbol not found: {name}"),
            GrpcurlError::ReflectionNotSupported => {
                write!(f, "server does not support the reflection API")
            }
            GrpcurlError::InvalidArgument(msg) => write!(f, "invalid argument: {msg}"),
            GrpcurlError::Io(err) => write!(f, "I/O error: {err}"),
            GrpcurlError::Proto(msg) => write!(f, "proto error: {msg}"),
            GrpcurlError::GrpcStatus(status) => {
                write!(f, "gRPC error: {} - {}", status.code(), status.message())
            }
            GrpcurlError::Other(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for GrpcurlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            GrpcurlError::Io(err) => Some(err),
            GrpcurlError::Other(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

impl From<std::io::Error> for GrpcurlError {
    fn from(err: std::io::Error) -> Self {
        GrpcurlError::Io(err)
    }
}

impl From<tonic::Status> for GrpcurlError {
    fn from(status: tonic::Status) -> Self {
        GrpcurlError::GrpcStatus(status)
    }
}

/// Convenience type alias used throughout the codebase.
pub type Result<T> = std::result::Result<T, GrpcurlError>;

#[cfg(test)]
mod tests {
    use super::*;

    /// Check whether an error represents a "not found" condition.
    ///
    /// Equivalent to Go's `isNotFoundError()` which checks for both the local
    /// `notFoundError` type and `grpcreflect.IsElementNotFoundError()`.
    fn is_not_found_error(err: &GrpcurlError) -> bool {
        match err {
            GrpcurlError::NotFound(_) => true,
            GrpcurlError::GrpcStatus(status) => status.code() == tonic::Code::NotFound,
            _ => false,
        }
    }

    #[test]
    fn not_found_error_detected() {
        let err = GrpcurlError::NotFound("my.Service".into());
        assert!(is_not_found_error(&err));
    }

    #[test]
    fn grpc_not_found_detected() {
        let status = tonic::Status::not_found("service not found");
        let err = GrpcurlError::GrpcStatus(status);
        assert!(is_not_found_error(&err));
    }

    #[test]
    fn other_errors_not_detected_as_not_found() {
        let err = GrpcurlError::InvalidArgument("bad input".into());
        assert!(!is_not_found_error(&err));

        let err = GrpcurlError::ReflectionNotSupported;
        assert!(!is_not_found_error(&err));
    }

    #[test]
    fn display_formatting() {
        let err = GrpcurlError::NotFound("my.Service".into());
        assert_eq!(err.to_string(), "Symbol not found: my.Service");

        let err = GrpcurlError::ReflectionNotSupported;
        assert_eq!(
            err.to_string(),
            "server does not support the reflection API"
        );
    }

    #[test]
    fn io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: GrpcurlError = io_err.into();
        assert!(matches!(err, GrpcurlError::Io(_)));
    }
}
