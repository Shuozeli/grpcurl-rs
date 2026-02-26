// See mod.rs for why this is needed.
#![allow(dead_code)]

use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// A managed test gRPC server instance.
///
/// Spawns the testserver binary on an ephemeral port. The server process
/// is killed when this struct is dropped.
pub struct TestServer {
    process: Child,
    pub port: u16,
    pub addr: String,
}

impl TestServer {
    /// Start a new testserver on an ephemeral port.
    ///
    /// Panics if the server fails to start or the port is not ready within 10s.
    pub fn start() -> Self {
        let port = find_free_port();
        let addr = format!("localhost:{port}");

        // The testserver binary is built as a workspace member.
        // Cargo places it alongside the main binary in the target directory.
        let bin = testserver_bin();

        let process = Command::new(&bin)
            .args(["-p", &port.to_string(), "-q"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("Failed to start testserver at {}: {e}", bin.display()));

        wait_for_port(port, Duration::from_secs(10));

        TestServer {
            process,
            port,
            addr,
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

/// Find the testserver binary path.
fn testserver_bin() -> std::path::PathBuf {
    // The testserver is a workspace member, so Cargo builds it in the same
    // target directory. We derive the path from the grpcurl binary location.
    let grpcurl = std::path::PathBuf::from(env!("CARGO_BIN_EXE_grpcurl"));
    let target_dir = grpcurl.parent().expect("grpcurl binary has no parent dir");
    target_dir.join("testserver")
}

/// Bind to port 0 to get an ephemeral port from the OS.
fn find_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind ephemeral port");
    listener.local_addr().unwrap().port()
}

/// Wait for a TCP port to accept connections, or panic after timeout.
fn wait_for_port(port: u16, timeout: Duration) {
    let start = Instant::now();
    loop {
        if TcpStream::connect_timeout(
            &format!("127.0.0.1:{port}").parse().unwrap(),
            Duration::from_millis(100),
        )
        .is_ok()
        {
            // Successfully connected -- server is accepting connections.
            return;
        }
        if start.elapsed() > timeout {
            panic!("Timed out waiting for testserver on port {port}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
