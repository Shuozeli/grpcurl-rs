use std::sync::Arc;
use std::time::Duration;

use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};

use crate::error::{GrpcurlError, Result};

/// Default connection timeout in seconds (matches Go's default).
const DEFAULT_CONNECT_TIMEOUT_SECS: f64 = 10.0;

/// Connection configuration for establishing a gRPC channel.
///
/// This struct decouples the library from any CLI framework (e.g. clap).
/// The CLI binary builds a `ConnectionConfig` from its parsed arguments
/// and passes it to `create_channel()`.
#[derive(Debug, Clone, Default)]
pub struct ConnectionConfig {
    /// Use plain-text HTTP/2 when connecting to server (no TLS).
    pub plaintext: bool,

    /// Skip server certificate and domain verification.
    pub insecure: bool,

    /// The authoritative name of the remote server.
    pub authority: Option<String>,

    /// Override server name when validating TLS certificate.
    pub servername: Option<String>,

    /// Maximum time, in seconds, to wait for connection to be established.
    pub connect_timeout: Option<f64>,

    /// If present, the maximum idle time in seconds for keepalive.
    pub keepalive_time: Option<f64>,

    /// Maximum total time the operation can take, in seconds.
    pub max_time: Option<f64>,

    /// Whether the server address is a Unix domain socket path.
    pub unix: bool,

    /// File containing trusted root certificates for verifying the server.
    pub cacert: Option<String>,

    /// File containing client certificate (public key).
    pub cert: Option<String>,

    /// File containing client private key.
    pub key: Option<String>,

    /// Use Application Layer Transport Security (ALTS).
    pub alts: bool,

    /// Custom User-Agent string to prepend.
    pub user_agent: Option<String>,

    /// Maximum encoded size of a response message, in bytes.
    pub max_msg_sz: Option<i32>,
}

/// Build a tonic Channel from connection configuration and address.
///
/// Handles:
/// - Address parsing (host:port or socket path)
/// - TLS configuration (system CAs, custom CA, client certs, insecure)
/// - Unix domain socket connections
/// - Connection timeout and keepalive
/// - User-Agent header
///
/// Equivalent to Go's BlockingDial() + ClientTLSConfig() in grpcurl.go.
pub async fn create_channel(config: &ConnectionConfig, address: &str) -> Result<Channel> {
    if config.alts {
        return Err(GrpcurlError::InvalidArgument(
            "ALTS is not yet supported in grpcurl.".into(),
        ));
    }

    // Unix domain socket
    if config.unix {
        return create_unix_channel(config, address).await;
    }

    // Insecure TLS (skip certificate verification) requires a custom connector
    if config.insecure {
        return create_insecure_channel(config, address).await;
    }

    // If SSLKEYLOGFILE is set, use custom rustls connector for key logging support
    // (tonic's ClientTlsConfig doesn't expose rustls key_log)
    if !config.plaintext && std::env::var("SSLKEYLOGFILE").is_ok() {
        return create_custom_tls_channel(config, address).await;
    }

    // Normal path: plaintext or standard TLS via tonic
    let scheme = if config.plaintext { "http" } else { "https" };
    let uri = format!("{scheme}://{address}");

    let mut endpoint = build_endpoint(&uri, config)?;

    // TLS configuration (skip for plaintext)
    if !config.plaintext {
        let tls = build_tonic_tls_config(config)?;
        endpoint = endpoint
            .tls_config(tls)
            .map_err(|e| GrpcurlError::Other(format!("TLS configuration error: {e}").into()))?;
    }

    // Connect eagerly (matching Go's BlockingDial behavior)
    let channel = endpoint
        .connect()
        .await
        .map_err(|e| GrpcurlError::Other(format!("failed to connect to {address}: {e}").into()))?;

    Ok(channel)
}

/// Build common Endpoint configuration (timeout, keepalive, user-agent).
fn build_endpoint(uri: &str, config: &ConnectionConfig) -> Result<Endpoint> {
    let mut endpoint: Endpoint = Channel::from_shared(uri.to_string())
        .map_err(|e| GrpcurlError::InvalidArgument(format!("invalid address: {e}")))?;

    // Connection timeout (default 10s, matching Go's default)
    let connect_timeout = config
        .connect_timeout
        .unwrap_or(DEFAULT_CONNECT_TIMEOUT_SECS);
    endpoint = endpoint.connect_timeout(Duration::from_secs_f64(connect_timeout));

    // Per-request timeout (--max-time)
    if let Some(max_time_secs) = config.max_time {
        endpoint = endpoint.timeout(Duration::from_secs_f64(max_time_secs));
    }

    // Keepalive
    if let Some(keepalive_secs) = config.keepalive_time {
        endpoint = endpoint
            .keep_alive_timeout(Duration::from_secs_f64(keepalive_secs))
            .keep_alive_while_idle(true);
    }

    // User-Agent
    let ua = build_user_agent(config);
    endpoint = endpoint
        .user_agent(ua.as_str())
        .map_err(|e| GrpcurlError::Other(format!("failed to set user-agent: {e}").into()))?;

    Ok(endpoint)
}

/// Create a channel over a Unix domain socket.
///
/// Handles both plaintext and TLS-over-Unix connections.
/// Equivalent to Go's handling of the -unix flag in BlockingDial().
async fn create_unix_channel(config: &ConnectionConfig, socket_path: &str) -> Result<Channel> {
    use hyper_util::rt::TokioIo;
    use tower::service_fn;

    // Use a dummy URI; the actual connection goes through the Unix socket
    let endpoint = build_endpoint("http://[::]:0", config)?;

    let path = socket_path.to_string();

    if config.plaintext {
        // Plaintext over Unix socket
        let channel = endpoint
            .connect_with_connector(service_fn(move |_: http::Uri| {
                let path = path.clone();
                async move {
                    let stream = tokio::net::UnixStream::connect(&path).await?;
                    Ok::<_, std::io::Error>(TokioIo::new(stream))
                }
            }))
            .await
            .map_err(|e| {
                GrpcurlError::Other(
                    format!("failed to connect to Unix socket '{socket_path}': {e}").into(),
                )
            })?;

        Ok(channel)
    } else {
        // TLS over Unix socket
        let rustls_config = if config.insecure {
            build_insecure_rustls_config(config)?
        } else {
            build_standard_rustls_config(config)?
        };
        let tls_connector = tokio_rustls::TlsConnector::from(Arc::new(rustls_config));

        // For server name, use --authority or --servername, default to "localhost"
        let server_name = config
            .authority
            .as_deref()
            .or(config.servername.as_deref())
            .unwrap_or("localhost")
            .to_string();

        let channel = endpoint
            .connect_with_connector(service_fn(move |_: http::Uri| {
                let tls = tls_connector.clone();
                let sni = server_name.clone();
                let path = path.clone();
                async move {
                    let stream = tokio::net::UnixStream::connect(&path).await?;
                    let server_name = rustls::pki_types::ServerName::try_from(sni.as_str())
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?
                        .to_owned();
                    let tls_stream = tls.connect(server_name, stream).await?;
                    Ok::<_, std::io::Error>(TokioIo::new(tls_stream))
                }
            }))
            .await
            .map_err(|e| {
                GrpcurlError::Other(
                    format!("failed to connect to Unix socket '{socket_path}': {e}").into(),
                )
            })?;

        Ok(channel)
    }
}

/// Create a channel using a custom rustls config for the TLS handshake.
///
/// Shared implementation for both insecure (--insecure) and custom TLS
/// (SSLKEYLOGFILE) paths, which differ only in which rustls config they use.
async fn create_channel_with_rustls(
    config: &ConnectionConfig,
    address: &str,
    rustls_config: rustls::ClientConfig,
) -> Result<Channel> {
    use hyper_util::rt::TokioIo;
    use tower::service_fn;

    let uri = format!("https://{address}");
    let endpoint = build_endpoint(&uri, config)?;

    let tls_connector = tokio_rustls::TlsConnector::from(Arc::new(rustls_config));

    // Extract host for SNI; --authority/--servername overrides
    let host = address.split(':').next().unwrap_or(address).to_string();
    let server_name = config
        .authority
        .as_deref()
        .or(config.servername.as_deref())
        .unwrap_or(&host)
        .to_string();

    let addr = address.to_string();

    let channel = endpoint
        .connect_with_connector(service_fn(move |_: http::Uri| {
            let tls = tls_connector.clone();
            let sni = server_name.clone();
            let addr = addr.clone();
            async move {
                let tcp = tokio::net::TcpStream::connect(&addr).await?;
                let server_name = rustls::pki_types::ServerName::try_from(sni.as_str())
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?
                    .to_owned();
                let tls_stream = tls.connect(server_name, tcp).await?;
                Ok::<_, std::io::Error>(TokioIo::new(tls_stream))
            }
        }))
        .await
        .map_err(|e| GrpcurlError::Other(format!("failed to connect to {address}: {e}").into()))?;

    Ok(channel)
}

/// Create a channel with TLS that skips certificate verification.
///
/// Equivalent to Go's `ClientTLSConfig(insecureSkipVerify=true, ...)`.
async fn create_insecure_channel(config: &ConnectionConfig, address: &str) -> Result<Channel> {
    let rustls_config = build_insecure_rustls_config(config)?;
    create_channel_with_rustls(config, address, rustls_config).await
}

/// Create a channel with standard TLS using a custom rustls connector.
///
/// Used when SSLKEYLOGFILE is set, since tonic's ClientTlsConfig doesn't
/// expose rustls's key_log field.
async fn create_custom_tls_channel(config: &ConnectionConfig, address: &str) -> Result<Channel> {
    let rustls_config = build_standard_rustls_config(config)?;
    create_channel_with_rustls(config, address, rustls_config).await
}

// -- TLS Configuration Builders -----------------------------------------------

/// Build tonic's ClientTlsConfig for the standard (non-insecure) path.
///
/// Used when connecting via normal TCP+TLS (not --insecure, not --unix).
fn build_tonic_tls_config(config: &ConnectionConfig) -> Result<ClientTlsConfig> {
    let mut tls = ClientTlsConfig::new();

    if let Some(ref cacert_path) = config.cacert {
        let ca_pem = std::fs::read(cacert_path).map_err(|e| {
            GrpcurlError::Io(std::io::Error::new(
                e.kind(),
                format!("failed to read CA certificate '{cacert_path}': {e}"),
            ))
        })?;
        tls = tls.ca_certificate(Certificate::from_pem(ca_pem));
    } else {
        tls = tls.with_native_roots();
    }

    // Server name override for TLS verification
    if let Some(ref authority) = config.authority {
        tls = tls.domain_name(authority.clone());
    } else if let Some(ref servername) = config.servername {
        tls = tls.domain_name(servername.clone());
    }

    // Client certificate for mTLS
    if let Some(ref cert_path) = config.cert {
        let key_path = config
            .key
            .as_ref()
            .ok_or_else(|| GrpcurlError::InvalidArgument("--key is required with --cert".into()))?;

        let cert_pem = std::fs::read(cert_path).map_err(|e| {
            GrpcurlError::Io(std::io::Error::new(
                e.kind(),
                format!("failed to read client certificate '{cert_path}': {e}"),
            ))
        })?;
        let key_pem = std::fs::read(key_path).map_err(|e| {
            GrpcurlError::Io(std::io::Error::new(
                e.kind(),
                format!("failed to read client key '{key_path}': {e}"),
            ))
        })?;

        tls = tls.identity(Identity::from_pem(cert_pem, key_pem));
    }

    Ok(tls)
}

/// Build a rustls ClientConfig that skips all certificate verification.
///
/// This matches Go's `InsecureSkipVerify: true` behavior.
fn build_insecure_rustls_config(config: &ConnectionConfig) -> Result<rustls::ClientConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let builder = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| GrpcurlError::Other(format!("failed to configure TLS: {e}").into()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(InsecureServerCertVerifier));

    let mut rustls_config = if let Some(ref cert_path) = config.cert {
        let key_path = config
            .key
            .as_ref()
            .ok_or_else(|| GrpcurlError::InvalidArgument("--key is required with --cert".into()))?;
        let certs = load_certs(cert_path)?;
        let key = load_private_key(key_path)?;
        builder.with_client_auth_cert(certs, key).map_err(|e| {
            GrpcurlError::Other(format!("failed to configure client certificate: {e}").into())
        })?
    } else {
        builder.with_no_client_auth()
    };

    apply_key_log(&mut rustls_config);
    Ok(rustls_config)
}

/// Build a standard rustls ClientConfig (with proper certificate verification).
///
/// Used for Unix socket + TLS connections where we bypass tonic's
/// ClientTlsConfig and build the rustls config directly.
fn build_standard_rustls_config(config: &ConnectionConfig) -> Result<rustls::ClientConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let mut root_store = rustls::RootCertStore::empty();

    if let Some(ref cacert_path) = config.cacert {
        let certs = load_certs(cacert_path)?;
        for cert in certs {
            root_store.add(cert).map_err(|e| {
                GrpcurlError::Other(format!("failed to add CA certificate: {e}").into())
            })?;
        }
    } else {
        let native_certs = rustls_native_certs::load_native_certs();
        for cert in native_certs.certs {
            root_store.add(cert).ok(); // Ignore individual cert errors
        }
    }

    let builder = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| GrpcurlError::Other(format!("failed to configure TLS: {e}").into()))?
        .with_root_certificates(root_store);

    let mut rustls_config = if let Some(ref cert_path) = config.cert {
        let key_path = config
            .key
            .as_ref()
            .ok_or_else(|| GrpcurlError::InvalidArgument("--key is required with --cert".into()))?;
        let certs = load_certs(cert_path)?;
        let key = load_private_key(key_path)?;
        builder.with_client_auth_cert(certs, key).map_err(|e| {
            GrpcurlError::Other(format!("failed to configure client certificate: {e}").into())
        })?
    } else {
        builder.with_no_client_auth()
    };

    apply_key_log(&mut rustls_config);
    Ok(rustls_config)
}

// -- SSLKEYLOGFILE Support ----------------------------------------------------

/// Apply SSLKEYLOGFILE support to a rustls ClientConfig.
///
/// If the SSLKEYLOGFILE environment variable is set, enables TLS key logging
/// to the specified file. This is used for debugging TLS connections with
/// tools like Wireshark. Matches Go's `tlsConf.KeyLogWriter` behavior.
fn apply_key_log(config: &mut rustls::ClientConfig) {
    if std::env::var("SSLKEYLOGFILE").is_ok() {
        config.key_log = Arc::new(rustls::KeyLogFile::new());
    }
}

// -- PEM Loading Helpers ------------------------------------------------------

fn load_certs(path: &str) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>> {
    let pem = std::fs::read(path).map_err(|e| {
        GrpcurlError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to read certificate '{path}': {e}"),
        ))
    })?;
    rustls_pemfile::certs(&mut &*pem)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| {
            GrpcurlError::Other(format!("failed to parse certificate '{path}': {e}").into())
        })
}

fn load_private_key(path: &str) -> Result<rustls::pki_types::PrivateKeyDer<'static>> {
    let pem = std::fs::read(path).map_err(|e| {
        GrpcurlError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to read private key '{path}': {e}"),
        ))
    })?;
    rustls_pemfile::private_key(&mut &*pem)
        .map_err(|e| {
            GrpcurlError::Other(format!("failed to parse private key '{path}': {e}").into())
        })?
        .ok_or_else(|| GrpcurlError::InvalidArgument(format!("no private key found in '{path}'")))
}

// -- Insecure TLS Verifier ----------------------------------------------------

/// A certificate verifier that accepts all server certificates without
/// validation. Equivalent to Go's `InsecureSkipVerify: true`.
///
/// WARNING: This is intentionally insecure and only used with --insecure flag.
#[derive(Debug)]
struct InsecureServerCertVerifier;

impl rustls::client::danger::ServerCertVerifier for InsecureServerCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Build the User-Agent string.
///
/// Format: "grpcurl/<version>" prepended with custom user-agent if specified.
/// Matches Go's behavior of "grpcurl/<version>" with optional prefix.
pub fn build_user_agent(config: &ConnectionConfig) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let base = format!("grpcurl/{version}");

    match &config.user_agent {
        Some(custom) => format!("{custom} {base}"),
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(f: impl FnOnce(&mut ConnectionConfig)) -> ConnectionConfig {
        let mut config = ConnectionConfig::default();
        f(&mut config);
        config
    }

    #[test]
    fn user_agent_default() {
        let config = ConnectionConfig::default();
        let ua = build_user_agent(&config);
        assert!(ua.starts_with("grpcurl/"));
    }

    #[test]
    fn user_agent_custom() {
        let config = make_config(|c| {
            c.user_agent = Some("my-tool/1.0".to_string());
        });
        let ua = build_user_agent(&config);
        assert!(ua.starts_with("my-tool/1.0 grpcurl/"));
    }

    #[test]
    fn tls_config_default_uses_native_roots() {
        let config = ConnectionConfig::default();
        let tls = build_tonic_tls_config(&config);
        assert!(tls.is_ok());
    }

    #[test]
    fn tls_config_with_nonexistent_cacert_fails() {
        let config = make_config(|c| {
            c.cacert = Some("/nonexistent/ca.pem".to_string());
        });
        let tls = build_tonic_tls_config(&config);
        assert!(tls.is_err());
    }

    #[test]
    fn tls_config_with_nonexistent_cert_fails() {
        let config = make_config(|c| {
            c.cert = Some("/nonexistent/cert.pem".to_string());
            c.key = Some("/nonexistent/key.pem".to_string());
        });
        let tls = build_tonic_tls_config(&config);
        assert!(tls.is_err());
    }

    #[test]
    fn insecure_rustls_config_builds_successfully() {
        let config = make_config(|c| {
            c.insecure = true;
        });
        let result = build_insecure_rustls_config(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn standard_rustls_config_builds_successfully() {
        let config = ConnectionConfig::default();
        let result = build_standard_rustls_config(&config);
        assert!(result.is_ok());
    }
}
