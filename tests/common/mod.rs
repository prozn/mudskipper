//! Seam 1 harness: drives the built engine binary over stdin and stdout.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

/// How long to wait for an expected reply before failing the test.
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// A running engine process with line-buffered access to its output.
pub struct Engine {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    /// Every line read so far, for failure messages.
    transcript: Vec<String>,
}

impl Engine {
    /// Starts the engine binary built by Cargo for this test run.
    pub fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mudskipper"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("engine binary should start");
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        let (tx, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin: Some(stdin),
            lines,
            transcript: Vec::new(),
        }
    }

    /// Sends one command, terminated with `\n`.
    pub fn send(&mut self, command: &str) {
        self.send_raw(&format!("{command}\n"));
    }

    /// Sends text exactly as given, so tests control line endings.
    pub fn send_raw(&mut self, text: &str) {
        self.send_bytes(text.as_bytes());
    }

    /// Sends raw bytes, which need not be valid UTF-8.
    pub fn send_bytes(&mut self, bytes: &[u8]) {
        let stdin = self.stdin.as_mut().expect("stdin is still open");
        stdin
            .write_all(bytes)
            .and_then(|()| stdin.flush())
            .expect("engine should accept input");
    }

    /// Reads lines until one satisfies `pred`, returning it and the lines before it.
    ///
    /// Panics with the transcript if no such line arrives in time.
    pub fn read_until(&mut self, what: &str, pred: impl Fn(&str) -> bool) -> (String, Vec<String>) {
        let deadline = Instant::now() + REPLY_TIMEOUT;
        let mut before = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(remaining) {
                Ok(line) => {
                    self.transcript.push(line.clone());
                    if pred(&line) {
                        return (line, before);
                    }
                    before.push(line);
                }
                Err(RecvTimeoutError::Timeout) => {
                    panic!(
                        "timed out waiting for {what}; output so far: {:?}",
                        self.transcript
                    )
                }
                Err(RecvTimeoutError::Disconnected) => {
                    panic!(
                        "engine closed stdout before {what}; output so far: {:?}",
                        self.transcript
                    )
                }
            }
        }
    }

    /// Reads lines until one equals `expected` exactly, returning the lines before it.
    pub fn expect_line(&mut self, expected: &str) -> Vec<String> {
        self.read_until(&format!("{expected:?}"), |line| line == expected)
            .1
    }

    /// Waits for the process to exit on its own.
    ///
    /// Panics if it is still running after the reply timeout.
    pub fn wait_exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + REPLY_TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().expect("engine status is readable") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "engine did not exit; output so far: {:?}",
                self.transcript
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Reports whether the process is still running.
    pub fn is_running(&mut self) -> bool {
        self.child
            .try_wait()
            .expect("engine status is readable")
            .is_none()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // Close stdin first so a well-behaved engine sees EOF, then make sure it is gone.
        drop(self.stdin.take());
        if self.is_running() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
