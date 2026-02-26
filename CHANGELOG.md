# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-02-26

### Fixed

- **[CRITICAL] Extra message rejection on unary/server-streaming RPCs** --
  Previously, extra messages in request data were silently ignored, causing data
  loss. Now returns an error matching Go grpcurl behavior. (C3)

- **[CRITICAL] Template generation crash on `google.protobuf.Any` fields** --
  `--msg-template` crashed with `unsupported type url ''` for messages containing
  Any fields. Added well-known type handling for Any, Value, Struct, ListValue,
  and Timestamp in template generation. (H2)

- **[CRITICAL] Extension registry returning wrong descriptor type** --
  `all_extensions_for_type` now returns `Vec<ExtensionDescriptor>` instead of
  `Vec<FieldDescriptor>`, matching the protobuf extension model. (C1)

- **[CRITICAL] Bidi streaming was not concurrent** --
  Changed from collecting all request messages upfront to channel-based
  concurrent send/receive using `tokio::spawn` and `mpsc`. (C2)

- **[HIGH] Reflection version not cached** --
  Added `AtomicU8`-based caching so the reflection API version (v1 vs v1alpha)
  is negotiated once and reused for subsequent calls. (H1)

- **[HIGH] Missing file descriptors in reflection** --
  Batch `file_containing_symbol` failures now fall back to one-at-a-time
  requests instead of failing the entire operation. (H4)

- **[HIGH] Headers/trailers not shown on error responses** --
  Response trailers are now printed even when the RPC returns an error
  status. (H5)

- **[HIGH] `--use-reflection` default when proto files provided** --
  Reflection is now auto-disabled when `--proto` or `--protoset` files are
  provided, matching Go grpcurl behavior. (H6)

- **[MEDIUM] gRPC status error details not parsed** --
  Error responses now decode the `grpc-status-details-bin` trailer and print
  detail type URLs. (M2)

- **[MEDIUM] Status code spelling** --
  Changed "Cancelled" to "Canceled" to match the gRPC specification and Go
  grpcurl output. (M3)

- **[MEDIUM] Silently dropped metadata headers** --
  Invalid metadata keys/values now print a warning to stderr instead of being
  silently ignored. (M4)

- **[MEDIUM] Descriptor text missing field options and reserved ranges** --
  `describe` output now includes field options (e.g., `[deprecated = true]`,
  `[json_name = "..."]`) and reserved ranges/names on messages. (M5, M6)

- **[MEDIUM] Client-streaming sent phantom empty message on zero input** --
  Removed the fallback that injected a default empty message when no request
  data was provided. Now sends 0 messages matching Go behavior. (M8)

- **[MEDIUM] Unary trailer misattribution** --
  Unary and client-streaming RPCs now use `response.metadata()` for trailers
  instead of showing an empty trailer section. (M9)

- **[MEDIUM] Text separator not controlled by verbosity** --
  Text format separator between messages is now only printed when verbosity
  is 0, matching Go grpcurl behavior. (H3)

- **[LOW] `.expect()` panics on TLS configuration errors** --
  Replaced 3 `.expect()` calls in connection setup with proper
  `Result`-returning error handling. (L5)

- **[LOW] Float formatting in `google.protobuf.Value` fields** --
  Whole-number doubles (e.g., `42.0`) are now rendered without the trailing
  `.0` (e.g., `42`) to match Go's JSON encoding.

### Added

- **`InvocationEventHandler` trait** --
  Pluggable handler for RPC lifecycle events (method resolution, send headers,
  receive headers, receive response, receive trailers). Enables custom
  formatting and logging without modifying the invocation engine. (M10)

- **`write_status` function** --
  Status printing now accepts `&mut dyn io::Write` for flexible output
  targeting. (M13)

- **`print_status` formatter parameter** --
  Status printer optionally accepts a `Formatter` to control how error details
  are rendered.
