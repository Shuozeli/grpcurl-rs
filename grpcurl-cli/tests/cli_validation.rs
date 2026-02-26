mod common;

use common::{assert_exit_code, assert_output_contains, run};

#[test]
fn no_arguments() {
    let r = run(&[]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "Too few arguments");
}

#[test]
fn plaintext_and_insecure() {
    let r = run(&["-plaintext", "-insecure", "localhost:8080", "list"]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "insecure");
}

#[test]
fn cert_without_key() {
    let r = run(&["-cert", "foo.pem", "localhost:8080", "list"]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "cert");
}

#[test]
fn key_without_cert() {
    let r = run(&["-key", "foo.pem", "localhost:8080", "list"]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "cert");
}

#[test]
fn protoset_and_proto_conflict() {
    let r = run(&["-protoset", "a.pb", "-proto", "b.proto", "list"]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "protoset");
}

#[test]
fn negative_connect_timeout() {
    let r = run(&["-connect-timeout", "-1", "localhost:8080", "list"]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "connect-timeout");
}

#[test]
fn negative_keepalive_time() {
    let r = run(&["-keepalive-time", "-1", "localhost:8080", "list"]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "keepalive-time");
}

#[test]
fn negative_max_time() {
    let r = run(&["-max-time", "-1", "localhost:8080", "list"]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "max-time");
}

#[test]
fn negative_max_msg_sz() {
    let r = run(&["-max-msg-sz", "-1", "localhost:8080", "list"]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "max-msg-sz");
}

#[test]
fn plaintext_and_alts() {
    let r = run(&["-plaintext", "-alts", "localhost:8080", "list"]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "plaintext");
}

#[test]
fn cert_with_plaintext() {
    let r = run(&[
        "-plaintext",
        "-cert",
        "foo.pem",
        "-key",
        "foo.key",
        "localhost:8080",
        "list",
    ]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "cert");
}

#[test]
fn alts_handshaker_without_alts() {
    let r = run(&[
        "-alts-handshaker-service",
        "host:7777",
        "localhost:8080",
        "list",
    ]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "alts");
}

#[test]
fn alts_target_without_alts() {
    let r = run(&[
        "-alts-target-service-account",
        "svc@proj.iam.gserviceaccount.com",
        "localhost:8080",
        "list",
    ]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "alts");
}

#[test]
fn invalid_format() {
    let r = run(&["-format", "xml", "localhost:8080", "list"]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "format");
}

#[test]
fn too_many_arguments() {
    let r = run(&["localhost:8080", "list", "foo", "bar"]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "Too many arguments");
}

#[test]
fn invoke_without_address() {
    let r = run(&["my.Service/Method"]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "Too few arguments");
}

#[test]
fn no_address_no_proto_no_protoset() {
    let r = run(&["list"]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "No host:port");
}

#[test]
fn use_reflection_false_without_sources() {
    let r = run(&["-use-reflection=false", "list"]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "No host:port specified, no protoset specified");
}

#[test]
fn servername_and_authority_differ() {
    let r = run(&[
        "-servername",
        "foo.com",
        "-authority",
        "bar.com",
        "localhost:8080",
        "list",
    ]);
    assert_exit_code(&r, 2);
    assert_output_contains(&r, "servername");
}
