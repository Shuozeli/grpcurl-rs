/// All error types produced by the grpcurl-rs library.
#[derive(Debug, thiserror::Error)]
pub enum GrpcurlError {
    /// The requested symbol (service, method, message, etc.) was not found.
    #[error("Symbol not found: {0}")]
    NotFound(String),

    /// The server does not support the gRPC reflection API.
    #[error("server does not support the reflection API")]
    ReflectionNotSupported,

    /// An invalid argument was provided (e.g., malformed method name).
    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    /// An I/O error (file read, network, etc.).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A protobuf encoding/decoding error.
    #[error("proto error: {0}")]
    Proto(String),

    /// A gRPC status error from the server.
    #[error("gRPC error: {} - {}", .0.code(), .0.message())]
    GrpcStatus(tonic::Status),

    /// A TLS configuration or handshake error.
    #[error("TLS error: {0}")]
    Tls(String),

    /// A server reflection protocol error.
    #[error("reflection error: {0}")]
    Reflection(String),

    /// A connection establishment error.
    #[error("connection error: {0}")]
    Connection(String),

    /// Any other error not covered by the above variants.
    #[error("{0}")]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

impl From<tonic::Status> for GrpcurlError {
    fn from(status: tonic::Status) -> Self {
        GrpcurlError::GrpcStatus(status)
    }
}

impl GrpcurlError {
    /// Check whether this error represents a "not found" condition.
    ///
    /// Returns true for `NotFound` variants and for gRPC status errors
    /// with code `NotFound`. Equivalent to Go's `isNotFoundError()`.
    pub fn is_not_found(&self) -> bool {
        match self {
            GrpcurlError::NotFound(_) => true,
            GrpcurlError::GrpcStatus(status) => status.code() == tonic::Code::NotFound,
            _ => false,
        }
    }
}

/// Convenience type alias used throughout the codebase.
pub type Result<T> = std::result::Result<T, GrpcurlError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_error_detected() {
        let err = GrpcurlError::NotFound("my.Service".into());
        assert!(err.is_not_found());
    }

    #[test]
    fn grpc_not_found_detected() {
        let status = tonic::Status::not_found("service not found");
        let err = GrpcurlError::GrpcStatus(status);
        assert!(err.is_not_found());
    }

    #[test]
    fn other_errors_not_detected_as_not_found() {
        let err = GrpcurlError::InvalidArgument("bad input".into());
        assert!(!err.is_not_found());

        let err = GrpcurlError::ReflectionNotSupported;
        assert!(!err.is_not_found());
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
