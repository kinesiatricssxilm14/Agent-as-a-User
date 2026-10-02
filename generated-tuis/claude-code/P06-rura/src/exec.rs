//! Real pipeline execution.
//!
//! Commands are handed to `bash -c` so that the user's pipeline behaves exactly as it would
//! in a shell: real processes, real filters, real exit codes. Nothing here simulates output.
//!
//! Execution runs on a worker thread and reports back over a channel so the UI stays
//! responsive and can show a "running" state for slow pipelines.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Hard cap on captured output so a runaway `yes` cannot exhaust memory.
const MAX_CAPTURE: usize = 8 * 1024 * 1024;
/// Wall clock limit for a single execution.
const TIMEOUT: Duration = Duration::from_secs(30);

/// What kind of run produced a result, for display purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunKind {
    /// The whole pipeline.
    Full,
    /// Only the stages up to the cursor: `(stage_number, total_stages)`.
    Partial(usize, usize),
}

/// Outcome of one execution.
#[derive(Debug, Clone)]
pub struct RunResult {
    pub command: String,
    pub kind: RunKind,
    /// Display lines: control characters expanded so they cannot corrupt the terminal.
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
    /// Exactly what the pipeline wrote to stdout. Saving uses this, so a saved file is
    /// byte-identical to redirecting the same pipeline to that path in a shell.
    pub stdout_raw: Vec<u8>,
    pub exit_code: Option<i32>,
    pub duration: Duration,
    pub truncated: bool,
    /// Set when the pipeline could not be spawned or was killed.
    pub error: Option<String>,
}

impl RunResult {
    pub fn line_count(&self) -> usize {
        self.stdout.len()
    }

    pub fn byte_count(&self) -> usize {
        self.stdout_raw.len()
    }

    pub fn succeeded(&self) -> bool {
        self.error.is_none() && self.exit_code == Some(0)
    }
}

/// Message sent from the worker thread to the UI loop.
pub enum ExecMsg {
    Finished(Box<RunResult>),
}

/// Spawn `command` under `bash -c` on a worker thread.
///
/// `cwd` becomes the working directory so relative paths in the pipeline resolve the way the
/// user expects. `cancel` lets the UI kill a long running pipeline.
pub fn spawn(
    command: String,
    kind: RunKind,
    cwd: PathBuf,
    tx: Sender<ExecMsg>,
    cancel: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let result = run_blocking(&command, kind, &cwd, &cancel);
        // The UI may already be gone; dropping the error is the correct response.
        let _ = tx.send(ExecMsg::Finished(Box::new(result)));
    });
}

/// Execute `command`, capturing stdout and stderr.
fn run_blocking(
    command: &str,
    kind: RunKind,
    cwd: &Path,
    cancel: &AtomicBool,
) -> RunResult {
    let started = Instant::now();
    let shell = shell_path();
    let mut child = match Command::new(&shell)
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return RunResult {
                command: command.to_string(),
                kind,
                stdout: Vec::new(),
                stderr: Vec::new(),
                stdout_raw: Vec::new(),
                exit_code: None,
                duration: started.elapsed(),
                truncated: false,
                error: Some(format!("failed to start {}: {e}", shell.display())),
            };
        }
    };

    // Both pipes must be drained concurrently: a pipeline that fills the stderr buffer while
    // we block on stdout (or vice versa) would deadlock.
    let out_buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let err_buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let out_trunc = Arc::new(AtomicBool::new(false));
    let err_trunc = Arc::new(AtomicBool::new(false));

    let mut readers = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        readers.push(drain(stdout, Arc::clone(&out_buf), Arc::clone(&out_trunc)));
    }
    if let Some(stderr) = child.stderr.take() {
        readers.push(drain(stderr, Arc::clone(&err_buf), Arc::clone(&err_trunc)));
    }

    let mut error = None;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if cancel.load(Ordering::Relaxed) {
                    let _ = child.kill();
                    error = Some("cancelled".to_string());
                    break child.wait().ok();
                }
                if started.elapsed() > TIMEOUT {
                    let _ = child.kill();
                    error = Some(format!("timed out after {}s", TIMEOUT.as_secs()));
                    break child.wait().ok();
                }
                thread::sleep(Duration::from_millis(15));
            }
            Err(e) => {
                error = Some(format!("wait failed: {e}"));
                break None;
            }
        }
    };

    for r in readers {
        let _ = r.join();
    }

    let stdout_bytes = out_buf.lock().map(|b| b.clone()).unwrap_or_default();
    let stderr_bytes = err_buf.lock().map(|b| b.clone()).unwrap_or_default();
    let truncated = out_trunc.load(Ordering::Relaxed) || err_trunc.load(Ordering::Relaxed);

    RunResult {
        command: command.to_string(),
        kind,
        stdout: to_lines(&stdout_bytes),
        stderr: to_lines(&stderr_bytes),
        stdout_raw: stdout_bytes,
        exit_code: status.and_then(|s| s.code()),
        duration: started.elapsed(),
        truncated,
        error,
    }
}

/// Read a child pipe into `buf`, stopping at [`MAX_CAPTURE`].
fn drain<R: Read + Send + 'static>(
    mut src: R,
    buf: Arc<Mutex<Vec<u8>>>,
    truncated: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut chunk = [0u8; 16 * 1024];
        loop {
            match src.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let Ok(mut b) = buf.lock() else { break };
                    if b.len() >= MAX_CAPTURE {
                        truncated.store(true, Ordering::Relaxed);
                        // Keep draining so the child is never blocked on a full pipe, but stop
                        // accumulating.
                        continue;
                    }
                    let room = MAX_CAPTURE - b.len();
                    if n > room {
                        b.extend_from_slice(&chunk[..room]);
                        truncated.store(true, Ordering::Relaxed);
                    } else {
                        b.extend_from_slice(&chunk[..n]);
                    }
                }
            }
        }
    })
}

/// Split captured bytes into display lines.
///
/// Output is lossy-decoded so binary data cannot break rendering, and tabs are expanded
/// because the terminal buffer has no tab handling.
fn to_lines(bytes: &[u8]) -> Vec<String> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_suffix('\n').unwrap_or(&text);
    if text.is_empty() {
        return Vec::new();
    }
    text.split('\n')
        .map(|l| {
            let l = l.strip_suffix('\r').unwrap_or(l);
            sanitize(l)
        })
        .collect()
}

/// Replace control characters that would corrupt the terminal.
fn sanitize(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for c in line.chars() {
        match c {
            '\t' => {
                let pad = 8 - (out.chars().count() % 8);
                for _ in 0..pad {
                    out.push(' ');
                }
            }
            c if c.is_control() => out.push('·'),
            c => out.push(c),
        }
    }
    out
}

/// Prefer `bash` (guaranteed present in the target image) and fall back to `sh`.
fn shell_path() -> PathBuf {
    for cand in ["/bin/bash", "/usr/bin/bash"] {
        if Path::new(cand).exists() {
            return PathBuf::from(cand);
        }
    }
    PathBuf::from("/bin/sh")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn run(cmd: &str) -> RunResult {
        let cancel = Arc::new(AtomicBool::new(false));
        run_blocking(cmd, RunKind::Full, Path::new("."), &cancel)
    }

    #[test]
    fn runs_a_real_pipeline() {
        let r = run("printf 'a\\nb\\nc\\n' | grep -c ''");
        assert_eq!(r.stdout, vec!["3"]);
        assert_eq!(r.exit_code, Some(0));
        assert!(r.succeeded());
    }

    #[test]
    fn captures_stderr_and_exit_code() {
        let r = run("echo oops >&2; exit 3");
        assert_eq!(r.stderr, vec!["oops"]);
        assert_eq!(r.exit_code, Some(3));
        assert!(!r.succeeded());
    }

    #[test]
    fn tail_and_grep_filters_work() {
        let r = run("printf '1\\n2\\n3\\n4\\n5\\n' | tail -n 2");
        assert_eq!(r.stdout, vec!["4", "5"]);
        let r = run("printf 'ERROR x\\nINFO y\\n' | grep ERROR");
        assert_eq!(r.stdout, vec!["ERROR x"]);
    }

    #[test]
    fn spawn_delivers_result_over_channel() {
        let (tx, rx) = mpsc::channel();
        spawn(
            "echo hi".to_string(),
            RunKind::Partial(1, 2),
            PathBuf::from("."),
            tx,
            Arc::new(AtomicBool::new(false)),
        );
        let ExecMsg::Finished(r) = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(r.stdout, vec!["hi"]);
        assert_eq!(r.kind, RunKind::Partial(1, 2));
    }

    #[test]
    fn sanitizes_control_characters() {
        assert_eq!(sanitize("a\tb"), "a       b");
        assert_eq!(sanitize("a\u{7}b"), "a·b");
    }

    #[test]
    fn to_lines_drops_single_trailing_newline() {
        assert_eq!(to_lines(b"a\nb\n"), vec!["a", "b"]);
        assert_eq!(to_lines(b"a\nb"), vec!["a", "b"]);
        assert_eq!(to_lines(b""), Vec::<String>::new());
        assert_eq!(to_lines(b"\n"), Vec::<String>::new());
    }
}
