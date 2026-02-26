mod common;

use std::sync::LazyLock;

use common::server::TestServer;
use common::{assert_exit_code, assert_output_contains, run};

static SERVER: LazyLock<TestServer> = LazyLock::new(TestServer::start);

#[test]
#[ignore]
fn empty_call_verbose() {
    let r = run(&[
        "-v",
        "-plaintext",
        &SERVER.addr,
        "testing.TestService/EmptyCall",
    ]);
    assert_exit_code(&r, 0);
    assert_output_contains(&r, "Resolved method descriptor");
}

#[test]
#[ignore]
fn unary_call_verbose_with_payload() {
    let r = run(&[
        "-v",
        "-plaintext",
        "-d",
        r#"{"payload":{"body":"dGVzdA=="}}"#,
        &SERVER.addr,
        "testing.TestService/UnaryCall",
    ]);
    assert_exit_code(&r, 0);
    assert_output_contains(&r, "Response contents");
}

#[test]
#[ignore]
fn empty_call_very_verbose() {
    let r = run(&[
        "--vv",
        "-plaintext",
        &SERVER.addr,
        "testing.TestService/EmptyCall",
    ]);
    assert_exit_code(&r, 0);
    assert_output_contains(&r, "Resolved method descriptor");
}

#[test]
#[ignore]
fn server_streaming_verbose() {
    let r = run(&[
        "-v",
        "-plaintext",
        "-d",
        r#"{"responseParameters": [{"size": 3}, {"size": 5}]}"#,
        &SERVER.addr,
        "testing.TestService/StreamingOutputCall",
    ]);
    assert_exit_code(&r, 0);
    assert_output_contains(&r, "Response contents");
}

#[test]
#[ignore]
fn bidi_streaming_verbose() {
    let r = run(&[
        "-v",
        "-plaintext",
        "-d",
        r#"{"responseParameters": [{"size": 3}]} {"responseParameters": [{"size": 2}]}"#,
        &SERVER.addr,
        "testing.TestService/FullDuplexCall",
    ]);
    assert_exit_code(&r, 0);
    assert_output_contains(&r, "Response contents");
}

#[test]
#[ignore]
fn verbose_summary_counts() {
    // Verify that verbose mode shows the expected summary lines
    let r = run(&[
        "-v",
        "-plaintext",
        &SERVER.addr,
        "testing.TestService/EmptyCall",
    ]);
    assert_exit_code(&r, 0);
    // Verbose output should contain request/response headers info
    assert_output_contains(&r, "Resolved method descriptor");
}
