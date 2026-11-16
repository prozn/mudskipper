//! Seam 1 harness: drives the built engine binary over stdin and stdout.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

/// How long to wait for an expected reply before failing the test.
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// A line the engine printed, with the lines it printed before it since the last read.
pub struct Reply {
    /// The line that was waited for.
    pub line: String,
    /// Lines printed earlier that did not match.
    pub before: Vec<String>,
}

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
    pub fn read_line_matching(&mut self, what: &str, mut pred: impl FnMut(&str) -> bool) -> Reply {
        let deadline = Instant::now() + REPLY_TIMEOUT;
        let mut before = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(remaining) {
                Ok(line) => {
                    self.transcript.push(line.clone());
                    if pred(&line) {
                        return Reply { line, before };
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

    /// Collects every line printed during the next `duration`, for checking that something
    /// does not happen.
    pub fn lines_for(&mut self, duration: Duration) -> Vec<String> {
        let deadline = Instant::now() + duration;
        let mut lines = Vec::new();
        while let Ok(line) = self
            .lines
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            self.transcript.push(line.clone());
            lines.push(line);
        }
        lines
    }

    /// Reads lines until one equals `expected` exactly, returning the lines before it.
    pub fn expect_line(&mut self, expected: &str) -> Vec<String> {
        self.read_line_matching(&format!("{expected:?}"), |line| line == expected)
            .before
    }

    /// Reads lines until an `info string` line arrives, returning that line.
    pub fn expect_info_string(&mut self) -> String {
        self.read_line_matching("an info string", |line| line.starts_with("info string"))
            .line
    }

    /// Closes stdin, so the engine sees the end of its input.
    pub fn close_stdin(&mut self) {
        drop(self.stdin.take());
    }

    /// Reads every line until the engine closes stdout, returning them.
    ///
    /// Panics with the transcript if stdout is still open after the reply timeout.
    pub fn read_to_end(&mut self) -> Vec<String> {
        let deadline = Instant::now() + REPLY_TIMEOUT;
        let mut lines = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(remaining) {
                Ok(line) => {
                    self.transcript.push(line.clone());
                    lines.push(line);
                }
                Err(RecvTimeoutError::Disconnected) => return lines,
                Err(RecvTimeoutError::Timeout) => panic!(
                    "engine did not close stdout; output so far: {:?}",
                    self.transcript
                ),
            }
        }
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
        // Best effort only: a panic here would abort a test that is already failing.
        drop(self.stdin.take());
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
