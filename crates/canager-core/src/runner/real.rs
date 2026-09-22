//! `RealRunner` is Unix-only by design for this plan: it puts the spawned
//! child in its own process group (`process_group(0)`) and kills that whole
//! group with `libc::killpg` on timeout/cancel, so that a `brew` invocation's
//! grandchildren (e.g. a `curl` download) die with it. Both APIs are
//! POSIX-only, so this module (and the `canager-core` crate as a whole) is
//! not expected to build or run on non-Unix platforms. Canager v1 targets
//! macOS only (see Global Constraints in the phase 0-1 plan), so this is not
//! a limitation in practice.

use super::{CommandOutput, CommandRunner, CommandSpec, LineCallback, OutputUse, RunnerError};
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

/// One stream's transcript, and how far along it the line splitter has got.
///
/// Every byte the child writes has to be kept anyway — `CommandOutput`
/// carries the whole transcript — so the line splitting reads out of that
/// same buffer rather than copying each chunk into a second one as well.
/// The second copy was every byte of output held twice for the life of the
/// operation: on `brew upgrade` of a large formula that is real memory for
/// no reason, since the only thing the line splitter actually needs is a
/// cursor saying where the unterminated tail begins.
struct StreamBuffer {
    /// Every byte read from this stream, in order — minus whatever the
    /// middle-elision in [`StreamBuffer::cap`] has dropped. Decoded once,
    /// after the read loop, from the complete byte sequence — never
    /// per-`read()` chunk — so a multi-byte UTF-8 character split across
    /// two `read()` calls at an arbitrary byte offset is still decoded
    /// correctly instead of turning into two separate replacement
    /// characters (U+FFFD) at the split point.
    bytes: Vec<u8>,
    /// Index in `bytes` where the trailing, not-yet-terminated line
    /// starts. Everything before it has already been handed to `on_line`.
    line_start: usize,
    /// How many bytes at the front of `bytes` are the retained head, once
    /// eliding has begun. Meaningless while `elided == 0`.
    head_len: usize,
    /// How many bytes [`StreamBuffer::cap`] has dropped from the middle.
    elided: usize,
    /// What this stream's bytes are for, and so what may be done to them
    /// when there are too many. See [`CapPolicy`].
    policy: CapPolicy,
    /// Set when a [`CapPolicy::Refuse`] stream went past [`PARSE_CAP`].
    /// The run fails with [`RunnerError::OutputTooLarge`]; nothing this
    /// buffer holds is ever returned afterwards.
    overflowed: bool,
}

/// What a buffer does when it will not fit: shorten, or refuse.
///
/// The choice is per *stream*, not per command, because the two streams of
/// one command are not the same kind of thing. stdout is whatever the
/// caller said it is ([`OutputUse`]). stderr is a message for a person on
/// every path this workspace has -- the five-line failure summary in
/// `run_plan`, the `stderr` an `AdapterError::CommandFailed` carries --
/// and nothing parses it, so it is always the eliding buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CapPolicy {
    /// Drop the middle, keep the head and the tail, say so in the text.
    ElideMiddle,
    /// Hold up to [`PARSE_CAP`] bytes and fail the whole run past that.
    Refuse,
}

impl StreamBuffer {
    fn new(policy: CapPolicy) -> StreamBuffer {
        StreamBuffer {
            bytes: Vec::new(),
            line_start: 0,
            head_len: 0,
            elided: 0,
            policy,
            overflowed: false,
        }
    }
}

/// How much of a *transcript* is kept when a tool will not stop talking:
/// the first `HEAD_CAP` bytes and the last `TAIL_CAP` bytes, with a note
/// in between saying how much went missing.
///
/// The buffer used to be unbounded, which is harmless for every real
/// package-manager run — a `brew upgrade` of a dozen formulas is a few
/// hundred kilobytes — and fatal for the one that goes wrong: a build
/// looping on a warning, or a fetch drawing a progress bar with no
/// terminal to collapse the carriage returns, grows the transcript until
/// the OS kills the app. 1 MiB at each end bounds a runaway tool at ~2 MiB
/// per stream rather than at all of RAM.
///
/// This applies only to bytes a person reads. It used to apply to every
/// stream of every command, under a comment claiming 2 MiB was "an order
/// of magnitude more than any package-manager output observed here, so
/// nothing real is ever elided" — which this repository's own fixture
/// disproves: `adapters/fixtures/brew/7.0.3/info-installed.json` is
/// 419,458 bytes for 93 installed formulae and casks, about 4.5 KiB each,
/// so an ordinary Mac with ~470 of them crosses 2 MiB. `brew info
/// --installed --json=v2` is parsed, not read, so eliding its middle did
/// not shorten a log, it corrupted a JSON document: `serde` then failed,
/// the refresh recorded a `SourceError`, and — the snapshot being in
/// memory only, so the first refresh after any launch has nothing to
/// carry forward — the whole Homebrew group vanished from the Installed
/// page, every launch, for good. Parsed output takes [`PARSE_CAP`]
/// instead.
///
/// Head *and* tail, rather than either alone, because those are the two
/// parts a person reading a log actually needs: the head is the command
/// that ran and what it resolved, the tail is the error it died of. Plain
/// truncation loses the failure; a plain ring buffer loses the context.
const HEAD_CAP: usize = 1024 * 1024;
const TAIL_CAP: usize = 1024 * 1024;

/// The most stdout a [`OutputUse::Parsed`] command may write before the
/// run is failed outright.
///
/// There is no shortening to be had here — half a JSON document is not
/// half an answer — so the only question is where "this cannot be real"
/// starts. 64 MiB is about 14,000 Homebrew entries at the 4.5 KiB each
/// the fixture above measures, roughly thirty times the largest install
/// anyone plausibly has, and the memory stays bounded either way: past
/// this the bytes are dropped and the run ends as
/// [`RunnerError::OutputTooLarge`], which every adapter turns into a
/// visible per-source error rather than a silently wrong answer.
const PARSE_CAP: usize = 64 * 1024 * 1024;

impl StreamBuffer {
    /// Records one chunk and hands `on_line` every line that chunk
    /// completed. Lines are split on both `\n` and `\r`, so `brew`'s
    /// carriage-return progress updates are treated as line boundaries too.
    fn push(&mut self, chunk: &[u8], stream: Stream, on_line: &Option<LineCallback>) {
        // Only the new bytes need scanning: everything before `scan_from`
        // was scanned when it arrived and holds no line terminator.
        let scan_from = self.bytes.len();
        self.bytes.extend_from_slice(chunk);
        for i in scan_from..self.bytes.len() {
            if self.bytes[i] == b'\n' || self.bytes[i] == b'\r' {
                // `i > line_start` skips zero-length segments, so a blank
                // line never reaches `on_line` even though it stays in the
                // transcript. That is the price of treating `\r` as a
                // terminator: `\r\n` arrives as two terminators one byte
                // apart, and without this guard every CRLF line would be
                // followed by a phantom empty one in the log drawer —
                // which is most lines, on any tool that ends them the
                // Windows way. A lost blank line is the cheaper of the
                // two, and it is deliberate, not an off-by-one.
                if i > self.line_start {
                    emit(&self.bytes[self.line_start..i], stream, on_line);
                }
                self.line_start = i + 1;
            }
        }
        self.cap();
    }

    /// Keeps `bytes` bounded, in whichever of the two ways this stream's
    /// [`CapPolicy`] allows. Called after every chunk; a no-op until the
    /// relevant cap is exceeded.
    fn cap(&mut self) {
        match self.policy {
            CapPolicy::ElideMiddle => self.cap_by_eliding(),
            CapPolicy::Refuse => self.cap_by_refusing(),
        }
    }

    /// Past [`PARSE_CAP`], remembers that this stream overflowed and stops
    /// holding its bytes -- this chunk's and every later one's, which is
    /// why the check is on `overflowed` as well as on the length. The
    /// bytes are dropped rather than kept because the run is already
    /// lost: `run` turns `overflowed` into
    /// [`RunnerError::OutputTooLarge`] and returns no `CommandOutput` at
    /// all, so nothing will ever read them, and continuing to accumulate
    /// would defeat the bound this exists to enforce.
    fn cap_by_refusing(&mut self) {
        if !self.overflowed && self.bytes.len() <= PARSE_CAP {
            return;
        }
        self.overflowed = true;
        self.bytes.clear();
        self.line_start = 0;
    }

    /// Drops the middle: `HEAD_CAP` bytes from
    /// the start and `TAIL_CAP` from the end survive, everything between
    /// them goes, and `elided` counts what went so the transcript can say
    /// so.
    fn cap_by_eliding(&mut self) {
        if self.bytes.len() <= HEAD_CAP + TAIL_CAP {
            return;
        }
        if self.elided == 0 {
            self.head_len = HEAD_CAP;
        }
        // `head_len` is `HEAD_CAP` here, and `cut_end` exceeds it because
        // the length exceeds `HEAD_CAP + TAIL_CAP`, so the range below can
        // never be reversed.
        let cut_end = self.bytes.len() - TAIL_CAP;
        let dropped = cut_end - self.head_len;
        self.bytes.drain(self.head_len..cut_end);
        self.elided += dropped;
        // Keep the cursor on the same unterminated tail. A line longer
        // than the whole cap — which is what a progress display that
        // never sends a terminator looks like — is the case that matters:
        // if its start was in the bytes just dropped, it now starts where
        // the retained tail does, and if it began before the head cut it
        // is emitted with its own middle missing, exactly as the
        // transcript is. Either way the line is delivered shorter rather
        // than held whole in memory to be printed in full, which is the
        // allocation this cap exists to prevent.
        if self.line_start >= cut_end {
            self.line_start -= dropped;
        } else if self.line_start > self.head_len {
            self.line_start = self.head_len;
        }
    }

    /// Delivers a trailing line that never got its newline.
    ///
    /// A tool's last line need not be terminated -- a prompt, a progress
    /// line, output cut off when the tool was killed. Those bytes were
    /// always in the full transcript, but `on_line` never saw them, and the
    /// log drawer is built entirely from `on_line`: the user read one line
    /// less than the tool actually said, and the missing one is the last,
    /// which on a failure is the one that matters.
    fn flush_partial_line(&mut self, stream: Stream, on_line: &Option<LineCallback>) {
        if self.line_start < self.bytes.len() {
            emit(&self.bytes[self.line_start..], stream, on_line);
            self.line_start = self.bytes.len();
        }
    }

    /// Notes in the transcript that this stream ended because reading it
    /// failed, not because the child stopped writing.
    ///
    /// `read()` returning an error and `read()` returning 0 both end the
    /// stream here, and the run still reports the child's exit code — so a
    /// mid-stream `EIO` used to cut the transcript short with nothing
    /// anywhere saying it had been cut, and the user read a short log of a
    /// run reported as successful. Nothing can be recovered (the pipe is
    /// gone), but the hole can at least be visible.
    fn note_read_error(
        &mut self,
        e: &std::io::Error,
        stream: Stream,
        on_line: &Option<LineCallback>,
    ) {
        let name = match stream {
            Stream::Stdout => "stdout",
            Stream::Stderr => "stderr",
        };
        let note = format!("\n[canager: reading {name} failed ({e}); output ends here]\n");
        self.push(note.as_bytes(), stream, on_line);
    }

    /// The whole stream, decoded in one go from the accumulated bytes.
    ///
    /// Takes `self`: the buffer is never used again, and handing the
    /// `Vec<u8>` to `String::from_utf8` lets the common case — output that
    /// is valid UTF-8, i.e. all of it that is not a tool writing raw bytes
    /// — reuse the same allocation instead of copying the whole transcript
    /// a second time at the moment the operation returns.
    fn into_transcript(self) -> String {
        if self.elided == 0 {
            return match String::from_utf8(self.bytes) {
                Ok(text) => text,
                Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
            };
        }
        let mut text = String::from_utf8_lossy(&self.bytes[..self.head_len]).into_owned();
        text.push_str(&format!(
            "\n[canager: {} bytes of output elided here; this transcript keeps the \
             first {} and last {} bytes]\n",
            self.elided, HEAD_CAP, TAIL_CAP
        ));
        text.push_str(&String::from_utf8_lossy(&self.bytes[self.head_len..]));
        text
    }
}

fn emit(raw: &[u8], stream: Stream, on_line: &Option<LineCallback>) {
    if let Some(cb) = on_line {
        cb(stream, String::from_utf8_lossy(raw).to_string());
    }
}

/// How long to keep reading the pipes after the process group has been
/// killed. `killpg` reaches the whole group, so both write ends should be
/// closed almost at once and the drain ends on EOF long before this; the
/// bound is only there so that a descendant which somehow escaped the group
/// cannot hold a cancelled operation open forever.
const POST_KILL_DRAIN: std::time::Duration = std::time::Duration::from_millis(250);

/// How long to keep reading the pipes after the child has exited on its
/// own.
///
/// Exit and EOF are not the same event in either direction: a tool is
/// entitled to leave something running that inherited its stdout and
/// stderr, and those write ends stay open after the process we waited on
/// is gone. Waiting for EOF then means waiting for the helper — which is
/// how a `brew install` that finished in forty seconds used to hold the
/// app for the full half-hour timeout and then SIGKILL the helper on the
/// way out. So: read what is still buffered, which for a child that took
/// its pipes with it is everything and reaches EOF in microseconds, and
/// give up after this.
const POST_EXIT_DRAIN: std::time::Duration = std::time::Duration::from_millis(250);

/// How long to wait for a SIGKILLed child to be reaped before abandoning it.
///
/// SIGKILL is not instantaneous: a process blocked in an uninterruptible
/// kernel wait (a read from a stalled disk or a hung network mount) stays
/// alive until that wait returns, and an unbounded `wait()` would hang the
/// whole operation — with no upper bound at all — on the path the user
/// reached by pressing Cancel. Abandoning the child leaks no zombie: Tokio
/// reaps a dropped `Child` in the background.
const POST_KILL_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

/// The longest `spec.timeout` this runner will honour literally.
///
/// `Instant::now() + spec.timeout` panics on overflow, and `timeout_secs`
/// is a public `u64` that no client input reaches today but that someone
/// will eventually set to `u64::MAX` as the obvious way to write "no
/// timeout" — which would panic inside the operation task, taking the
/// whole operation down at spawn with no output and no error the UI can
/// explain. Clamping instead: a day is far longer than any package
/// operation can plausibly run (the longest in the tree is an hour, for
/// `ollama pull`) and cannot overflow.
const MAX_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// SIGKILLs the whole process group, so a `brew` invocation's grandchildren
/// (a `curl` download, a `git` clone) die with it rather than outliving the
/// operation that started them.
fn kill_group(pid: Option<libc::pid_t>) {
    if let Some(pid) = pid {
        // SAFETY: `killpg` takes two integers and no pointers; the worst a
        // stale pid can do here is return ESRCH, which is ignored.
        unsafe {
            libc::killpg(pid, libc::SIGKILL);
        }
    }
}

/// Waits for an already-killed child, bounded by `POST_KILL_WAIT`.
async fn reap(child: &mut tokio::process::Child) {
    let _ = tokio::time::timeout(POST_KILL_WAIT, child.wait()).await;
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

        // stdout follows what the caller said it would do with the bytes;
        // stderr is a message for a person on every path there is, so it
        // always elides. See `CapPolicy`.
        let mut out = StreamBuffer::new(match spec.output_use {
            OutputUse::Transcript => CapPolicy::ElideMiddle,
            OutputUse::Parsed => CapPolicy::Refuse,
        });
        let mut err = StreamBuffer::new(CapPolicy::ElideMiddle);
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
        let mut child_done = false;
        let mut child_code: Option<i32> = None;

        // One deadline for the whole operation, held by the read loop —
        // `spec.timeout` must bound the run from spawn to exit, not just
        // the part of it that produces output. See `MAX_TIMEOUT` for why
        // the duration is clamped rather than added as given.
        let deadline = tokio::time::Instant::now() + spec.timeout.min(MAX_TIMEOUT);
        let sleep = tokio::time::sleep_until(deadline);
        tokio::pin!(sleep);

        // The loop ends on whichever of *child exit*, *deadline* and
        // *cancel* comes first. Both pipes reaching EOF deliberately does
        // not end it: EOF is not exit. A child that closes (or hands off)
        // stdout and stderr and keeps running just disables the two read
        // arms and leaves the other three live — which is the whole point,
        // because it means `cancel` is watched for the entire life of the
        // process rather than only while it is talking. Watching it only
        // until EOF is how Cancel came to be a no-op for up to half an
        // hour on exactly that shape of child, ending in a cancelled run
        // reported as `Succeeded`.
        //
        // `Child::wait` is cancel-safe, so re-creating its future on every
        // iteration loses nothing, and it is fused: once it has reaped the
        // child it returns the same status again rather than waiting on a
        // pid that is no longer there. It takes `&mut child`, which the
        // already-`take`n stdout/stderr handles leave free.
        while !child_done && !timed_out && !cancelled {
            tokio::select! {
                // `biased` keeps `cancel` and the deadline ahead of the
                // reads, so neither can be starved by a child flooding its
                // pipes: they are polled at least once per 4096-byte chunk.
                // The cost is that stdout is structurally preferred over
                // stderr, so the interleaving *between* the two streams is
                // approximate (within one stream it is exact). That is
                // self-limiting rather than a deadlock: a child that keeps
                // stdout ready forever eventually fills the 64 KiB stderr
                // pipe and blocks writing to it, stdout goes quiet, and
                // stderr is read. Measured under a 172 KiB stdout flood,
                // both the first and the last stderr line still arrived.
                biased;
                _ = cancel.cancelled() => {
                    cancelled = true;
                    kill_group(pid);
                }
                _ = &mut sleep => {
                    timed_out = true;
                    kill_group(pid);
                }
                res = child.wait() => {
                    child_done = true;
                    child_code = res.ok().and_then(|status| status.code());
                }
                res = stdout.read(&mut stdout_read_buf), if !stdout_done => {
                    match res {
                        Ok(0) => stdout_done = true,
                        Ok(n) => out.push(&stdout_read_buf[..n], Stream::Stdout, &on_line),
                        Err(e) => {
                            stdout_done = true;
                            out.note_read_error(&e, Stream::Stdout, &on_line);
                        }
                    }
                }
                res = stderr.read(&mut stderr_read_buf), if !stderr_done => {
                    match res {
                        Ok(0) => stderr_done = true,
                        Ok(n) => err.push(&stderr_read_buf[..n], Stream::Stderr, &on_line),
                        Err(e) => {
                            stderr_done = true;
                            err.note_read_error(&e, Stream::Stderr, &on_line);
                        }
                    }
                }
            }
        }

        // The loop left on the child or on a kill, not on EOF, so whatever
        // the child had already written and the loop had not yet read is
        // still sitting in the pipes.
        //
        // On the kill paths the `biased` select makes that the *likely*
        // case, not a rare one: a read that was ready in the same poll as
        // the cancel loses to it. Dropping it meant the transcript stopped
        // short exactly when the user opens the log drawer to find out what
        // happened.
        //
        // On the exit path it is the normal case, now that the child
        // exiting ends the loop: a short command's entire output can still
        // be in the pipe when `wait()` returns. Both drains read to EOF,
        // which for a child that took its pipes with it is immediate; the
        // bounds only matter when something outside the child is still
        // holding a write end.
        let drain_budget = if timed_out || cancelled {
            Some(POST_KILL_DRAIN)
        } else if !(stdout_done && stderr_done) {
            Some(POST_EXIT_DRAIN)
        } else {
            None
        };
        if let Some(budget) = drain_budget {
            let _ = tokio::time::timeout(budget, async {
                while !(stdout_done && stderr_done) {
                    tokio::select! {
                        res = stdout.read(&mut stdout_read_buf), if !stdout_done => {
                            match res {
                                Ok(0) => stdout_done = true,
                                Ok(n) => out.push(&stdout_read_buf[..n], Stream::Stdout, &on_line),
                                Err(e) => {
                                    stdout_done = true;
                                    out.note_read_error(&e, Stream::Stdout, &on_line);
                                }
                            }
                        }
                        res = stderr.read(&mut stderr_read_buf), if !stderr_done => {
                            match res {
                                Ok(0) => stderr_done = true,
                                Ok(n) => err.push(&stderr_read_buf[..n], Stream::Stderr, &on_line),
                                Err(e) => {
                                    stderr_done = true;
                                    err.note_read_error(&e, Stream::Stderr, &on_line);
                                }
                            }
                        }
                    }
                }
            })
            .await;
        }

        // After the drain, so a trailing unterminated line the child wrote
        // just before it died is delivered too.
        out.flush_partial_line(Stream::Stdout, &on_line);
        err.flush_partial_line(Stream::Stderr, &on_line);

        let exit_code = if timed_out || cancelled {
            // The child was SIGKILLed by this run, so there is no exit code
            // worth reporting — only the flag saying which of the two
            // happened. `reap` is the only `wait()` on these paths, so the
            // pid `kill_group` used was still a zombie when it used it.
            reap(&mut child).await;
            None
        } else {
            // The loop cannot end any other way, so the child exited and
            // `child_code` is its status: `None` here means it was killed
            // by a signal, not that nothing was waited for.
            child_code
        };

        // Before any `CommandOutput` is built, so there is no path on
        // which a caller that asked for `Parsed` output receives bytes
        // this runner shortened. A failure the adapter turns into a
        // visible per-source error is the worst this can now do; a parser
        // reading a spliced document was the worse thing it used to do.
        if out.overflowed {
            return Err(RunnerError::OutputTooLarge { limit: PARSE_CAP });
        }

        Ok(CommandOutput {
            exit_code,
            stdout: out.into_transcript(),
            stderr: err.into_transcript(),
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
            output_use: OutputUse::Transcript,
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
            output_use: OutputUse::Transcript,
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
            output_use: OutputUse::Transcript,
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
            output_use: OutputUse::Transcript,
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
            output_use: OutputUse::Transcript,
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
            output_use: OutputUse::Transcript,
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
            output_use: OutputUse::Transcript,
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
            output_use: OutputUse::Transcript,
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

    #[tokio::test]
    async fn test_timeout_still_applies_after_the_pipes_close() {
        // EOF on both pipes is not the same event as the child exiting. A
        // child that closes its stdout and stderr and keeps running ends
        // the read loop -- and with it the only thing that was enforcing
        // `spec.timeout` -- while the process is still alive, so the final
        // `wait()` had nothing bounding it and the operation hung for as
        // long as the child felt like living. `sh` closes both pipes here
        // with `exec`, then sleeps far longer than the timeout.
        let runner = RealRunner::new();
        let spec = CommandSpec {
            program: sh(),
            args: vec!["-c".to_string(), "exec 1>&- 2>&-; sleep 30".to_string()],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_millis(300),
            output_use: OutputUse::Transcript,
        };
        let started = std::time::Instant::now();
        let output = runner
            .run(spec, None, CancellationToken::new())
            .await
            .expect("spawn /bin/sh");
        let elapsed = started.elapsed();

        assert!(output.timed_out, "the run must be reported as timed out");
        assert_eq!(output.exit_code, None);
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "the timeout must bound the whole run, not just the reads; took {elapsed:?}"
        );
    }

    /// A unique path under the temp dir, so tests that create files on
    /// disk cannot collide with each other or with a previous run.
    fn unique_temp_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "canager-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[tokio::test]
    async fn test_a_program_that_is_not_there_reports_not_found() {
        // Not exotic: this is what the user hits when Homebrew (or pipx,
        // or uv) is uninstalled from a terminal while Canager is open and
        // still holding the path it detected at startup. The operation
        // must come back as a clean `NotFound` carrying the path, not as
        // some spawn errno the UI has to guess at.
        let missing = unique_temp_path("no-such-program");
        assert!(!missing.exists());

        let runner = RealRunner::new();
        let spec = CommandSpec {
            program: missing.clone(),
            args: vec!["--version".to_string()],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(5),
            output_use: OutputUse::Transcript,
        };
        let err = runner
            .run(spec, None, CancellationToken::new())
            .await
            .expect_err("a program that is not on disk cannot be run");

        match err {
            RunnerError::NotFound(path) => assert_eq!(path, missing),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_a_program_that_cannot_be_executed_reports_the_spawn_failure() {
        // The other half of the same story: the file is still on disk, so
        // the `exists()` check passes, but `exec` refuses it -- a package
        // manager mid-reinstall, a shim left non-executable, a binary on a
        // volume mounted `noexec`. That has to surface as `Spawn` carrying
        // the OS error, not as a panic or a silent success.
        let not_executable = unique_temp_path("not-executable");
        std::fs::write(&not_executable, b"#!/bin/sh\necho hi\n").expect("write the fixture");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&not_executable, std::fs::Permissions::from_mode(0o644))
            .expect("drop the execute bit");

        let runner = RealRunner::new();
        let spec = CommandSpec {
            program: not_executable.clone(),
            args: vec![],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(5),
            output_use: OutputUse::Transcript,
        };
        let err = runner
            .run(spec, None, CancellationToken::new())
            .await
            .expect_err("a file without the execute bit cannot be spawned");
        let _ = std::fs::remove_file(&not_executable);

        match err {
            RunnerError::Spawn(io) => assert_eq!(
                io.kind(),
                std::io::ErrorKind::PermissionDenied,
                "expected the refusal from exec, got {io:?}"
            ),
            other => panic!("expected Spawn, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_cancel_reaches_a_child_that_has_closed_its_pipes() {
        // The mirror of `test_timeout_still_applies_after_the_pipes_close`,
        // and the hole that test's fix left open. `sh` closes stdout and
        // stderr with `exec` and keeps running -- the exact shape a tool
        // that hands its pipes to a helper has. EOF on both pipes used to
        // end the read loop, and the bounded `wait()` that followed
        // watched the clock and never the token: Cancel did nothing for
        // up to `spec.timeout` (1800 s for every brew install and
        // upgrade), `killpg` was never sent so the process group Cancel
        // exists to destroy ran to completion, and a child that finished
        // first came back `exit_code: Some(0), cancelled: false` --
        // mapped all the way through to `Succeeded`. A cancelled run
        // reporting success.
        //
        // The marker file is the second half: it proves the group was
        // actually killed, not merely that `run` returned early.
        let marker = unique_temp_path("cancel-after-pipes-closed");
        let _ = std::fs::remove_file(&marker);

        let runner = RealRunner::new();
        let cancel = CancellationToken::new();
        let canceller = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            canceller.cancel();
        });

        let spec = CommandSpec {
            program: sh(),
            args: vec![
                "-c".to_string(),
                format!("exec 1>&- 2>&-; sleep 1; touch {}", marker.display()),
            ],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(30),
            output_use: OutputUse::Transcript,
        };
        let started = std::time::Instant::now();
        let output = runner.run(spec, None, cancel).await.expect("spawn /bin/sh");
        let elapsed = started.elapsed();

        assert!(
            output.cancelled,
            "a cancelled run must say so, not report the child's own exit"
        );
        assert!(!output.timed_out, "the user cancelled; nothing timed out");
        assert_eq!(output.exit_code, None);
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "cancel must be answered while the child runs, not at the timeout; took {elapsed:?}"
        );

        // Long enough for the `touch` to have happened had the group
        // survived the cancel.
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        assert!(
            !marker.exists(),
            "cancel must kill the process group, not just stop watching it"
        );
    }

    #[tokio::test]
    async fn test_a_grandchild_holding_the_pipes_does_not_turn_success_into_a_timeout() {
        // `( ... ) &` leaves a grandchild that inherited stdout and
        // stderr, so both write ends stay open after the child exits 0.
        // The read loop used to end only on EOF, so it sat there for the
        // whole `spec.timeout` -- thirty minutes, for a `brew install`
        // that had finished in forty seconds -- then SIGKILLed the group,
        // including the helper the tool deliberately left running, and
        // reported `timed_out: true, exit_code: None`: `Unconfirmed`, for
        // a command that had succeeded.
        let marker = unique_temp_path("grandchild-survives");
        let _ = std::fs::remove_file(&marker);

        let runner = RealRunner::new();
        let spec = CommandSpec {
            program: sh(),
            args: vec![
                "-c".to_string(),
                format!(
                    "(sleep 0.5; touch {}) & printf 'work done\\n'; exit 0",
                    marker.display()
                ),
            ],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(20),
            output_use: OutputUse::Transcript,
        };
        let started = std::time::Instant::now();
        let output = runner
            .run(spec, None, CancellationToken::new())
            .await
            .expect("spawn /bin/sh");
        let elapsed = started.elapsed();

        assert_eq!(
            output.exit_code,
            Some(0),
            "the child exited 0; a pipe someone else is holding is not a timeout"
        );
        assert!(!output.timed_out);
        assert!(!output.cancelled);
        assert_eq!(output.stdout, "work done\n");
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "the run must end when the child does, not at the timeout; took {elapsed:?}"
        );

        tokio::time::sleep(std::time::Duration::from_millis(900)).await;
        assert!(
            marker.exists(),
            "a helper the tool left running must outlive the operation, not be SIGKILLed by it"
        );
        let _ = std::fs::remove_file(&marker);
    }

    #[tokio::test]
    async fn test_a_child_that_exits_at_once_still_delivers_all_of_its_output() {
        // The `child.wait()` arm that closes the two holes above can win
        // the race against a ready read: a short command's whole output
        // can still be in the pipe when `wait()` returns. Reporting that
        // exit code with a truncated transcript would be a worse bug than
        // either -- success with the evidence missing -- so the child
        // exiting is followed by a drain of what is left in the pipes.
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
                "i=0; while [ $i -lt 500 ]; do printf 'line-%s\\n' $i; i=$((i+1)); done; \
                 printf 'to stderr\\n' 1>&2; exit 0"
                    .to_string(),
            ],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(20),
            output_use: OutputUse::Transcript,
        };
        let output = runner
            .run(spec, Some(on_line), CancellationToken::new())
            .await
            .expect("spawn /bin/sh");

        assert_eq!(output.exit_code, Some(0));
        assert!(output.stdout.starts_with("line-0\n"));
        assert!(
            output.stdout.ends_with("line-499\n"),
            "the transcript stops short: {:?}",
            &output.stdout[output.stdout.len().saturating_sub(40)..]
        );
        assert_eq!(output.stdout.lines().count(), 500);
        assert_eq!(output.stderr, "to stderr\n");
        let seen = lines.lock().unwrap().clone();
        assert_eq!(seen.len(), 501, "every line must reach the log drawer too");
        assert_eq!(seen[499], (Stream::Stdout, "line-499".to_string()));
    }

    #[tokio::test]
    async fn test_an_enormous_timeout_is_clamped_instead_of_panicking() {
        // `timeout_secs` is a public `u64`, and `u64::MAX` is the obvious
        // way for a caller to write "no timeout". `Instant::now() +
        // Duration::from_secs(u64::MAX)` panics with "overflow when
        // adding duration to instant", and in the app that panic happens
        // inside the operation task: the operation dies at spawn, with no
        // output and nothing for the UI to explain.
        let runner = RealRunner::new();
        let spec = CommandSpec {
            program: sh(),
            args: vec!["-c".to_string(), "printf 'ok\\n'".to_string()],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(u64::MAX),
            output_use: OutputUse::Transcript,
        };
        let output = runner
            .run(spec, None, CancellationToken::new())
            .await
            .expect("an absurd timeout must not take the operation down");

        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout, "ok\n");
        assert!(!output.timed_out);
    }

    #[test]
    fn test_blank_lines_stay_in_the_transcript_and_out_of_the_callbacks() {
        // Deliberate, and the price of treating `\r` as a terminator so
        // that `\r\n` yields one line rather than two. Locked in here so
        // that a future reader who notices the mismatch between the log
        // drawer and the saved transcript finds out it was a decision.
        let lines: Arc<Mutex<Vec<(Stream, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let lines_cb = lines.clone();
        let on_line: Option<LineCallback> = Some(Arc::new(move |stream, line| {
            lines_cb.lock().unwrap().push((stream, line));
        }));

        let mut buf = StreamBuffer::new(CapPolicy::ElideMiddle);
        buf.push(b"a\n\nb\r\nc\n", Stream::Stdout, &on_line);

        assert_eq!(
            *lines.lock().unwrap(),
            vec![
                (Stream::Stdout, "a".to_string()),
                (Stream::Stdout, "b".to_string()),
                (Stream::Stdout, "c".to_string()),
            ]
        );
        assert_eq!(buf.into_transcript(), "a\n\nb\r\nc\n");
    }

    #[test]
    fn test_a_runaway_transcript_is_capped_keeping_the_head_and_the_tail() {
        // Unbounded, this is how a build that loops printing a warning
        // takes the app down with it. Capped, the two parts worth reading
        // survive: the head (what was run) and the tail (what it died
        // of), with a note in between saying how much went missing.
        let lines: Arc<Mutex<Vec<(Stream, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let lines_cb = lines.clone();
        let on_line: Option<LineCallback> = Some(Arc::new(move |stream, line| {
            lines_cb.lock().unwrap().push((stream, line));
        }));

        let mut buf = StreamBuffer::new(CapPolicy::ElideMiddle);
        let mut written = 0usize;
        let mut last = String::new();
        let mut i = 0usize;
        // Comfortably past the cap, pushed in chunks the size the runner
        // actually reads, so the compaction runs many times over.
        while written < 3 * (HEAD_CAP + TAIL_CAP) {
            let mut chunk = String::new();
            while chunk.len() < 4096 {
                last = format!("line-{i}-{}", "y".repeat(60));
                chunk.push_str(&last);
                chunk.push('\n');
                i += 1;
            }
            written += chunk.len();
            buf.push(chunk.as_bytes(), Stream::Stdout, &on_line);
        }

        assert!(
            buf.bytes.len() <= HEAD_CAP + TAIL_CAP,
            "the buffer grew past the cap: {}",
            buf.bytes.len()
        );
        // Capping the stored transcript must not cost the user a single
        // streamed line: `on_line` sees everything, whatever is kept.
        let seen = lines.lock().unwrap().clone();
        assert_eq!(seen.len(), i, "every line must still reach the log drawer");
        assert_eq!(seen[i - 1], (Stream::Stdout, last.clone()));

        let text = buf.into_transcript();
        assert!(text.starts_with("line-0-"), "the head is gone");
        assert!(
            text.ends_with(&format!("{last}\n")),
            "the tail is gone -- which is the half with the error in it"
        );
        assert!(text.contains("bytes of output elided here"));
        assert!(text.len() < HEAD_CAP + TAIL_CAP + 512);
    }

    #[test]
    fn test_capping_a_single_endless_line_keeps_the_cursor_valid() {
        // A progress display that never sends a terminator (no `\n`, no
        // `\r`) is one line as far as the splitter is concerned, so the
        // unterminated tail *is* the whole buffer and the compaction has
        // to move the cursor with it. Getting that wrong is a reversed
        // slice range, i.e. a panic inside the operation task.
        let lines: Arc<Mutex<Vec<(Stream, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let lines_cb = lines.clone();
        let on_line: Option<LineCallback> = Some(Arc::new(move |stream, line| {
            lines_cb.lock().unwrap().push((stream, line));
        }));

        let mut buf = StreamBuffer::new(CapPolicy::ElideMiddle);
        let chunk = vec![b'x'; 4096];
        for _ in 0..((3 * (HEAD_CAP + TAIL_CAP)) / chunk.len()) {
            buf.push(&chunk, Stream::Stdout, &on_line);
        }
        assert!(buf.bytes.len() <= HEAD_CAP + TAIL_CAP);
        assert!(lines.lock().unwrap().is_empty(), "no line ever ended");

        buf.flush_partial_line(Stream::Stdout, &on_line);
        let seen = lines.lock().unwrap().clone();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].1.len(), HEAD_CAP + TAIL_CAP);
    }

    #[tokio::test]
    async fn test_parsed_stdout_past_the_transcript_cap_arrives_whole_and_parses() {
        // The regression this exists for: the head+tail cap was applied to
        // every stream of every command, including the stdout of
        // `brew info --installed --json=v2`, which nobody reads -- a
        // parser does. `adapters/fixtures/brew/7.0.3/info-installed.json`
        // is 419,458 bytes for 93 entries, so a Mac with a few hundred
        // formulae and casks crosses 2 MiB, the middle of the document
        // was replaced with an English sentence, `serde_json` failed, and
        // the whole Homebrew section disappeared from the Installed page
        // on every launch. A parser's input is never shortened.
        let runner = RealRunner::new();
        let spec = CommandSpec {
            program: sh(),
            args: vec![
                "-c".to_string(),
                // ~3 MiB of valid JSON: comfortably past HEAD_CAP +
                // TAIL_CAP, nowhere near PARSE_CAP.
                "printf '['; yes '\"yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy\",' \
                 | head -n 70000; printf '\"end\"]'"
                    .to_string(),
            ],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(30),
            output_use: OutputUse::Parsed,
        };
        let output = runner
            .run(spec, None, CancellationToken::new())
            .await
            .expect("spawn /bin/sh");

        assert_eq!(output.exit_code, Some(0));
        assert!(
            output.stdout.len() > HEAD_CAP + TAIL_CAP,
            "the test did not actually exceed the transcript cap: {} bytes",
            output.stdout.len()
        );
        assert!(
            !output.stdout.contains("elided"),
            "a parser's input was shortened"
        );
        let parsed: serde_json::Value = serde_json::from_str(&output.stdout)
            .expect("stdout must still be the document brew wrote");
        assert_eq!(
            parsed.as_array().expect("a JSON array").len(),
            70001,
            "every entry must survive"
        );
    }

    #[tokio::test]
    async fn test_parsed_stdout_past_the_parse_cap_fails_loudly_instead_of_splicing() {
        // The other half of the same rule. Past PARSE_CAP there is no
        // honest answer left -- half a JSON document is not half an
        // answer -- so the run ends as an error the adapter turns into a
        // visible per-source problem, rather than as a `CommandOutput`
        // whose `stdout` silently is not what the tool wrote.
        let runner = RealRunner::new();
        let spec = CommandSpec {
            program: sh(),
            args: vec![
                "-c".to_string(),
                format!(
                    "dd if=/dev/zero bs=1048576 count={} 2>/dev/null",
                    PARSE_CAP / (1024 * 1024) + 8
                ),
            ],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(60),
            output_use: OutputUse::Parsed,
        };
        let result = runner.run(spec, None, CancellationToken::new()).await;
        assert!(
            matches!(result, Err(RunnerError::OutputTooLarge { limit }) if limit == PARSE_CAP),
            "expected OutputTooLarge, got {:?}",
            result.map(|o| o.stdout.len())
        );
    }

    #[test]
    fn test_a_refusing_buffer_drops_what_it_holds_once_it_has_overflowed() {
        // The memory bound has to survive the refusal: a stream that has
        // already lost the run must not keep accumulating on the way to
        // the error.
        let mut buf = StreamBuffer::new(CapPolicy::Refuse);
        let chunk = vec![b'x'; 1024 * 1024];
        for _ in 0..(PARSE_CAP / chunk.len() + 4) {
            buf.push(&chunk, Stream::Stdout, &None);
        }
        assert!(
            buf.overflowed,
            "the buffer must remember that it overflowed"
        );
        assert!(
            buf.bytes.len() <= chunk.len(),
            "an overflowed buffer must not keep accumulating: {} bytes",
            buf.bytes.len()
        );
    }

    #[test]
    fn test_a_refusing_buffer_below_the_parse_cap_never_elides() {
        // Below PARSE_CAP a parsed stream is byte-for-byte what the tool
        // wrote, even well past the transcript cap that used to apply to
        // it.
        let mut buf = StreamBuffer::new(CapPolicy::Refuse);
        let chunk = vec![b'x'; 64 * 1024];
        let rounds = (3 * (HEAD_CAP + TAIL_CAP)) / chunk.len();
        for _ in 0..rounds {
            buf.push(&chunk, Stream::Stdout, &None);
        }
        assert!(!buf.overflowed);
        let text = buf.into_transcript();
        assert_eq!(text.len(), rounds * chunk.len());
        assert!(!text.contains("elided"));
    }

    #[test]
    fn test_a_transcript_that_is_not_utf8_still_decodes_losslessly_enough() {
        // `into_transcript` hands its `Vec<u8>` straight to
        // `String::from_utf8` so that the common case costs no copy at
        // all; a tool writing raw bytes must still get a transcript
        // rather than an error.
        let mut buf = StreamBuffer::new(CapPolicy::ElideMiddle);
        buf.push(b"before\xffafter\n", Stream::Stdout, &None);
        assert_eq!(buf.into_transcript(), "before\u{fffd}after\n");
    }
}
