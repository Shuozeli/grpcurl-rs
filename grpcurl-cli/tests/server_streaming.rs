mod common;

use std::sync::LazyLock;

use common::server::TestServer;
use common::{assert_exit_code, assert_output_contains, assert_stdout_contains, run};

static SERVER: LazyLock<TestServer> = LazyLock::new(TestServer::start);

#[test]
#[ignore]
fn streaming_output_call_two_responses() {
    let r = run(&[
        "-plaintext",
        "-d",
        r#"{"responseParameters":[{"size":3},{"size":5}]}"#,
        &SERVER.addr,
        "testing.TestService/StreamingOutputCall",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "payload");
}

#[test]
#[ignore]
fn streaming_output_call_empty() {
    let r = run(&[
        "-plaintext",
        "-d",
        r#"{"responseParameters":[]}"#,
        &SERVER.addr,
        "testing.TestService/StreamingOutputCall",
    ]);
    assert_exit_code(&r, 0);
}

#[test]
#[ignore]
fn streaming_output_call_with_fail_late() {
    let r = run(&[
        "-plaintext",
        "-H",
        "fail-late: 10",
        "-d",
        r#"{"responseParameters":[{"size":3}]}"#,
        &SERVER.addr,
        "testing.TestService/StreamingOutputCall",
    ]);
    assert_output_contains(&r, "Aborted");
}

#[test]
#[ignore]
fn streaming_input_call_two_requests() {
    let r = run(&[
        "-plaintext",
        "-d",
        r#"{"payload":{"body":"AQID"}} {"payload":{"body":"BAUG"}}"#,
        &SERVER.addr,
        "testing.TestService/StreamingInputCall",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "aggregatedPayloadSize");
}

#[test]
#[ignore]
fn full_duplex_call_two_exchanges() {
    let r = run(&[
        "-plaintext",
        "-d",
        r#"{"responseParameters":[{"size":3}]} {"responseParameters":[{"size":2}]}"#,
        &SERVER.addr,
        "testing.TestService/FullDuplexCall",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "payload");
}

#[test]
#[ignore]
fn server_streaming_via_reflection() {
    let r = run(&[
        "-plaintext",
        "-d",
        r#"{"responseParameters": [{"size": 3}, {"size": 5}]}"#,
        &SERVER.addr,
        "testing.TestService/StreamingOutputCall",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "payload");
}

#[test]
#[ignore]
fn bidi_streaming_via_reflection() {
    let r = run(&[
        "-plaintext",
        "-d",
        r#"{"responseParameters": [{"size": 3}]} {"responseParameters": [{"size": 2}]}"#,
        &SERVER.addr,
        "testing.TestService/FullDuplexCall",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "payload");
}
