//! `RealRunner` is Unix-only by design for this plan: it puts the spawned
//! child in its own process group (`process_group(0)`) and stops that whole
//! group with `libc::killpg` on timeout/cancel -- SIGTERM, a grace period,
//! then SIGKILL for whatever is left -- so that a `brew` invocation's
//! grandchildren (e.g. a `curl` download) stop with it. Both APIs are
//! POSIX-only, so this module (and the `canager-core` crate as a whole) is
//! not expected to build or run on non-Unix platforms. Canager v1 targets
//! macOS only (see Global Constraints in the phase 0-1 plan), so this is not
//! a limitation in practice.

use super::{
    CommandOutput, CommandRunner, CommandSpec, LineCallback, OutputUse, RunLine, RunnerError,
};
use crate::events::{LogNote, Stream};
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

/// What stands in a transcript for the middle [`StreamBuffer`] dropped.
/// See `StreamBuffer::into_transcript`.
const ELISION_MARK: &str = "\n[…]\n";

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

    /// Tells the log that this stream ended because reading it failed, not
    /// because the child stopped writing.
    ///
    /// `read()` returning an error and `read()` returning 0 both end the
    /// stream here, and the run still reports the child's exit code — so a
    /// mid-stream `EIO` used to cut the log short with nothing anywhere
    /// saying it had been cut, and the user read a short log of a run
    /// reported as successful. Nothing can be recovered (the pipe is gone),
    /// but the hole can at least be visible.
    ///
    /// The remark goes to `on_line` as a [`LogNote`], not into the
    /// transcript as text: it is Canager speaking, and the log drawer
    /// localises what Canager says (it used to be an English
    /// `[canager: ...]` sentence spliced into the tool's own bytes, where
    /// it also ended up in a failed run's five-line stderr summary). Any
    /// half-line already read is delivered first, so the note lands
    /// after the last thing the tool said, which is where the hole is.
    fn note_read_error(
        &mut self,
        e: &std::io::Error,
        stream: Stream,
        on_line: &Option<LineCallback>,
    ) {
        self.flush_partial_line(stream, on_line);
        if let Some(cb) = on_line {
            cb(RunLine::Note(LogNote::ReadFailed {
                stream,
                error: e.to_string(),
            }));
        }
    }

    /// Takes one `read()` result on this stream's pipe: records the bytes,
    /// or marks the stream `done` on EOF or on an error (noting the error
    /// in the transcript). The one place every read loop in `run` -- the
    /// main loop, the stop grace period and the final drain -- turns a
    /// read into transcript, so the three cannot drift apart.
    fn take_read(
        &mut self,
        res: std::io::Result<usize>,
        read_buf: &[u8],
        done: &mut bool,
        stream: Stream,
        on_line: &Option<LineCallback>,
    ) {
        match res {
            Ok(0) => *done = true,
            Ok(n) => self.push(&read_buf[..n], stream, on_line),
            Err(e) => {
                *done = true;
                self.note_read_error(&e, stream, on_line);
            }
        }
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
        // A bare ellipsis, not a sentence. This text is the tool's output,
        // and its last five stderr lines are shown to the user verbatim as
        // a failed run's summary -- which reaches back past this marker
        // whenever the retained tail holds fewer than five lines. An
        // English "[canager: N bytes elided]" there was Canager speaking
        // untranslated inside text the UI promises is only the tool's own.
        // `[…]` is the one omission mark every reader of either locale
        // already knows, and it needs no translating.
        text.push_str(ELISION_MARK);
        text.push_str(&String::from_utf8_lossy(&self.bytes[self.head_len..]));
        text
    }
}

fn emit(raw: &[u8], stream: Stream, on_line: &Option<LineCallback>) {
    if let Some(cb) = on_line {
        cb(RunLine::Output(
            stream,
            String::from_utf8_lossy(raw).to_string(),
        ));
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

/// How long to wait for a stopped child to be reaped before abandoning it.
///
/// Reached after the stop grace period, when the group has either emptied
/// on its own (the child is a zombie and this returns at once) or been
/// SIGKILLed. SIGKILL is not instantaneous: a process blocked in an
/// uninterruptible kernel wait (a read from a stalled disk or a hung
/// network mount) stays alive until that wait returns, and an unbounded
/// `wait()` would hang the whole operation — with no upper bound at all —
/// on the path the user reached by pressing Cancel. Abandoning the child
/// leaks no zombie: Tokio reaps a dropped `Child` in the background.
const POST_KILL_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

/// How long a cancelled or timed-out command's process group gets, after
/// SIGTERM, to exit on its own before whatever is left of it is SIGKILLed.
///
/// SIGTERM is the request to stop that tools clean up on: git removes its
/// `index.lock` (and every other `*.lock` it holds) from its signal
/// handler, and Ruby (Homebrew), Node (npm), Python (pip, pipx) and Go
/// (ollama) all run their exit paths or default to a prompt exit. None of
/// that cleanup is more than a few file operations, so it needs
/// milliseconds; five seconds leaves two orders of magnitude of headroom
/// for a machine under load. The grace period ends the moment the group is
/// empty, so it only costs anything when something in the group ignores
/// SIGTERM or hangs on it -- and then it is added to the user's wait after
/// pressing Cancel, which is the other side of the trade: `docker stop`'s
/// ten seconds or launchd's twenty are for daemons flushing state, not for
/// a window someone is watching. Five seconds, plus at most
/// `POST_KILL_WAIT`, keeps a Cancel on the most stubborn command inside
/// ten seconds.
const STOP_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// How often the stop grace period checks whether anything is left in the
/// group. There is no event for "a process group became empty": the
/// child's own exit has one (SIGCHLD), but reaping it would free the group
/// id this run still needs to be able to SIGKILL, and grandchildren are not
/// ours to wait for. So it asks `killpg(pgid, 0)` -- a check, no signal --
/// this often, which bounds how late a cleanly exiting group is noticed.
const GROUP_POLL: std::time::Duration = std::time::Duration::from_millis(20);

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

/// Sends `sig` to the whole process group, so a `brew` invocation's
/// grandchildren (a `curl` download, a `git` clone) are stopped with it
/// rather than outliving the operation that started them.
///
/// Called only through `GroupedChild`, which owns the group id and decides
/// when it is still safe to signal.
fn signal_group(pgid: libc::pid_t, sig: libc::c_int) {
    #[cfg(test)]
    probe::record_signal(pgid, sig);
    // SAFETY: `killpg` takes two integers and no pointers; the worst a
    // stale pid can do here is return ESRCH, which is ignored.
    unsafe {
        libc::killpg(pgid, sig);
    }
}

/// The spawned child together with the signals its run may send its
/// process group: at most one SIGTERM and at most one SIGKILL, SIGTERM
/// first, and never either once the child has been reaped.
///
/// Dropping `run` mid-flight used to orphan the child: `tokio::process::Child`
/// does not kill on drop, so the command kept going with nothing reading its
/// pipes and nobody left to stop it. That matters most to `Session::refresh`,
/// whose workers run their commands under the instance's resource lock and
/// are aborted when the refresh future is dropped -- the abort releases the
/// lock, and without this the command the lock was protecting would carry on
/// underneath the next operation to take it.
///
/// Armed from spawn until the child's own exit is seen, the run SIGKILLs
/// the group, or a stop finds nothing left in the group to signal. All
/// three consume the group id, so it is never signalled again after that,
/// and only ever while the child is unreaped: the sole reaping `wait()` in
/// the read loop disarms this in the same select arm that observes it,
/// before anything else can be awaited, and the stop path disarms or kills
/// before `reap` runs. `terminate` (SIGTERM) deliberately leaves it armed:
/// the grace period after it still needs to check the group and, if
/// something ignored the request, SIGKILL it.
///
/// **Dropping this SIGKILLs, with no grace period.** A `Drop` cannot await,
/// so it cannot wait one out, and the alternatives are worse than the kill:
/// handing the child to a spawned task that waits out the grace period
/// would let the caller release its resource lock (the refresh worker's
/// `_lock` drops right after the run) while the command is still running
/// and writing for up to `STOP_GRACE` -- seconds of an unlocked command
/// where the kill leaves microseconds -- and needs a live runtime inside a
/// destructor that may run during shutdown or a panic. Sending SIGTERM and
/// SIGKILL back to back is a SIGKILL with extra steps: the kill lands before
/// any handler runs. So the drop path stays SIGKILL, and the commands that
/// must not be killed are kept off it instead: `brew update` runs in a task
/// of its own that nothing drops (`BrewAdapter::maybe_update`), and nothing
/// in production drops any other run except by panicking.
///
/// The `Child` is a field rather than a separate local on purpose. Dropping
/// a `Child` whose process has already exited reaps it on the spot (tokio's
/// `Reaper::drop` calls `try_wait`), freeing the pid for reuse. The drop-path
/// `killpg` must therefore happen before the `Child` is dropped, and the
/// language guarantees exactly that here: a value's `Drop::drop` runs before
/// any of its fields are dropped. With two locals the same guarantee rested
/// on their declaration order, which a one-line rebinding silently reversed.
struct GroupedChild {
    child: tokio::process::Child,
    pgid: Option<libc::pid_t>,
    /// Whether `terminate` has sent its one SIGTERM.
    terminated: bool,
}

impl GroupedChild {
    /// Takes the freshly spawned child (spawned with `process_group(0)`, so
    /// its pid is also its group's id) and arms the kill.
    fn new(child: tokio::process::Child) -> Self {
        let pgid = child.id().map(|p| p as libc::pid_t);
        #[cfg(test)]
        if let Some(pid) = pgid {
            probe::record_spawn(pid);
        }
        Self {
            child,
            pgid,
            terminated: false,
        }
    }

    /// Ask the group to stop: SIGTERM, once. Stays armed, so the group can
    /// still be checked with `group_alive` and SIGKILLed with `kill`.
    fn terminate(&mut self) {
        if let (Some(pgid), false) = (self.pgid, self.terminated) {
            self.terminated = true;
            signal_group(pgid, libc::SIGTERM);
        }
    }

    /// Whether anything in the group can still be signalled.
    ///
    /// `killpg(pgid, 0)` sends nothing; it answers 0 while at least one
    /// member can be signalled. On macOS a group whose only member is the
    /// exited-but-unreaped child answers EPERM, not 0 (measured on Darwin
    /// 27: a zombie cannot be signalled), and an empty one ESRCH -- both
    /// mean there is nothing a SIGKILL could still reach. Only asked while
    /// armed, when the child is unreaped, so the group id cannot yet belong
    /// to anyone else. Were a kernel to count the zombie as signallable,
    /// the grace period would simply run to its end and SIGKILL an
    /// empty group: slower, never wrong.
    fn group_alive(&self) -> bool {
        match self.pgid {
            // SAFETY: signal 0 delivers nothing and takes no pointers.
            Some(pgid) => (unsafe { libc::killpg(pgid, 0) }) == 0,
            None => false,
        }
    }

    /// SIGKILL the group now, if it has not already been killed, seen to
    /// exit, or found empty.
    fn kill(&mut self) {
        if let Some(pgid) = self.pgid.take() {
            signal_group(pgid, libc::SIGKILL);
        }
    }

    /// Never signal the group again: the child exited on its own and has
    /// been reaped, or a stop found nothing left in the group and the child
    /// is about to be reaped. Either way its pid may be reused from here.
    fn disarm(&mut self) {
        self.pgid = None;
    }
}

impl Drop for GroupedChild {
    fn drop(&mut self) {
        // Runs before `self.child` is dropped, so the child cannot have been
        // reaped by its own drop yet. See the type's doc comment for why
        // this is a SIGKILL and not a graceful stop.
        self.kill();
    }
}

/// Test-only hooks that let a test see which child a run spawned, and each
/// signal its group was sent together with the state the child was in at
/// that moment.
///
/// Thread-local, because `#[tokio::test]` runs each test on its own
/// current-thread runtime, and both the spawn and the kill happen on the
/// thread that polls or drops the `run` future. Tests running in parallel
/// on other threads therefore never see each other's records.
#[cfg(test)]
mod probe {
    use std::cell::RefCell;

    /// What `waitid(WNOWAIT)` said about the child just before a `killpg`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) enum ChildState {
        /// Not exited yet.
        Running,
        /// Exited but not reaped: the pid is still held, so it cannot have
        /// been reused and `killpg` can only reach this child's own group.
        Zombie,
        /// Already reaped (ECHILD): the pid is free and may belong to
        /// anything by now. A signal in this state is the bug.
        Reaped,
    }

    thread_local! {
        static SPAWNED: RefCell<Vec<libc::pid_t>> = const { RefCell::new(Vec::new()) };
        static SIGNALS: RefCell<Vec<(libc::pid_t, libc::c_int, ChildState)>> =
            const { RefCell::new(Vec::new()) };
    }

    /// Asks, without reaping, whether `pid` is still an unreaped child.
    pub(super) fn state(pid: libc::pid_t) -> ChildState {
        // SAFETY: `siginfo_t` is plain data, all-zero is a valid value, and
        // `waitid` only writes into it. `WNOWAIT` leaves a zombie in place.
        unsafe {
            let mut info: libc::siginfo_t = std::mem::zeroed();
            let rc = libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            );
            if rc != 0 {
                ChildState::Reaped
            } else if info.si_pid() == 0 {
                ChildState::Running
            } else {
                ChildState::Zombie
            }
        }
    }

    pub(super) fn record_spawn(pid: libc::pid_t) {
        SPAWNED.with(|s| s.borrow_mut().push(pid));
    }

    pub(super) fn record_signal(pid: libc::pid_t, sig: libc::c_int) {
        let seen = state(pid);
        SIGNALS.with(|k| k.borrow_mut().push((pid, sig, seen)));
    }

    /// Drains this thread's spawn records. Draining rather than reading
    /// keeps a test from seeing an earlier test's records when libtest runs
    /// several on one thread (`--test-threads=1`).
    pub(super) fn take_spawned() -> Vec<libc::pid_t> {
        SPAWNED.with(|s| std::mem::take(&mut *s.borrow_mut()))
    }

    /// Drains this thread's signal records, in the order they were sent.
    /// See `take_spawned`.
    pub(super) fn take_signals() -> Vec<(libc::pid_t, libc::c_int, ChildState)> {
        SIGNALS.with(|k| std::mem::take(&mut *k.borrow_mut()))
    }
}

/// Waits for a stopped child, bounded by `POST_KILL_WAIT`, and returns its
/// exit status if it was reaped in time.
async fn reap(child: &mut tokio::process::Child) -> Option<std::process::ExitStatus> {
    tokio::time::timeout(POST_KILL_WAIT, child.wait())
        .await
        .ok()
        .and_then(Result::ok)
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

        // Owns the child, so on a drop mid-run its `Drop` signals the group
        // before the `Child` inside it can reap anything. See `GroupedChild`.
        let mut child = GroupedChild::new(cmd.spawn()?);
        let mut stdout = child.child.stdout.take().expect("stdout was piped");
        let mut stderr = child.child.stderr.take().expect("stderr was piped");

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
        // pid that is no longer there. It takes `&mut child.child`, which the
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
                // Neither arm signals anything itself: both leave the loop
                // for the stop below, which asks the group to stop before
                // it forces it to.
                _ = cancel.cancelled() => cancelled = true,
                _ = &mut sleep => timed_out = true,
                res = child.child.wait() => {
                    child.disarm();
                    child_done = true;
                    child_code = res.ok().and_then(|status| status.code());
                }
                res = stdout.read(&mut stdout_read_buf), if !stdout_done => {
                    out.take_read(res, &stdout_read_buf, &mut stdout_done, Stream::Stdout, &on_line);
                }
                res = stderr.read(&mut stderr_read_buf), if !stderr_done => {
                    err.take_read(res, &stderr_read_buf, &mut stderr_done, Stream::Stderr, &on_line);
                }
            }
        }

        // Cancelled or timed out: stop the group, gracefully first.
        //
        // SIGTERM goes to the whole group, then the group gets `STOP_GRACE`
        // to empty on its own -- git removing its `index.lock`, npm and
        // pip unwinding -- and only what is still there after that is
        // SIGKILLed. SIGKILL straight away, which is what this used to do,
        // gives nothing a chance to clean up: a `brew update` killed that
        // way inside git leaves `.git/index.lock` behind and Homebrew
        // refusing to update until someone deletes it by hand.
        //
        // The pipes are read throughout. A tool that reports what it is
        // doing as it cleans up would otherwise fill a pipe nobody is
        // reading, block on the write, and turn a clean exit into a SIGKILL
        // at the end of the grace period.
        //
        // The child is not reaped here, even once it has exited: its pid
        // is the group's id, and reaping it could free that id for reuse
        // while a grandchild that ignored SIGTERM still needs SIGKILLing.
        // `child.child.wait()` is deliberately not polled; `group_alive`
        // asks without reaping. Only once the group is SIGKILLed or found
        // empty -- and the guard disarmed either way -- does `reap` below
        // collect the child.
        let stopping = timed_out || cancelled;
        if stopping {
            child.terminate();
            let grace = tokio::time::sleep(STOP_GRACE);
            tokio::pin!(grace);
            let mut grace_over = false;
            while !grace_over && child.group_alive() {
                tokio::select! {
                    biased;
                    _ = &mut grace => grace_over = true,
                    _ = tokio::time::sleep(GROUP_POLL) => {}
                    res = stdout.read(&mut stdout_read_buf), if !stdout_done => {
                        out.take_read(res, &stdout_read_buf, &mut stdout_done, Stream::Stdout, &on_line);
                    }
                    res = stderr.read(&mut stderr_read_buf), if !stderr_done => {
                        err.take_read(res, &stderr_read_buf, &mut stderr_done, Stream::Stderr, &on_line);
                    }
                }
            }
            if child.group_alive() {
                // Something ignored SIGTERM, or is still cleaning up after
                // `STOP_GRACE`. The child is unreaped, so the group id is
                // still ours to signal.
                child.kill();
            } else {
                // Nothing left that a signal could reach. Disarm before
                // `reap` frees the pid, so the guard's `Drop` has nothing
                // to send either.
                child.disarm();
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
        let drain_budget = if stopping {
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
                            out.take_read(res, &stdout_read_buf, &mut stdout_done, Stream::Stdout, &on_line);
                        }
                        res = stderr.read(&mut stderr_read_buf), if !stderr_done => {
                            err.take_read(res, &stderr_read_buf, &mut stderr_done, Stream::Stderr, &on_line);
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

        let exit_code = if stopping {
            // `reap` is the only `wait()` on these paths, and it runs after
            // the guard was disarmed or fired, so every signal the stop sent
            // reached a child that was not yet reaped.
            match reap(&mut child.child)
                .await
                .and_then(|status| status.code())
            {
                // It exited 0 inside the grace period: it finished its work.
                // Either it was already done when the deadline or the cancel
                // landed (the `biased` select prefers both over `wait()`, so
                // a child that exited in the same instant still arrives
                // here), or it completed while SIGTERM was on its way. The
                // runner believes an exit status of 0 on every other path,
                // and there is no other evidence to go on here. Reporting it
                // as timed out or cancelled would tell the caller a
                // finished command was not -- `brew update` would not record
                // its update, and an install would come back `Unconfirmed`.
                // The tools Canager runs report a stop they obeyed as death
                // by SIGTERM (git, Ruby, Python, Rust and Go all end that
                // way by default) or a non-zero exit (npm), so a 0 here is
                // not what obeying looks like.
                Some(0) => {
                    timed_out = false;
                    cancelled = false;
                    Some(0)
                }
                // Stopped by this run -- by SIGTERM, by the SIGKILL after
                // it, or by its own exit path answering SIGTERM with a
                // failure status -- so there is no exit code worth
                // reporting, only the flag saying which of the two happened.
                _ => None,
            }
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

    /// A callback for tests that only look at the tool's own lines. A note
    /// arriving where none should is a failure, not something to skip.
    fn output_lines(f: impl Fn(Stream, String) + Send + Sync + 'static) -> LineCallback {
        Arc::new(move |run_line| match run_line {
            RunLine::Output(stream, line) => f(stream, line),
            RunLine::Note(note) => panic!("unexpected note: {note:?}"),
        })
    }

    fn sh() -> std::path::PathBuf {
        std::path::PathBuf::from("/bin/sh")
    }

    #[tokio::test]
    async fn test_streams_stdout_lines_and_reports_exit_code() {
        let runner = RealRunner::new();
        let lines: Arc<Mutex<Vec<(Stream, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let lines_cb = lines.clone();
        let on_line: LineCallback = output_lines(move |stream, line| {
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
    async fn test_cancel_signals_the_group_exactly_once() {
        // The stop's contract, pinned through the test-only recorder in
        // `signal_group` (which cannot exist in a release build): one
        // SIGTERM, then -- only if something in the group outlasts
        // `STOP_GRACE` -- one SIGKILL, never either after the child has
        // been reaped. A child that obeys SIGTERM gets exactly the SIGTERM.
        //
        // The exact list is the point. `output.cancelled` is identical
        // however many signals went out, and the ones that matter are the
        // ones a lost `Option::take` would add: `GroupedChild`'s `Drop`
        // fires on every return, after `reap()` has reaped the child, so a
        // `disarm()` or `kill()` that left the pgid in place would send a
        // SIGKILL at a freed pid -- exactly the PID-reuse hazard the merge
        // gate ruled out (signal after reap). The recorded state says the
        // SIGTERM reached a live child, and the elapsed time says the grace
        // period ended as soon as the group was empty rather than running
        // to its end.
        use probe::ChildState;

        probe::take_spawned();
        probe::take_signals();

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
        let started = std::time::Instant::now();
        let output = runner.run(spec, None, cancel).await.expect("spawn /bin/sh");
        let elapsed = started.elapsed();
        assert!(output.cancelled);
        assert_eq!(output.exit_code, None);

        let spawned = probe::take_spawned();
        assert_eq!(spawned.len(), 1, "one run spawns exactly one child");
        let pid = spawned[0];
        assert_eq!(
            probe::take_signals(),
            vec![(pid, libc::SIGTERM, ChildState::Running)],
            "a child that obeys SIGTERM must get exactly one SIGTERM and \
             nothing else -- including after `run` has returned, which is \
             where a lost `take()` would add a SIGKILL"
        );
        assert!(
            elapsed < STOP_GRACE,
            "the grace period must end when the group empties, not run out; took {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn test_timeout_signals_the_group_exactly_once() {
        // The timeout arm's mirror of `test_cancel_signals_the_group_exactly_once`:
        // same stop, a different select arm reaching it. See that test's
        // comment for why the exact list matters and `output.timed_out`
        // alone does not.
        use probe::ChildState;

        probe::take_spawned();
        probe::take_signals();

        let runner = RealRunner::new();
        let spec = CommandSpec {
            program: sh(),
            args: vec!["-c".to_string(), "sleep 5".to_string()],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_millis(200),
            output_use: OutputUse::Transcript,
        };
        let started = std::time::Instant::now();
        let output = runner
            .run(spec, None, CancellationToken::new())
            .await
            .expect("spawn /bin/sh");
        let elapsed = started.elapsed();
        assert!(output.timed_out);
        assert_eq!(output.exit_code, None);

        let spawned = probe::take_spawned();
        assert_eq!(spawned.len(), 1, "one run spawns exactly one child");
        let pid = spawned[0];
        assert_eq!(
            probe::take_signals(),
            vec![(pid, libc::SIGTERM, ChildState::Running)],
            "a timeout on a child that obeys SIGTERM must send exactly one \
             SIGTERM and nothing else"
        );
        assert!(
            elapsed < STOP_GRACE,
            "the grace period must end when the group empties, not run out; took {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn test_a_child_that_traps_sigterm_is_given_time_to_clean_up() {
        // The reason the stop is graceful at all. `brew update` is git
        // rewriting a checkout; git removes its `index.lock` from its
        // SIGTERM handler, and a SIGKILL -- which runs no handler -- leaves
        // the lock behind and Homebrew refusing to update until someone
        // deletes it by hand. Here the shell stands in for git: it traps
        // SIGTERM, takes a moment to clean up (longer than a SIGKILL would
        // have given it: none), says so on stdout, leaves a marker file,
        // and exits with the conventional 128+15.
        //
        // Also pinned: the cleanup's own output reaches the transcript (the
        // pipes are read through the grace period), the run is still
        // reported as cancelled (a stop the child obeyed is not the child
        // finishing), and the group got SIGTERM and nothing else.
        use probe::ChildState;

        let marker = unique_temp_path("sigterm-cleanup");
        let _ = std::fs::remove_file(&marker);
        probe::take_spawned();
        probe::take_signals();

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
                format!(
                    "trap 'sleep 0.3; echo cleaning up; echo cleaned > {}; exit 143' TERM; \
                     sleep 30 & wait",
                    marker.display()
                ),
            ],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(30),
            output_use: OutputUse::Transcript,
        };
        let started = std::time::Instant::now();
        let output = runner.run(spec, None, cancel).await.expect("spawn /bin/sh");
        let elapsed = started.elapsed();

        let cleaned = std::fs::read_to_string(&marker).ok();
        let _ = std::fs::remove_file(&marker);
        assert_eq!(
            cleaned.as_deref(),
            Some("cleaned\n"),
            "a child that handles SIGTERM must be given the time to clean up, not SIGKILLed"
        );
        assert!(
            output.cancelled,
            "the child stopped because it was asked to"
        );
        assert!(!output.timed_out);
        assert_eq!(output.exit_code, None);
        assert!(
            output.stdout.contains("cleaning up"),
            "what the child wrote while cleaning up is missing: {:?}",
            output.stdout
        );
        let pid = probe::take_spawned()[0];
        assert_eq!(
            probe::take_signals(),
            vec![(pid, libc::SIGTERM, ChildState::Running)],
            "a child that exits inside the grace period must never be SIGKILLed"
        );
        assert!(
            elapsed < STOP_GRACE,
            "the stop must end when the child does, not when the grace period runs out; \
             took {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn test_a_child_that_ignores_sigterm_is_sigkilled_after_the_grace_period() {
        // The other half of the contract: SIGTERM is a request, and a
        // group that ignores it must still end. The shell ignores SIGTERM
        // and so does the `sleep` it starts (an ignored signal stays
        // ignored across fork and exec), so only the SIGKILL at the end of
        // `STOP_GRACE` can stop either. The grandchild is what is watched:
        // it is reparented and reaped as soon as it dies, and only a
        // group-wide signal reaches it at all. Through the timeout arm, so
        // this and the trap test above cover both ways into the stop.
        use probe::ChildState;

        probe::take_spawned();
        probe::take_signals();

        let runner = RealRunner::new();
        let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let lines_cb = lines.clone();
        let on_line: LineCallback = output_lines(move |_stream, line| {
            lines_cb.lock().unwrap().push(line);
        });
        let spec = CommandSpec {
            program: sh(),
            args: vec![
                "-c".to_string(),
                "trap '' TERM; sleep 30 & echo $!; wait".to_string(),
            ],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_millis(300),
            output_use: OutputUse::Transcript,
        };
        let started = std::time::Instant::now();
        let output = runner
            .run(spec, Some(on_line), CancellationToken::new())
            .await
            .expect("spawn /bin/sh");
        let elapsed = started.elapsed();

        assert!(output.timed_out);
        assert_eq!(output.exit_code, None);
        assert!(
            elapsed >= STOP_GRACE,
            "a group that ignores SIGTERM must get the whole grace period; took {elapsed:?}"
        );
        let pid = probe::take_spawned()[0];
        assert_eq!(
            probe::take_signals(),
            vec![
                (pid, libc::SIGTERM, ChildState::Running),
                (pid, libc::SIGKILL, ChildState::Running),
            ],
            "SIGTERM, then after the grace period exactly one SIGKILL, both \
             while the child was unreaped"
        );

        let grandchild: libc::pid_t = lines
            .lock()
            .unwrap()
            .first()
            .expect("the shell prints the sleep's pid")
            .trim()
            .parse()
            .expect("the shell prints the sleep's pid");
        // SAFETY: signal 0 only checks that the pid exists.
        let alive = || unsafe { libc::kill(grandchild, 0) } == 0;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while alive() {
            assert!(
                std::time::Instant::now() < deadline,
                "a grandchild that ignores SIGTERM must still be SIGKILLed with its group"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn test_a_child_that_finishes_inside_the_grace_period_is_not_timed_out() {
        // A deadline that lands just before a command finishes used to
        // SIGKILL it at once and call it timed out, whatever it was about
        // to report. With a grace period the command can finish, and if it
        // does -- exits 0 -- the truthful report is that it finished: no
        // `timed_out`, its exit code, all of its output. (`brew update`
        // reads exactly this to decide whether to record the update.)
        // The shell ignores SIGTERM here only so that it survives to
        // finish; what is under test is how its exit is reported.
        use probe::ChildState;

        probe::take_spawned();
        probe::take_signals();

        let runner = RealRunner::new();
        let spec = CommandSpec {
            program: sh(),
            args: vec![
                "-c".to_string(),
                "trap '' TERM; sleep 0.5; echo done".to_string(),
            ],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_millis(150),
            output_use: OutputUse::Transcript,
        };
        let output = runner
            .run(spec, None, CancellationToken::new())
            .await
            .expect("spawn /bin/sh");

        assert!(
            !output.timed_out,
            "a command that finished inside the grace period did not time out"
        );
        assert!(!output.cancelled);
        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout, "done\n");
        let pid = probe::take_spawned()[0];
        assert_eq!(
            probe::take_signals(),
            vec![(pid, libc::SIGTERM, ChildState::Running)],
            "it was asked to stop once and finished instead; nothing more was sent"
        );
    }

    #[tokio::test]
    async fn test_a_normal_exit_never_signals_the_group() {
        // The third leg of the same guarantee: a child that exits on its
        // own must never be `killpg`'d at all. `disarm()` in the `wait()`
        // arm takes the pgid before anything else can be awaited, so the
        // guard's own `Drop` at the end of `run` finds nothing left to
        // kill.
        probe::take_spawned();
        probe::take_signals();

        let runner = RealRunner::new();
        let spec = CommandSpec {
            program: sh(),
            args: vec!["-c".to_string(), "exit 0".to_string()],
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

        let spawned = probe::take_spawned();
        assert_eq!(spawned.len(), 1, "one run spawns exactly one child");
        assert_eq!(
            probe::take_signals(),
            Vec::new(),
            "a child that exits on its own must never be signalled"
        );
    }

    #[tokio::test]
    async fn test_dropping_a_run_mid_flight_kills_its_process_group() {
        // `Session::refresh` aborts its workers when it is dropped, which
        // drops whatever `run` future they were awaiting. The command must
        // die with it: an orphaned child would keep working underneath the
        // resource lock the abort just released. The grandchild is what is
        // watched, because the shell itself stays an unreaped zombie of
        // this process for a while and `kill(pid, 0)` still finds a zombie;
        // the grandchild is reparented and reaped as soon as it dies, and
        // only a group-wide kill reaches it at all.
        let runner = RealRunner::new();
        let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let lines_cb = lines.clone();
        let on_line: LineCallback = output_lines(move |_stream, line| {
            lines_cb.lock().unwrap().push(line);
        });
        let spec = CommandSpec {
            program: sh(),
            args: vec!["-c".to_string(), "sleep 30 & echo $!; wait".to_string()],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(60),
            output_use: OutputUse::Transcript,
        };
        let mut run = Box::pin(runner.run(spec, Some(on_line), CancellationToken::new()));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let grandchild: libc::pid_t = loop {
            tokio::select! {
                _ = &mut run => panic!("`sleep 30` cannot have finished"),
                _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {}
            }
            if let Some(line) = lines.lock().unwrap().first() {
                break line
                    .trim()
                    .parse()
                    .expect("the shell prints the sleep's pid");
            }
            assert!(std::time::Instant::now() < deadline, "no pid printed");
        };
        // SAFETY: signal 0 only checks that the pid exists.
        let alive = || unsafe { libc::kill(grandchild, 0) } == 0;
        assert!(alive(), "the grandchild must be running before the drop");

        drop(run);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while alive() {
            assert!(
                std::time::Instant::now() < deadline,
                "dropping `run` must kill the whole process group, not orphan it"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn test_dropping_a_run_whose_child_exited_unobserved_signals_before_reaping() {
        // The one drop where the order of kill and reap matters: the child
        // has already exited, but the read loop has not polled `wait()`
        // since, so tokio has not reaped it. Dropping the `Child` would reap
        // it on the spot (`Reaper::drop` calls `try_wait`) and free its pid
        // for reuse, so the drop-path `killpg` must come first, while the
        // child is still a zombie holding the pid. A running child cannot
        // tell the two orders apart -- nothing reaps it either way -- which
        // is why the mid-flight drop test above does not cover this.
        use probe::ChildState;

        probe::take_spawned();
        probe::take_signals();

        let runner = RealRunner::new();
        let spec = CommandSpec {
            program: std::path::PathBuf::from("/bin/sleep"),
            args: vec!["30".to_string()],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(60),
            output_use: OutputUse::Transcript,
        };
        let mut run = runner.run(spec, None, CancellationToken::new());

        // One poll spawns the child and parks the loop on its arms. A no-op
        // waker, so nothing re-polls `run` behind the test's back.
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(
            run.as_mut().poll(&mut cx).is_pending(),
            "`sleep 30` cannot have finished on the first poll"
        );
        let spawned = probe::take_spawned();
        assert_eq!(spawned.len(), 1, "one poll spawns exactly one child");
        let pid = spawned[0];
        assert_eq!(probe::state(pid), ChildState::Running);

        // End the child from outside -- itself only, not its group, and not
        // through the run -- then block this thread until it is a zombie.
        // The runtime is current-thread, so while this thread is blocked
        // nothing polls `run`, and tokio cannot observe or reap the exit.
        // SAFETY: plain signal to a pid this test's own run just spawned
        // and nothing has reaped, so it cannot have been reused.
        assert_eq!(unsafe { libc::kill(pid, libc::SIGTERM) }, 0);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while probe::state(pid) != ChildState::Zombie {
            assert!(
                std::time::Instant::now() < deadline,
                "the child did not exit after SIGTERM"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        drop(run);

        assert_eq!(
            probe::take_signals(),
            vec![(pid, libc::SIGKILL, ChildState::Zombie)],
            "the drop must SIGKILL the group exactly once -- a `Drop` cannot \
             wait out a grace period, so no SIGTERM first -- and while the \
             child is still an unreaped zombie, never after its pid was freed"
        );
        assert_eq!(
            probe::state(pid),
            ChildState::Reaped,
            "dropping the run must still reap the child, not leak a zombie"
        );
    }

    #[tokio::test]
    async fn test_dropping_a_run_during_its_grace_period_sigkills_once() {
        // A run can be dropped after its stop has sent SIGTERM and while
        // it is waiting out `STOP_GRACE`. The guard is still armed then --
        // `terminate` does not consume the group id, precisely so the group
        // can still be SIGKILLed -- so the drop must SIGKILL it, once, and
        // the one SIGTERM already sent must not be repeated. The children
        // ignore SIGTERM, so only that SIGKILL can end them.
        use probe::ChildState;

        probe::take_spawned();
        probe::take_signals();

        let runner = RealRunner::new();
        let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let lines_cb = lines.clone();
        let on_line: LineCallback = output_lines(move |_stream, line| {
            lines_cb.lock().unwrap().push(line);
        });
        let spec = CommandSpec {
            program: sh(),
            args: vec![
                "-c".to_string(),
                "trap '' TERM; sleep 30 & echo $!; wait".to_string(),
            ],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(60),
            output_use: OutputUse::Transcript,
        };
        let cancel = CancellationToken::new();
        let mut run = Box::pin(runner.run(spec, Some(on_line), cancel.clone()));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let grandchild: libc::pid_t = loop {
            tokio::select! {
                _ = &mut run => panic!("`sleep 30` cannot have finished"),
                _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {}
            }
            if let Some(line) = lines.lock().unwrap().first() {
                break line
                    .trim()
                    .parse()
                    .expect("the shell prints the sleep's pid");
            }
            assert!(std::time::Instant::now() < deadline, "no pid printed");
        };

        // Into the grace period: cancel, then keep polling for a while
        // that is well inside `STOP_GRACE`.
        cancel.cancel();
        tokio::select! {
            _ = &mut run => panic!("a group that ignores SIGTERM cannot have stopped yet"),
            _ = tokio::time::sleep(std::time::Duration::from_millis(300)) => {}
        }
        drop(run);

        let pid = probe::take_spawned()[0];
        assert_eq!(
            probe::take_signals(),
            vec![
                (pid, libc::SIGTERM, ChildState::Running),
                (pid, libc::SIGKILL, ChildState::Running),
            ],
            "dropped mid-grace: the one SIGTERM, then the drop's one SIGKILL"
        );
        // SAFETY: signal 0 only checks that the pid exists.
        let alive = || unsafe { libc::kill(grandchild, 0) } == 0;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while alive() {
            assert!(
                std::time::Instant::now() < deadline,
                "dropping the run mid-grace must still kill the whole group"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
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
        let on_line: LineCallback = output_lines(move |stream, line| {
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
        // Cancelling and timing out both leave the read loop at once to stop
        // the process group, so anything the child had already written but
        // the loop had not yet read used to die with it -- missing from the
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
        let on_line: LineCallback = output_lines(move |stream, line| {
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
    async fn test_a_grandchild_holding_the_pipes_past_the_deadline_does_not_turn_success_into_a_timeout(
    ) {
        // `( ... ) &` leaves a grandchild that inherited stdout and
        // stderr, so both write ends stay open after the child exits 0.
        // The read loop used to end only on EOF, so it sat there for the
        // whole `spec.timeout` -- thirty minutes, for a `brew install`
        // that had finished in forty seconds -- then SIGKILLed the group,
        // including the helper the tool deliberately left running, and
        // reported `timed_out: true, exit_code: None`: `Unconfirmed`, for
        // a command that had succeeded.
        //
        // The grandchild here (6 s) deliberately outlives the deadline
        // (3 s). An earlier version of this test used a 0.5 s grandchild
        // under a 20 s deadline -- forty times shorter -- which an
        // EOF-terminated read loop (the very bug this test exists to
        // forbid) also satisfies: it too returns at ~0.5 s with
        // `exit_code: Some(0)`, `timed_out: false`, the full stdout and
        // the marker created, so it passed against the bug with every
        // assertion green. Only a grandchild that is still holding the
        // pipes when the deadline would otherwise fire can tell an
        // end-on-child loop from an end-on-EOF one apart: the old loop
        // sits there until the 3 s deadline and reports
        // `timed_out: true, exit_code: None`; the fixed loop returns the
        // moment the child exits and leaves the grandchild to finish on
        // its own. (The 0.5 s/20 s shape is kept below, as
        // `test_a_grandchild_holding_the_pipes_briefly_is_not_sigkilled`,
        // for the property it does still prove: a helper released well
        // inside the deadline is not SIGKILLed.)
        let marker = unique_temp_path("grandchild-outlives-deadline");
        let _ = std::fs::remove_file(&marker);

        let runner = RealRunner::new();
        let spec = CommandSpec {
            program: sh(),
            args: vec![
                "-c".to_string(),
                format!(
                    "(sleep 6; touch {}) & printf 'work done\\n'; exit 0",
                    marker.display()
                ),
            ],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(3),
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
            "the child exited 0 well before the 3 s deadline; a grandchild \
             still holding the pipes must not turn that into a timeout"
        );
        assert!(!output.timed_out);
        assert!(!output.cancelled);
        assert_eq!(output.stdout, "work done\n");
        assert!(
            elapsed < std::time::Duration::from_secs(3),
            "the run must end when the child does, not at the 3 s deadline; took {elapsed:?}"
        );

        // Wait out the grandchild's own 6 s sleep (plus a margin), counted
        // from when the command was started rather than from now, so a
        // slow `run()` return does not shorten the wait.
        let remaining = std::time::Duration::from_secs(7).saturating_sub(started.elapsed());
        tokio::time::sleep(remaining).await;
        assert!(
            marker.exists(),
            "a helper the tool left running must outlive the operation, not be SIGKILLed by it"
        );
        let _ = std::fs::remove_file(&marker);
    }

    #[tokio::test]
    async fn test_a_grandchild_holding_the_pipes_briefly_is_not_sigkilled() {
        // The property the original version of the test above proved,
        // kept on its own: a grandchild that releases the pipes well
        // inside the deadline is left running, not SIGKILLed alongside
        // the group. This shape does *not* distinguish an end-on-child
        // loop from an end-on-EOF one -- see the long comment on
        // `test_a_grandchild_holding_the_pipes_past_the_deadline_does_not_turn_success_into_a_timeout`
        // above -- so it is a narrower, additional check, not the
        // regression guard.
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
        let on_line: LineCallback = output_lines(move |stream, line| {
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
        let on_line: Option<LineCallback> = Some(output_lines(move |stream, line| {
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
    fn test_a_failed_read_is_a_note_for_the_log_not_english_in_the_transcript() {
        // The read error used to be spliced into the tool's bytes as
        // "[canager: reading stderr failed (...); output ends here]": an
        // English sentence in Canager's voice that the log drawer showed
        // verbatim and a failed run's five-line summary could quote. It is
        // now a `LogNote` the front end localises, after whatever half-line
        // the tool had got out, and the transcript stays the tool's alone.
        let seen: Arc<Mutex<Vec<RunLine>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_cb = seen.clone();
        let on_line: Option<LineCallback> = Some(Arc::new(move |run_line| {
            seen_cb.lock().unwrap().push(run_line);
        }));

        let mut buf = StreamBuffer::new(CapPolicy::ElideMiddle);
        let mut done = false;
        let first = b"Error: disk full\npartial";
        buf.take_read(Ok(first.len()), first, &mut done, Stream::Stderr, &on_line);
        assert!(!done);
        buf.take_read(
            Err(std::io::Error::from_raw_os_error(libc::EIO)),
            &[],
            &mut done,
            Stream::Stderr,
            &on_line,
        );
        assert!(done, "a failed read must end the stream");
        buf.flush_partial_line(Stream::Stderr, &on_line);

        let error = std::io::Error::from_raw_os_error(libc::EIO).to_string();
        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                RunLine::Output(Stream::Stderr, "Error: disk full".to_string()),
                RunLine::Output(Stream::Stderr, "partial".to_string()),
                RunLine::Note(LogNote::ReadFailed {
                    stream: Stream::Stderr,
                    error,
                }),
            ]
        );
        assert_eq!(buf.into_transcript(), "Error: disk full\npartial");
    }

    #[test]
    fn test_a_runaway_transcript_is_capped_keeping_the_head_and_the_tail() {
        // Unbounded, this is how a build that loops printing a warning
        // takes the app down with it. Capped, the two parts worth reading
        // survive: the head (what was run) and the tail (what it died
        // of), with a note in between saying how much went missing.
        let lines: Arc<Mutex<Vec<(Stream, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let lines_cb = lines.clone();
        let on_line: Option<LineCallback> = Some(output_lines(move |stream, line| {
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
        assert!(text.contains(ELISION_MARK));
        assert!(
            !text.contains("canager"),
            "Canager's own English is back inside the tool's transcript"
        );
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
        let on_line: Option<LineCallback> = Some(output_lines(move |stream, line| {
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
