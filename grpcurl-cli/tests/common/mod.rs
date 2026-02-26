// Each integration test file (`cli_args.rs`, `server_unary.rs`, etc.) is compiled
// as its own independent binary crate, each pulling in its own copy of this module.
// Helpers used by some test binaries but not others trigger false "dead code" warnings
// in the binaries that don't call them. Allow dead_code to silence these per-binary
// false positives.
#![allow(dead_code)]

pub mod server;

use std::path::PathBuf;
use std::process::{Command, Output};

/// Result of running the grpcurl binary.
pub struct RunResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl RunResult {
    fn from_output(output: Output) -> Self {
        RunResult {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        }
    }

    /// Combined stdout + stderr.
    pub fn combined(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

/// Path to the compiled grpcurl binary (set by Cargo for [[bin]] targets).
pub fn grpcurl_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_grpcurl"))
}

/// Path to the tests/testdata directory.
pub fn testdata_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("testdata")
}

/// Path to a specific testdata file.
pub fn testdata(filename: &str) -> String {
    testdata_dir().join(filename).to_string_lossy().into_owned()
}

/// Run the grpcurl binary with the given arguments.
pub fn run(args: &[&str]) -> RunResult {
    let output = Command::new(grpcurl_bin())
        .args(args)
        .output()
        .expect("failed to execute grpcurl binary");
    RunResult::from_output(output)
}

/// Run the grpcurl binary with stdin data piped in.
pub fn run_with_stdin(args: &[&str], stdin_data: &str) -> RunResult {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new(grpcurl_bin())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn grpcurl binary");

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(stdin_data.as_bytes())
            .expect("failed to write stdin");
    }

    let output = child
        .wait_with_output()
        .expect("failed to wait on grpcurl binary");
    RunResult::from_output(output)
}

// -- Assertion helpers --------------------------------------------------------

/// Assert the exit code matches.
pub fn assert_exit_code(result: &RunResult, expected: i32) {
    assert_eq!(
        result.exit_code, expected,
        "Expected exit code {expected}, got {}.\nstdout: {}\nstderr: {}",
        result.exit_code, result.stdout, result.stderr,
    );
}

/// Assert stdout contains a substring.
pub fn assert_stdout_contains(result: &RunResult, needle: &str) {
    assert!(
        result.stdout.contains(needle),
        "Expected stdout to contain {needle:?}.\nstdout: {}\nstderr: {}",
        result.stdout,
        result.stderr,
    );
}

/// Assert combined output (stdout+stderr) contains a substring (case-insensitive).
pub fn assert_output_contains(result: &RunResult, needle: &str) {
    let combined = result.combined().to_lowercase();
    assert!(
        combined.contains(&needle.to_lowercase()),
        "Expected output to contain {needle:?} (case-insensitive).\nstdout: {}\nstderr: {}",
        result.stdout,
        result.stderr,
    );
}

/// Assert combined output does NOT contain a substring.
pub fn assert_output_not_contains(result: &RunResult, needle: &str) {
    let combined = result.combined();
    assert!(
        !combined.contains(needle),
        "Expected output NOT to contain {needle:?}.\nstdout: {}\nstderr: {}",
        result.stdout,
        result.stderr,
    );
}

/// Assert stdout exactly equals expected string.
pub fn assert_stdout_eq(result: &RunResult, expected: &str) {
    assert_eq!(
        result.stdout, expected,
        "stdout does not match expected.\nstderr: {}",
        result.stderr,
    );
}
