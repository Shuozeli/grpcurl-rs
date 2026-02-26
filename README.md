# grpcurl

A Rust port of [grpcurl](https://github.com/fullstorydev/grpcurl) -- a
command-line tool for interacting with gRPC servers. Like `curl` for gRPC.

grpcurl supports all core features from the Go version: server reflection, proto/protoset
file sources, all four RPC types (unary, server streaming, client streaming,
bidi streaming), TLS/mTLS, verbose output, and descriptor export.

## Installation

### From source

```bash
cargo install --locked grpcurl
```

### Build from repository

```bash
git clone https://github.com/shuozeli/grpcurl-rs.git
cd grpcurl-rs
cargo build --release
# Binary at target/release/grpcurl
```

## Usage

### List services

```bash
# Via server reflection
grpcurl --plaintext localhost:50051 list

# Via proto file (no server needed)
grpcurl --proto service.proto list

# Via protoset file (no server needed)
grpcurl --protoset descriptors.pb list
```

### List methods of a service

```bash
grpcurl --plaintext localhost:50051 list my.package.MyService
```

### Describe a symbol

```bash
# Describe a service
grpcurl --plaintext localhost:50051 describe my.package.MyService

# Describe a message with a JSON template
grpcurl --msg-template --plaintext localhost:50051 describe my.package.MyRequest
```

### Invoke an RPC

```bash
# Unary call with inline JSON
grpcurl --plaintext -d '{"name": "world"}' localhost:50051 my.package.Greeter/SayHello

# Read request from stdin
echo '{"name": "world"}' | grpcurl --plaintext -d @ localhost:50051 my.package.Greeter/SayHello

# Server streaming
grpcurl --plaintext -d '{"query": "foo"}' localhost:50051 my.package.MyService/Search

# Verbose output (headers, trailers, metadata)
grpcurl -v --plaintext -d '{}' localhost:50051 my.package.MyService/GetItem
```

### TLS connections

```bash
# Standard TLS (system root CAs)
grpcurl localhost:50051 list

# Custom CA certificate
grpcurl --cacert ca.pem localhost:50051 list

# Mutual TLS (client certificate)
grpcurl --cert client.pem --key client-key.pem localhost:50051 list

# Skip certificate verification (insecure)
grpcurl --insecure localhost:50051 list

# Plaintext (no TLS)
grpcurl --plaintext localhost:50051 list
```

### Headers and metadata

```bash
# Add headers to all requests
grpcurl -H "Authorization: Bearer token" --plaintext localhost:50051 list

# RPC-only headers (excluded from reflection)
grpcurl --rpc-header "x-request-id: 123" --plaintext -d '{}' localhost:50051 my.Service/Method

# Reflection-only headers
grpcurl --reflect-header "Authorization: Bearer token" --plaintext localhost:50051 list

# Expand environment variables in headers
grpcurl --expand-headers -H 'Authorization: Bearer ${TOKEN}' --plaintext localhost:50051 list
```

### Export descriptors

```bash
# Export as protoset (binary FileDescriptorSet)
grpcurl --protoset-out output.pb --plaintext localhost:50051 describe my.package.Service

# Export as .proto source files
grpcurl --proto-out-dir ./protos --plaintext localhost:50051 describe my.package.Service
```

## Feature Parity with Go grpcurl

grpcurl achieves 95.8% feature parity with the Go version (69/72
verification cases match byte-for-byte).

### Supported

- All three modes: `list`, `describe`, `invoke`
- Server reflection (v1 and v1alpha with auto-negotiation)
- Proto source files (`--proto`, `--import-path`)
- Protoset files (`--protoset`)
- Composite source (reflection + file fallback)
- All four RPC types: unary, server streaming, client streaming, bidi streaming
- JSON and text format (`--format json|text`)
- Verbose output (`-v`, `--vv`)
- TLS, mTLS, insecure, plaintext connections
- Unix domain sockets (`--unix`)
- Custom headers (`-H`, `--rpc-header`, `--reflect-header`, `--expand-headers`)
- `--emit-defaults`, `--allow-unknown-fields`, `--msg-template`
- `--protoset-out`, `--proto-out-dir` descriptor export
- `--max-msg-sz`, `--max-time`, `--connect-timeout`, `--keepalive-time`
- `--format-error` for structured error output
- `SSLKEYLOGFILE` support
- Gzip compression (transparent decompression)
- gRPC status code to exit code mapping (+64 offset)
- Single-dash flag compatibility (`-plaintext` works like `--plaintext`)

### Known Differences

| Area | Go grpcurl | grpcurl (Rust) | Impact |
|------|-----------|------------|--------|
| Help output | `-plaintext` | `--plaintext` | Both accepted at runtime |
| Text format | Legacy `<>` syntax | Modern `{}` syntax | Both valid protobuf text format |
| `--vv` timing | Timing data tree | Omitted | Stretch goal |
| ALTS | Supported | Not supported | No Rust equivalent exists |
| xDS | Supported | Not supported | No Rust equivalent exists |

## Project Structure

```
grpcurl-rs/
  grpcurl-core/     -- Library crate (descriptor sources, connection, formatting, invocation)
  grpcurl-cli/      -- Binary crate (CLI parsing, validation, main entry point)
  testing/
    testserver/     -- Test gRPC server with reflection
    bankdemo/       -- Bank demo gRPC server (auth, streaming, chat)
    tls/            -- Test TLS certificates
```

## Library Usage

The `grpcurl-core` crate can be used programmatically:

```rust
use grpcurl_core::connection::{ConnectionConfig, create_channel};
use grpcurl_core::reflection::ServerSource;
use grpcurl_core::descriptor::DescriptorSource;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ConnectionConfig {
        plaintext: true,
        ..Default::default()
    };
    let channel = create_channel(&config, "localhost:50051").await?;
    let source = ServerSource::new(channel);

    let services = source.list_services().await?;
    for svc in services {
        println!("{svc}");
    }
    Ok(())
}
```

## Documentation

- [CLI Usage Guide](docs/CLI_USAGE.md) -- comprehensive flag reference, examples, and patterns
- [Architecture](docs/ARCHITECTURE.md) -- crate split, module map, data flow, design decisions
- [Contributing](docs/CONTRIBUTING.md) -- build, test, add flags, release process

## Testing

```bash
# Run offline tests (no server needed)
cargo test -p grpcurl-core -p grpcurl

# Run all tests including server-dependent tests
cargo test -- --include-ignored
```

See [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) for details on the test
infrastructure and categories.

## License

MIT
