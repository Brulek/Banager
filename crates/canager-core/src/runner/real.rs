//! `RealRunner` is Unix-only by design for this plan: it puts the spawned
//! child in its own process group (`process_group(0)`) and kills that whole
//! group with `libc::killpg` on timeout/cancel, so that a `brew` invocation's
//! grandchildren (e.g. a `curl` download) die with it. Both APIs are
//! POSIX-only, so this module (and the `canager-core` crate as a whole) is
//! not expected to build or run on non-Unix platforms. Canager v1 targets
//! macOS only (see Global Constraints in the phase 0-1 plan), so this is not
//! a limitation in practice.

use super::{CommandOutput, CommandRunner, CommandSpec, LineCallback, RunnerError};
use crate::events::Stream;
use async_trait::async_trait;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

pub struct RealRunner;

impl RealRunner {
    pub fn new() -> RealRunner {
        RealRunner
    }
}

impl Default for RealRunner {
    fn default() -> Self {
        RealRunner::new()
    }
}

/// Drains complete lines (split on both `\n` and `\r`, so `brew`'s
/// carriage-return progress updates are treated as line boundaries too) from
/// the front of `buf`, leaving any trailing partial line buffered.
fn drain_lines(buf: &mut Vec<u8>) -> Vec<String> {
    let mut lines = Vec::new();
    let mut start = 0;
    for i in 0..buf.len() {
        if buf[i] == b'\n' || buf[i] == b'\r' {
            if i > start {
                lines.push(String::from_utf8_lossy(&buf[start..i]).to_string());
            }
            start = i + 1;
        }
    }
    buf.drain(0..start);
    lines
}

#[async_trait]
impl CommandRunner for RealRunner {
    async fn run(
        &self,
        spec: CommandSpec,
        on_line: Option<LineCallback>,
        cancel: CancellationToken,
    ) -> Result<CommandOutput, RunnerError> {
        if !spec.program.exists() {
            return Err(RunnerError::NotFound(spec.program.clone()));
        }

        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args);
        cmd.envs(spec.env.iter().cloned());
        if let Some(cwd) = &spec.cwd {
            cmd.current_dir(cwd);
        }
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.process_group(0);

        let mut child = cmd.spawn()?;
        let pid = child.id().map(|p| p as libc::pid_t);
        let mut stdout = child.stdout.take().expect("stdout was piped");
        let mut stderr = child.stderr.take().expect("stderr was piped");

        let mut stdout_buf: Vec<u8> = Vec::new();
        let mut stderr_buf: Vec<u8> = Vec::new();
        let mut stdout_all = String::new();
        let mut stderr_all = String::new();
        // Separate read buffers for stdout/stderr: both branches of the
        // `tokio::select!` below hold a `.read(&mut _)` future live at the
        // same time, so a single shared buffer would need two concurrent
        // mutable borrows.
        let mut stdout_read_buf = [0u8; 4096];
        let mut stderr_read_buf = [0u8; 4096];

        let mut stdout_done = false;
        let mut stderr_done = false;
        let mut timed_out = false;
        let mut cancelled = false;

        let sleep = tokio::time::sleep(spec.timeout);
        tokio::pin!(sleep);

        while !(stdout_done && stderr_done) && !timed_out && !cancelled {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    cancelled = true;
                    if let Some(pid) = pid {
                        unsafe { libc::killpg(pid, libc::SIGKILL); }
                    }
                }
                _ = &mut sleep => {
                    timed_out = true;
                    if let Some(pid) = pid {
                        unsafe { libc::killpg(pid, libc::SIGKILL); }
                    }
                }
                res = stdout.read(&mut stdout_read_buf), if !stdout_done => {
                    match res {
                        Ok(0) => stdout_done = true,
                        Ok(n) => {
                            stdout_buf.extend_from_slice(&stdout_read_buf[..n]);
                            stdout_all.push_str(&String::from_utf8_lossy(&stdout_read_buf[..n]));
                            for line in drain_lines(&mut stdout_buf) {
                                if let Some(cb) = &on_line {
                                    cb(Stream::Stdout, line);
                                }
                            }
                        }
                        Err(_) => stdout_done = true,
                    }
                }
                res = stderr.read(&mut stderr_read_buf), if !stderr_done => {
                    match res {
                        Ok(0) => stderr_done = true,
                        Ok(n) => {
                            stderr_buf.extend_from_slice(&stderr_read_buf[..n]);
                            stderr_all.push_str(&String::from_utf8_lossy(&stderr_read_buf[..n]));
                            for line in drain_lines(&mut stderr_buf) {
                                if let Some(cb) = &on_line {
                                    cb(Stream::Stderr, line);
                                }
                            }
                        }
                        Err(_) => stderr_done = true,
                    }
                }
            }
        }

        let exit_code = if timed_out || cancelled {
            let _ = child.wait().await;
            None
        } else {
            child.wait().await.ok().and_then(|status| status.code())
        };

        Ok(CommandOutput {
            exit_code,
            stdout: stdout_all,
            stderr: stderr_all,
            timed_out,
            cancelled,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn sh() -> std::path::PathBuf {
        std::path::PathBuf::from("/bin/sh")
    }

    #[tokio::test]
    async fn test_streams_stdout_lines_and_reports_exit_code() {
        let runner = RealRunner::new();
        let lines: Arc<Mutex<Vec<(Stream, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let lines_cb = lines.clone();
        let on_line: LineCallback = Arc::new(move |stream, line| {
            lines_cb.lock().unwrap().push((stream, line));
        });

        let spec = CommandSpec {
            program: sh(),
            args: vec!["-c".to_string(), "printf 'a\\nb\\n'".to_string()],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(5),
        };
        let output = runner
            .run(spec, Some(on_line), CancellationToken::new())
            .await
            .expect("spawn /bin/sh");

        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout, "a\nb\n");
        assert!(!output.timed_out);
        assert!(!output.cancelled);
        assert_eq!(
            *lines.lock().unwrap(),
            vec![
                (Stream::Stdout, "a".to_string()),
                (Stream::Stdout, "b".to_string())
            ]
        );
    }

    #[tokio::test]
    async fn test_reports_nonzero_exit_code() {
        let runner = RealRunner::new();
        let spec = CommandSpec {
            program: sh(),
            args: vec!["-c".to_string(), "exit 3".to_string()],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(5),
        };
        let output = runner
            .run(spec, None, CancellationToken::new())
            .await
            .expect("spawn /bin/sh");
        assert_eq!(output.exit_code, Some(3));
    }

    #[tokio::test]
    async fn test_timeout_kills_process_group() {
        let runner = RealRunner::new();
        let spec = CommandSpec {
            program: sh(),
            args: vec!["-c".to_string(), "sleep 5".to_string()],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_millis(200),
        };
        let output = runner
            .run(spec, None, CancellationToken::new())
            .await
            .expect("spawn /bin/sh");
        assert!(output.timed_out);
        assert_eq!(output.exit_code, None);
    }

    #[tokio::test]
    async fn test_cancel_kills_process_group() {
        let runner = RealRunner::new();
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            cancel_clone.cancel();
        });

        let spec = CommandSpec {
            program: sh(),
            args: vec!["-c".to_string(), "sleep 5".to_string()],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(5),
        };
        let output = runner.run(spec, None, cancel).await.expect("spawn /bin/sh");
        assert!(output.cancelled);
        assert_eq!(output.exit_code, None);
    }
}
