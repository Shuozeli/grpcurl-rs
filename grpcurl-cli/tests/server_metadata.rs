mod common;

use std::sync::LazyLock;

use common::server::TestServer;
use common::{assert_exit_code, assert_output_contains, run};

static SERVER: LazyLock<TestServer> = LazyLock::new(TestServer::start);

#[test]
#[ignore]
fn reply_with_headers_echo() {
    let r = run(&[
        "-v",
        "-plaintext",
        "-H",
        "reply-with-headers: x-custom: hello",
        &SERVER.addr,
        "testing.TestService/EmptyCall",
    ]);
    assert_exit_code(&r, 0);
    assert_output_contains(&r, "x-custom: hello");
}

#[test]
#[ignore]
fn fail_early_permission_denied() {
    let r = run(&[
        "-plaintext",
        "-H",
        "fail-early: 7",
        &SERVER.addr,
        "testing.TestService/EmptyCall",
    ]);
    assert_output_contains(&r, "PermissionDenied");
}

#[test]
#[ignore]
fn fail_late_aborted() {
    let r = run(&[
        "-plaintext",
        "-H",
        "fail-late: 10",
        &SERVER.addr,
        "testing.TestService/EmptyCall",
    ]);
    assert_output_contains(&r, "Aborted");
}

#[test]
#[ignore]
fn custom_headers_verbose() {
    let r = run(&[
        "-v",
        "-plaintext",
        "-H",
        "x-my-header: my-value",
        &SERVER.addr,
        "testing.TestService/EmptyCall",
    ]);
    assert_exit_code(&r, 0);
    assert_output_contains(&r, "x-my-header: my-value");
}

#[test]
#[ignore]
fn multiple_headers() {
    let r = run(&[
        "-v",
        "-plaintext",
        "-H",
        "x-first: value1",
        "-H",
        "x-second: value2",
        &SERVER.addr,
        "testing.TestService/EmptyCall",
    ]);
    assert_exit_code(&r, 0);
    assert_output_contains(&r, "x-first: value1");
    assert_output_contains(&r, "x-second: value2");
}

#[test]
#[ignore]
fn rpc_header_only() {
    let r = run(&[
        "-v",
        "-plaintext",
        "-rpc-header",
        "x-rpc-only: rpc-value",
        &SERVER.addr,
        "testing.TestService/EmptyCall",
    ]);
    assert_exit_code(&r, 0);
    assert_output_contains(&r, "x-rpc-only: rpc-value");
}
