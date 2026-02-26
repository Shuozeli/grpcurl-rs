mod common;

use common::{assert_exit_code, run, testdata};

#[test]
fn proto_out_dir_with_list() {
    let pb = testdata("test.pb");
    let dir = tempfile::tempdir().unwrap();
    let out_dir = dir.path().join("protos");
    std::fs::create_dir(&out_dir).unwrap();
    let r = run(&[
        "-protoset",
        &pb,
        "-proto-out-dir",
        out_dir.to_str().unwrap(),
        "list",
    ]);
    assert_exit_code(&r, 0);
    let proto_files: Vec<_> = std::fs::read_dir(&out_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "proto"))
        .collect();
    assert!(
        !proto_files.is_empty(),
        "proto-out-dir should create .proto files"
    );
}

#[test]
fn proto_out_dir_with_describe() {
    let pb = testdata("test.pb");
    let dir = tempfile::tempdir().unwrap();
    let out_dir = dir.path().join("protos");
    std::fs::create_dir(&out_dir).unwrap();
    let r = run(&[
        "-protoset",
        &pb,
        "-proto-out-dir",
        out_dir.to_str().unwrap(),
        "describe",
        "test.v1.Greeter",
    ]);
    assert_exit_code(&r, 0);
    let proto_files: Vec<_> = std::fs::read_dir(&out_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "proto"))
        .collect();
    assert!(
        !proto_files.is_empty(),
        "proto-out-dir should create .proto files"
    );
}

// Server-dependent proto-out-dir test
#[test]
#[ignore]
fn proto_out_dir_with_reflection() {
    let server = common::server::TestServer::start();
    let dir = tempfile::tempdir().unwrap();
    let out_dir = dir.path().join("protos");
    std::fs::create_dir(&out_dir).unwrap();
    let r = run(&[
        "-plaintext",
        "-proto-out-dir",
        out_dir.to_str().unwrap(),
        &server.addr,
        "describe",
        "testing.TestService",
    ]);
    assert_exit_code(&r, 0);
    let proto_files: Vec<_> = std::fs::read_dir(&out_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "proto"))
        .collect();
    assert!(
        !proto_files.is_empty(),
        "proto-out-dir should create .proto files"
    );
}
