mod common;

use common::{assert_exit_code, assert_stdout_contains, run, testdata};

#[test]
fn list_all_services() {
    let pb = testdata("test.pb");
    let r = run(&["-protoset", &pb, "list"]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "test.v1.Echo");
    assert_stdout_contains(&r, "test.v1.Greeter");
}

#[test]
fn list_methods() {
    let pb = testdata("test.pb");
    let r = run(&["-protoset", &pb, "list", "test.v1.Greeter"]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "SayHello");
}

#[test]
fn list_nonexistent_service() {
    let pb = testdata("test.pb");
    let r = run(&["-protoset", &pb, "list", "no.Such.Service"]);
    assert_exit_code(&r, 1);
}
