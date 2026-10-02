//! Real command execution.
//!
//! Pipelines are executed by the actual `bash` binary so that all shell
//! semantics (pipes, redirections, globbing, variables...) are authentic. A
//! background thread collects the output while the main thread enforces a
//! timeout so a runaway command (e.g. `tail -f`) cannot hang the UI forever.

use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Maximum captured bytes per stream before truncation.
const MAX_OUT: usize = 1024 * 1024;
/// Wall-clock timeout for a single execution.
const TIMEOUT: Duration = Duration::from_secs(30);

pub struct RunResult {
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
    pub duration: Duration,
    pub truncated: bool,
}

fn decode_truncated(mut bytes: Vec<u8>) -> (String, bool) {
    if bytes.len() <= MAX_OUT {
        return (String::from_utf8_lossy(&bytes).into_owned(), false);
    }
    bytes.truncate(MAX_OUT);
    // Back off to a UTF-8 boundary so we never produce replacement garbage.
    while !bytes.is_empty() {
        if let Ok(s) = String::from_utf8(bytes.clone()) {
            return (s, true);
        }
        bytes.pop();
    }
    (String::new(), true)
}

pub fn run(cmd: &str) -> RunResult {
    let start = Instant::now();

    let child = Command::new("bash")
        .arg("-c")
        .arg(cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let child = match child {
        Ok(c) => c,
        Err(e) => {
            return RunResult {
                stdout: String::new(),
                stderr: format!("failed to spawn bash: {e}"),
                code: None,
                duration: start.elapsed(),
                truncated: false,
            };
        }
    };

    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });

    match rx.recv_timeout(TIMEOUT) {
        Ok(Ok(out)) => {
            let (stdout, t_out) = decode_truncated(out.stdout);
            let (stderr, t_err) = decode_truncated(out.stderr);
            RunResult {
                stdout,
                stderr,
                code: out.status.code(),
                duration: start.elapsed(),
                truncated: t_out || t_err,
            }
        }
        Ok(Err(e)) => RunResult {
            stdout: String::new(),
            stderr: format!("error waiting for command: {e}"),
            code: None,
            duration: start.elapsed(),
            truncated: false,
        },
        Err(_) => {
            // Timed out: kill the whole process group via the shell's pid.
            let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
            RunResult {
                stdout: String::new(),
                stderr: "command timed out after 30s and was killed".to_string(),
                code: None,
                duration: start.elapsed(),
                truncated: false,
            }
        }
    }
}
