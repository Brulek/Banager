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

/// Buffers one chunk, records it in the full transcript, and hands every
/// complete line it completes to `on_line`.
fn emit_chunk(
    chunk: &[u8],
    buf: &mut Vec<u8>,
    all_bytes: &mut Vec<u8>,
    stream: Stream,
    on_line: &Option<LineCallback>,
) {
    buf.extend_from_slice(chunk);
    all_bytes.extend_from_slice(chunk);
    for line in drain_lines(buf) {
        if let Some(cb) = on_line {
            cb(stream, line);
        }
    }
}

/// How long to keep reading the pipes after the process group has been
/// killed. `killpg` reaches the whole group, so both write ends should be
/// closed almost at once and the drain ends on EOF long before this; the
/// bound is only there so that a descendant which somehow escaped the group
/// cannot hold a cancelled operation open forever.
const POST_KILL_DRAIN: std::time::Duration = std::time::Duration::from_millis(250);

/// Delivers a trailing line that never got its newline.
///
/// A tool's last line need not be terminated -- a prompt, a progress line,
/// output cut off when the tool was killed. Those bytes were always in the
/// full transcript, but `on_line` never saw them, and the log drawer is
/// built entirely from `on_line`: the user read one line less than the tool
/// actually said, and the missing one is the last, which on a failure is
/// the one that matters.
fn flush_partial_line(buf: &mut Vec<u8>, stream: Stream, on_line: &Option<LineCallback>) {
    if buf.is_empty() {
        return;
    }
    let line = String::from_utf8_lossy(buf).to_string();
    buf.clear();
    if let Some(cb) = on_line {
        cb(stream, line);
    }
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

        if cancel.is_cancelled() {
            // Cancelled before we ever spawned anything — report it the same
            // way an in-flight cancellation would, but without starting a
            // process at all.
            return Ok(CommandOutput {
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: true,
            });
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
        // Full-transcript byte accumulators. These collect every raw byte
        // read from each stream, independent of `stdout_buf`/`stderr_buf`
        // (which are drained line-by-line for `on_line`). Decoding happens
        // once, after the loop, from the complete byte sequence — never
        // per-`read()`-chunk — so a multi-byte UTF-8 character split across
        // two `read()` calls at an arbitrary byte offset is still decoded
        // correctly instead of turning into two separate replacement
        // characters (U+FFFD) at the split point.
        let mut stdout_all_bytes: Vec<u8> = Vec::new();
        let mut stderr_all_bytes: Vec<u8> = Vec::new();
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
                        Ok(0) | Err(_) => stdout_done = true,
                        Ok(n) => emit_chunk(
                            &stdout_read_buf[..n],
                            &mut stdout_buf,
                            &mut stdout_all_bytes,
                            Stream::Stdout,
                            &on_line,
                        ),
                    }
                }
                res = stderr.read(&mut stderr_read_buf), if !stderr_done => {
                    match res {
                        Ok(0) | Err(_) => stderr_done = true,
                        Ok(n) => emit_chunk(
                            &stderr_read_buf[..n],
                            &mut stderr_buf,
                            &mut stderr_all_bytes,
                            Stream::Stderr,
                            &on_line,
                        ),
                    }
                }
            }
        }

        if timed_out || cancelled {
            // The loop above left on the kill, not on EOF, so whatever the
            // child had already written and the loop had not yet read is
            // still sitting in the pipes -- and the `biased` select makes
            // that the *likely* case, not a rare one: a read that was ready
            // in the same poll as the cancel loses to it. Dropping it meant
            // the transcript stopped short exactly when the user opens the
            // log drawer to find out what happened.
            let _ = tokio::time::timeout(POST_KILL_DRAIN, async {
                while !(stdout_done && stderr_done) {
                    tokio::select! {
                        res = stdout.read(&mut stdout_read_buf), if !stdout_done => {
                            match res {
                                Ok(0) | Err(_) => stdout_done = true,
                                Ok(n) => emit_chunk(
                                    &stdout_read_buf[..n],
                                    &mut stdout_buf,
                                    &mut stdout_all_bytes,
                                    Stream::Stdout,
                                    &on_line,
                                ),
                            }
                        }
                        res = stderr.read(&mut stderr_read_buf), if !stderr_done => {
                            match res {
                                Ok(0) | Err(_) => stderr_done = true,
                                Ok(n) => emit_chunk(
                                    &stderr_read_buf[..n],
                                    &mut stderr_buf,
                                    &mut stderr_all_bytes,
                                    Stream::Stderr,
                                    &on_line,
                                ),
                            }
                        }
                    }
                }
            })
            .await;
        }

        // After the drain, so a trailing unterminated line the child wrote
        // just before it died is delivered too.
        flush_partial_line(&mut stdout_buf, Stream::Stdout, &on_line);
        flush_partial_line(&mut stderr_buf, Stream::Stderr, &on_line);

        let exit_code = if timed_out || cancelled {
            let _ = child.wait().await;
            None
        } else {
            child.wait().await.ok().and_then(|status| status.code())
        };

        // Decode the full transcript once from the accumulated bytes (not
        // per-chunk) so a multi-byte UTF-8 character split across a read
        // boundary decodes correctly instead of corrupting into replacement
        // characters on both sides of the split.
        let stdout_all = String::from_utf8_lossy(&stdout_all_bytes).into_owned();
        let stderr_all = String::from_utf8_lossy(&stderr_all_bytes).into_owned();

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
    async fn test_full_transcript_reassembles_multibyte_utf8_split_across_reads() {
        // U+4E03 ('七') encodes as the 3 UTF-8 bytes 0xE4 0xB8 0x83 (octal
        // \344 \270 \203). Write the first two bytes, then sleep long enough
        // that the runner's `read()` returns just those two bytes as their
        // own chunk, then write the final byte in a second chunk — forcing
        // the character to straddle a `read()` boundary. Decoding each raw
        // chunk independently would turn this into two U+FFFD replacement
        // characters instead of the original character.
        let runner = RealRunner::new();
        let spec = CommandSpec {
            program: sh(),
            args: vec![
                "-c".to_string(),
                "printf '\\344\\270'; sleep 0.3; printf '\\203'".to_string(),
            ],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(5),
        };
        let output = runner
            .run(spec, None, CancellationToken::new())
            .await
            .expect("spawn /bin/sh");

        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout, "\u{4e03}");
        assert!(!output.stdout.contains('\u{fffd}'));
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
    async fn test_cancel_before_spawn_never_starts_process() {
        // A token that is already cancelled *before* `run` is called at all
        // must short-circuit before `spawn()` — proven here by targeting a
        // command that would otherwise leave an observable trace (creating a
        // file) if it ran.
        let marker = std::env::temp_dir().join(format!(
            "canager-cancel-before-spawn-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&marker);

        let runner = RealRunner::new();
        let cancel = CancellationToken::new();
        cancel.cancel();

        let spec = CommandSpec {
            program: sh(),
            args: vec!["-c".to_string(), format!("touch {}", marker.display())],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(5),
        };
        let output = runner
            .run(spec, None, cancel)
            .await
            .expect("run must not error, just report cancelled");

        assert!(output.cancelled);
        assert_eq!(output.exit_code, None);
        assert!(!marker.exists(), "the process must never have been spawned");
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
    #[tokio::test]
    async fn test_delivers_a_trailing_line_that_never_got_its_newline() {
        // A tool's last line need not end in one -- a prompt, a progress
        // line, output cut off when the tool died. It was accumulated into
        // the transcript but never handed to `on_line`, so the log drawer,
        // which is built entirely from those callbacks, ended one line
        // short of what the tool actually said.
        let runner = RealRunner::new();
        let lines: Arc<Mutex<Vec<(Stream, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let lines_cb = lines.clone();
        let on_line: LineCallback = Arc::new(move |stream, line| {
            lines_cb.lock().unwrap().push((stream, line));
        });

        let spec = CommandSpec {
            program: sh(),
            args: vec![
                "-c".to_string(),
                "printf 'done\n'; printf 'Password:'; printf 'oops' 1>&2".to_string(),
            ],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(5),
        };
        let output = runner
            .run(spec, Some(on_line), CancellationToken::new())
            .await
            .expect("spawn /bin/sh");

        assert_eq!(output.exit_code, Some(0));
        let seen = lines.lock().unwrap().clone();
        assert!(
            seen.contains(&(Stream::Stdout, "Password:".to_string())),
            "trailing stdout line missing from {seen:?}"
        );
        assert!(
            seen.contains(&(Stream::Stderr, "oops".to_string())),
            "trailing stderr line missing from {seen:?}"
        );
    }

    #[tokio::test]
    async fn test_delivers_what_was_still_in_the_pipe_when_the_child_was_killed() {
        // Cancelling and timing out both SIGKILL the process group and
        // leave the loop at once, so anything the child had already written
        // but the loop had not yet read died with it -- missing from the
        // exact transcript the user opens the log drawer to read.
        //
        // The race is made deterministic: the callback blocks the runtime
        // while the child writes its second line, and the token is
        // cancelled from a plain OS thread meanwhile. When the callback
        // returns, the `biased` select takes the cancel branch before the
        // ready read, which is precisely the case that used to lose data.
        let runner = RealRunner::new();
        let lines: Arc<Mutex<Vec<(Stream, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let lines_cb = lines.clone();
        let on_line: LineCallback = Arc::new(move |stream, line| {
            let first = line == "first";
            lines_cb.lock().unwrap().push((stream, line));
            if first {
                std::thread::sleep(std::time::Duration::from_millis(600));
            }
        });

        let cancel = CancellationToken::new();
        let canceller = cancel.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(400));
            canceller.cancel();
        });

        let spec = CommandSpec {
            program: sh(),
            args: vec![
                "-c".to_string(),
                "printf 'first\n'; sleep 0.2; printf 'second\n'; sleep 30".to_string(),
            ],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(30),
        };
        let output = runner
            .run(spec, Some(on_line), cancel)
            .await
            .expect("spawn /bin/sh");
        handle.join().unwrap();

        assert!(output.cancelled);
        let seen = lines.lock().unwrap().clone();
        assert!(
            seen.contains(&(Stream::Stdout, "second".to_string())),
            "output written before the kill is missing from {seen:?}"
        );
        assert!(
            output.stdout.contains("second"),
            "output written before the kill is missing from the transcript: {:?}",
            output.stdout
        );
    }
}
