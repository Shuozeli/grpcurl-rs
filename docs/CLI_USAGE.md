# grpcurl CLI Usage Guide

A comprehensive reference for the grpcurl command-line tool. Covers every flag,
every mode, exit codes, and real-world patterns. Useful for both humans and
LLM agents that need to construct grpcurl commands.

## Syntax

```
grpcurl [flags] <address> list [service]
grpcurl [flags] <address> describe [symbol]
grpcurl [flags] <address> <service/method>
grpcurl [flags] --proto <file> list              (no server needed)
grpcurl [flags] --protoset <file> describe       (no server needed)
```

The address is `host:port` (or a Unix socket path with `--unix`). IPv6
addresses must be bracketed: `[::1]:50051`.

## Modes

### list

Enumerate services or methods.

```bash
# List all services on a server
grpcurl --plaintext localhost:50051 list

# List methods of a specific service
grpcurl --plaintext localhost:50051 list my.package.MyService

# List from a protoset file (no server needed)
grpcurl --protoset descriptors.pb list
```

**Output format:** one fully-qualified name per line.

### describe

Show the protobuf definition of a symbol.

```bash
# Describe all services
grpcurl --plaintext localhost:50051 describe

# Describe a specific service
grpcurl --plaintext localhost:50051 describe my.package.MyService

# Describe a message type
grpcurl --plaintext localhost:50051 describe my.package.MyRequest

# Show a JSON input template for a message
grpcurl --msg-template --plaintext localhost:50051 describe my.package.MyRequest
```

**Output format:** proto source text representation of the symbol.

### invoke

Call an RPC method. The method must be fully-qualified in `service/method` or
`service.method` format.

```bash
# Unary call with inline JSON
grpcurl --plaintext -d '{"name": "world"}' localhost:50051 my.package.Greeter/SayHello

# Read request from stdin
echo '{"name": "world"}' | grpcurl --plaintext -d @ localhost:50051 my.package.Greeter/SayHello

# Empty request (sent automatically for unary/server-streaming if no -d)
grpcurl --plaintext localhost:50051 my.package.TestService/EmptyCall

# Server streaming
grpcurl --plaintext -d '{"query": "foo"}' localhost:50051 my.package.Svc/Search

# Client streaming (newline-delimited JSON)
printf '{"value":1}\n{"value":2}\n{"value":3}' | \
  grpcurl --plaintext -d @ localhost:50051 my.package.Svc/Aggregate

# Bidi streaming
printf '{"msg":"hello"}\n{"msg":"world"}' | \
  grpcurl --plaintext -d @ localhost:50051 my.package.Svc/Chat

# Verbose output (headers, trailers, metadata)
grpcurl -v --plaintext -d '{}' localhost:50051 my.package.Svc/GetItem
```

For client and bidi streaming, multiple messages are sent as newline-delimited
JSON (or `0x1E`-separated text format messages).

---

## Flag Reference

### Connection and Networking

#### `--plaintext`

Use plain-text HTTP/2 (no TLS). Required for most local development servers.

```bash
grpcurl --plaintext localhost:50051 list
```

#### `--insecure`

Skip server certificate verification. Mutually exclusive with `--plaintext`.

```bash
grpcurl --insecure myserver:443 list
```

#### `--authority <value>`

Set the `:authority` pseudo-header in HTTP/2. Also used as the TLS server name
for certificate verification.

```bash
grpcurl --authority api.example.com 10.0.0.1:443 list
```

#### `--servername <value>`

Override TLS server name verification. Prefer `--authority` instead. Cannot
have a different value from `--authority` if both are specified.

#### `--connect-timeout <seconds>`

Connection establishment timeout in seconds. Default: 10.

```bash
grpcurl --connect-timeout 30 --plaintext slow-server:50051 list
```

#### `--keepalive-time <seconds>`

Idle time in seconds before sending a keepalive probe.

```bash
grpcurl --keepalive-time 60 --plaintext localhost:50051 list
```

#### `--max-time <seconds>`

Total operation timeout in seconds.

```bash
grpcurl --max-time 5 --plaintext localhost:50051 my.Svc/SlowMethod
```

#### `--unix`

Interpret the address as a Unix domain socket path.

```bash
grpcurl --plaintext --unix /var/run/grpc.sock list
```

### TLS and Security

#### `--cacert <file>`

Custom CA certificate file for server verification.

```bash
grpcurl --cacert ca.pem myserver:443 list
```

#### `--cert <file>`

Client certificate for mutual TLS. Must be paired with `--key`.

```bash
grpcurl --cert client.pem --key client-key.pem myserver:443 list
```

#### `--key <file>`

Client private key for mutual TLS. Must be paired with `--cert`.

#### `--alts`

Use Application Layer Transport Security. **Not supported** in grpcurl
(no Rust equivalent exists). Prints an error if used.

#### `--alts-handshaker-service <address>`

ALTS handshaker server address. Requires `--alts`. **Not supported.**

#### `--alts-target-service-account <email>`

Expected ALTS service account. Can be repeated. Requires `--alts`.
**Not supported.**

### Descriptor Sources

#### `--proto <file>`

Proto source file to load. Can be repeated. Mutually exclusive with
`--protoset`. Enables offline operations (no server needed for list/describe).

```bash
grpcurl --proto service.proto list
grpcurl --proto api.proto --proto types.proto describe my.Message
```

#### `--import-path <dir>`

Import directory for proto file resolution. Can be repeated. Only used with
`--proto`.

```bash
grpcurl --proto api.proto --import-path ./protos --import-path ./third_party list
```

#### `--protoset <file>`

Pre-compiled `FileDescriptorSet` binary file. Can be repeated. Mutually
exclusive with `--proto`.

```bash
grpcurl --protoset descriptors.pb list
grpcurl --protoset svc1.pb --protoset svc2.pb describe
```

#### `--use-reflection`

Force server reflection even when `--proto` or `--protoset` is provided.
Default: `true` unless file sources are given.

When both file sources and reflection are available, a CompositeSource is
created (reflection primary, file fallback).

```bash
# Force reflection alongside a protoset
grpcurl --protoset types.pb --use-reflection --plaintext localhost:50051 list
```

### Request Data

#### `-d <data>`

Request body. Use `@` to read from stdin. JSON format by default; text format
with `--format text`. For client/bidi streaming, provide newline-delimited
messages.

```bash
# Inline JSON
grpcurl --plaintext -d '{"id": 123}' localhost:50051 my.Svc/GetItem

# From stdin
echo '{"id": 123}' | grpcurl --plaintext -d @ localhost:50051 my.Svc/GetItem

# Multiple messages for streaming
printf '{"id":1}\n{"id":2}' | grpcurl --plaintext -d @ localhost:50051 my.Svc/BatchGet
```

#### `--format <json|text>`

Request and response data format. Default: `json`.

```bash
grpcurl --format text -d 'name: "world"' --plaintext localhost:50051 my.Greeter/SayHello
```

#### `--allow-unknown-fields`

Accept unknown fields in JSON request data without error.

### Response Formatting

#### `--emit-defaults`

Include fields with default/zero values in JSON output. Only applies to JSON
format; a warning is emitted if used with text format.

```bash
grpcurl --emit-defaults --plaintext -d '{}' localhost:50051 my.Svc/GetItem
```

#### `--msg-template`

Show a JSON input template when using `describe` on a message type.

```bash
grpcurl --msg-template --plaintext localhost:50051 describe my.package.MyRequest
```

#### `--format-error`

Format error responses using `--format` instead of the default error output.

### Headers and Metadata

#### `-H <header>`

Add a header to **all** requests (RPCs + reflection). Repeatable.
Format: `"Name: Value"`.

```bash
grpcurl -H "Authorization: Bearer token" --plaintext localhost:50051 list
grpcurl -H "Authorization: Bearer token" -H "X-Request-ID: abc" \
  --plaintext -d '{}' localhost:50051 my.Svc/Method
```

#### `--rpc-header <header>`

Add a header to **RPC invocations only** (not reflection). Repeatable.

```bash
grpcurl --rpc-header "x-request-id: abc123" \
  --plaintext -d '{}' localhost:50051 my.Svc/Method
```

#### `--reflect-header <header>`

Add a header to **reflection requests only**. Repeatable. A warning is
emitted if used with `--protoset` (no reflection needed).

```bash
grpcurl --reflect-header "Authorization: Bearer token" \
  --plaintext localhost:50051 list
```

#### `--expand-headers`

Enable `${VAR}` expansion in header values using environment variables. Fails
if any referenced variable is undefined.

```bash
export TOKEN=my-secret
grpcurl --expand-headers -H 'Authorization: Bearer ${TOKEN}' \
  --plaintext localhost:50051 list
```

#### `--user-agent <string>`

Custom User-Agent string. Prepended to the default `grpcurl/<version>`.

### Output and Export

#### `--protoset-out <file>`

Write discovered descriptors as a binary `FileDescriptorSet`. Works with
list, describe, and invoke.

```bash
grpcurl --protoset-out output.pb --plaintext localhost:50051 describe my.Service
```

#### `--proto-out-dir <dir>`

Write discovered descriptors as `.proto` source files to a directory.

```bash
grpcurl --proto-out-dir ./exported --plaintext localhost:50051 describe my.Service
```

### Performance

#### `--max-msg-sz <bytes>`

Maximum response message size in bytes. Default: 4,194,304 (4 MB). Also
applies to server reflection queries.

```bash
grpcurl --max-msg-sz 16777216 --plaintext localhost:50051 my.Svc/LargeResponse
```

### Verbosity

#### `-v`

Verbose output. Shows:
- Resolved method descriptor
- Request metadata
- Response headers
- Response contents
- Response trailers
- Request/response count summary

#### `--vv`

Very verbose output. Includes everything from `-v` plus estimated response
message sizes in bytes. (Timing data tree present in Go grpcurl is not yet
implemented.)

---

## Go-Style Flag Compatibility

grpcurl accepts Go-style single-dash flags:

```
-plaintext         is equivalent to   --plaintext
-connect-timeout 5 is equivalent to   --connect-timeout 5
-d '{}'            unchanged           (short flag)
```

This is handled by `normalize_args()` which converts known single-dash long
flags to double-dash before clap processes them.

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error (connection failure, descriptor source error, etc.) |
| 2 | Argument validation error |
| 65 | gRPC Cancelled (1 + 64) |
| 66 | gRPC Unknown (2 + 64) |
| 67 | gRPC InvalidArgument (3 + 64) |
| 68 | gRPC DeadlineExceeded (4 + 64) |
| 69 | gRPC NotFound (5 + 64) |
| 70 | gRPC AlreadyExists (6 + 64) |
| 71 | gRPC PermissionDenied (7 + 64) |
| 72 | gRPC ResourceExhausted (8 + 64) |
| 73 | gRPC FailedPrecondition (9 + 64) |
| 74 | gRPC Aborted (10 + 64) |
| 75 | gRPC OutOfRange (11 + 64) |
| 76 | gRPC Unimplemented (12 + 64) |
| 77 | gRPC Internal (13 + 64) |
| 78 | gRPC Unavailable (14 + 64) |
| 79 | gRPC DataLoss (15 + 64) |
| 80 | gRPC Unauthenticated (16 + 64) |

Formula: exit code = gRPC status code + 64.

---

## Common Patterns

### Explore an Unknown Server

```bash
# 1. Discover services
grpcurl --plaintext localhost:50051 list

# 2. List methods of a service
grpcurl --plaintext localhost:50051 list my.package.MyService

# 3. Inspect the service definition
grpcurl --plaintext localhost:50051 describe my.package.MyService

# 4. Get a JSON template for the request message
grpcurl --msg-template --plaintext localhost:50051 describe my.package.MyRequest

# 5. Call a method
grpcurl --plaintext -d '{}' localhost:50051 my.package.MyService/MyMethod
```

### Authenticated Server

```bash
grpcurl -H "Authorization: Bearer $TOKEN" --plaintext localhost:50051 list

grpcurl -H "Authorization: Bearer $TOKEN" --plaintext \
  -d '{"id": 1}' localhost:50051 my.Service/GetItem
```

### Separate Auth for Reflection vs RPC

```bash
grpcurl --reflect-header "Authorization: Bearer $REFLECT_TOKEN" \
        --rpc-header "Authorization: Bearer $RPC_TOKEN" \
        --plaintext -d '{}' localhost:50051 my.Service/GetItem
```

### Offline Proto Exploration

```bash
grpcurl --proto service.proto list
grpcurl --proto service.proto --import-path ./protos describe my.Message
grpcurl --protoset descriptors.pb describe my.package.MyService
```

### Export Descriptors from a Live Server

```bash
# As binary FileDescriptorSet
grpcurl --protoset-out schema.pb --plaintext localhost:50051 describe

# As .proto source files
grpcurl --proto-out-dir ./exported --plaintext localhost:50051 describe
```

### Mutual TLS

```bash
grpcurl --cacert ca.pem --cert client.pem --key client-key.pem \
  myserver:443 list
```

### Streaming RPCs

```bash
# Server streaming (single request, multiple responses)
grpcurl --plaintext -d '{"query": "test"}' localhost:50051 my.Svc/ServerStream

# Client streaming (multiple requests via stdin)
printf '{"value":1}\n{"value":2}\n{"value":3}' | \
  grpcurl --plaintext -d @ localhost:50051 my.Svc/Aggregate

# Bidi streaming
printf '{"msg":"hello"}\n{"msg":"world"}' | \
  grpcurl --plaintext -d @ localhost:50051 my.Svc/Chat
```

### Verbose Debugging

```bash
# See headers, trailers, and metadata
grpcurl -v --plaintext -d '{}' localhost:50051 my.Svc/Method

# Also see message sizes
grpcurl --vv --plaintext -d '{}' localhost:50051 my.Svc/Method
```

### Unix Domain Socket

```bash
grpcurl --plaintext --unix /var/run/grpc.sock list
```

### TLS Key Logging (for Wireshark)

```bash
export SSLKEYLOGFILE=/tmp/tls-keys.log
grpcurl myserver:443 list
# Open /tmp/tls-keys.log in Wireshark to decrypt captured traffic
```

---

## Validation Rules

The CLI enforces 28 validation rules matching the original Go grpcurl. Hard
errors produce exit code 2. Warnings print to stderr but do not prevent
execution.

| # | Rule | Type |
|---|------|------|
| 1 | `--connect-timeout` must not be negative | Error |
| 2 | `--keepalive-time` must not be negative | Error |
| 3 | `--max-time` must not be negative | Error |
| 4 | `--max-msg-sz` must not be negative | Error |
| 5 | `--plaintext` and `--alts` are mutually exclusive | Error |
| 6 | `--insecure` requires TLS mode (no `--plaintext`, no `--alts`) | Error |
| 7 | `--cert` requires TLS mode | Error |
| 8 | `--key` requires TLS mode | Error |
| 9 | `--cert` and `--key` must both be present or both absent | Error |
| 10 | `--alts-handshaker-service` requires `--alts` | Error |
| 11 | `--alts-target-service-account` requires `--alts` | Error |
| 12 | `--format` must be `json` or `text` | Error |
| 13 | `--emit-defaults` with non-json format | Warning |
| 14 | At least one positional argument required | Error |
| 15 | First non-verb positional is the address | Parse |
| 16 | Verb must be `list`, `describe`, or a method name (invoke) | Parse |
| 17 | Invoke requires a method symbol | Error |
| 18 | `-d` with list/describe is unused | Warning |
| 19 | `--rpc-header` with list/describe is unused | Warning |
| 20 | No extra positional arguments allowed | Error |
| 21 | Invoke requires an address | Error |
| 22 | At least one of: address, `--protoset`, or `--proto` | Error |
| 23 | `--reflect-header` with `--protoset` is unused | Warning |
| 24 | `--protoset` and `--proto` are mutually exclusive | Error |
| 25 | `--import-path` without `--proto` is unused | Warning |
| 26 | `--use-reflection=false` requires `--protoset` or `--proto` | Error |
| 27 | Reflection defaults to false when file sources provided | Behavior |
| 28 | `--servername` and `--authority` cannot have different values | Error |
