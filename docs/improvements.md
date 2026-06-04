# grpcurl-rs - Improvements and Future Work

Track planned improvements, tech debt, and ideas for future work.

---

## I-001: Remove dependency on prost-reflect

**Priority:** High
**Status:** Open

The `prost-reflect` crate is currently used for dynamic message handling,
descriptor pools, and JSON/text serialization of protobuf messages. We should
not trust this third-party library in our codebase long-term.

**Current usage:**
- `src/descriptor.rs`: `DescriptorPool`, `DynamicMessage`, `FieldDescriptor`,
  `ExtensionDescriptor`, `MessageDescriptor`, `ServiceDescriptor`,
  `MethodDescriptor`, `EnumDescriptor`, `EnumValueDescriptor`,
  `OneofDescriptor`, `FileDescriptor`
- `src/metadata.rs`: (indirect -- `Bytes` type re-exported through prost-reflect)

**Options to evaluate:**
1. Build our own thin descriptor pool and dynamic message layer on top of raw
   `prost` + `prost-types` (FileDescriptorSet, FileDescriptorProto, etc.)
2. Fork/vendor prost-reflect with only the subset we need
3. Use `protox` for parsing and build our own runtime reflection on top of the
   parsed descriptors

**Blocked by:** Migration is complete. The next step is to catalog exactly
which prost-reflect APIs are used (DescriptorPool, DynamicMessage,
SerializeOptions, DeserializeOptions, MessageDescriptor, ServiceDescriptor,
MethodDescriptor, EnumDescriptor, FieldDescriptor, ExtensionDescriptor,
OneofDescriptor, EnumValueDescriptor, FileDescriptor, Kind, Value, MapKey)
and evaluate the cost of replacing them with a custom thin layer on top of
raw prost + prost-types.

---

## I-002: Comprehensive test coverage before landing

**Priority:** Critical (must complete before v1.0)
**Status:** Partially complete

Before shipping grpcurl-rs, we need a rigorous test strategy that proves
behavioral parity with the Go implementation. This involves three workstreams:

### A. Audit all Go test cases

Go through every test file in the original Go grpcurl repo and catalog each
test case:

- `grpcurl_test.go` -- core library tests
- `cmd/grpcurl/` -- CLI-level tests if any
- Any test helpers, test servers, or test proto files

For each Go test case, document:
1. What it tests (input, expected output, edge case)
2. Whether we have an equivalent test in grpcurl-rs
3. If not, create one or explain why it's not applicable

Produce a coverage matrix: Go test name -> Rust equivalent -> status (covered /
missing / not-applicable).

### B. Systematic test strategy

Design a layered test approach:

1. **Unit tests** (per-module, already started):
   - `error.rs`: error type construction, is_not_found_error
   - `descriptor.rs`: symbol lookup, list_services, list_methods, edge cases
   - `metadata.rs`: header parsing, env var expansion, base64 decode
   - `format.rs` (future): JSON/text parsing, formatting
   - `invoke.rs` (future): request supplier, event handler callbacks

2. **Integration tests** (cross-module):
   - Load protoset -> list -> verify output
   - Load protoset -> describe -> verify descriptor text
   - Load proto files -> list -> verify output
   - Connection + reflection -> list -> verify output

3. **CLI verification tests** (converted from verify_cli.py to native Rust, 49 offline + 54 server = 103 integration tests):
   - Every validation rule produces the correct error
   - Every flag combination is accepted or rejected correctly
   - Output verified against expected strings (Python scripts deleted)

4. **Golden tests** (new -- see section C below)

5. **End-to-end tests** (Phase 9B):
   - Real gRPC server, all 4 RPC types, all streaming modes
   - TLS modes, headers, verbose output

### C. Golden tests: user input -> gRPC call mapping

Add a golden test framework that captures the **exact gRPC calls** grpcurl-rs
would make for a given set of CLI arguments, without actually connecting to a
server. This proves the tool translates user intent into the correct wire
behavior.

**Design:**
- Introduce a test-only abstraction (e.g., a `GrpcCallRecorder` or mock
  transport) that captures:
  - Target address and method name
  - Request metadata (headers)
  - Request message(s) (serialized or as JSON)
  - Call type (unary / client-stream / server-stream / bidi)
  - TLS configuration used
  - Timeout / keepalive settings
- Each golden test specifies:
  - **Input:** CLI args + stdin data (if any) + proto/protoset files
  - **Expected output:** The exact gRPC call(s) that would be made

**Example golden test cases:**
```
# Unary call with JSON body
input:  grpcurl -plaintext -d '{"name":"world"}' localhost:8080 test.Greeter/SayHello
expect:
  method: /test.Greeter/SayHello
  type: unary
  metadata: {}
  messages: [{"name":"world"}]

# List via reflection
input:  grpcurl -plaintext localhost:8080 list
expect:
  method: /grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo
  type: bidi-stream
  messages: [ListServices request]

# Call with custom headers and env expansion
input:  grpcurl -H 'Authorization: Bearer ${TOKEN}' -rpc-header 'x-request-id: abc' ...
expect:
  metadata: {authorization: "Bearer <expanded>", x-request-id: "abc"}

# Client streaming with multiple messages
input:  grpcurl -d @ ... < multi.json
expect:
  type: client-stream
  messages: [{...}, {...}, {...}]
```

**Format:** Store golden files as TOML or JSON in `tests/golden/`. Each file
contains the CLI args, optional stdin, the protoset/proto reference, and the
expected call descriptor. A Rust integration test reads each golden file,
constructs the call plan (without executing), and asserts it matches.

**Status update:** Phase 6 is complete. CLI verification tests were converted
from Python to 103 native Rust integration tests (Step 33 of migration journal).
Golden test framework has not yet been built.
