use std::io::Write;

/// Output handler for grpcurl operations.
///
/// Separates primary output (stdout) from diagnostic output (stderr).
/// The core library writes to this instead of directly to stdout/stderr,
/// making it testable and reusable as a library.
pub struct Output {
    out: Box<dyn Write + Send>,
    err: Box<dyn Write + Send>,
}

impl Output {
    /// Create an Output that writes to real stdout and stderr.
    pub fn stdio() -> Self {
        Output {
            out: Box::new(std::io::stdout()),
            err: Box::new(std::io::stderr()),
        }
    }

    /// Create an Output that captures to in-memory buffers (for testing).
    #[cfg(test)]
    pub fn capture() -> (Self, CapturedOutput) {
        let out = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let err = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let output = Output {
            out: Box::new(SharedWriter(out.clone())),
            err: Box::new(SharedWriter(err.clone())),
        };
        let captured = CapturedOutput { out, err };
        (output, captured)
    }

    /// Write a line to primary output (stdout).
    pub fn println(&mut self, msg: &str) {
        let _ = writeln!(self.out, "{msg}");
    }

    /// Write formatted text to primary output (stdout).
    pub fn write_fmt_out(&mut self, args: std::fmt::Arguments<'_>) {
        let _ = self.out.write_fmt(args);
        let _ = writeln!(self.out);
    }

    /// Write a line to diagnostic output (stderr).
    pub fn eprintln(&mut self, msg: &str) {
        let _ = writeln!(self.err, "{msg}");
    }

    /// Write formatted text to diagnostic output (stderr).
    pub fn write_fmt_err(&mut self, args: std::fmt::Arguments<'_>) {
        let _ = self.err.write_fmt(args);
        let _ = writeln!(self.err);
    }
}

/// Captured output buffers for testing.
#[cfg(test)]
pub struct CapturedOutput {
    out: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    err: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
}

#[cfg(test)]
impl CapturedOutput {
    pub fn stdout(&self) -> String {
        let buf = self.out.lock().expect("stdout capture lock");
        String::from_utf8_lossy(&buf).to_string()
    }

    pub fn stderr(&self) -> String {
        let buf = self.err.lock().expect("stderr capture lock");
        String::from_utf8_lossy(&buf).to_string()
    }
}

/// A Write impl backed by a shared Arc<Mutex<Vec<u8>>> for test capture.
#[cfg(test)]
struct SharedWriter(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

#[cfg(test)]
impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("shared writer lock").write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().expect("shared writer lock").flush()
    }
}

/// Write a formatted line to the primary output (stdout).
#[macro_export]
macro_rules! out {
    ($output:expr, $($arg:tt)*) => {
        $output.write_fmt_out(format_args!($($arg)*))
    };
}

/// Write a formatted line to the diagnostic output (stderr).
#[macro_export]
macro_rules! err {
    ($output:expr, $($arg:tt)*) => {
        $output.write_fmt_err(format_args!($($arg)*))
    };
}
