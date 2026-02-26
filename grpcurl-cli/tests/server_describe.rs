mod common;

use std::sync::LazyLock;

use common::server::TestServer;
use common::{assert_exit_code, assert_output_contains, assert_stdout_contains, run};

static SERVER: LazyLock<TestServer> = LazyLock::new(TestServer::start);

#[test]
#[ignore]
fn describe_all_services() {
    let r = run(&["-plaintext", &SERVER.addr, "describe"]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "is a service");
}

#[test]
#[ignore]
fn describe_test_service() {
    let r = run(&[
        "-plaintext",
        &SERVER.addr,
        "describe",
        "testing.TestService",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "rpc EmptyCall");
}

#[test]
#[ignore]
fn describe_simple_request() {
    let r = run(&[
        "-plaintext",
        &SERVER.addr,
        "describe",
        "testing.SimpleRequest",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "response_type");
}

#[test]
#[ignore]
fn describe_payload_type_enum() {
    let r = run(&[
        "-plaintext",
        &SERVER.addr,
        "describe",
        "testing.PayloadType",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "COMPRESSABLE");
}

#[test]
#[ignore]
fn describe_empty_call_method() {
    let r = run(&[
        "-plaintext",
        &SERVER.addr,
        "describe",
        "testing.TestService.EmptyCall",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "rpc EmptyCall");
}

#[test]
#[ignore]
fn describe_complex_message() {
    let r = run(&[
        "-plaintext",
        &SERVER.addr,
        "describe",
        "testing.ComplexMessage",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "map<string, string>");
}

#[test]
#[ignore]
fn describe_well_known_types_message() {
    let r = run(&[
        "-plaintext",
        &SERVER.addr,
        "describe",
        "testing.WellKnownTypesMessage",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "google.protobuf.Timestamp");
}

#[test]
#[ignore]
fn describe_nonexistent_symbol() {
    let r = run(&["-plaintext", &SERVER.addr, "describe", "no.such.Symbol"]);
    assert_exit_code(&r, 1);
}

#[test]
#[ignore]
fn msg_template_simple_request() {
    let r = run(&[
        "-plaintext",
        "-msg-template",
        &SERVER.addr,
        "describe",
        "testing.SimpleRequest",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "Message template:");
}

#[test]
#[ignore]
fn msg_template_complex_message() {
    let r = run(&[
        "-plaintext",
        "-msg-template",
        &SERVER.addr,
        "describe",
        "testing.ComplexMessage",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "Message template:");
}

#[test]
#[ignore]
fn describe_method_via_reflection() {
    let r = run(&[
        "-plaintext",
        &SERVER.addr,
        "describe",
        "testing.TestService.EmptyCall",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "rpc EmptyCall");
}

#[test]
#[ignore]
fn describe_message_via_reflection() {
    let r = run(&[
        "-plaintext",
        &SERVER.addr,
        "describe",
        "testing.SimpleRequest",
    ]);
    assert_exit_code(&r, 0);
    assert_output_contains(&r, "response_type");
}
