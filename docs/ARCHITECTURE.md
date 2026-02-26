# Architecture

## Crate Split

grpcurl is a Cargo workspace with two primary crates:

**grpcurl-core** (library) -- Framework-agnostic gRPC introspection and
invocation library. Zero CLI dependencies (no clap). Can be used
programmatically by other Rust projects.

**grpcurl-cli** (binary) -- Thin CLI layer on top of grpcurl-core. Handles
argument parsing via clap, Go-style flag compatibility, 28 validation rules,
and command dispatch.

## Module Map

### grpcurl-core/src/

#### descriptor.rs -- Descriptor Source Abstraction

The core trait that all descriptor sources implement:

```
trait DescriptorSource (async, via async-trait):
    list_services()              -> Vec<String>
    find_symbol(name)            -> SymbolDescriptor
    all_extensions_for_type(name)-> Vec<FieldDescriptor>
    get_all_files()              -> Vec<FileDescriptorProto>
    descriptor_pool()            -> Option<&DescriptorPool>  (sync)
```

Implementations:
- **FileSource** -- loads from proto files (via protox) or protoset files.
  Pure in-memory after loading; async methods return immediately.
- **CompositeSource** -- delegates to a primary (typically ServerSource) and
  falls back to a secondary (typically FileSource).

Helper functions: `list_services()`, `list_methods()`, `get_all_files()`,
`write_protoset()`, `write_proto_files()`,
`descriptor_source_from_protosets()`, `descriptor_source_from_proto_files()`

Enum `SymbolDescriptor`: Service, Method, Message, Enum, Field, Extension,
OneOf, EnumValue, File.

#### reflection.rs -- Server Reflection Client

**ServerSource** implements `DescriptorSource` via gRPC server reflection.
- Auto-negotiates v1 vs v1alpha reflection API
- Lazily populates a `DescriptorPool` as symbols are queried
- Thread-safe via `Mutex<DescriptorPool>`
- Supports `--max-msg-sz` and custom reflection headers

#### connection.rs -- Channel Creation and TLS

**ConnectionConfig** struct decouples connection parameters from CLI:
```
ConnectionConfig {
    plaintext, insecure, authority, servername,
    connect_timeout, keepalive_time, max_time, unix,
    cacert, cert, key, alts, user_agent, max_msg_sz
}
```

`create_channel(config, address)` -> `tonic::Channel` handles:
- Plaintext HTTP/2
- Standard TLS (system root CAs via rustls-native-certs)
- Custom CA (`--cacert`)
- Mutual TLS (`--cert` + `--key`)
- Insecure TLS (custom `ServerCertVerifier` that skips verification)
- Unix domain sockets (via hyper-util + tower connector)
- Connection timeout, keepalive, User-Agent header

#### format.rs -- Request Parsing and Response Formatting

- `Format` enum: Json, Text
- `FormatOptions` struct: emit_defaults, allow_unknown_fields
- `RequestParser` trait with `JsonRequestParser` and `TextRequestParser`
- `Formatter` struct for response output (JSON or text)
- gRPC status code name formatting

#### commands/list.rs -- List Command

`run_list(source, symbol?)` -- lists all services or all methods of a service.

#### commands/describe.rs -- Describe Command

`run_describe(source, symbol?, options, msg_template)` -- prints descriptor
text and optional JSON input template.

#### commands/invoke.rs -- RPC Invocation

**InvokeConfig** struct decouples invocation parameters from CLI:
```
InvokeConfig {
    format, emit_defaults, allow_unknown_fields, format_error,
    data, headers, rpc_headers, expand_headers,
    max_msg_sz, verbosity, protoset_out, proto_out_dir
}
```

`run_invoke(config, channel, symbol, source)` handles all 4 RPC types:
- Unary (one request, one response)
- Server streaming (one request, multiple responses)
- Client streaming (multiple requests, one response)
- Bidirectional streaming (multiple requests, multiple responses)

Uses `DynamicCodec` for runtime protobuf encoding/decoding.

#### codec.rs -- Dynamic gRPC Codec

`DynamicCodec` implements `tonic::Codec` for `prost_reflect::DynamicMessage`,
enabling RPC invocation without compile-time generated stubs.

#### descriptor_text.rs -- Proto Source Text Output

`get_descriptor_text(symbol)` -- formats any `SymbolDescriptor` as .proto
source text. Used by the `describe` command.

`format_proto_file(file)` -- generates complete .proto file content from a
`FileDescriptor`. Used by `--proto-out-dir` export.

#### metadata.rs -- Header/Metadata Utilities

- `metadata_from_headers(strings)` -- parses "Name: Value" strings into
  `tonic::metadata::MetadataMap`
- `expand_headers(strings)` -- `${VAR}` expansion in header values
- `metadata_to_string(map)` -- human-readable formatting for verbose output
- Binary header support (keys ending in `-bin`) with base64 decoding

#### error.rs -- Error Types

`GrpcurlError` enum: NotFound, ReflectionNotSupported, InvalidArgument, Io,
Proto, GrpcStatus, Other.

Type alias: `Result<T> = std::result::Result<T, GrpcurlError>`

### grpcurl-cli/src/

#### cli.rs -- CLI Argument Definitions

`Cli` struct with 30+ flags organized by category (clap derive).

`normalize_args()` -- pre-processes argv to convert Go-style single-dash
(`-plaintext`) to double-dash (`--plaintext`) for all known long flags. This
preserves full Go grpcurl CLI compatibility.

`Cli::connection_config()` and `Cli::invoke_config()` bridge CLI args to the
library's config structs.

`Command` enum: List, Describe, Invoke.
`ParsedArgs` struct: address, command, symbol.

#### validate.rs -- 28 Validation Rules

`validate(cli)` -> `Result<ParsedArgs, String>` implements all 28 validation
rules from the original Go grpcurl in order. Hard errors return `Err`,
warnings print to stderr.

#### main.rs -- Entry Point and Command Dispatch

1. `normalize_args()` -> clap parse -> `validate()`
2. Build `ConnectionConfig` and `InvokeConfig` from CLI
3. `create_channel()` for server connection
4. `create_descriptor_source()` -- builds FileSource, ServerSource, or
   CompositeSource based on CLI flags
5. Dispatch to `run_list()`, `run_describe()`, or `run_invoke()`
6. Map gRPC status code to exit code (status + 64 offset)

## Data Flow

```
CLI argv
  |
  v
normalize_args()          Convert -flag to --flag
  |
  v
Cli::parse()              Clap derives struct from args
  |
  v
validate()                28 rules -> ParsedArgs {address, command, symbol}
  |
  +-> connection_config() -> ConnectionConfig
  +-> invoke_config()     -> InvokeConfig
  |
  v
create_channel()          ConnectionConfig + address -> tonic::Channel
  |
  v
create_descriptor_source()
  |  +-> FileSource          (from --proto or --protoset)
  |  +-> ServerSource        (via gRPC reflection)
  |  +-> CompositeSource     (both, reflection primary)
  |
  v
Command dispatch
  +-> run_list(source, symbol?)
  +-> run_describe(source, symbol?, options, msg_template)
  +-> run_invoke(config, channel, symbol, source)
        |
        v
      resolve_method()    source.find_symbol() -> MethodDescriptor
        |
        v
      parse request       RequestParser (JSON or text) -> DynamicMessage(s)
        |
        v
      DynamicCodec        encode -> gRPC call -> decode
        |
        v
      format response     Formatter -> stdout (JSON or text)
        |
        v
      exit code           gRPC status + 64 offset
```

## Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| `async-trait` for `DescriptorSource` | Native async fn in traits doesn't support `dyn Trait`. The codebase uses `&dyn DescriptorSource` in ~15 locations for dynamic dispatch. |
| `ConnectionConfig` / `InvokeConfig` structs | Decouple library from CLI framework. Enables programmatic use of grpcurl-core without clap. |
| `DynamicCodec` | Enables RPC invocation without compile-time proto stubs. Uses prost-reflect for runtime message encoding/decoding. |
| Lazy reflection pool | `ServerSource` only queries the server when a symbol is requested, minimizing reflection roundtrips. |
| `normalize_args()` | Full Go CLI compatibility without modifying clap's behavior. Users can use either `-plaintext` or `--plaintext`. |
| `protox` for proto parsing | Pure-Rust protobuf compiler, avoids `protoc` binary dependency for the core crate. |
| `rustls` (not native-tls) | Pure-Rust TLS, consistent behavior across platforms, supports SSLKEYLOGFILE. |

## Key Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| tonic | 0.14 | gRPC framework (channel, codegen, TLS) |
| prost | 0.14 | Protobuf runtime |
| prost-reflect | 0.16 | Dynamic protobuf messages |
| prost-types | 0.14 | Well-known protobuf types |
| protox | 0.9 | Proto file parsing (pure Rust, replaces protoc) |
| async-trait | 0.1 | Async fn in traits with dyn support |
| clap | 4 | CLI argument parsing (grpcurl-cli only) |
| rustls | 0.23 | TLS implementation |
| tokio | 1 | Async runtime |
| serde_json | 1 | JSON formatting |
| base64 | 0.22 | Binary header encoding |
| regex | 1 | Environment variable expansion |
