mod common;

use std::sync::LazyLock;

use common::server::TestServer;
use common::{assert_exit_code, assert_output_contains, assert_stdout_contains, run};

static SERVER: LazyLock<TestServer> = LazyLock::new(TestServer::start);

#[test]
#[ignore]
fn empty_call() {
    let r = run(&["-plaintext", &SERVER.addr, "testing.TestService/EmptyCall"]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "{}");
}

#[test]
#[ignore]
fn unary_call_with_payload() {
    let r = run(&[
        "-plaintext",
        "-d",
        r#"{"payload":{"body":"dGVzdA=="}}"#,
        &SERVER.addr,
        "testing.TestService/UnaryCall",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "dGVzdA==");
}

#[test]
#[ignore]
fn empty_call_with_emit_defaults() {
    let r = run(&[
        "-plaintext",
        "-emit-defaults",
        &SERVER.addr,
        "testing.TestService/EmptyCall",
    ]);
    assert_exit_code(&r, 0);
}

#[test]
#[ignore]
fn unary_call_with_response_size() {
    let r = run(&[
        "-plaintext",
        "-d",
        r#"{"responseSize": 5, "payload": {"body": "AQID"}}"#,
        &SERVER.addr,
        "testing.TestService/UnaryCall",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "payload");
}

#[test]
#[ignore]
fn invoke_with_custom_header() {
    let r = run(&[
        "-v",
        "-plaintext",
        "-H",
        "x-custom: test-value",
        &SERVER.addr,
        "testing.TestService/EmptyCall",
    ]);
    assert_exit_code(&r, 0);
    assert_output_contains(&r, "x-custom: test-value");
}
