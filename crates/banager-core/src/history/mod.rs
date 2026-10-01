//! What Banager did, kept across launches: one record per finished update
//! or uninstall, in `history.json` beside `settings.json` in Banager's
//! application data directory (docs/what-we-run.md, "Files Banager
//! writes"). The Updates page's 「最近更新」 reads it, so an update that
//! worked -- and that Banager checked by reading the version again -- is
//! still listed after Banager is quit and opened again.
//!
//! What a record holds is in `HistoryRecord`: when, which package, from
//! which version to which, and how it ended, as a category. Never a line
//! of a log, a command line or a path other than the ones a package's key
//! already carries (an instance id names its source's prefix, which can be
//! in the home folder; the window never shows one).
//!
//! Bounded: the newest `MAX_RECORDS`, none older than `MAX_AGE_MS` before
//! a time the file keeps as trusted (`Clock`): a time the clock jumped to
//! is believed only once the clock has run on from it for
//! `CLOCK_CONFIRM_MS`, so a clock set far ahead -- and the records stamped
//! with it -- drops nothing until then. What is listed is never older than
//! `MAX_AGE_MS` by the clock (`HistoryStore::view`).
//! A missing, unreadable or malformed file is an empty history, and a file
//! a newer Banager wrote is left exactly as it is (`HistoryStore::open`).
//! Written whole, to a staging file beside it renamed into place, as
//! `settings::save` writes settings -- on a thread of its own, never on an
//! operation's way to its end.

mod failure_cause;

pub use failure_cause::{failure_cause, FailureCause};

use crate::events::OpId;
use crate::model::{AdapterId, ArtifactKey, ArtifactKind, Attention, Fault, OpKind, Outcome};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Condvar, Mutex, Weak};
use std::time::Duration;

/// The file's format. A file that says a higher one was written by a newer
/// Banager: this one reads none of it and never writes over it.
pub const HISTORY_FORMAT: u32 = 1;

/// How many records the file keeps: the newest.
pub const MAX_RECORDS: usize = 1_000;

/// How old a record may be before it is dropped: 180 days.
pub const MAX_AGE_MS: i64 = 180 * 24 * 60 * 60 * 1_000;

/// How long the clock must run on from a time it moved ahead to before
/// that time is trusted (`Clock`): a week. A clock set ahead by mistake
/// is usually put right long before.
pub const CLOCK_CONFIRM_MS: i64 = 7 * 24 * 60 * 60 * 1_000;

/// What an operation was. Banager runs no install, so there are two.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryKind {
    Update,
    Uninstall,
}

/// How an operation ended, as a category: `Outcome` without the words of
/// the programs it ran and without the paths Banager's own reasons carry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryResult {
    /// `Outcome::Succeeded`. Whether Banager also saw the change for
    /// itself is `HistoryRecord::verified`.
    Succeeded,
    /// `Outcome::NeedsAttention`, with its reason as it is.
    NeedsAttention(Attention),
    /// `Outcome::Failed` or `Outcome::BanagerFailed`, with the cause in a
    /// word where one is known: read off the tool's last lines
    /// (`failure_cause`), or Banager's own `HomebrewStillUpdating`.
    Failed { cause: Option<FailureCause> },
    /// `Outcome::Unconfirmed`.
    Unconfirmed,
    /// `Outcome::Cancelled`, for an operation Banager had handed to its
    /// source's adapter (`Adapter::execute`). That is not proof that the
    /// tool's command ran: an adapter can be stopped while it is still
    /// getting ready -- Homebrew's waits for a `brew update` to end first
    /// -- and then nothing of the tool's ran at all. One cancelled before
    /// it reached the adapter (waiting for its turn, or while the version
    /// was read) is not recorded.
    Cancelled,
}

/// One finished operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryRecord {
    /// Which launch of Banager ran it (`HistoryStore::run`): with `op_id`,
    /// what lets the window tell a record of an operation it watched from
    /// the others, and list it once.
    pub run: String,
    /// The operation's id in that launch.
    pub op_id: OpId,
    /// When it finished, in milliseconds since 1970 (UTC).
    pub finished_at: i64,
    pub key: ArtifactKey,
    /// The name its row had when it was started (`InstalledArtifact::
    /// display_name`), or the key's name where no row listed it.
    pub display_name: String,
    /// The source's adapter, which the window names in its own language.
    pub adapter_id: AdapterId,
    pub kind: HistoryKind,
    /// The version it had before: read just before an update ran, or, where
    /// that reading has none, the one the list showed. `None` for a model,
    /// whose "version" is a digest, never shown.
    pub from_version: Option<String>,
    /// The version read back after an update whose tool exited 0. `None`
    /// for an uninstall, for an update that failed or was stopped, and for
    /// a model.
    pub to_version: Option<String>,
    pub result: HistoryResult,
    /// Whether Banager saw the result for itself: for an update, that the
    /// installed version it read before the command and the one it read
    /// after differ (a model's digests included); for an uninstall, that
    /// the reading after it found the package gone. `false` for an update
    /// taken as done on presence alone, with nothing to compare.
    pub verified: bool,
}

/// What the window is given (`get_history`): this launch's `run`, when the
/// list was last cleared, and every record, newest first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryView {
    pub run: String,
    /// The Updates page's Clear, kept: the time it was pressed, in
    /// milliseconds since 1970. The page lists nothing that finished
    /// before it; the records stay in the file.
    pub cleared_before: Option<i64>,
    pub records: Vec<HistoryRecord>,
}

/// The file. `records` oldest first. `trusted_at` and `pending_at` are
/// `Clock`'s; a file without them (written before they were) starts from
/// `age_anchor`.
#[derive(Serialize, Deserialize)]
struct HistoryFile {
    format: u32,
    cleared_before: Option<i64>,
    records: Vec<HistoryRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trusted_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pending_at: Option<i64>,
}

/// What `Session::submit` knows of an operation when it starts, kept for
/// when it finishes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Started {
    pub display_name: String,
    pub adapter_id: AdapterId,
    /// The version the list showed.
    pub listed_version: Option<String>,
}

/// What `OperationManager` hands back as an operation finishes
/// (`ops::Finished`, which builds one of these).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ended<'a> {
    pub op_id: OpId,
    pub key: &'a ArtifactKey,
    pub op_kind: OpKind,
    pub outcome: &'a Outcome,
    /// Whether the operation got as far as its command (`execute`).
    pub started: bool,
    /// The installed version read before an update, and after it.
    pub before: Option<&'a str>,
    pub after: Option<&'a str>,
}

/// The record for an operation that ended, or `None` for one that is not
/// kept: an install (Banager runs none), or one cancelled before it reached
/// its adapter's `execute` -- nothing was done, so there is nothing to
/// remember. (One cancelled inside `execute` is kept as `Cancelled`, even
/// where the adapter had not started the tool's command yet:
/// `HistoryResult::Cancelled`.)
pub fn record_for(
    ended: &Ended<'_>,
    started: &Started,
    run: &str,
    finished_at: i64,
) -> Option<HistoryRecord> {
    let kind = match ended.op_kind {
        OpKind::Upgrade => HistoryKind::Update,
        OpKind::Uninstall => HistoryKind::Uninstall,
        OpKind::Install => return None,
    };
    if !ended.started && *ended.outcome == Outcome::Cancelled {
        return None;
    }
    let result = match ended.outcome {
        Outcome::Succeeded => HistoryResult::Succeeded,
        Outcome::Cancelled => HistoryResult::Cancelled,
        Outcome::NeedsAttention(a) => HistoryResult::NeedsAttention(*a),
        Outcome::Failed { summary, .. } => HistoryResult::Failed {
            cause: failure_cause(summary),
        },
        Outcome::BanagerFailed(Fault::HomebrewStillUpdating { .. }) => HistoryResult::Failed {
            cause: Some(FailureCause::HomebrewUpdating),
        },
        Outcome::BanagerFailed(_) => HistoryResult::Failed { cause: None },
        Outcome::Unconfirmed => HistoryResult::Unconfirmed,
    };
    let known = |v: Option<&str>| v.filter(|v| !v.is_empty()).map(str::to_string);
    let (before, after) = (known(ended.before), known(ended.after));
    let verified = match kind {
        HistoryKind::Update => {
            result == HistoryResult::Succeeded
                && before.is_some()
                && after.is_some()
                && before != after
        }
        HistoryKind::Uninstall => result == HistoryResult::Succeeded,
    };
    let exited_zero = matches!(
        result,
        HistoryResult::Succeeded | HistoryResult::NeedsAttention(_)
    );
    let (from_version, to_version) = if ended.key.kind == ArtifactKind::Model {
        (None, None)
    } else {
        match kind {
            HistoryKind::Update => (
                before.or_else(|| known(started.listed_version.as_deref())),
                after.filter(|_| exited_zero),
            ),
            HistoryKind::Uninstall => (known(started.listed_version.as_deref()), None),
        }
    };
    Some(HistoryRecord {
        run: run.to_string(),
        op_id: ended.op_id,
        finished_at,
        key: ended.key.clone(),
        display_name: started.display_name.clone(),
        adapter_id: started.adapter_id.clone(),
        kind,
        from_version,
        to_version,
        result,
        verified,
    })
}

/// Where a history with no `Clock` yet starts trusting from: `now`, or
/// the newest of `records` when that is earlier. A clock set far ahead -- a wrong answer
/// from the network at boot, a date changed by hand -- then drops nothing
/// the records do not show to be old among themselves: dropped records
/// are gone from the file for good once it is written, and the clock may
/// be put right a minute later. Records are still dropped once a newer
/// one is 180 days past them.
fn age_anchor(records: &[HistoryRecord], now: i64) -> i64 {
    records
        .iter()
        .map(|r| r.finished_at)
        .max()
        .map_or(now, |newest| newest.min(now))
}

/// What the records' age is measured from, kept in the file: the latest
/// time the history trusts, and a later time the clock said that it does
/// not trust yet. A record is stamped with the clock as it is, but only
/// `trusted` -- or `now`, when the clock says earlier -- decides what is
/// too old (`anchor`). A time the clock moved ahead to becomes trusted
/// once the clock has said one at least `CLOCK_CONFIRM_MS` later; set back
/// before then, it is forgotten. So a clock set far ahead, and every
/// record stamped with it, drops nothing for a week, also across launches,
/// and nothing at all when it is put right within the week; a Mac whose
/// clock is right drops records up to a week (plus the time to the next
/// record) after they pass 180 days.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Clock {
    trusted: i64,
    pending: Option<i64>,
}

impl Clock {
    /// What the clock says now, taken in.
    fn observe(&mut self, now: i64) {
        match self.pending {
            // Set back before it was believed: that time is not kept.
            Some(pending) if now < pending => self.pending = None,
            // Run on from it for long enough: believed.
            Some(pending) if now >= pending.saturating_add(CLOCK_CONFIRM_MS) => {
                self.trusted = self.trusted.max(pending);
                self.pending = None;
            }
            _ => {}
        }
        if self.pending.is_none() && now > self.trusted {
            self.pending = Some(now);
        }
    }

    /// What age is measured from: `trusted`, or `now` when it is earlier.
    fn anchor(&self, now: i64) -> i64 {
        self.trusted.min(now)
    }
}

/// Drops what is older than `MAX_AGE_MS` before `anchor` (`Clock::anchor`),
/// then all but the newest `MAX_RECORDS`. Leaves `records` oldest first.
fn bound(records: &mut Vec<HistoryRecord>, anchor: i64) {
    records.retain(|r| r.finished_at >= anchor.saturating_sub(MAX_AGE_MS));
    records.sort_by_key(|r| r.finished_at);
    if records.len() > MAX_RECORDS {
        let extra = records.len() - MAX_RECORDS;
        records.drain(..extra);
    }
}

/// How a file on disk read.
#[derive(Debug, PartialEq, Eq)]
enum Loaded {
    /// Its records and Clear, or nothing for a file that is not there or
    /// cannot be read as a history: start empty, and write the file anew
    /// at the first record.
    Usable {
        cleared_before: Option<i64>,
        records: Vec<HistoryRecord>,
        clock: Clock,
        /// Whether `bound` dropped records the file still has: too old, or
        /// past the newest `MAX_RECORDS`. The file is then written again
        /// at once, so that it holds no more than its bounds say for
        /// longer than this launch takes to start.
        pruned: bool,
    },
    /// A newer Banager's file: start empty and never write over it.
    Newer,
}

fn load(path: &Path, now: i64) -> Loaded {
    let empty = Loaded::Usable {
        cleared_before: None,
        records: Vec::new(),
        clock: Clock {
            trusted: now,
            pending: None,
        },
        pruned: false,
    };
    let Ok(bytes) = std::fs::read(path) else {
        return empty;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return empty;
    };
    let Some(format) = value.get("format").and_then(serde_json::Value::as_u64) else {
        return empty;
    };
    if format > u64::from(HISTORY_FORMAT) {
        return Loaded::Newer;
    }
    let cleared_before = value
        .get("cleared_before")
        .and_then(serde_json::Value::as_i64);
    // One record that does not read -- cut short, or from a hand that
    // edited the file -- costs that record, not the rest.
    let mut records: Vec<HistoryRecord> = value
        .get("records")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value(item.clone()).ok())
                .collect()
        })
        .unwrap_or_default();
    let read = records.len();
    let mut clock = Clock {
        trusted: value
            .get("trusted_at")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or_else(|| age_anchor(&records, now)),
        pending: value.get("pending_at").and_then(serde_json::Value::as_i64),
    };
    clock.observe(now);
    bound(&mut records, clock.anchor(now));
    Loaded::Usable {
        cleared_before,
        pruned: records.len() < read,
        records,
        clock,
    }
}

/// Per-process counter for the staging file's name, as `settings::save`
/// has: two writes never share one.
static WRITE_TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// `bytes` into `path`: written to `<path>.tmp.<n>` beside it, then renamed
/// over it, so a crash mid-write leaves the old file or the new one, never
/// half of one. The directory is made if it is missing. A staging file
/// left by a failed write is removed.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let seq = WRITE_TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut tmp_os = path.as_os_str().to_os_string();
    tmp_os.push(format!(".tmp.{seq}"));
    let tmp_path = PathBuf::from(tmp_os);
    let written = std::fs::write(&tmp_path, bytes).and_then(|()| std::fs::rename(&tmp_path, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
    written
}

fn system_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A fresh id for this launch: 16 random bytes, in hex.
fn new_run_id() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        // Not expected on a Mac. The time still tells launches apart.
        return format!("t{}", system_now_ms());
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

struct State {
    cleared_before: Option<i64>,
    /// Oldest first.
    records: Vec<HistoryRecord>,
    /// What the records' age is measured from.
    clock: Clock,
    /// False for a newer Banager's file, which is never written over.
    writable: bool,
    /// Bumped on every change; `written` is the change the file has.
    changes: u64,
    written: u64,
    /// Writes the writer thread has begun.
    tries: u64,
    /// The last write, when it failed: what `flush` stops waiting at.
    failed: Option<FailedWrite>,
}

/// A write that failed: which of the writer's tries it was, and the change
/// it was writing.
#[derive(Clone, Copy)]
struct FailedWrite {
    try_no: u64,
    change: u64,
}

/// Whether the file still lacks a change it will be written with.
fn owed(state: &State) -> bool {
    state.writable && state.written < state.changes
}

/// The history of one launch: the records in memory, and the thread that
/// writes them to the file.
pub struct HistoryStore {
    path: PathBuf,
    run: String,
    now_fn: fn() -> i64,
    state: Mutex<State>,
    flushed: Condvar,
    wake: Mutex<mpsc::Sender<()>>,
}

impl HistoryStore {
    /// Reads `path` (`history.json` in Banager's application data
    /// directory) and starts the thread that writes it. Never fails: a
    /// file that does not read is an empty history.
    pub fn open(path: PathBuf) -> Arc<HistoryStore> {
        HistoryStore::open_with_clock(path, system_now_ms)
    }

    /// `open`, with the clock a test sets: `now_fn` answers in
    /// milliseconds since 1970.
    pub fn open_with_clock(path: PathBuf, now_fn: fn() -> i64) -> Arc<HistoryStore> {
        let now = now_fn();
        let (cleared_before, records, clock, writable, pruned) = match load(&path, now) {
            Loaded::Usable {
                cleared_before,
                records,
                clock,
                pruned,
            } => (cleared_before, records, clock, true, pruned),
            Loaded::Newer => (
                None,
                Vec::new(),
                Clock {
                    trusted: now,
                    pending: None,
                },
                false,
                false,
            ),
        };
        let (tx, rx) = mpsc::channel::<()>();
        let store = Arc::new(HistoryStore {
            path,
            run: new_run_id(),
            now_fn,
            state: Mutex::new(State {
                cleared_before,
                records,
                clock,
                writable,
                // A file with records past its bounds is written again
                // straight away, without them.
                changes: u64::from(pruned),
                written: 0,
                tries: 0,
                failed: None,
            }),
            flushed: Condvar::new(),
            wake: Mutex::new(tx),
        });
        let weak: Weak<HistoryStore> = Arc::downgrade(&store);
        let spawned = std::thread::Builder::new()
            .name("banager-history".to_string())
            .spawn(move || {
                while rx.recv().is_ok() {
                    // Several records that came together are one write.
                    while rx.try_recv().is_ok() {}
                    let Some(store) = weak.upgrade() else {
                        return;
                    };
                    store.write_now();
                }
            });
        if spawned.is_err() {
            // No thread, no file: the records stay in memory for this
            // launch, and `flush` does not wait for a write that will not
            // come.
            eprintln!("[banager] could not start the history writer; keeping history in memory");
            store.state.lock().unwrap().writable = false;
        } else if pruned {
            store.wake();
        }
        store
    }

    /// This launch's id (`HistoryRecord::run`).
    pub fn run(&self) -> &str {
        &self.run
    }

    /// Keeps the record for an operation that ended, if it is one to keep
    /// (`record_for`), and has the file written. Returns at once: the
    /// write is the writer thread's.
    pub fn record(&self, ended: &Ended<'_>, started: &Started) {
        let now = (self.now_fn)();
        let Some(record) = record_for(ended, started, &self.run, now) else {
            return;
        };
        {
            let mut state = self.state.lock().unwrap();
            // Measured from the time the history trusts, never from this
            // record's own, which is `now` and may be wrong -- nor from
            // another record stamped under the same wrong clock.
            state.clock.observe(now);
            let anchor = state.clock.anchor(now);
            state.records.push(record);
            bound(&mut state.records, anchor);
            state.changes += 1;
        }
        self.wake();
    }

    /// The Updates page's Clear: lists nothing that finished before now,
    /// from now on. The records stay.
    pub fn clear(&self) -> HistoryView {
        {
            let mut state = self.state.lock().unwrap();
            state.cleared_before = Some((self.now_fn)());
            state.changes += 1;
        }
        self.wake();
        self.view()
    }

    /// What `get_history` answers: the records, newest first, but none
    /// older than `MAX_AGE_MS` by the clock. The file can keep older ones
    /// (`age_anchor`: after more than 180 days with no update, up to 180
    /// days before its newest record); they are not listed, so an idle
    /// Mac's 「最近更新」 shows nothing older than 180 days either. A
    /// clock set far ahead lists little until it is put right, and loses
    /// nothing.
    pub fn view(&self) -> HistoryView {
        let oldest = (self.now_fn)().saturating_sub(MAX_AGE_MS);
        let state = self.state.lock().unwrap();
        HistoryView {
            run: self.run.clone(),
            cleared_before: state.cleared_before,
            records: state
                .records
                .iter()
                .rev()
                .filter(|r| r.finished_at >= oldest)
                .cloned()
                .collect(),
        }
    }

    /// Waits, at most `timeout`, until the file has every change made so
    /// far; true if it has (or if this launch never writes the file).
    /// A change still owed may be one whose write failed, which nothing
    /// else tries again before the next record or Clear: the writer thread
    /// is woken to try it now, and the wait ends with the first try begun
    /// after that, written or not. False when that try failed too, or the
    /// timeout came first: the change is still owed, and the next record,
    /// Clear or flush tries again. For tests and for Banager's exit
    /// (`src-tauri/src/history.rs`, `flush_on_exit`), never on an
    /// operation's way to its end.
    pub fn flush(&self, timeout: Duration) -> bool {
        let since = {
            let state = self.state.lock().unwrap();
            if !owed(&state) {
                return true;
            }
            state.tries
        };
        self.wake();
        let state = self.state.lock().unwrap();
        let (state, _) = self
            .flushed
            .wait_timeout_while(state, timeout, |s| {
                // A try begun before this flush that fails is not the end:
                // the one it woke the writer for is still to come.
                let failed_again = s
                    .failed
                    .is_some_and(|f| f.try_no > since && f.change == s.changes);
                owed(s) && !failed_again
            })
            .unwrap();
        !owed(&state)
    }

    fn wake(&self) {
        // A send fails only once the writer thread has gone, which it does
        // only when this store is dropped.
        let _ = self.wake.lock().unwrap().send(());
    }

    /// Writes what is in memory now. The writer thread's.
    fn write_now(&self) {
        let (bytes, change, try_no) = {
            let mut state = self.state.lock().unwrap();
            if !owed(&state) {
                return;
            }
            state.tries += 1;
            let file = HistoryFile {
                format: HISTORY_FORMAT,
                cleared_before: state.cleared_before,
                records: state.records.clone(),
                trusted_at: Some(state.clock.trusted),
                pending_at: state.clock.pending,
            };
            (serde_json::to_vec_pretty(&file), state.changes, state.tries)
        };
        let result = bytes
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            .and_then(|bytes| write_atomically(&self.path, &bytes));
        let mut state = self.state.lock().unwrap();
        match result {
            Ok(()) => {
                state.written = state.written.max(change);
                state.failed = None;
            }
            // The records stay in memory for this launch, and the change is
            // still owed: the next record, Clear or flush wakes this
            // thread, which tries the file again, and `flush` does not
            // report it written.
            Err(e) => {
                eprintln!("[banager] could not write the history file: {e}");
                state.failed = Some(FailedWrite { try_no, change });
            }
        }
        self.flushed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ArtifactKind;

    const DAY: i64 = 24 * 60 * 60 * 1_000;
    const NOW: i64 = 1_790_000_000_000;

    fn now() -> i64 {
        NOW
    }

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> TempDir {
            let dir = std::env::temp_dir().join(format!(
                "banager-history-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
        fn file(&self) -> PathBuf {
            self.0.join("history.json")
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn key(name: &str) -> ArtifactKey {
        ArtifactKey {
            instance_id: "brew:/opt/homebrew".to_string(),
            kind: ArtifactKind::Formula,
            name: name.to_string(),
        }
    }

    fn started(name: &str) -> Started {
        Started {
            display_name: name.to_string(),
            adapter_id: "brew".to_string(),
            listed_version: Some("3.31.6".to_string()),
        }
    }

    fn ended<'a>(key: &'a ArtifactKey, outcome: &'a Outcome) -> Ended<'a> {
        Ended {
            op_id: 4,
            key,
            op_kind: OpKind::Upgrade,
            outcome,
            started: true,
            before: Some("3.31.6"),
            after: Some("4.0.0"),
        }
    }

    fn record_at(name: &str, finished_at: i64) -> HistoryRecord {
        let k = key(name);
        let mut r = record_for(
            &ended(&k, &Outcome::Succeeded),
            &started(name),
            "r",
            finished_at,
        )
        .expect("an update is kept");
        r.op_id = finished_at as u64;
        r
    }

    #[test]
    fn test_an_update_that_moved_the_version_is_kept_as_verified_from_one_version_to_the_other() {
        let k = key("cmake");
        let r = record_for(
            &ended(&k, &Outcome::Succeeded),
            &started("cmake"),
            "run1",
            NOW,
        )
        .unwrap();
        assert_eq!(r.kind, HistoryKind::Update);
        assert_eq!(r.result, HistoryResult::Succeeded);
        assert!(r.verified);
        assert_eq!(r.from_version.as_deref(), Some("3.31.6"));
        assert_eq!(r.to_version.as_deref(), Some("4.0.0"));
        assert_eq!((r.run.as_str(), r.op_id, r.finished_at), ("run1", 4, NOW));
    }

    #[test]
    fn test_an_update_with_nothing_to_compare_is_kept_but_not_called_verified() {
        let k = key("cmake");
        let mut e = ended(&k, &Outcome::Succeeded);
        e.before = None;
        let r = record_for(&e, &started("cmake"), "r", NOW).unwrap();
        assert!(!r.verified);
        // The list's version stands in for the reading that did not come.
        assert_eq!(r.from_version.as_deref(), Some("3.31.6"));
    }

    #[test]
    fn test_needs_attention_is_kept_as_it_is_and_a_failed_update_has_no_new_version() {
        let k = key("cmake");
        let unchanged = Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade);
        let mut e = ended(&k, &unchanged);
        e.after = Some("3.31.6");
        let r = record_for(&e, &started("cmake"), "r", NOW).unwrap();
        assert_eq!(
            r.result,
            HistoryResult::NeedsAttention(Attention::UnchangedAfterUpgrade)
        );
        assert!(!r.verified);
        assert_eq!(r.to_version.as_deref(), Some("3.31.6"));

        let failed = Outcome::Failed {
            exit_code: Some(1),
            summary: "curl: (6) Could not resolve host: ghcr.io".to_string(),
        };
        let r = record_for(&ended(&k, &failed), &started("cmake"), "r", NOW).unwrap();
        assert_eq!(
            r.result,
            HistoryResult::Failed {
                cause: Some(FailureCause::Network)
            }
        );
        assert_eq!(r.to_version, None);
        assert!(!r.verified);
    }

    #[test]
    fn test_no_log_text_command_or_path_reaches_the_record() {
        let k = key("cmake");
        let failed = Outcome::Failed {
            exit_code: Some(1),
            summary: "Error: Permission denied @ apply2files - /Users/me/secret/thing".to_string(),
        };
        let r = record_for(&ended(&k, &failed), &started("cmake"), "r", NOW).unwrap();
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"permission\""), "{json}");
        assert!(!json.contains("apply2files"), "{json}");
        assert!(!json.contains("/Users/me"), "{json}");

        let missing = Outcome::BanagerFailed(Fault::ProgramMissing {
            program: "/Users/me/.local/bin/claude".to_string(),
        });
        let r = record_for(&ended(&k, &missing), &started("cmake"), "r", NOW).unwrap();
        assert_eq!(r.result, HistoryResult::Failed { cause: None });
        assert!(!serde_json::to_string(&r).unwrap().contains("/Users/me"));

        let waited = Outcome::BanagerFailed(Fault::HomebrewStillUpdating { minutes: 10 });
        let r = record_for(&ended(&k, &waited), &started("cmake"), "r", NOW).unwrap();
        assert_eq!(
            r.result,
            HistoryResult::Failed {
                cause: Some(FailureCause::HomebrewUpdating)
            }
        );
    }

    #[test]
    fn test_nothing_is_kept_for_an_operation_cancelled_before_its_command_started_or_for_an_install(
    ) {
        let k = key("cmake");
        let mut e = ended(&k, &Outcome::Cancelled);
        e.started = false;
        assert_eq!(record_for(&e, &started("cmake"), "r", NOW), None);
        // Stopped once it had reached its adapter's `execute`: that is kept.
        e.started = true;
        assert_eq!(
            record_for(&e, &started("cmake"), "r", NOW).map(|r| r.result),
            Some(HistoryResult::Cancelled)
        );
        let mut e = ended(&k, &Outcome::Succeeded);
        e.op_kind = OpKind::Install;
        assert_eq!(record_for(&e, &started("cmake"), "r", NOW), None);
    }

    #[test]
    fn test_an_uninstall_keeps_the_version_it_had_and_a_model_keeps_no_version() {
        let k = key("cmake");
        let mut e = ended(&k, &Outcome::Succeeded);
        e.op_kind = OpKind::Uninstall;
        e.before = None;
        e.after = None;
        let r = record_for(&e, &started("cmake"), "r", NOW).unwrap();
        assert_eq!(r.kind, HistoryKind::Uninstall);
        assert!(r.verified, "the reading after found it gone");
        assert_eq!(
            (r.from_version.as_deref(), r.to_version.as_deref()),
            (Some("3.31.6"), None)
        );

        let model = ArtifactKey {
            instance_id: "ollama:http://127.0.0.1:11434".to_string(),
            kind: ArtifactKind::Model,
            name: "qwen3:8b".to_string(),
        };
        let mut e = ended(&model, &Outcome::Succeeded);
        e.before = Some("sha256:aaaa");
        e.after = Some("sha256:bbbb");
        let r = record_for(&e, &started("qwen3:8b"), "r", NOW).unwrap();
        assert!(r.verified, "the digest moved");
        assert_eq!((r.from_version, r.to_version), (None, None));
    }

    #[test]
    fn test_record_wire_shape_matches_the_hand_written_ts_mirror() {
        // `HistoryRecord` in src/lib/types.ts; src/lib/history.test.ts
        // parses this same string.
        let k = key("cmake");
        let r = record_for(
            &ended(&k, &Outcome::Succeeded),
            &started("cmake"),
            "run1",
            NOW,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"run":"run1","op_id":4,"finished_at":1790000000000,"key":{"instance_id":"brew:/opt/homebrew","kind":"Formula","name":"cmake"},"display_name":"cmake","adapter_id":"brew","kind":"Update","from_version":"3.31.6","to_version":"4.0.0","result":"Succeeded","verified":true}"#
        );
        let failed = HistoryResult::Failed {
            cause: Some(FailureCause::DiskFull),
        };
        assert_eq!(
            serde_json::to_string(&failed).unwrap(),
            r#"{"Failed":{"cause":"diskFull"}}"#
        );
        let back: HistoryRecord =
            serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn test_records_are_appended_written_and_read_back_by_the_next_launch() {
        let dir = TempDir::new("append");
        let store = HistoryStore::open_with_clock(dir.file(), now);
        let k = key("cmake");
        store.record(&ended(&k, &Outcome::Succeeded), &started("cmake"));
        let k2 = key("git");
        let mut e = ended(&k2, &Outcome::Succeeded);
        e.op_id = 5;
        store.record(&e, &started("git"));
        assert!(store.flush(Duration::from_secs(5)));
        let first_run = store.run().to_string();
        drop(store);

        let next = HistoryStore::open_with_clock(dir.file(), now);
        assert_ne!(next.run(), first_run, "each launch has its own id");
        let view = next.view();
        assert_eq!(
            view.records
                .iter()
                .map(|r| r.key.name.as_str())
                .collect::<Vec<_>>(),
            vec!["git", "cmake"],
            "newest first"
        );
        assert!(view.records.iter().all(|r| r.run == first_run));
        assert_eq!(view.cleared_before, None);
    }

    #[test]
    fn test_the_file_keeps_the_newest_thousand_and_nothing_older_than_180_days() {
        let mut records: Vec<HistoryRecord> = (0..1_005)
            .map(|i| record_at("cmake", NOW - 1_005 + i))
            .collect();
        records.insert(0, record_at("old", NOW - 181 * DAY));
        records.push(record_at("edge", NOW - 180 * DAY));
        bound(&mut records, NOW);
        assert_eq!(records.len(), MAX_RECORDS);
        assert!(
            records.iter().all(|r| r.key.name == "cmake"),
            "the newest stay"
        );
        assert_eq!(records.last().unwrap().finished_at, NOW - 1);

        // And at load: a 200-day-old record in the file is dropped.
        let dir = TempDir::new("age");
        let file = HistoryFile {
            format: HISTORY_FORMAT,
            cleared_before: Some(NOW - DAY),
            records: vec![
                record_at("old", NOW - 200 * DAY),
                record_at("new", NOW - DAY),
            ],
            trusted_at: None,
            pending_at: None,
        };
        std::fs::write(dir.file(), serde_json::to_vec(&file).unwrap()).unwrap();
        let store = HistoryStore::open_with_clock(dir.file(), now);
        let view = store.view();
        assert_eq!(
            view.records
                .iter()
                .map(|r| r.key.name.as_str())
                .collect::<Vec<_>>(),
            vec!["new"]
        );
        assert_eq!(view.cleared_before, Some(NOW - DAY));
        // ... and the file is written again without it, with no new record.
        assert!(store.flush(Duration::from_secs(5)));
        let on_disk: HistoryFile =
            serde_json::from_slice(&std::fs::read(dir.file()).unwrap()).unwrap();
        assert_eq!(
            on_disk
                .records
                .iter()
                .map(|r| r.key.name.as_str())
                .collect::<Vec<_>>(),
            vec!["new"]
        );
        assert_eq!(on_disk.cleared_before, Some(NOW - DAY));
        drop(store);

        // A file within its bounds is left exactly as it is.
        let bytes = std::fs::read(dir.file()).unwrap();
        let again = HistoryStore::open_with_clock(dir.file(), now);
        assert!(again.flush(Duration::from_secs(5)));
        assert_eq!(std::fs::read(dir.file()).unwrap(), bytes);
    }

    fn a_year_ahead() -> i64 {
        NOW + 365 * DAY
    }

    #[test]
    fn test_a_clock_set_far_ahead_drops_no_record_at_load_or_at_a_new_one() {
        // The Mac's clock a year ahead at launch, then an update finishing
        // under it: the records of the past days stay, and the file is not
        // written again at load. Put right later, the history is whole.
        let dir = TempDir::new("clock-ahead");
        let file = HistoryFile {
            format: HISTORY_FORMAT,
            cleared_before: None,
            records: vec![
                record_at("older", NOW - 100 * DAY),
                record_at("recent", NOW - DAY),
            ],
            trusted_at: None,
            pending_at: None,
        };
        let bytes = serde_json::to_vec(&file).unwrap();
        std::fs::write(dir.file(), &bytes).unwrap();
        let store = HistoryStore::open_with_clock(dir.file(), a_year_ahead);
        assert!(store.flush(Duration::from_secs(5)));
        assert_eq!(std::fs::read(dir.file()).unwrap(), bytes, "not rewritten");
        let k = key("cmake");
        store.record(&ended(&k, &Outcome::Succeeded), &started("cmake"));
        assert!(store.flush(Duration::from_secs(5)));
        // Under the wrong clock the page lists only what is within 180
        // days of it ...
        let listed: Vec<String> = store
            .view()
            .records
            .iter()
            .map(|r| r.key.name.clone())
            .collect();
        assert_eq!(listed, vec!["cmake"]);
        drop(store);
        // ... and with the clock put right, the history is whole.
        let again = HistoryStore::open_with_clock(dir.file(), now);
        let mut names: Vec<String> = again
            .view()
            .records
            .iter()
            .map(|r| r.key.name.clone())
            .collect();
        names.sort();
        assert_eq!(names, vec!["cmake", "older", "recent"]);

        // The age rule itself still holds, from the newest record: one 181
        // days before it goes, whatever the clock says.
        let mut records = vec![
            record_at("gone", NOW - 181 * DAY),
            record_at("kept", NOW - 179 * DAY),
            record_at("newest", NOW),
        ];
        let anchor = age_anchor(&records, a_year_ahead());
        bound(&mut records, anchor);
        assert_eq!(
            records
                .iter()
                .map(|r| r.key.name.as_str())
                .collect::<Vec<_>>(),
            vec!["kept", "newest"]
        );
    }

    fn a_year_ahead_an_hour_on() -> i64 {
        a_year_ahead() + 60 * 60 * 1_000
    }

    fn a_year_and_eight_days_ahead() -> i64 {
        a_year_ahead() + 8 * DAY
    }

    /// A file of two records from the past days, as the clock had them.
    fn past_days(dir: &TempDir) {
        let file = HistoryFile {
            format: HISTORY_FORMAT,
            cleared_before: None,
            records: vec![
                record_at("older", NOW - 100 * DAY),
                record_at("recent", NOW - DAY),
            ],
            trusted_at: None,
            pending_at: None,
        };
        std::fs::write(dir.file(), serde_json::to_vec(&file).unwrap()).unwrap();
    }

    fn update(store: &HistoryStore, name: &str, op_id: OpId) {
        let k = key(name);
        let mut e = ended(&k, &Outcome::Succeeded);
        e.op_id = op_id;
        store.record(&e, &started(name));
        assert!(store.flush(Duration::from_secs(5)));
    }

    fn names_on_disk(dir: &TempDir) -> Vec<String> {
        let on_disk: HistoryFile =
            serde_json::from_slice(&std::fs::read(dir.file()).unwrap()).unwrap();
        let mut names: Vec<String> = on_disk.records.into_iter().map(|r| r.key.name).collect();
        names.sort();
        names
    }

    #[test]
    fn test_a_second_update_under_a_clock_set_far_ahead_drops_nothing_either() {
        // The first record under the wrong clock is stamped with it: it
        // must not be what the second one measures the others' age from.
        let dir = TempDir::new("clock-ahead-twice");
        past_days(&dir);
        let store = HistoryStore::open_with_clock(dir.file(), a_year_ahead);
        update(&store, "cmake", 1);
        update(&store, "git", 2);
        drop(store);
        assert_eq!(names_on_disk(&dir), vec!["cmake", "git", "older", "recent"]);
    }

    #[test]
    fn test_opening_again_under_a_clock_still_far_ahead_drops_nothing() {
        let dir = TempDir::new("clock-ahead-reopen");
        past_days(&dir);
        let store = HistoryStore::open_with_clock(dir.file(), a_year_ahead);
        update(&store, "cmake", 1);
        drop(store);
        let again = HistoryStore::open_with_clock(dir.file(), a_year_ahead_an_hour_on);
        assert!(again.flush(Duration::from_secs(5)));
        assert_eq!(names_on_disk(&dir), vec!["cmake", "older", "recent"]);
        update(&again, "git", 2);
        drop(again);
        assert_eq!(names_on_disk(&dir), vec!["cmake", "git", "older", "recent"]);
        // Put right, the whole history is listed again.
        let right = HistoryStore::open_with_clock(dir.file(), now);
        assert_eq!(right.view().records.len(), 4);
    }

    #[test]
    fn test_a_clock_that_keeps_running_from_where_it_jumped_is_believed_after_a_week() {
        // Not put right: after more than `CLOCK_CONFIRM_MS` on from the
        // time it jumped to, that time is taken as true, and what is 180
        // days older than it goes.
        let dir = TempDir::new("clock-ahead-kept");
        past_days(&dir);
        let store = HistoryStore::open_with_clock(dir.file(), a_year_ahead);
        update(&store, "cmake", 1);
        drop(store);
        let later = HistoryStore::open_with_clock(dir.file(), a_year_and_eight_days_ahead);
        update(&later, "git", 2);
        drop(later);
        assert_eq!(names_on_disk(&dir), vec!["cmake", "git"]);
    }

    #[test]
    fn test_after_half_a_year_with_no_update_the_page_lists_nothing_older_than_180_days() {
        // No update for more than 180 days: the file's age rule counts
        // from its newest record, so it may keep these, but the page
        // lists nothing older than 180 days by the clock.
        let dir = TempDir::new("idle");
        let file = HistoryFile {
            format: HISTORY_FORMAT,
            cleared_before: None,
            records: vec![
                record_at("long-ago", NOW - 300 * DAY),
                record_at("last", NOW - 200 * DAY),
            ],
            trusted_at: None,
            pending_at: None,
        };
        std::fs::write(dir.file(), serde_json::to_vec(&file).unwrap()).unwrap();
        let store = HistoryStore::open_with_clock(dir.file(), now);
        assert_eq!(store.view().records, vec![]);
        let k = key("cmake");
        store.record(&ended(&k, &Outcome::Succeeded), &started("cmake"));
        let listed: Vec<String> = store
            .view()
            .records
            .iter()
            .map(|r| r.key.name.clone())
            .collect();
        assert_eq!(listed, vec!["cmake"]);
    }

    #[test]
    fn test_a_missing_or_corrupt_file_is_an_empty_history_and_is_replaced_whole_at_the_next_record()
    {
        let dir = TempDir::new("corrupt");
        assert_eq!(
            HistoryStore::open_with_clock(dir.file(), now)
                .view()
                .records,
            vec![]
        );

        for bytes in [
            &b"{ not json"[..],
            b"[]",
            b"{\"format\":\"one\"}",
            b"\xff\xfe",
        ] {
            std::fs::write(dir.file(), bytes).unwrap();
            let store = HistoryStore::open_with_clock(dir.file(), now);
            assert_eq!(store.view().records, vec![]);
            // Opening it leaves the file alone.
            assert_eq!(std::fs::read(dir.file()).unwrap(), bytes);
        }
        let store = HistoryStore::open_with_clock(dir.file(), now);
        let k = key("cmake");
        store.record(&ended(&k, &Outcome::Succeeded), &started("cmake"));
        assert!(store.flush(Duration::from_secs(5)));
        let file: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.file()).unwrap()).unwrap();
        assert_eq!(file["format"], 1);
        assert_eq!(file["records"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_one_unreadable_record_costs_only_itself() {
        let dir = TempDir::new("one-bad");
        let good = serde_json::to_value(record_at("git", NOW - DAY)).unwrap();
        let file = serde_json::json!({
            "format": 1,
            "cleared_before": null,
            "records": [{"run": "x", "op_id": "not a number"}, good],
        });
        std::fs::write(dir.file(), serde_json::to_vec(&file).unwrap()).unwrap();
        let store = HistoryStore::open_with_clock(dir.file(), now);
        assert_eq!(store.view().records.len(), 1);
    }

    #[test]
    fn test_a_newer_banagers_file_is_never_written_over() {
        let dir = TempDir::new("newer");
        let bytes = br#"{"format":2,"records":[{"what":"a newer shape"}]}"#;
        std::fs::write(dir.file(), bytes).unwrap();
        let store = HistoryStore::open_with_clock(dir.file(), now);
        assert_eq!(store.view().records, vec![]);
        let k = key("cmake");
        store.record(&ended(&k, &Outcome::Succeeded), &started("cmake"));
        store.clear();
        assert!(store.flush(Duration::from_secs(5)));
        assert_eq!(
            store.view().records.len(),
            1,
            "kept in memory for this launch"
        );
        assert_eq!(std::fs::read(dir.file()).unwrap(), bytes);
    }

    #[test]
    fn test_the_file_is_written_by_rename_leaving_no_staging_file() {
        let dir = TempDir::new("atomic");
        let store = HistoryStore::open_with_clock(dir.file(), now);
        let k = key("cmake");
        for op_id in 0..20 {
            let mut e = ended(&k, &Outcome::Succeeded);
            e.op_id = op_id;
            store.record(&e, &started("cmake"));
        }
        assert!(store.flush(Duration::from_secs(5)));
        let names: Vec<String> = std::fs::read_dir(&dir.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["history.json".to_string()]);
        let file: HistoryFile =
            serde_json::from_slice(&std::fs::read(dir.file()).unwrap()).unwrap();
        assert_eq!(file.records.len(), 20);

        // A path whose folder cannot be made: the write fails, nothing is
        // left behind, and nothing panics.
        let blocker = dir.0.join("blocker");
        std::fs::write(&blocker, b"a file, not a folder").unwrap();
        let err = write_atomically(&blocker.join("history.json"), b"{}");
        assert!(err.is_err());
    }

    #[test]
    fn test_clear_is_kept_and_leaves_every_record() {
        let dir = TempDir::new("clear");
        let store = HistoryStore::open_with_clock(dir.file(), now);
        let k = key("cmake");
        store.record(&ended(&k, &Outcome::Succeeded), &started("cmake"));
        let view = store.clear();
        assert_eq!(view.cleared_before, Some(NOW));
        assert_eq!(view.records.len(), 1);
        assert!(store.flush(Duration::from_secs(5)));
        drop(store);
        let next = HistoryStore::open_with_clock(dir.file(), now);
        assert_eq!(next.view().cleared_before, Some(NOW));
        assert_eq!(next.view().records.len(), 1);
    }

    #[test]
    fn test_operations_finishing_together_are_all_kept_and_written_once_whole() {
        // Several operations end at the same moment, each on its own
        // thread (`OperationManager::finish` runs on the task that ran
        // it): every record lands in memory and in the file, and no
        // staging file is left.
        let dir = TempDir::new("concurrent");
        let store = HistoryStore::open_with_clock(dir.file(), now);
        let threads: Vec<_> = (0..16u64)
            .map(|op_id| {
                let store = store.clone();
                std::thread::spawn(move || {
                    let k = key(&format!("tool{op_id}"));
                    let mut e = ended(&k, &Outcome::Succeeded);
                    e.op_id = op_id;
                    store.record(&e, &started("tool"));
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert!(store.flush(Duration::from_secs(5)));
        let mut in_memory: Vec<u64> = store.view().records.iter().map(|r| r.op_id).collect();
        in_memory.sort_unstable();
        assert_eq!(in_memory, (0..16).collect::<Vec<_>>());
        let file: HistoryFile =
            serde_json::from_slice(&std::fs::read(dir.file()).unwrap()).unwrap();
        let mut on_disk: Vec<u64> = file.records.iter().map(|r| r.op_id).collect();
        on_disk.sort_unstable();
        assert_eq!(on_disk, in_memory);
        let names: Vec<String> = std::fs::read_dir(&dir.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["history.json".to_string()]);
    }

    #[test]
    fn test_a_failed_write_stays_owed_and_the_next_change_writes_it() {
        let dir = TempDir::new("owed");
        // A file where the folder should be: no write can succeed.
        let folder = dir.0.join("data");
        std::fs::write(&folder, b"a file, not a folder").unwrap();
        let store = HistoryStore::open_with_clock(folder.join("history.json"), now);
        let k = key("cmake");
        store.record(&ended(&k, &Outcome::Succeeded), &started("cmake"));
        assert!(
            !store.flush(Duration::from_millis(300)),
            "a write that failed is not reported as written"
        );
        // The folder can be made now: the next change writes both.
        std::fs::remove_file(&folder).unwrap();
        store.clear();
        assert!(store.flush(Duration::from_secs(5)));
        let file: HistoryFile =
            serde_json::from_slice(&std::fs::read(folder.join("history.json")).unwrap()).unwrap();
        assert_eq!(file.records.len(), 1);
        assert_eq!(file.cleared_before, Some(NOW));
    }

    /// Banager's exit after a write that failed, with nothing recorded
    /// since: the flush itself has the owed write tried again, and waits
    /// for that try, not for the next record or Clear.
    #[test]
    fn test_a_flush_after_a_failed_write_tries_it_again_with_no_new_change() {
        let dir = TempDir::new("retry");
        let folder = dir.0.join("data");
        std::fs::write(&folder, b"a file, not a folder").unwrap();
        let store = HistoryStore::open_with_clock(folder.join("history.json"), now);
        let k = key("cmake");
        store.record(&ended(&k, &Outcome::Succeeded), &started("cmake"));
        assert!(!store.flush(Duration::from_secs(5)));
        // The folder can be made now, and nothing new is recorded.
        std::fs::remove_file(&folder).unwrap();
        assert!(
            store.flush(Duration::from_secs(5)),
            "the flush tried the owed write again"
        );
        let file: HistoryFile =
            serde_json::from_slice(&std::fs::read(folder.join("history.json")).unwrap()).unwrap();
        assert_eq!(file.records.len(), 1);
    }

    /// A flush whose try fails again ends with that try, well inside its
    /// timeout, instead of waiting it out: Banager's exit does not wait
    /// the whole half second at every quit after one failed write.
    #[test]
    fn test_a_flush_ends_as_soon_as_its_try_has_failed_again_not_at_its_timeout() {
        let dir = TempDir::new("fails-fast");
        let folder = dir.0.join("data");
        std::fs::write(&folder, b"a file, not a folder").unwrap();
        let store = HistoryStore::open_with_clock(folder.join("history.json"), now);
        let k = key("cmake");
        store.record(&ended(&k, &Outcome::Succeeded), &started("cmake"));
        for attempt in ["the first flush", "a flush with nothing new since"] {
            let began = std::time::Instant::now();
            assert!(!store.flush(Duration::from_secs(30)), "{attempt}");
            assert!(
                began.elapsed() < Duration::from_secs(10),
                "{attempt} waited {:?}, as if for its whole timeout",
                began.elapsed()
            );
        }
    }
}
