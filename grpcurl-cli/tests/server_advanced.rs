mod common;

use std::sync::LazyLock;

use common::server::TestServer;
use common::{assert_exit_code, assert_stdout_contains, run, run_with_stdin};

static SERVER: LazyLock<TestServer> = LazyLock::new(TestServer::start);

#[test]
#[ignore]
fn complex_service_get_complex_echo() {
    let r = run(&[
        "-plaintext",
        "-d",
        r#"{"stringField":"hello","int32Field":42,"tags":["a","b"],"labels":{"k":"v"}}"#,
        &SERVER.addr,
        "testing.ComplexService/GetComplex",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "hello");
}

#[test]
#[ignore]
fn complex_service_get_well_known() {
    let r = run(&[
        "-plaintext",
        "-d",
        r#"{"timestampField":"2024-01-01T00:00:00Z"}"#,
        &SERVER.addr,
        "testing.ComplexService/GetWellKnown",
    ]);
    assert_exit_code(&r, 0);
}

#[test]
#[ignore]
fn max_msg_sz_enforcement() {
    let r = run(&[
        "-plaintext",
        "-max-msg-sz",
        "1",
        "-d",
        r#"{"responseSize": 100, "payload": {"body": "AQID"}}"#,
        &SERVER.addr,
        "testing.TestService/UnaryCall",
    ]);
    // Should fail because response exceeds max message size
    assert!(
        r.exit_code != 0,
        "Expected non-zero exit code for max-msg-sz violation"
    );
}

#[test]
#[ignore]
fn stdin_input() {
    let r = run_with_stdin(
        &[
            "-plaintext",
            "-d",
            "@",
            &SERVER.addr,
            "testing.TestService/EmptyCall",
        ],
        "",
    );
    assert_exit_code(&r, 0);
}
