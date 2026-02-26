mod common;

use common::{assert_exit_code, run, testdata};

#[test]
fn protoset_out_with_list_all_services() {
    let pb = testdata("test.pb");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.pb");
    let r = run(&[
        "-protoset",
        &pb,
        "-protoset-out",
        out.to_str().unwrap(),
        "list",
    ]);
    assert_exit_code(&r, 0);
    assert!(out.exists(), "protoset-out file should be created");
    assert!(
        out.metadata().unwrap().len() > 0,
        "protoset-out file should be non-empty"
    );
}

#[test]
fn protoset_out_with_list_single_service() {
    let pb = testdata("test.pb");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.pb");
    let r = run(&[
        "-protoset",
        &pb,
        "-protoset-out",
        out.to_str().unwrap(),
        "list",
        "test.v1.Greeter",
    ]);
    assert_exit_code(&r, 0);
    assert!(out.exists(), "protoset-out file should be created");
    assert!(
        out.metadata().unwrap().len() > 0,
        "protoset-out file should be non-empty"
    );
}

#[test]
fn protoset_out_with_describe() {
    let pb = testdata("test.pb");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.pb");
    let r = run(&[
        "-protoset",
        &pb,
        "-protoset-out",
        out.to_str().unwrap(),
        "describe",
        "test.v1.HelloRequest",
    ]);
    assert_exit_code(&r, 0);
    assert!(out.exists(), "protoset-out file should be created");
}

// Server-dependent protoset-out test
#[test]
#[ignore]
fn protoset_out_with_reflection() {
    let server = common::server::TestServer::start();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.pb");
    let r = run(&[
        "-plaintext",
        "-protoset-out",
        out.to_str().unwrap(),
        &server.addr,
        "list",
    ]);
    assert_exit_code(&r, 0);
    assert!(out.exists(), "protoset-out file should be created");
    assert!(
        out.metadata().unwrap().len() > 0,
        "protoset-out file should be non-empty"
    );
}
