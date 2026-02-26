mod common;

use common::{assert_exit_code, assert_output_contains, assert_stdout_contains, run, testdata};

// -- Describe tests using test_full.pb -----------------------------------------

#[test]
fn describe_service() {
    let pb = testdata("test_full.pb");
    let r = run(&["-protoset", &pb, "describe", "test.v1.Greeter"]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "service Greeter {");
}

#[test]
fn describe_message() {
    let pb = testdata("test_full.pb");
    let r = run(&["-protoset", &pb, "describe", "test.v1.HelloRequest"]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "message HelloRequest {");
}

#[test]
fn describe_method() {
    let pb = testdata("test_full.pb");
    let r = run(&["-protoset", &pb, "describe", "test.v1.Greeter.SayHello"]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "rpc SayHello");
}

#[test]
fn describe_enum() {
    let pb = testdata("test_full.pb");
    let r = run(&["-protoset", &pb, "describe", "test.v1.Status"]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "enum Status {");
}

#[test]
fn describe_enum_value() {
    let pb = testdata("test_full.pb");
    let r = run(&["-protoset", &pb, "describe", "test.v1.Status.STATUS_ACTIVE"]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "STATUS_ACTIVE = 1");
}

#[test]
fn describe_field() {
    let pb = testdata("test_full.pb");
    let r = run(&["-protoset", &pb, "describe", "test.v1.HelloRequest.name"]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "string name = 1");
}

#[test]
fn describe_all_services() {
    let pb = testdata("test_full.pb");
    let r = run(&["-protoset", &pb, "describe"]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "is a service");
}

#[test]
fn describe_not_found_symbol() {
    let pb = testdata("test_full.pb");
    let r = run(&["-protoset", &pb, "describe", "does.not.Exist"]);
    assert_exit_code(&r, 1);
    assert_output_contains(&r, "not found");
}

// -- Describe tests using test_complex.pb --------------------------------------

#[test]
fn describe_complex_message() {
    let pb = testdata("test_complex.pb");
    let r = run(&["-protoset", &pb, "describe", "test.v1.ComplexMessage"]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "map<string, int32>");
}

#[test]
fn describe_streaming_service() {
    let pb = testdata("test_complex.pb");
    let r = run(&["-protoset", &pb, "describe", "test.v1.ComplexService"]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "stream .test.v1.ComplexMessage");
}

// -- msg-template tests --------------------------------------------------------

#[test]
fn msg_template_simple_message() {
    let pb = testdata("test_full.pb");
    let r = run(&[
        "-protoset",
        &pb,
        "-msg-template",
        "describe",
        "test.v1.HelloRequest",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "Message template:");
}

#[test]
fn msg_template_complex_message() {
    let pb = testdata("test_complex.pb");
    let r = run(&[
        "-protoset",
        &pb,
        "-msg-template",
        "describe",
        "test.v1.ComplexMessage",
    ]);
    assert_exit_code(&r, 0);
    assert_stdout_contains(&r, "Message template:");
}
