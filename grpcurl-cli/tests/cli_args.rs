mod common;

use common::{assert_exit_code, run};

/// Assert that the exit code is NOT 2 (validation error).
/// These tests verify that args pass validation, even if the operation
/// itself fails (e.g., /dev/null is not a valid protoset/proto file).
fn assert_passes_validation(result: &common::RunResult) {
    assert_ne!(
        result.exit_code, 2,
        "Expected args to pass validation (exit != 2), got exit code 2.\nstderr: {}",
        result.stderr,
    );
}

// -- Valid argument parsing (passes validation) --------------------------------

#[test]
fn list_from_protoset_no_address() {
    let r = run(&["-protoset", "/dev/null", "list"]);
    assert_exit_code(&r, 0);
}

#[test]
fn list_with_service_symbol() {
    // /dev/null is empty so the symbol lookup fails, but validation passes
    let r = run(&["-protoset", "/dev/null", "list", "my.Service"]);
    assert_passes_validation(&r);
}

#[test]
fn describe_from_proto_no_address() {
    // /dev/null is not a valid proto file, but validation passes
    let r = run(&["-proto", "/dev/null", "describe"]);
    assert_passes_validation(&r);
}

#[test]
fn describe_with_symbol() {
    // /dev/null is not a valid proto file, but validation passes
    let r = run(&["-proto", "/dev/null", "describe", "my.Message"]);
    assert_passes_validation(&r);
}

// -- Single-dash vs double-dash compatibility ----------------------------------

#[test]
fn double_dash_plaintext_works() {
    let r = run(&["--protoset", "/dev/null", "--plaintext", "list"]);
    assert_exit_code(&r, 0);
}

#[test]
fn single_dash_plaintext_works() {
    let r = run(&["-protoset", "/dev/null", "-plaintext", "list"]);
    assert_exit_code(&r, 0);
}

// -- Warnings (should not cause failure) ----------------------------------------

#[test]
fn data_with_list_warning_not_error() {
    let r = run(&["-protoset", "/dev/null", "-d", "{}", "list"]);
    assert_exit_code(&r, 0);
}

#[test]
fn import_path_without_proto_warning_not_error() {
    let r = run(&["-protoset", "/dev/null", "-import-path", "/tmp", "list"]);
    assert_exit_code(&r, 0);
}
