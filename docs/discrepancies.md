# grpcurl Go vs Rust - Discrepancy Tracker

Track all known behavioral differences between the Go grpcurl and the Rust
grpcurl-rs implementation. Each entry notes whether the difference is
intentional (by design), a known limitation, or a temporary gap.

---

## Intentional Differences

### D-001: Flag syntax in --help output
- **Go:** Single-dash flags (`-plaintext`) shown in help
- **Rust:** Double-dash flags (`--plaintext`) shown in help
- **Mitigation:** Both forms accepted at runtime via `normalize_args()`
- **Status:** Permanent (idiomatic Rust CLI convention)

### D-002: Error message wording
- **Go:** Uses single-dash flag names in error messages (e.g., "The -insecure argument")
- **Rust:** Uses double-dash flag names (e.g., "The --insecure argument")
- **Status:** Permanent (matches respective --help output)

## Known Limitations

### D-003: ALTS transport not supported
- **Go:** Full ALTS support via `google.golang.org/grpc/credentials/alts`
- **Rust:** No equivalent crate exists in the tonic ecosystem
- **Mitigation:** `--alts` flag accepted but prints clear error at runtime
- **Status:** May revisit if a Rust ALTS crate becomes available

### D-004: xDS resolver not supported
- **Go:** Full xDS support via `google.golang.org/grpc/xds`
- **Rust:** No equivalent crate exists in the tonic ecosystem
- **Mitigation:** `xds:///` address prefix detected and prints clear error
- **Status:** May revisit if a Rust xDS crate becomes available

### D-010: gRPC status detail parsing
- **Go:** Parses `google.rpc.Status` details from the error response and formats each detail
  as a proto message with numbered prefix ("1)", "2)") and 2-space indentation
- **Rust:** Shows raw byte length only: `"Details (raw): N bytes (detail parsing not yet implemented)"`
- **Impact:** Error details are not human-readable in the Rust version
- **Status:** Known limitation. Requires implementing `google.rpc.Status` detail parsing with
  Any type resolution via the descriptor pool.

### D-008: Verbose `--vv` timing data
- **Go:** Prints a timing data tree at the end of `--vv` output: `Timing Data: Xms\n  Dial: Yms\n  InvokeRPC: Zms`
- **Rust:** Omits timing data
- **Impact:** All other verbose output (method descriptor, headers, trailers, message sizes, response count) is byte-identical
- **Status:** Stretch goal. Would require instrumenting connection + invocation timing.

### D-007: Text format output syntax
- **Go:** Uses legacy proto text format with angle brackets and colons: `field: <\n  ...\n>`
- **Rust:** Uses modern proto text format with curly braces: `field {\n  ...\n}`
- **Impact:** Both are valid protobuf text format representations, parsers accept both
- **Status:** Permanent (prost-reflect uses modern syntax, Go's deprecated `proto.TextMarshaler` uses legacy)

## Resolved

### D-005: Core commands implemented (RESOLVED)
- All commands (list, describe, invoke) fully working with protoset, proto files,
  and server reflection. All 4 RPC types, verbose output, headers, exit codes,
  --msg-template, --protoset-out, --max-time all implemented and verified.

### D-006: --insecure flag (RESOLVED)
- Implemented via custom rustls `ServerCertVerifier` that accepts all certificates.
- Uses `Endpoint::connect_with_connector()` with `tokio-rustls::TlsConnector`
  wrapping the insecure config. Matches Go's `InsecureSkipVerify: true` behavior.

### D-009: `--max-msg-sz` enforcement scope (RESOLVED)
- **Go:** Applies max-msg-sz to all gRPC calls including server reflection queries.
- **Rust:** Now also applies max-msg-sz to reflection clients (v1 and v1alpha).
- **Resolution:** Added `with_max_msg_sz()` builder method to `ServerSource`. Both
  binaries now fail at the reflection stage with exit code 1 when max-msg-sz is too small.
