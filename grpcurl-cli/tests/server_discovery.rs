mod common;

use std::sync::LazyLock;

use common::server::TestServer;
use common::{assert_exit_code, assert_stdout_contains, run};

static SERVER: LazyLock<TestServer> = LazyLock::new(TestServer::start);

#[test]
#[ignore]
fn list_all_services() {
    let r = run(&["-plaintext", &SERVER.addr, "list"]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "testing.TestService");
}

#[test]
#[ignore]
fn list_test_service_methods() {
    let r = run(&["-plaintext", &SERVER.addr, "list", "testing.TestService"]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "testing.TestService.EmptyCall");
}

#[test]
#[ignore]
fn list_complex_service_methods() {
    let r = run(&["-plaintext", &SERVER.addr, "list", "testing.ComplexService"]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "testing.ComplexService.GetComplex");
}

#[test]
#[ignore]
fn list_unimplemented_service_methods() {
    let r = run(&[
        "-plaintext",
        &SERVER.addr,
        "list",
        "testing.UnimplementedService",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "testing.UnimplementedService.UnimplementedCall");
}

#[test]
#[ignore]
fn list_nonexistent_service() {
    let r = run(&["-plaintext", &SERVER.addr, "list", "no.Such.Service"]);
    assert_exit_code(&r, 1);
}
