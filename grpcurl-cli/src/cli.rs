use clap::Parser;

use grpcurl_core::commands::invoke::InvokeConfig;
use grpcurl_core::connection::ConnectionConfig;
use grpcurl_core::format::Format;

/// All known long flag names (without dashes).
/// Used by `normalize_args` to convert Go-style `-flag` to `--flag`.
const LONG_FLAGS: &[&str] = &[
    "plaintext",
    "insecure",
    "authority",
    "servername",
    "connect-timeout",
    "keepalive-time",
    "max-time",
    "unix",
    "cacert",
    "cert",
    "key",
    "alts",
    "alts-handshaker-service",
    "alts-target-service-account",
    "proto",
    "import-path",
    "protoset",
    "use-reflection",
    "format",
    "allow-unknown-fields",
    "emit-defaults",
    "msg-template",
    "format-error",
    "rpc-header",
    "reflect-header",
    "expand-headers",
    "user-agent",
    "protoset-out",
    "proto-out-dir",
    "max-msg-sz",
    "vv",
    "help",
    "version",
];

/// Normalize command-line arguments for Go-style single-dash compatibility.
///
/// Go's `flag` package treats `-flag` and `--flag` identically. Clap only
/// recognizes `--flag` for long flags. This function rewrites `-flag` to
/// `--flag` for all known long flag names, while leaving true short flags
/// (`-d`, `-H`, `-v`, `-V`) and already-double-dashed flags untouched.
///
/// Examples:
///   `-plaintext`           -> `--plaintext`
///   `-connect-timeout 5`   -> `--connect-timeout 5`
///   `-d '{}'`              -> `-d '{}'`       (short flag, unchanged)
///   `--plaintext`          -> `--plaintext`   (already double-dash)
///   `-proto a.proto`       -> `--proto a.proto`
pub fn normalize_args(args: impl IntoIterator<Item = String>) -> Vec<String> {
    args.into_iter()
        .map(|arg| {
            // Only transform args that start with single dash but not double dash
            if let Some(rest) = arg.strip_prefix('-') {
                if rest.starts_with('-') {
                    // Already double-dashed (e.g. `--plaintext`), leave it
                    return arg;
                }
                // Extract the flag name (before any `=` sign)
                let flag_name = rest.split('=').next().unwrap_or(rest);
                if LONG_FLAGS.contains(&flag_name) {
                    return format!("-{arg}");
                }
            }
            arg
        })
        .collect()
}

/// Like cURL, but for gRPC: command-line tool for interacting with gRPC servers.
///
/// The 'address' is only optional when used with 'list' or 'describe' and a
/// protoset or proto flag is provided.
///
/// If 'list' is indicated, the symbol (if present) should be a fully-qualified
/// service name. If present, all methods of that service are listed. If not
/// present, all exposed services are listed, or all services defined in protosets.
///
/// If 'describe' is indicated, the descriptor for the given symbol is shown. The
/// symbol should be a fully-qualified service, enum, or message name. If no symbol
/// is given then the descriptors for all exposed or known services are shown.
///
/// If neither verb is present, the symbol must be a fully-qualified method name in
/// 'service/method' or 'service.method' format. In this case, the request body will
/// be used to invoke the named method. If no body is given but one is required
/// (i.e. the method is unary or server-streaming), an empty instance of the
/// method's request type will be sent.
///
/// The address will typically be in the form "host:port" where host can be an IP
/// address or a hostname and port is a numeric port or service name. If an IPv6
/// address is given, it must be surrounded by brackets, like "[2001:db8::1]". For
/// Unix variants, if a --unix flag is present, then the address must be the
/// path to the domain socket.
#[derive(Parser, Debug)]
#[command(
    name = "grpcurl",
    version,
    after_help = "Example usage:\n  \
        grpcurl -plaintext localhost:8080 list\n  \
        grpcurl -plaintext localhost:8080 describe my.package.MyService\n  \
        grpcurl -d '{\"id\": 123}' localhost:8080 my.package.MyService/GetItem"
)]
pub struct Cli {
    // -- Connection and Networking --
    /// Use plain-text HTTP/2 when connecting to server (no TLS).
    #[arg(long)]
    pub plaintext: bool,

    /// Skip server certificate and domain verification. (NOT SECURE!)
    /// Not valid with -plaintext option.
    #[arg(long)]
    pub insecure: bool,

    /// The authoritative name of the remote server. This value is passed as the
    /// value of the ":authority" pseudo-header in the HTTP/2 protocol. When TLS
    /// is used, this will also be used as the server name when verifying the
    /// server's certificate.
    #[arg(long)]
    pub authority: Option<String>,

    /// Override server name when validating TLS certificate. This flag is
    /// ignored if -plaintext or -insecure is used.
    /// NOTE: Prefer -authority. This flag may be removed in the future.
    #[arg(long)]
    pub servername: Option<String>,

    /// The maximum time, in seconds, to wait for connection to be established.
    /// Defaults to 10 seconds.
    #[arg(long, value_name = "SECONDS")]
    pub connect_timeout: Option<f64>,

    /// If present, the maximum idle time in seconds, after which a keepalive
    /// probe is sent.
    #[arg(long, value_name = "SECONDS")]
    pub keepalive_time: Option<f64>,

    /// The maximum total time the operation can take, in seconds.
    #[arg(long, value_name = "SECONDS")]
    pub max_time: Option<f64>,

    /// Indicates that the server address is the path to a Unix domain socket.
    #[arg(long)]
    pub unix: bool,

    // -- TLS and Security --
    /// File containing trusted root certificates for verifying the server.
    /// Ignored if -insecure is specified.
    #[arg(long, value_name = "FILE")]
    pub cacert: Option<String>,

    /// File containing client certificate (public key), to present to the
    /// server. Not valid with -plaintext option. Must also provide -key option.
    #[arg(long, value_name = "FILE")]
    pub cert: Option<String>,

    /// File containing client private key, to present to the server. Not valid
    /// with -plaintext option. Must also provide -cert option.
    #[arg(long, value_name = "FILE")]
    pub key: Option<String>,

    /// Use Application Layer Transport Security (ALTS) when connecting to server.
    #[arg(long)]
    pub alts: bool,

    /// If set, this server will be used to do the ALTS handshaking.
    #[arg(long, value_name = "ADDRESS")]
    pub alts_handshaker_service: Option<String>,

    /// Expected ALTS server service account. May be specified multiple times.
    #[arg(long, value_name = "EMAIL")]
    pub alts_target_service_account: Vec<String>,

    // -- Descriptor Sources --
    /// The name of a proto source file. May specify more than one via multiple
    /// --proto flags. It is an error to use both --protoset and --proto flags.
    #[arg(long, value_name = "FILE")]
    pub proto: Vec<String>,

    /// The path to a directory from which proto sources can be imported.
    /// Multiple import paths can be configured by specifying multiple flags.
    #[arg(long, value_name = "DIR")]
    pub import_path: Vec<String>,

    /// The name of a file containing an encoded FileDescriptorSet. May specify
    /// more than one via multiple --protoset flags. It is an error to use both
    /// --protoset and --proto flags.
    #[arg(long, value_name = "FILE")]
    pub protoset: Vec<String>,

    /// When true, server reflection will be used to determine the RPC schema.
    /// Defaults to true unless a --proto or --protoset option is provided.
    #[arg(long)]
    pub use_reflection: Option<bool>,

    // -- Request Data --
    /// Data for request contents. If the value is '@' then the request contents
    /// are read from stdin.
    #[arg(short = 'd', value_name = "DATA")]
    pub data: Option<String>,

    /// The format of request data. The allowed values are 'json' or 'text'.
    #[arg(long, default_value = "json")]
    pub format: Format,

    /// When true, the request contents, if 'json' format is used, allows
    /// unknown fields to be present.
    #[arg(long)]
    pub allow_unknown_fields: bool,

    // -- Response Formatting --
    /// Emit default values for JSON-encoded responses.
    #[arg(long)]
    pub emit_defaults: bool,

    /// When describing messages, show a template of input data.
    #[arg(long)]
    pub msg_template: bool,

    /// When a non-zero status is returned, format the response using the
    /// value set by the --format flag.
    #[arg(long)]
    pub format_error: bool,

    // -- Headers and Metadata --
    /// Additional headers in 'name: value' format. May specify more than one
    /// via multiple flags. These headers will also be included in reflection
    /// requests to a server.
    #[arg(short = 'H', value_name = "HEADER")]
    pub header: Vec<String>,

    /// Additional RPC headers in 'name: value' format. These headers will
    /// *only* be used when invoking the requested RPC method.
    #[arg(long, value_name = "HEADER")]
    pub rpc_header: Vec<String>,

    /// Additional reflection headers in 'name: value' format. These headers
    /// will *only* be used during reflection requests.
    #[arg(long, value_name = "HEADER")]
    pub reflect_header: Vec<String>,

    /// If set, headers may use '${NAME}' syntax to reference environment
    /// variables.
    #[arg(long)]
    pub expand_headers: bool,

    /// If set, the specified value will be added to the User-Agent header.
    #[arg(long, value_name = "STRING")]
    pub user_agent: Option<String>,

    // -- Output and Export --
    /// The name of a file to be written that will contain a FileDescriptorSet
    /// proto.
    #[arg(long, value_name = "FILE")]
    pub protoset_out: Option<String>,

    /// The name of a directory where the generated .proto files will be written.
    #[arg(long, value_name = "DIR")]
    pub proto_out_dir: Option<String>,

    // -- Performance and Limits --
    /// The maximum encoded size of a response message, in bytes, that grpcurl
    /// will accept. If not specified, defaults to 4,194,304 (4 megabytes).
    #[arg(long, value_name = "BYTES")]
    pub max_msg_sz: Option<i32>,

    // -- Verbosity --
    /// Enable verbose output.
    #[arg(short = 'v')]
    pub verbose: bool,

    /// Enable very verbose output (includes timing data).
    #[arg(long = "vv")]
    pub very_verbose: bool,

    // -- Positional Arguments --
    /// Positional arguments: [address] [list|describe] [symbol]
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}

impl Cli {
    /// Compute the verbosity level from CLI flags.
    /// 0 = default, 1 = -v, 2 = --vv
    pub fn verbosity(&self) -> u8 {
        if self.very_verbose {
            2
        } else if self.verbose {
            1
        } else {
            0
        }
    }

    /// Build a `ConnectionConfig` from CLI arguments.
    pub fn connection_config(&self) -> ConnectionConfig {
        ConnectionConfig {
            plaintext: self.plaintext,
            insecure: self.insecure,
            authority: self.authority.clone(),
            servername: self.servername.clone(),
            connect_timeout: self.connect_timeout,
            keepalive_time: self.keepalive_time,
            max_time: self.max_time,
            unix: self.unix,
            cacert: self.cacert.clone(),
            cert: self.cert.clone(),
            key: self.key.clone(),
            alts: self.alts,
            user_agent: self.user_agent.clone(),
            max_msg_sz: self.max_msg_sz,
        }
    }

    /// Build an `InvokeConfig` from CLI arguments.
    pub fn invoke_config(&self) -> InvokeConfig {
        InvokeConfig {
            format: self.format,
            emit_defaults: self.emit_defaults,
            allow_unknown_fields: self.allow_unknown_fields,
            format_error: self.format_error,
            data: self.data.clone(),
            headers: self.header.clone(),
            rpc_headers: self.rpc_header.clone(),
            expand_headers: self.expand_headers,
            max_msg_sz: self.max_msg_sz,
            verbosity: self.verbosity(),
            protoset_out: self.protoset_out.clone(),
            proto_out_dir: self.proto_out_dir.clone(),
        }
    }
}

/// The resolved command to execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    List,
    Describe,
    Invoke,
}

/// Result of parsing and validating positional arguments.
#[derive(Debug)]
pub struct ParsedArgs {
    pub address: Option<String>,
    pub command: Command,
    pub symbol: Option<String>,
}
