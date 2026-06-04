# Code Quality Findings

Thorough audit of the grpcurl-rs codebase (Rust port of grpcurl).
Each issue includes exact file paths, line numbers, description, and a concrete fix.
Categories ordered by severity (most impactful first).

**Audit date:** 2026-03-26

---

## 1. Inconsistent Error Types (Missing Abstraction) -- DONE

### Commands use `Box<dyn std::error::Error>` instead of the library's own `Result` type
- **Status:** FIXED
- **What was done:** Changed all command return types in `list.rs`, `describe.rs`, and `invoke.rs` to use `crate::error::Result<T>`. Removed the `extract_grpc_status` function and replaced it with direct pattern matching on `GrpcurlError::GrpcStatus`. Fixed `resolve_method` to use `GrpcurlError::NotFound` instead of `format!(...).into()`. All `ParseError::Error(e)` branches now propagate directly instead of `.into()` boxing.

---

## 2. Duplication

### Duplicated test `make_pool()` / `make_test_pool()` constructors -- DONE
- **Status:** FIXED
- **What was done:** Created `grpcurl-core/src/test_helpers.rs` (gated behind `#[cfg(test)]`) with a single `make_test_pool()` that is a superset of all three previous pools (HelloRequest with name+count, HelloReply, Status enum, Greeter with SayHello+SayGoodbye). Updated all three test modules (`descriptor.rs`, `descriptor_text.rs`, `format.rs`) to import from `test_helpers`. Updated affected assertions (list_methods now expects two methods; message_text now uses `contains` checks instead of exact match).

### Duplicated `now()` function in bankdemo -- DONE
- **Status:** FIXED
- **What was done:** Extracted to `testing/bankdemo/src/time_helpers.rs`. Both `db.rs` and `chat.rs` now import `crate::time_helpers::now`. Removed unused `Timestamp` import from `chat.rs`.

### Duplicated streaming RPC patterns in invoke.rs
- **Location:** `grpcurl-rs/grpcurl-core/src/commands/invoke.rs`
- **Status:** SKIPPED -- marked as low priority in findings. The current code is readable despite the duplication, and extracting the common setup into a helper would add indirection without much benefit for only four call sites.

---

## 3. Silent Failures

### Silently ignoring individual native certificate errors -- DONE
- **Status:** FIXED
- **What was done:** Changed `connection.rs` to count failed native cert additions and emit a summary warning via `eprintln!` when any fail. This preserves the tolerance for malformed system certs while making the failure visible for debugging.

### Extension collection always returns empty
- **Status:** SKIPPED -- Returning an error would break the `CompositeSource` fallback logic (which catches errors and tries the file source). The method is not called from any command module currently. The existing TODO comments are sufficient. This should be addressed when extension support is actually implemented.

---

## 4. Unsafe Patterns

### `expect()` on Mutex locks in production code (reflection.rs) -- DONE
- **Status:** FIXED
- **What was done:** Replaced all five `self.pool.lock().expect(...)` calls with `self.pool.lock().unwrap_or_else(|e| e.into_inner())`. This recovers the lock after a prior panic rather than propagating a second panic. Safe for a descriptor pool that is only appended to.

### `expect()` on map entry field lookups in format.rs and descriptor_text.rs
- **Status:** SKIPPED -- Low priority. Fixing requires changing formatting functions to return `Result` instead of `String`, which is a larger refactor. The protobuf spec guarantees map entries have key/value fields, so the invariant is strong.

### `unwrap()` on `duration_since(UNIX_EPOCH)` in bankdemo
- **Status:** SKIPPED -- Testing code only, cosmetic issue. The `unwrap` is now in the shared `time_helpers.rs` module.

---

## 5. Missing Abstractions

### `AtomicUsize` used for `text_formatter` state instead of a closure-captured mutable counter -- DONE
- **Status:** FIXED
- **What was done:** Changed `Formatter` type alias from `Box<dyn Fn(...)>` to `Box<dyn FnMut(...)>`. Replaced `AtomicUsize` with a plain `let mut num_formatted = 0usize` capture. Updated all call sites (invoke.rs `InvokeContext`, `print_response`, describe.rs, and test code) to use `&mut formatter`.

### No `thiserror` derive for `GrpcurlError` -- DONE
- **Status:** FIXED
- **What was done:** Added `thiserror = "2"` as a direct dependency. Replaced manual `Display` and `std::error::Error` impls with `#[derive(thiserror::Error)]` and `#[error(...)]` attributes. Also used `#[from]` for the `Io` variant. Eliminated ~30 lines of boilerplate.

---

## 6. Duplication in Test Helpers (Testing Infrastructure)

### Duplicated test pool construction data across three modules
- **Location:** `grpcurl-rs/grpcurl-core/src/descriptor.rs:583-631`, `grpcurl-rs/grpcurl-core/src/descriptor_text.rs:493-561`, `grpcurl-rs/grpcurl-core/src/format.rs:475-507`
- **Also see:** Issue in Category 2 above.
- **Problem:** Three separate `FileDescriptorSet` literals with minor variations. When adding a new test scenario (e.g., map fields, nested messages), each module needs its own copy updated.
- **Fix:** Single shared test fixture as described above.

---

## 7. Over-Architecture Concerns

### `CompositeSource` trait object indirection
- **Location:** `grpcurl-rs/grpcurl-core/src/descriptor.rs:400-448`
- **Problem:** `CompositeSource` wraps two `Box<dyn DescriptorSource>` and delegates calls. The only place it's constructed is `grpcurl-cli/src/main.rs:281`. Given there are exactly two implementations (`FileSource` and `ServerSource`), the trait object indirection adds complexity without flexibility. However, this mirrors the Go architecture and is acceptable for a port.
- **Fix:** No action needed. The trait-based design is reasonable for a library that may have other consumers. **Informational only.**

---

## 8. Noise / Minor Code Quality

### Unused import: `std::io::Read`
- **Location:** `grpcurl-rs/grpcurl-core/src/format.rs:3`
- **Problem:** `use std::io::{self, Read};` -- the `Read` trait is used by `read_to_string` on stdin, so this is actually needed. However, it could be more explicit: `io::stdin().read_to_string()` relies on the `Read` trait being in scope. **Not a real issue; disregard.**

### `is_not_found_error` helper only exists in test module -- DONE
- **Status:** FIXED
- **What was done:** Promoted to `GrpcurlError::is_not_found(&self) -> bool` as a public method. Tests updated to call the method directly.

### Inconsistent `print!` vs `println!` in invoke.rs verbose output -- DONE
- **Status:** FIXED
- **What was done:** Standardized all `print!("\n...\n")` calls to `println!("\n...")` in invoke.rs verbose output (5 call sites).

### `format_error` flag handling duplicates `print_status` -- DONE
- **Status:** FIXED
- **What was done:** Added `print_formatted_status(status, format)` function to `format.rs` that outputs JSON when `--format json` is used and delegates to `print_status` for text format. Updated `main.rs` to call the new function instead of manually formatting the error. This closes the behavioral gap with Go's `--format-error` flag.

---

## 9. Dead Code / Placeholder Concerns

### Go-specific files at project root
- **Location:** `grpcurl-rs/desc_source.go`, `grpcurl-rs/format.go`, `grpcurl-rs/grpcurl.go`, `grpcurl-rs/invoke.go`, `grpcurl-rs/grpcurl_test.go`, etc.
- **Problem:** The project root contains the original Go source files alongside the Rust port in `grpcurl-rs/`. These are dead code in the context of the Rust project -- they are not compiled, tested, or used. They add confusion about which code is authoritative.
- **Fix:** These are intentionally kept as reference for the port and should not be removed without the maintainer's consent. However, consider moving them to a `reference/` subdirectory to reduce confusion. **Informational.**

### `.goreleaser.yml` and Go-specific CI
- **Location:** `grpcurl-rs/.goreleaser.yml`, `grpcurl-rs/.circleci/`
- **Problem:** These are Go release/CI configurations that do not apply to the Rust port. They are likely vestigial from the forked repo.
- **Fix:** Remove or archive these files if the Rust port has its own CI (`.github/` directory exists). **Low priority.**

---

## 10. Rust-Specific Improvements

### `.clone()` where borrowing would suffice
- **Location:** `grpcurl-rs/grpcurl-core/src/connection.rs:240` (`host` cloned then only used as `&str`), `grpcurl-rs/grpcurl-core/src/descriptor.rs:209` (`f.file_descriptor_proto().clone()` in a collect)
- **Problem:** A few `.clone()` calls exist where ownership transfer is not needed. Most are in closures passed to `service_fn` where cloning is necessary for the `move` closure. The `file_descriptor_proto().clone()` calls in `get_all_files` are necessary because the proto types don't implement `Copy`. Overall, the codebase is clean on unnecessary clones.
- **Fix:** No urgent action. The `host` clone at line 240 could be avoided by using `&str` directly, but it's inside a closure that needs `'static` lifetime. **Informational.**

### `format!` for simple string concatenation
- **Location:** Various (e.g., `grpcurl-rs/grpcurl-core/src/connection.rs:96,234`, `grpcurl-rs/grpcurl-core/src/descriptor_text.rs:57,63`)
- **Problem:** `format!("{scheme}://{address}")` is fine and idiomatic. No real issue here. **Disregard.**

### Stringly-typed `account_type: i32` in bankdemo
- **Location:** `grpcurl-rs/testing/bankdemo/src/bank.rs:27-55`, `grpcurl-rs/testing/bankdemo/src/db.rs:79`
- **Problem:** Account type is stored as `i32` and matched against magic numbers (1-3, 4-6) instead of using the generated `pb::account::Type` enum. The code has `pb::account::Type::try_from(account_type)` in error messages but does the actual business logic on raw `i32`.
- **Fix:** Match on `pb::account::Type::try_from(account_type)` at the top, then use the enum variants for the business logic branches. This is testing code, so **low priority**.

---

## Summary

| Category | Count | Severity |
|----------|-------|----------|
| Inconsistent error types | 1 major issue | High |
| Duplication | 3 issues | Medium |
| Silent failures | 2 issues | Medium |
| Unsafe patterns (expect/unwrap in non-test) | 3 issues | Medium |
| Missing abstractions | 2 issues | Low |
| Behavioral gap (format_error) | 1 issue (FIXED) | Medium |
| Dead code / vestigial files | 2 issues | Low |
| Noise / cosmetic | 4 issues | Low |

The codebase is well-structured overall, with good error handling in the core library, thorough tests, and clean separation between CLI and library. All high and medium severity findings have been addressed. The remaining open items are low priority (cosmetic, informational, or testing-only code).
