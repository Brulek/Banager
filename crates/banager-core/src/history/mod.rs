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
//! Bounded: the newest `MAX_RECORDS`, none older than `MAX_AGE_MS`.
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

/// The file. `records` oldest first.
#[derive(Serialize, Deserialize)]
struct HistoryFile {
    format: u32,
    cleared_before: Option<i64>,
    records: Vec<HistoryRecord>,
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

/// Drops what is older than `MAX_AGE_MS` before `now`, then all but the
/// newest `MAX_RECORDS`. Leaves `records` oldest first.
fn bound(records: &mut Vec<HistoryRecord>, now: i64) {
    records.retain(|r| r.finished_at >= now.saturating_sub(MAX_AGE_MS));
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
    bound(&mut records, now);
    Loaded::Usable {
        cleared_before,
        pruned: records.len() < read,
        records,
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
    /// False for a newer Banager's file, which is never written over.
    writable: bool,
    /// Bumped on every change; `written` is the change the file has.
    changes: u64,
    written: u64,
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
        let (cleared_before, records, writable, pruned) = match load(&path, now_fn()) {
            Loaded::Usable {
                cleared_before,
                records,
                pruned,
            } => (cleared_before, records, true, pruned),
            Loaded::Newer => (None, Vec::new(), false, false),
        };
        let (tx, rx) = mpsc::channel::<()>();
        let store = Arc::new(HistoryStore {
            path,
            run: new_run_id(),
            now_fn,
            state: Mutex::new(State {
                cleared_before,
                records,
                writable,
                // A file with records past its bounds is written again
                // straight away, without them.
                changes: u64::from(pruned),
                written: 0,
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
            state.records.push(record);
            bound(&mut state.records, now);
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

    /// What `get_history` answers.
    pub fn view(&self) -> HistoryView {
        let state = self.state.lock().unwrap();
        HistoryView {
            run: self.run.clone(),
            cleared_before: state.cleared_before,
            records: state.records.iter().rev().cloned().collect(),
        }
    }

    /// Waits, at most `timeout`, until the file has every change made so
    /// far; true if it has (or if this launch never writes the file).
    /// False after a write that failed: the change is still owed, and the
    /// next record or Clear tries again. For tests and for Banager's exit
    /// (`src-tauri/src/history.rs`, `flush_on_exit`), never on an
    /// operation's way to its end.
    pub fn flush(&self, timeout: Duration) -> bool {
        let state = self.state.lock().unwrap();
        let (state, _) = self
            .flushed
            .wait_timeout_while(state, timeout, |s| s.writable && s.written < s.changes)
            .unwrap();
        !state.writable || state.written >= state.changes
    }

    fn wake(&self) {
        // A send fails only once the writer thread has gone, which it does
        // only when this store is dropped.
        let _ = self.wake.lock().unwrap().send(());
    }

    /// Writes what is in memory now. The writer thread's.
    fn write_now(&self) {
        let (bytes, change) = {
            let state = self.state.lock().unwrap();
            if !state.writable || state.written >= state.changes {
                return;
            }
            let file = HistoryFile {
                format: HISTORY_FORMAT,
                cleared_before: state.cleared_before,
                records: state.records.clone(),
            };
            (serde_json::to_vec_pretty(&file), state.changes)
        };
        let result = bytes
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            .and_then(|bytes| write_atomically(&self.path, &bytes));
        let mut state = self.state.lock().unwrap();
        match result {
            Ok(()) => state.written = state.written.max(change),
            // The records stay in memory for this launch, and the change is
            // still owed: the next record or Clear wakes this thread, which
            // tries the file again, and `flush` does not report it written.
            Err(e) => eprintln!("[banager] could not write the history file: {e}"),
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
}
