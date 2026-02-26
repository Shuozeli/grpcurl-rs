mod common;

use std::sync::LazyLock;

use common::server::TestServer;
use common::{assert_exit_code, assert_output_contains, run};

static SERVER: LazyLock<TestServer> = LazyLock::new(TestServer::start);

#[test]
#[ignore]
fn unary_call_with_error_status() {
    let r = run(&[
        "-plaintext",
        "-d",
        r#"{"responseStatus":{"code":2,"message":"custom error"}}"#,
        &SERVER.addr,
        "testing.TestService/UnaryCall",
    ]);
    // Go grpcurl uses exit 66 for non-OK gRPC status
    assert_output_contains(&r, "custom error");
}

#[test]
#[ignore]
fn unary_call_not_found() {
    let r = run(&[
        "-plaintext",
        "-d",
        r#"{"responseStatus":{"code":5,"message":"resource missing"}}"#,
        &SERVER.addr,
        "testing.TestService/UnaryCall",
    ]);
    assert_output_contains(&r, "resource missing");
}

#[test]
#[ignore]
fn unimplemented_service_call() {
    let r = run(&[
        "-plaintext",
        &SERVER.addr,
        "testing.UnimplementedService/UnimplementedCall",
    ]);
    assert_output_contains(&r, "Unimplemented");
}

#[test]
#[ignore]
fn invoke_nonexistent_method() {
    let r = run(&[
        "-plaintext",
        &SERVER.addr,
        "testing.TestService/NonExistent",
    ]);
    assert_exit_code(&r, 1);
}

#[test]
#[ignore]
fn invoke_nonexistent_service() {
    let r = run(&["-plaintext", &SERVER.addr, "no.Such.Service/Method"]);
    assert_exit_code(&r, 1);
}

#[test]
#[ignore]
fn describe_nonexistent_symbol() {
    let r = run(&["-plaintext", &SERVER.addr, "describe", "no.such.Symbol"]);
    assert_exit_code(&r, 1);
}

#[test]
#[ignore]
fn format_error_with_error_status() {
    let r = run(&[
        "-plaintext",
        "-format-error",
        "-d",
        r#"{"responseStatus": {"code": 2, "message": "test error"}}"#,
        &SERVER.addr,
        "testing.TestService/UnaryCall",
    ]);
    assert_output_contains(&r, "test error");
}
