# Contributing to grpcurl

## Development Setup

**Prerequisites:**
- Rust 1.75+ (for async fn in trait support)
- Cargo (ships with Rust)

```bash
git clone https://github.com/shuozeli/grpcurl-rs.git
cd grpcurl-rs
cargo build
```

### Workspace Structure

```
grpcurl-rs/
  grpcurl-core/     Library crate (descriptor sources, connection, formatting, invocation)
  grpcurl-cli/      Binary crate (CLI parsing, validation, main entry point)
  testing/
    testserver/     Test gRPC server with reflection (requires protoc)
    bankdemo/       Bank demo gRPC server (requires protoc)
    tls/            Test TLS certificates
```

## Building

```bash
# Debug build (core + CLI only, skips test servers that need protoc)
cargo build -p grpcurl-core -p grpcurl

# Release build
cargo build --release -p grpcurl-core -p grpcurl

# Binary location
./target/release/grpcurl
```

The release profile uses LTO and stripping for smaller binaries. To build the
test servers as well, install `protoc` (`apt install protobuf-compiler`) and
run `cargo build` at the workspace root.

## Testing

### Quick Start

```bash
# Run offline tests only (unit + integration, no server needed)
cargo test -p grpcurl-core -p grpcurl

# Run ALL tests including server tests
cargo test -- --include-ignored
```

### Test Categories

#### Unit Tests (57 tests)

Located in `grpcurl-core/src/` modules (`#[cfg(test)]` blocks). These test
internal logic: argument validation, header parsing, JSON formatting, descriptor
text output, etc.

```bash
cargo test -p grpcurl-core --lib
```

#### Offline Integration Tests (49 tests)

Located in `grpcurl-cli/tests/`. These invoke the compiled `grpcurl` binary and
verify CLI behavior without a running server:

| File | Tests | What it covers |
|------|-------|----------------|
| `cli_help.rs` | 2 | `-help` and `-version` flags |
| `cli_validation.rs` | 19 | Argument validation errors (exit code 2) |
| `cli_args.rs` | 8 | Valid parsing, single/double-dash compat, warnings |
| `protoset_list.rs` | 3 | `list` from protoset files |
| `protoset_describe.rs` | 12 | `describe` + `msg-template` from protoset files |
| `protoset_export.rs` | 3 | `-protoset-out` offline export |
| `proto_export.rs` | 2 | `-proto-out-dir` offline export |

```bash
cargo test -p grpcurl --test cli_help --test cli_validation --test cli_args \
           --test protoset_list --test protoset_describe \
           --test protoset_export --test proto_export
```

#### Server Integration Tests (54 tests, `#[ignore]`)

These tests are marked `#[ignore]` and require the testserver. Each test file
automatically starts and stops its own testserver instance on an ephemeral port
-- no manual server setup is needed.

| File | Tests | What it covers |
|------|-------|----------------|
| `server_discovery.rs` | 5 | `list` via reflection |
| `server_describe.rs` | 12 | `describe` + `msg-template` via reflection |
| `server_unary.rs` | 5 | Unary RPCs (EmptyCall, UnaryCall, emit-defaults) |
| `server_streaming.rs` | 7 | Server, client, and bidi streaming |
| `server_errors.rs` | 7 | Error status codes, non-existent methods/services |
| `server_metadata.rs` | 6 | Header echo, fail-early, fail-late |
| `server_verbose.rs` | 6 | Verbose (`-v`) and very-verbose (`--vv`) output |
| `server_advanced.rs` | 4 | Complex types, max-msg-sz, stdin (`-d @`) |
| `protoset_export.rs` | 1 | `-protoset-out` via reflection |
| `proto_export.rs` | 1 | `-proto-out-dir` via reflection |

```bash
# Run server tests only
cargo test -- --ignored

# Run a specific server test file
cargo test --test server_unary -- --ignored
```

**Prerequisite:** The testserver binary must be built before running server
tests. Cargo builds it automatically as a workspace member, but if you see
"testserver not found" errors, run:

```bash
cargo build -p testserver
```

### Test Infrastructure

- **`tests/common/mod.rs`** -- shared helpers: `run()` spawns the grpcurl
  binary, captures stdout/stderr/exit code; assertion helpers for exit codes
  and output content.
- **`tests/common/server.rs`** -- `TestServer` struct that starts the
  testserver on an ephemeral port and kills it on `Drop`. Uses
  `std::sync::LazyLock` for one-time initialization per test file.
- **`tests/testdata/`** -- pre-compiled protoset files (`.pb`) and proto
  source files used by offline tests.

## Code Quality

```bash
# Lint (zero warnings policy)
cargo clippy -p grpcurl-core -p grpcurl -- -D warnings

# Format check
cargo fmt --check
```

- Zero clippy warnings policy -- all PRs must pass `clippy -D warnings`
- Internal `unwrap()` calls on infallible operations (regex compilation,
  known-good parsing) are acceptable
- User-facing code paths should use proper error handling with `Result`

## Adding a New CLI Flag

1. Add the field to the `Cli` struct in `grpcurl-cli/src/cli.rs`
2. If it's a long flag, add its name to the `LONG_FLAGS` array (for Go-style
   `-flag` compatibility)
3. Add validation rules in `grpcurl-cli/src/validate.rs` if the flag has
   constraints or conflicts
4. Wire the value through `ConnectionConfig` or `InvokeConfig` as appropriate
   (see `Cli::connection_config()` and `Cli::invoke_config()`)
5. Implement the behavior in `grpcurl-core`
6. Add unit tests + integration tests
7. Update `docs/CLI_USAGE.md` with the new flag

## Adding a New Command Module

1. Create the module in `grpcurl-core/src/commands/`
2. Add `pub mod` to `grpcurl-core/src/commands/mod.rs`
3. Wire up dispatch in `grpcurl-cli/src/main.rs`
4. Add integration tests in `grpcurl-cli/tests/`

## Pre-Release Validation

```bash
# Full build + lint
cargo build --release -p grpcurl-core -p grpcurl
cargo clippy -p grpcurl-core -p grpcurl -- -D warnings

# Binary sanity check
./target/release/grpcurl -version
./target/release/grpcurl -plaintext localhost:5555 list

# Run all tests (unit + integration + server)
cargo test -- --include-ignored
```

## Release Process

### Cross-Platform Targets

| Target | Platform |
|--------|----------|
| `x86_64-unknown-linux-gnu` | Linux x64 |
| `x86_64-apple-darwin` | macOS x64 |
| `aarch64-apple-darwin` | macOS ARM64 |
| `x86_64-pc-windows-msvc` | Windows x64 |

### Publishing

1. Bump version in both `grpcurl-core/Cargo.toml` and `grpcurl-cli/Cargo.toml`
2. Run pre-release validation (see above)
3. Publish library first: `cargo publish -p grpcurl-core`
4. Publish binary: `cargo publish -p grpcurl`
