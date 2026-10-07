//! What Banager did, kept across launches: one record per finished update
//! or uninstall, in `history.json` beside `settings.json` in Banager's
//! application data directory (docs/what-we-run.md, "Files Banager
//! writes"). The Updates page's 「最近的更新记录」 reads it, so an update that
//! worked -- and that Banager checked by reading the version again -- is
//! still listed after Banager is quit and opened again.
//!
//! What a record holds is in `HistoryRecord`: when, which package, from
//! which version to which, and how it ended, as a category. Never a line
//! of a log, a command line or a path other than the ones a package's key
//! already carries (an instance id names its source's prefix, which can be
//! in the home folder; the window never shows one) -- but for one line: a
//! failure that no cause names keeps the first line of the tool's error,
//! with the home folder, any login and any query masked and cut to
//! `DETAIL_CHARS` (`failure_detail`), so that 「最近的更新记录」 can still say
//! why after the window that watched it has closed (r6 y3-batch).
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
#[cfg(test)]
mod format_one_reader;

pub use failure_cause::{failure_cause, operation_failure_cause, FailureCause};

use crate::events::OpId;
use crate::follow_up::FollowUpWarning;
use crate::model::{
    AdapterId, AlreadyUpdated, ArtifactKey, ArtifactKind, Attention, Fault, OpKind, Outcome,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Condvar, Mutex, Weak};
use std::time::Duration;

/// The file's format. A file that says a higher one was written by a newer
/// Banager: this one reads none of it and never writes over it.
pub const HISTORY_FORMAT: u32 = 2;

/// JavaScript Date's inclusive millisecond range, also exactly representable.
pub fn valid_timestamp_ms(value: i64) -> bool {
    (-8_640_000_000_000_000..=8_640_000_000_000_000).contains(&value)
}

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
    /// word where one is known: read off the tool's last lines as it wrote
    /// them (`operation_failure_cause`, `Outcome::Failed`'s `cause`), or
    /// one of Banager's own faults (`record_for`). Where none is, `detail`
    /// is the first line of the tool's error, masked (`failure_detail`) --
    /// `None` where the tool wrote nothing, and in a record from before it
    /// existed (which may have a cause of `None` for any failure).
    Failed {
        cause: Option<FailureCause>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
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
    /// For an update that succeeded though its own command changed
    /// nothing, as it was already at the version its confirmed plan aimed
    /// for when its turn came: how it got there, as far as Banager saw
    /// (`AlreadyUpdated`; `OpSummary::already_updated` in ops/mod.rs).
    /// Absent for every other record, and in one written before it
    /// existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub already_updated: Option<AlreadyUpdated>,
    /// For an update that succeeded, what of its follow-up did not end as
    /// planned (`FollowUpWarning`: a `brew cleanup` that did not finish,
    /// commands left unlinked), so that 「最近的更新记录」 can still say so
    /// after a restart. Absent for every other record, and in one written
    /// before it existed; a note this build does not know is skipped, not
    /// the record (`follow_up::known_warnings`).
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "crate::follow_up::known_warnings"
    )]
    pub follow_up_warnings: Vec<FollowUpWarning>,
    /// Clear dismisses existing records, never future completions by time.
    #[serde(default)]
    pub dismissed: bool,
}

/// What the window is given (`get_history`): this launch's `run`, when the
/// list was last cleared, and every record, newest first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryView {
    pub run: String,
    /// Last Clear time, retained for legacy file migration and diagnostics.
    /// Current readers use each record's `dismissed`, never this cutoff.
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
    /// What the operation's summary says of an update already at its
    /// target when its turn came (`OpSummary::already_updated`).
    pub already_updated: Option<AlreadyUpdated>,
    pub follow_up_warnings: Vec<FollowUpWarning>,
}

/// The record for an operation that ended, or `None` for one that is not
/// kept: an install (Banager runs none), a link (`OpKind::Link`, which
/// updates and uninstalls nothing), or one cancelled before it reached
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
        OpKind::Install | OpKind::Link => return None,
    };
    if !ended.started && *ended.outcome == Outcome::Cancelled {
        return None;
    }
    let result = match ended.outcome {
        Outcome::Succeeded => HistoryResult::Succeeded,
        Outcome::Cancelled => HistoryResult::Cancelled,
        Outcome::NeedsAttention(a) => HistoryResult::NeedsAttention(*a),
        // Read as the tool wrote it, before a login was masked out of the
        // summary (`Outcome::Failed`'s `cause`), never off the summary.
        // Where it names none, the summary's first error line, masked --
        // and beside a cause whose words send a person to the tool's
        // (`FailureCause::keeps_its_line`).
        Outcome::Failed {
            cause: Some(cause),
            summary,
            ..
        } => HistoryResult::Failed {
            cause: Some(*cause),
            detail: if cause.keeps_its_line() {
                failure_detail(summary)
            } else {
                None
            },
        },
        Outcome::Failed {
            cause: None,
            summary,
            ..
        } => HistoryResult::Failed {
            cause: None,
            detail: failure_detail(summary),
        },
        Outcome::BanagerFailed(fault) => fault_result(fault),
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
        key: ArtifactKey {
            instance_id: crate::runner::redact::without_ollama_login(&ended.key.instance_id)
                .into_owned(),
            ..ended.key.clone()
        },
        display_name: started.display_name.clone(),
        adapter_id: started.adapter_id.clone(),
        kind,
        from_version,
        to_version,
        follow_up_warnings: if result == HistoryResult::Succeeded {
            ended.follow_up_warnings.clone()
        } else {
            Vec::new()
        },
        already_updated: ended
            .already_updated
            .filter(|_| result == HistoryResult::Succeeded),
        result,
        verified,
        dismissed: false,
    })
}

/// How long a kept error line may be, in characters, the `…` that ends a
/// cut one included (`failure_detail`).
pub const DETAIL_CHARS: usize = 160;

/// A failure of Banager's own, as the history keeps it: a cause for each,
/// and none of the paths or names a `Fault` carries. macOS's words for a
/// program it would not start are read like a tool's, and kept as a line
/// where they name no cause.
fn fault_result(fault: &Fault) -> HistoryResult {
    let cause = |cause| HistoryResult::Failed {
        cause: Some(cause),
        detail: None,
    };
    match fault {
        Fault::HomebrewStillUpdating { .. } => cause(FailureCause::HomebrewUpdating),
        Fault::ProgramMissing { .. } => cause(FailureCause::NotFound),
        Fault::SpawnFailed { detail } => match operation_failure_cause(detail) {
            Some(known) if known.keeps_its_line() => HistoryResult::Failed {
                cause: Some(known),
                detail: failure_detail(detail),
            },
            Some(known) => cause(known),
            None => HistoryResult::Failed {
                cause: None,
                detail: failure_detail(detail),
            },
        },
        Fault::PathChanged { .. }
        | Fault::FormulaChanged { .. }
        | Fault::HomebrewSettingsChanged
        | Fault::LinkTaken { .. }
        | Fault::LinkRollbackRisk { .. } => cause(FailureCause::Changed),
        Fault::Panicked | Fault::Internal => cause(FailureCause::Internal),
    }
}

/// The patterns `failure_detail` uses, compiled once.
struct DetailPatterns {
    escape: regex::Regex,
    error: regex::Regex,
    bookkeeping: regex::Regex,
    label: regex::Regex,
    home: regex::Regex,
    login: regex::Regex,
    query: regex::Regex,
    address_path: regex::Regex,
}

fn detail_patterns() -> &'static DetailPatterns {
    static PATTERNS: std::sync::OnceLock<DetailPatterns> = std::sync::OnceLock::new();
    PATTERNS.get_or_init(|| {
        let re = |p: &str| regex::Regex::new(p).expect("a fixed pattern compiles");
        DetailPatterns {
            escape: re(r"\x1b\[[0-9;?]*[ -/]*[@-~]"),
            error: re(r"(?i)^(?:error|fatal|npm (?:err!|error))\b|^E:"),
            bookkeeping: re(
                r"(?i)^npm (?:err!|error) (?:code|errno|syscall|path|dest|signal|command|cwd|\d{3}\s*$|a complete log|log files)",
            ),
            label: re(
                r"(?i)^(?:(?:error|fatal)\b\s*(?:\[[^\]]*\])?\s*:?|npm (?:err!|error)\b|E:)\s*",
            ),
            home: re(r"/Users/([^/\s'\x22`]+)"),
            login: re(r"([A-Za-z][A-Za-z0-9+.-]*://)[^/\s@'\x22`]+@"),
            query: re(r"([A-Za-z][A-Za-z0-9+.-]*://[^\s?#'\x22`]*)[?#][^\s'\x22`]*"),
            address_path: re(r"([A-Za-z][A-Za-z0-9+.-]*://[^/\s'\x22`]+)(/[^\s'\x22`]*)"),
        }
    })
}

/// Whether one part of an address's path looks like a token a mirror or a
/// registry put there in place of a login (`https://host/<token>/simple/`):
/// 20 characters or more, only letters, digits, `-` and `_`, with both a
/// letter and a digit. A package's name, a version or a file has a dot or
/// no digit; a commit's hash masked too costs nothing (review of r6
/// y3-batch, finding 8). `looksLikeAToken` in src/lib/failureCause.ts.
fn looks_like_a_token(part: &str) -> bool {
    part.len() >= 20
        && part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && part.chars().any(|c| c.is_ascii_digit())
        && part.chars().any(|c| c.is_ascii_alphabetic())
}

/// The one line of a failed tool's words the history keeps where no cause
/// is named: the first line that says it is an error -- `Error:`,
/// `error:`, `fatal:`, npm's `npm error` but for its bookkeeping (`code`,
/// `errno`, `path`, the log file) -- or, with none, the last line, with
/// its label taken off; and where it ends with a colon, the line after it
/// too, which says what the colon announces. Masked: the escape codes
/// that colour it, any home folder (`/Users/<name>` becomes `~`; not
/// `/Users/Shared`), any login in an address (one the runner did not
/// already mask, `runner::redact`), an address's query and fragment, and a
/// part of its path that looks like a token (`looks_like_a_token`); then
/// cut to `DETAIL_CHARS`. `None` for words that are all blank.
fn failure_detail(summary: &str) -> Option<String> {
    let p = detail_patterns();
    let lines: Vec<String> = summary
        .lines()
        .map(|line| p.escape.replace_all(line, "").trim().to_string())
        .filter(|line| !line.is_empty() && line != "[…]")
        .collect();
    let index = lines
        .iter()
        .position(|line| p.error.is_match(line) && !p.bookkeeping.is_match(line))
        .or_else(|| lines.len().checked_sub(1))?;
    let line = &lines[index];
    let unlabelled = p.label.replace(line, "");
    let mut text = if unlabelled.trim().is_empty() {
        line.to_string()
    } else {
        unlabelled.trim().to_string()
    };
    // "An exception occurred within a child process:", and the reason on
    // the next line: the two together (review of r6 y3-batch, finding 5).
    if text.ends_with(':') {
        if let Some(next) = lines.get(index + 1) {
            text = format!("{text} {next}");
        }
    }
    // `/Users/Shared` is no one's home (review of r6 y3-batch, finding 8).
    let text = p.home.replace_all(&text, |c: &regex::Captures<'_>| {
        if &c[1] == "Shared" {
            c[0].to_string()
        } else {
            "~".to_string()
        }
    });
    let text = p.login.replace_all(&text, "${1}****@");
    let text = p.query.replace_all(&text, "${1}");
    let text = p
        .address_path
        .replace_all(&text, |c: &regex::Captures<'_>| {
            let path: Vec<&str> = c[2]
                .split('/')
                .map(|part| {
                    if looks_like_a_token(part) {
                        "****"
                    } else {
                        part
                    }
                })
                .collect();
            format!("{}{}", &c[1], path.join("/"))
        });
    let text = text.trim();
    if text.chars().count() <= DETAIL_CHARS {
        return Some(text.to_string());
    }
    let mut cut: String = text.chars().take(DETAIL_CHARS - 1).collect();
    cut.push('…');
    Some(cut)
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
        /// Whether the records read differ from the file's: `bound`
        /// dropped some (too old, or past the newest `MAX_RECORDS`), or an
        /// Ollama login was taken out of a key
        /// (`redact::without_ollama_login`). The file is then written
        /// again at once, so that it holds no more than its bounds say,
        /// and no login, for longer than this launch takes to start.
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
        .and_then(serde_json::Value::as_i64)
        .filter(|value| valid_timestamp_ms(*value));
    // One record that does not read -- cut short, or from a hand that
    // edited the file -- costs that record, not the rest.
    let mut records: Vec<HistoryRecord> = value
        .get("records")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let mut record: HistoryRecord = serde_json::from_value(item.clone()).ok()?;
                    if !valid_timestamp_ms(record.finished_at) {
                        return None;
                    }
                    // Format-1 releases did not write this field. Convert their
                    // cutoff once, including when the clock has moved backwards.
                    if item.get("dismissed").is_none() {
                        record.dismissed =
                            cleared_before.is_some_and(|time| record.finished_at <= time);
                    }
                    Some(record)
                })
                .collect()
        })
        .unwrap_or_default();
    let mut redacted = false;
    for record in &mut records {
        if let std::borrow::Cow::Owned(id) =
            crate::runner::redact::without_ollama_login(&record.key.instance_id)
        {
            record.key.instance_id = id;
            redacted = true;
        }
    }
    let read = records.len();
    let mut clock = Clock {
        trusted: value
            .get("trusted_at")
            .and_then(serde_json::Value::as_i64)
            .filter(|value| valid_timestamp_ms(*value))
            .unwrap_or_else(|| age_anchor(&records, now)),
        pending: value
            .get("pending_at")
            .and_then(serde_json::Value::as_i64)
            .filter(|value| valid_timestamp_ms(*value)),
    };
    clock.observe(now);
    bound(&mut records, clock.anchor(now));
    Loaded::Usable {
        cleared_before,
        pruned: redacted || records.len() < read,
        records,
        clock,
    }
}

/// Exclusive same-directory staging, shared with settings.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    crate::atomic_file::write(path, bytes)
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
                // Rewrite records past their bounds or containing a legacy
                // Ollama login straight away, without those details.
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

    /// Clear dismisses records already present. Later records remain visible
    /// regardless of clock corrections; the dismissed records stay on disk.
    pub fn clear(&self) -> HistoryView {
        {
            let mut state = self.state.lock().unwrap();
            state.cleared_before = Some((self.now_fn)());
            for record in &mut state.records {
                record.dismissed = true;
            }
            state.changes += 1;
        }
        self.wake();
        self.view()
    }

    /// What `get_history` answers: the records, newest first, but none
    /// older than `MAX_AGE_MS` by the clock. The file can keep older ones
    /// (`age_anchor`: after more than 180 days with no update, up to 180
    /// days before its newest record); they are not listed, so an idle
    /// Mac's 「最近的更新记录」 shows nothing older than 180 days either. A
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
            already_updated: None,
            follow_up_warnings: Vec::new(),
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

    /// `history.json` as the build before format 2 (e2e6abe6) wrote it:
    /// `to_vec_pretty` of its `HistoryFile`, format 1, no `dismissed`, a
    /// Clear pressed at `NOW - DAY`, its clock fields, and one record of
    /// every result and of each optional field.
    const FORMAT_ONE_FILE: &str = r#"{
  "format": 1,
  "cleared_before": 1789913600000,
  "records": [
    {
      "run": "earlier",
      "op_id": 1,
      "finished_at": 1789740800000,
      "key": {
        "instance_id": "brew:/opt/homebrew",
        "kind": "Formula",
        "name": "cmake"
      },
      "display_name": "cmake",
      "adapter_id": "brew",
      "kind": "Update",
      "from_version": "3.31.6",
      "to_version": "4.0.0",
      "result": "Succeeded",
      "verified": true,
      "already_updated": "ByEarlierUpdate"
    },
    {
      "run": "earlier",
      "op_id": 2,
      "finished_at": 1789827200000,
      "key": {
        "instance_id": "npm:/opt/homebrew",
        "kind": "Package",
        "name": "typescript"
      },
      "display_name": "typescript",
      "adapter_id": "npm",
      "kind": "Update",
      "from_version": "5.8.3",
      "to_version": "5.9.2",
      "result": {
        "Failed": {
          "cause": null,
          "detail": "npm error code E403"
        }
      },
      "verified": false
    },
    {
      "run": "earlier",
      "op_id": 3,
      "finished_at": 1789913600000,
      "key": {
        "instance_id": "brew:/opt/homebrew",
        "kind": "Cask",
        "name": "visual-studio-code"
      },
      "display_name": "Visual Studio Code",
      "adapter_id": "brew",
      "kind": "Update",
      "from_version": "1.104.0",
      "to_version": null,
      "result": {
        "NeedsAttention": "GoneBeforeUpgrade"
      },
      "verified": false
    },
    {
      "run": "later",
      "op_id": 1,
      "finished_at": 1789956800000,
      "key": {
        "instance_id": "ollama:http://127.0.0.1:11434",
        "kind": "Model",
        "name": "llama3:latest"
      },
      "display_name": "llama3:latest",
      "adapter_id": "ollama",
      "kind": "Uninstall",
      "from_version": "365c0bd3c000",
      "to_version": null,
      "result": "Succeeded",
      "verified": true
    },
    {
      "run": "later",
      "op_id": 2,
      "finished_at": 1789960400000,
      "key": {
        "instance_id": "brew:/opt/homebrew",
        "kind": "Formula",
        "name": "git"
      },
      "display_name": "git",
      "adapter_id": "brew",
      "kind": "Update",
      "from_version": "2.50.1",
      "to_version": "2.51.0",
      "result": {
        "Failed": {
          "cause": "network"
        }
      },
      "verified": false
    },
    {
      "run": "later",
      "op_id": 3,
      "finished_at": 1789964000000,
      "key": {
        "instance_id": "pipx:/Users/someone/.local/pipx",
        "kind": "Package",
        "name": "black"
      },
      "display_name": "black",
      "adapter_id": "pipx",
      "kind": "Update",
      "from_version": "25.1.0",
      "to_version": "25.9.0",
      "result": "Cancelled",
      "verified": false
    },
    {
      "run": "later",
      "op_id": 4,
      "finished_at": 1789967600000,
      "key": {
        "instance_id": "brew:/opt/homebrew",
        "kind": "Formula",
        "name": "node"
      },
      "display_name": "node",
      "adapter_id": "brew",
      "kind": "Update",
      "from_version": "24.8.0",
      "to_version": "24.9.0",
      "result": "Unconfirmed",
      "verified": false
    }
  ],
  "trusted_at": 1789827200000,
  "pending_at": 1789967600000
}"#;

    #[test]
    fn test_a_format_one_file_of_the_previous_build_reads_whole_and_is_never_read_back_by_it() {
        let dir = TempDir::new("format-one-real");
        std::fs::write(dir.file(), FORMAT_ONE_FILE).unwrap();
        let written: serde_json::Value = serde_json::from_str(FORMAT_ONE_FILE).unwrap();
        let cleared = written["cleared_before"].as_i64().unwrap();
        // Each record as this build hands it on: every field the earlier
        // one wrote, plus `dismissed` from the kept Clear (at or before it).
        let expected = |item: &serde_json::Value| {
            let mut item = item.clone();
            let at = item["finished_at"].as_i64().unwrap();
            item["dismissed"] = serde_json::json!(at <= cleared);
            item
        };
        let wanted: Vec<serde_json::Value> = written["records"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .map(expected)
            .collect();
        let store = HistoryStore::open_with_clock(dir.file(), now);
        let view = serde_json::to_value(store.view()).unwrap();
        assert_eq!(view["cleared_before"], cleared);
        assert_eq!(view["records"].as_array().unwrap(), &wanted);
        let dismissed: Vec<bool> = store.view().records.iter().map(|r| r.dismissed).collect();
        assert_eq!(dismissed, [false, false, false, false, true, true, true]);

        // The next write is format 2 with each dismissal spelled out, and
        // reads back the same whatever the kept Clear time says.
        let k = key("wget");
        let mut e = ended(&k, &Outcome::Succeeded);
        e.op_id = 9;
        store.record(&e, &started("wget"));
        assert!(store.flush(Duration::from_secs(5)));
        let bytes = std::fs::read(dir.file()).unwrap();
        let file: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(file["format"], 2);
        assert_eq!(file["cleared_before"], cleared);
        assert_eq!(file["trusted_at"], written["trusted_at"]);
        assert!(file["records"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["dismissed"].is_boolean()));
        let reopened = HistoryStore::open_with_clock(dir.file(), now);
        let again = serde_json::to_value(reopened.view()).unwrap();
        assert_eq!(again["records"].as_array().unwrap()[1..], wanted[..]);
        assert_eq!(again["records"][0]["key"]["name"], "wget");
        assert_eq!(again["records"][0]["dismissed"], false);

        // A format-1 reader -- the earlier build's own guard -- leaves it
        // as it is.
        assert_eq!(format_one_reader::clear_and_rewrite(&dir.file(), NOW), 0);
        assert_eq!(std::fs::read(dir.file()).unwrap(), bytes);
    }

    #[test]
    fn test_f2_clear_survives_backward_clock_and_legacy_format_one() {
        let dir = TempDir::new("f2-clock");
        // Exactly the existing format: no dismissal field, optional clock fields absent.
        let mut old = serde_json::to_value(record_at("old", NOW + DAY)).unwrap();
        old.as_object_mut().unwrap().remove("dismissed");
        std::fs::write(
            dir.file(),
            serde_json::to_vec(&serde_json::json!({
                "format": 1, "cleared_before": NOW + DAY, "records": [old]
            }))
            .unwrap(),
        )
        .unwrap();
        let store = HistoryStore::open_with_clock(dir.file(), now);
        let k = key("new");
        store.record(&ended(&k, &Outcome::Succeeded), &started("new"));
        let wire = serde_json::to_value(store.view()).unwrap();
        assert_eq!(wire["records"][0]["dismissed"], true);
        assert_eq!(wire["records"][1]["dismissed"], false);
        store.clear(); // Corrected time must not resurrect the advanced-clock record.
        assert!(store.flush(Duration::from_secs(5)));
        let next = HistoryStore::open_with_clock(dir.file(), now);
        for r in serde_json::to_value(next.view()).unwrap()["records"]
            .as_array()
            .unwrap()
        {
            assert_eq!(r["dismissed"], true);
        }
        let k = key("later");
        let mut e = ended(&k, &Outcome::Succeeded);
        e.op_id = 5;
        next.record(&e, &started("later"));
        assert!(next.flush(Duration::from_secs(5)));
        let reopened = HistoryStore::open_with_clock(dir.file(), now);
        let wire = serde_json::to_value(reopened.view()).unwrap();
        assert_eq!(wire["records"][1]["dismissed"], false);
    }

    #[test]
    fn test_f4_bad_dates_do_not_cost_valid_history() {
        let dir = TempDir::new("f4-dates");
        let good = serde_json::to_value(record_at("good", NOW)).unwrap();
        let mut bad = good.clone();
        bad["finished_at"] = serde_json::json!(9_000_000_000_000_000_i64);
        std::fs::write(
            dir.file(),
            serde_json::to_vec(&serde_json::json!({
                "format": 1, "cleared_before": 9_000_000_000_000_000_i64,
                "trusted_at": i64::MAX, "pending_at": i64::MIN,
                "records": [bad, good]
            }))
            .unwrap(),
        )
        .unwrap();
        let store = HistoryStore::open_with_clock(dir.file(), now);
        assert_eq!(store.view().records.len(), 1);
        assert_eq!(store.view().cleared_before, None);
    }

    #[test]
    fn test_f5_current_enum_values_require_a_newer_format() {
        let dir = TempDir::new("f5-downgrade");
        let mut gone = record_at("gone", NOW);
        gone.result = HistoryResult::NeedsAttention(Attention::GoneBeforeUpgrade);
        let mut failed = record_at("failed", NOW);
        failed.result = HistoryResult::Failed {
            cause: Some(FailureCause::DiskFull),
            detail: None,
        };
        // Existing releases wrote these shapes as format 1. Keep reading them.
        std::fs::write(
            dir.file(),
            serde_json::to_vec(&serde_json::json!({
                "format": 1, "cleared_before": null, "records": [gone, failed]
            }))
            .unwrap(),
        )
        .unwrap();
        let store = HistoryStore::open_with_clock(dir.file(), now);
        assert_eq!(store.view().records.len(), 2);
        store.clear();
        assert!(store.flush(Duration::from_secs(5)));
        let bytes = std::fs::read(dir.file()).unwrap();
        let file: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // The actual previous reader checks this before attempting typed records.
        assert!(file["format"].as_u64().unwrap() > 1);
        assert_eq!(
            file["records"][0]["result"]["NeedsAttention"],
            "GoneBeforeUpgrade"
        );
        format_one_reader::clear_and_rewrite(&dir.file(), NOW);
        assert_eq!(
            std::fs::read(dir.file()).unwrap(),
            bytes,
            "downgrade leaves newer bytes untouched"
        );
        let next = HistoryStore::open_with_clock(dir.file(), now);
        assert_eq!(next.view().records.len(), 2);
        // Prove this fixture really has the former defect for the former shape.
        let mut legacy = file;
        legacy["format"] = serde_json::json!(1);
        std::fs::write(dir.file(), serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(format_one_reader::clear_and_rewrite(&dir.file(), NOW), 1);
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

    /// What f13b keeps of a successful update's follow-ups (F4 of
    /// r13-failures), as the store writes it and reads it back after a
    /// restart, beside a record as 7d904368 (before the field) wrote it.
    #[test]
    fn test_follow_up_warnings_are_written_and_read_back_beside_an_older_record() {
        use crate::follow_up::FollowUpWarning;
        let dir = TempDir::new("follow-up");
        // A record exactly as the build before this field wrote it.
        let earlier = r#"{"run":"t1","op_id":2,"finished_at":1789000000000,"key":{"instance_id":"brew:/opt/homebrew","kind":"Formula","name":"jq"},"display_name":"jq","adapter_id":"brew","kind":"Update","from_version":"1.7","to_version":"1.7.1","result":"Succeeded","verified":true}"#;
        std::fs::write(
            dir.file(),
            format!(r#"{{"format":1,"trusted_at":{NOW},"records":[{earlier}]}}"#),
        )
        .unwrap();
        let warnings = vec![
            FollowUpWarning::OldVersionsNotCleanedUp {
                name: "node@22".into(),
                exit_code: Some(1),
            },
            FollowUpWarning::NoLongerLinked {
                name: "node@22".into(),
                commands: vec!["node".into(), "npm".into()],
            },
        ];
        {
            let store = HistoryStore::open_with_clock(dir.file(), || NOW);
            assert_eq!(store.view().records.len(), 1);
            assert!(store.view().records[0].follow_up_warnings.is_empty());
            let k = key("node@22");
            let mut e = ended(&k, &Outcome::Succeeded);
            e.follow_up_warnings = warnings.clone();
            store.record(&e, &started("node@22"));
            assert!(store.flush(Duration::from_secs(5)));
        }
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.file()).unwrap()).unwrap();
        assert_eq!(
            written["format"], HISTORY_FORMAT,
            "written in the current format, as every write is"
        );
        let records = written["records"].as_array().unwrap();
        let by_name = |name: &str| {
            records
                .iter()
                .find(|r| r["key"]["name"] == name)
                .unwrap()
                .clone()
        };
        assert_eq!(
            by_name("node@22")["follow_up_warnings"],
            serde_json::json!([
                {"OldVersionsNotCleanedUp": {"name": "node@22", "exit_code": 1}},
                {"NoLongerLinked": {"name": "node@22", "commands": ["node", "npm"]}}
            ])
        );
        // The older record is written back as it was but for Clear's
        // `dismissed`, which every record now carries: no warnings key.
        let mut as_it_was = serde_json::from_str::<serde_json::Value>(earlier).unwrap();
        as_it_was["dismissed"] = serde_json::json!(false);
        assert_eq!(by_name("jq"), as_it_was);
        let reopened = HistoryStore::open_with_clock(dir.file(), || NOW);
        let view = reopened.view();
        let node = view
            .records
            .iter()
            .find(|r| r.key.name == "node@22")
            .unwrap();
        assert_eq!(node.result, HistoryResult::Succeeded);
        assert_eq!(node.follow_up_warnings, warnings);
    }

    /// A build older than f13b reads `follow_up_warnings` as a field it
    /// does not know, and a record keeps reading wherever a newer build
    /// adds another: `HistoryRecord` must never deny unknown fields.
    #[test]
    fn test_a_record_with_a_field_this_build_does_not_know_still_reads() {
        let dir = TempDir::new("unknown-field");
        let record = r#"{"run":"t1","op_id":2,"finished_at":1789000000000,"key":{"instance_id":"brew:/opt/homebrew","kind":"Formula","name":"jq"},"display_name":"jq","adapter_id":"brew","kind":"Update","from_version":"1.7","to_version":"1.7.1","result":"Succeeded","verified":true,"a_later_field":{"x":[1]}}"#;
        std::fs::write(
            dir.file(),
            format!(r#"{{"format":1,"trusted_at":{NOW},"records":[{record}]}}"#),
        )
        .unwrap();
        let store = HistoryStore::open_with_clock(dir.file(), || NOW);
        assert_eq!(store.view().records.len(), 1);
    }

    /// A follow-up a later build knows and this one does not costs that
    /// note, never the record (`load` drops a record that does not read).
    #[test]
    fn test_a_follow_up_warning_this_build_does_not_know_drops_only_that_warning() {
        use crate::follow_up::FollowUpWarning;
        let dir = TempDir::new("unknown-warning");
        let record = r#"{"run":"t1","op_id":2,"finished_at":1789000000000,"key":{"instance_id":"brew:/opt/homebrew","kind":"Formula","name":"cmake"},"display_name":"cmake","adapter_id":"brew","kind":"Update","from_version":"3.31.6","to_version":"4.0.0","result":"Succeeded","verified":true,"follow_up_warnings":[{"SomethingLater":{"name":"cmake"}},{"OldVersionsNotCleanedUp":{"name":"cmake","exit_code":null}},"AnotherShape"]}"#;
        std::fs::write(
            dir.file(),
            format!(r#"{{"format":1,"trusted_at":{NOW},"records":[{record}]}}"#),
        )
        .unwrap();
        let store = HistoryStore::open_with_clock(dir.file(), || NOW);
        let view = store.view();
        assert_eq!(view.records.len(), 1);
        assert_eq!(
            view.records[0].follow_up_warnings,
            vec![FollowUpWarning::OldVersionsNotCleanedUp {
                name: "cmake".into(),
                exit_code: None,
            }]
        );
    }

    /// As `already_updated`: the page shows follow-ups for a success
    /// only, so only a success keeps them.
    #[test]
    fn test_follow_up_warnings_are_kept_only_with_a_success() {
        use crate::follow_up::FollowUpWarning;
        let k = key("node@22");
        let failed = Outcome::Failed {
            exit_code: Some(1),
            summary: "Error: node@22: an unexpected error".to_string(),
            cause: None,
        };
        let mut e = ended(&k, &failed);
        e.follow_up_warnings = vec![FollowUpWarning::NoLongerLinked {
            name: "node@22".into(),
            commands: vec!["node".into()],
        }];
        let r = record_for(&e, &started("node@22"), "r", NOW).unwrap();
        assert!(matches!(r.result, HistoryResult::Failed { .. }));
        assert!(r.follow_up_warnings.is_empty());
        let json = serde_json::to_value(&r).unwrap();
        assert!(json.get("follow_up_warnings").is_none());
    }

    #[test]
    fn test_an_update_already_at_its_target_is_kept_as_succeeded_and_says_how() {
        // r6 y3-batch, finding 2: libpng, upgraded by harfbuzz's update
        // earlier in the same Update all, read 1.6.59 before its own
        // command and after it.
        let k = key("libpng");
        let mut e = ended(&k, &Outcome::Succeeded);
        e.before = Some("1.6.59");
        e.after = Some("1.6.59");
        e.already_updated = Some(AlreadyUpdated::ByEarlierUpdate);
        let r = record_for(&e, &started("libpng"), "r", NOW).unwrap();
        assert_eq!(r.result, HistoryResult::Succeeded);
        assert_eq!(r.already_updated, Some(AlreadyUpdated::ByEarlierUpdate));
        assert_eq!(r.to_version.as_deref(), Some("1.6.59"));
        // This update's own command did not move it: not "read the change".
        assert!(!r.verified);
        let json = serde_json::to_string(&r).unwrap();
        assert!(
            json.contains(
                r#""result":"Succeeded","verified":false,"already_updated":"ByEarlierUpdate""#
            ),
            "{json}"
        );
        // A record from before the field existed reads as none.
        let older = json.replace(r#","already_updated":"ByEarlierUpdate""#, "");
        let back: HistoryRecord = serde_json::from_str(&older).unwrap();
        assert_eq!(back.already_updated, None);
        // Only an update that succeeded says it.
        let unchanged = Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade);
        let mut e = ended(&k, &unchanged);
        e.already_updated = Some(AlreadyUpdated::BeforeItsTurn);
        let r = record_for(&e, &started("libpng"), "r", NOW).unwrap();
        assert_eq!(r.already_updated, None);
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
            cause: Some(FailureCause::Network),
        };
        let r = record_for(&ended(&k, &failed), &started("cmake"), "r", NOW).unwrap();
        assert_eq!(
            r.result,
            HistoryResult::Failed {
                cause: Some(FailureCause::Network),
                detail: None
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
            cause: Some(FailureCause::Permission),
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
        assert_eq!(
            r.result,
            HistoryResult::Failed {
                cause: Some(FailureCause::NotFound),
                detail: None
            }
        );
        assert!(!serde_json::to_string(&r).unwrap().contains("/Users/me"));

        let waited = Outcome::BanagerFailed(Fault::HomebrewStillUpdating { minutes: 10 });
        let r = record_for(&ended(&k, &waited), &started("cmake"), "r", NOW).unwrap();
        assert_eq!(
            r.result,
            HistoryResult::Failed {
                cause: Some(FailureCause::HomebrewUpdating),
                detail: None
            }
        );
    }

    fn failed_with(summary: &str) -> Outcome {
        Outcome::Failed {
            exit_code: Some(1),
            summary: summary.to_string(),
            cause: crate::history::operation_failure_cause(summary),
        }
    }

    fn kept_result(outcome: &Outcome) -> HistoryResult {
        let k = key("claudebar");
        record_for(&ended(&k, outcome), &started("claudebar"), "r", NOW)
            .unwrap()
            .result
    }

    #[test]
    fn test_every_failure_keeps_a_cause_and_one_no_cause_names_keeps_its_first_error_line() {
        // r6 y3-batch, finding 3: Claudebar's and OnyX's updates were kept
        // as failed with no cause, and once the window closed nothing said
        // why. A cause, where the tool's words name one ...
        assert_eq!(
            kept_result(&failed_with(
                "Error: It seems the App source '/Applications/Claudebar.app' is not there."
            )),
            HistoryResult::Failed {
                cause: Some(FailureCause::AppMissing),
                detail: None
            }
        );
        // ... and otherwise the first line that says what went wrong, its
        // label off, the rest of the summary dropped.
        assert_eq!(
            kept_result(&failed_with(
                "==> Purging files for version 0.2.0 of Cask claudebar\n\
                 Error: SHA256 mismatch\n\
                 Expected: 1f2e\n\
                   Actual: 3a4b"
            )),
            HistoryResult::Failed {
                cause: None,
                detail: Some("SHA256 mismatch".to_string())
            }
        );
        // npm's bookkeeping lines are not what went wrong.
        assert_eq!(
            kept_result(&failed_with(
                "npm error code ETARGET\nnpm error notarget No matching version found for typescript@99.\nnpm error A complete log of this run can be found in: /Users/me/.npm/_logs/x.log"
            )),
            HistoryResult::Failed {
                cause: None,
                detail: Some("notarget No matching version found for typescript@99.".to_string())
            }
        );
        // With no error line, the last line the tool wrote.
        assert_eq!(
            kept_result(&failed_with("Building wheel\nsomething odd happened")),
            HistoryResult::Failed {
                cause: None,
                detail: Some("something odd happened".to_string())
            }
        );
        // A tool that said nothing keeps nothing to quote.
        assert_eq!(
            kept_result(&failed_with("  \n")),
            HistoryResult::Failed {
                cause: None,
                detail: None
            }
        );
    }

    #[test]
    fn test_a_cause_that_says_the_line_names_what_keeps_the_line_too() {
        // Review of r6 y3-batch, finding 4. A conflict, something missing
        // and a Mac it does not support each say the tool's words name what
        // -- which file, what is missing, what it needs -- and once the
        // window closes the log that had them is gone: the first error line
        // is kept beside the cause.
        assert_eq!(
            kept_result(&failed_with(
                "Error: It seems there is already an App at '/Applications/Foo.app'."
            )),
            HistoryResult::Failed {
                cause: Some(FailureCause::Conflict),
                detail: Some(
                    "It seems there is already an App at '/Applications/Foo.app'.".to_string()
                )
            }
        );
        assert_eq!(
            kept_result(&failed_with("env: node: No such file or directory")),
            HistoryResult::Failed {
                cause: Some(FailureCause::NotFound),
                detail: Some("env: node: No such file or directory".to_string())
            }
        );
        assert_eq!(
            kept_result(&failed_with(
                "Error: onyx: This cask does not run on macOS versions older than Tahoe."
            )),
            HistoryResult::Failed {
                cause: Some(FailureCause::Unsupported),
                detail: Some(
                    "onyx: This cask does not run on macOS versions older than Tahoe.".to_string()
                )
            }
        );
        // Masked as any kept line is.
        let HistoryResult::Failed { detail, .. } = kept_result(&failed_with(
            "npm error code E404\nnpm error 404 Not Found - GET https://me:pw@registry.example/foo?x=1",
        )) else {
            panic!("a failure");
        };
        assert_eq!(
            detail.as_deref(),
            Some("404 Not Found - GET https://****@registry.example/foo")
        );
        // A cause whose words say it all keeps no line.
        assert_eq!(
            kept_result(&failed_with("Error: Failed to download resource \"jq\"")),
            HistoryResult::Failed {
                cause: Some(FailureCause::Network),
                detail: None
            }
        );
    }

    #[test]
    fn test_the_kept_line_reads_every_shared_case_as_the_window_does() {
        // `failure_detail_cases.json` is read by src/lib/failureCause.test.ts
        // too, so a failure says the same before a restart and after it.
        #[derive(serde::Deserialize)]
        struct Case {
            name: String,
            summary: String,
            detail: Option<String>,
        }
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("failure_detail_cases.json")).expect("cases parse");
        assert!(cases.len() >= 8, "the shared cases are all there");
        for case in cases {
            assert_eq!(failure_detail(&case.summary), case.detail, "{}", case.name);
        }
    }

    #[test]
    fn test_a_kept_error_line_has_the_home_folder_logins_and_queries_masked_and_is_short() {
        let HistoryResult::Failed { detail, .. } = kept_result(&failed_with(
            "Error: \u{1b}[31mcannot read\u{1b}[0m /Users/brulek/Library/Caches/x and /Users/other/y via https://me:secret@mirror.example/simple/?token=abc#frag",
        )) else {
            panic!("a failure");
        };
        let detail = detail.expect("a line");
        assert_eq!(
            detail,
            "cannot read ~/Library/Caches/x and ~/y via https://****@mirror.example/simple/"
        );
        let long = format!("Error: {}", "x".repeat(400));
        let HistoryResult::Failed { detail, .. } = kept_result(&failed_with(&long)) else {
            panic!("a failure");
        };
        let detail = detail.expect("a line");
        assert_eq!(detail.chars().count(), DETAIL_CHARS);
        assert!(detail.ends_with('…'));
    }

    #[test]
    fn test_banagers_own_failures_keep_a_cause_of_their_own() {
        let program_missing = Outcome::BanagerFailed(Fault::ProgramMissing {
            program: "/opt/homebrew/bin/npm".to_string(),
        });
        let cases = [
            (program_missing, Some(FailureCause::NotFound), None),
            (
                Outcome::BanagerFailed(Fault::SpawnFailed {
                    detail: "Permission denied (os error 13)".to_string(),
                }),
                Some(FailureCause::Permission),
                None,
            ),
            (
                Outcome::BanagerFailed(Fault::SpawnFailed {
                    detail: "Exec format error (os error 8)".to_string(),
                }),
                None,
                Some("Exec format error (os error 8)".to_string()),
            ),
            (
                Outcome::BanagerFailed(Fault::PathChanged {
                    path: "~/.local/bin/claude".to_string(),
                }),
                Some(FailureCause::Changed),
                None,
            ),
            (
                Outcome::BanagerFailed(Fault::FormulaChanged {
                    name: "node".to_string(),
                }),
                Some(FailureCause::Changed),
                None,
            ),
            (
                Outcome::BanagerFailed(Fault::HomebrewSettingsChanged),
                Some(FailureCause::Changed),
                None,
            ),
            (
                Outcome::BanagerFailed(Fault::LinkTaken {
                    name: "node@22".to_string(),
                    paths: vec!["/opt/homebrew/bin/npm".to_string()],
                }),
                Some(FailureCause::Changed),
                None,
            ),
            (
                Outcome::BanagerFailed(Fault::Panicked),
                Some(FailureCause::Internal),
                None,
            ),
            (
                Outcome::BanagerFailed(Fault::Internal),
                Some(FailureCause::Internal),
                None,
            ),
        ];
        for (outcome, cause, detail) in cases {
            assert_eq!(
                kept_result(&outcome),
                HistoryResult::Failed { cause, detail },
                "{outcome:?}"
            );
        }
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
        // Nor a link: it updates and uninstalls nothing.
        e.op_kind = OpKind::Link;
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
            r#"{"run":"run1","op_id":4,"finished_at":1790000000000,"key":{"instance_id":"brew:/opt/homebrew","kind":"Formula","name":"cmake"},"display_name":"cmake","adapter_id":"brew","kind":"Update","from_version":"3.31.6","to_version":"4.0.0","result":"Succeeded","verified":true,"dismissed":false}"#
        );
        let failed = HistoryResult::Failed {
            cause: Some(FailureCause::DiskFull),
            detail: None,
        };
        assert_eq!(
            serde_json::to_string(&failed).unwrap(),
            r#"{"Failed":{"cause":"diskFull"}}"#
        );
        // A line where no cause is named; one from before it existed reads.
        let other = HistoryResult::Failed {
            cause: None,
            detail: Some("SHA256 mismatch".to_string()),
        };
        assert_eq!(
            serde_json::to_string(&other).unwrap(),
            r#"{"Failed":{"cause":null,"detail":"SHA256 mismatch"}}"#
        );
        assert_eq!(
            serde_json::from_str::<HistoryResult>(r#"{"Failed":{"cause":null}}"#).unwrap(),
            HistoryResult::Failed {
                cause: None,
                detail: None
            }
        );
        let back: HistoryRecord =
            serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn test_ollama_login_is_redacted_before_history_is_kept_or_written() {
        for host in [
            "http://alice:secret@server:11434",
            "http://alice:s%40cret@server:11434",
            "http://token@server:11434",
            "http://:secret@server:11434",
        ] {
            let dir = TempDir::new("ollama-login");
            let store = HistoryStore::open_with_clock(dir.file(), now);
            let k = ArtifactKey {
                instance_id: format!("ollama:{host}"),
                kind: ArtifactKind::Model,
                name: "llama3:latest".to_string(),
            };
            for op_kind in [OpKind::Upgrade, OpKind::Uninstall] {
                let mut e = ended(&k, &Outcome::Succeeded);
                e.op_kind = op_kind;
                store.record(
                    &e,
                    &Started {
                        adapter_id: "ollama".to_string(),
                        ..started("llama3:latest")
                    },
                );
            }
            assert_eq!(
                store.view().records[0].key.instance_id,
                "ollama:http://server:11434"
            );
            assert!(store.flush(Duration::from_secs(5)));
            let disk = std::fs::read_to_string(dir.file()).unwrap();
            for secret in ["alice", "secret", "s%40cret", "token@"] {
                assert!(!disk.contains(secret), "history retained {secret}");
            }
            assert_eq!(
                k.instance_id,
                format!("ollama:{host}"),
                "live identity is unchanged"
            );
        }
    }

    #[test]
    fn test_old_ollama_history_is_redacted_on_read_and_rewritten() {
        let dir = TempDir::new("old-ollama-login");
        let mut record = record_at("llama3:latest", NOW);
        record.key.instance_id = "ollama:http://alice:secret@server:11434".to_string();
        record.key.kind = ArtifactKind::Model;
        record.adapter_id = "ollama".to_string();
        std::fs::write(
            dir.file(),
            serde_json::to_vec(&serde_json::json!({
                "format": HISTORY_FORMAT, "records": [record]
            }))
            .unwrap(),
        )
        .unwrap();
        let store = HistoryStore::open_with_clock(dir.file(), now);
        assert_eq!(
            store.view().records[0].key.instance_id,
            "ollama:http://server:11434"
        );
        assert!(store.flush(Duration::from_secs(5)));
        assert!(!std::fs::read_to_string(dir.file())
            .unwrap()
            .contains("secret"));
    }

    /// A file with no Ollama login is read as it is and not written again
    /// for one: an `@` in another source's id, or an Ollama id with no
    /// login, is not one.
    #[test]
    fn test_history_without_an_ollama_login_is_read_as_it_is_and_not_rewritten() {
        let dir = TempDir::new("no-ollama-login");
        let mut pip = record_at("requests", NOW);
        pip.key.instance_id = "pip:/opt/homebrew/opt/python@3.13/bin/python3.13".to_string();
        pip.key.kind = ArtifactKind::Package;
        pip.adapter_id = "pip".to_string();
        let mut model = record_at("llama3:latest", NOW - DAY);
        model.key.instance_id = "ollama:http://127.0.0.1:11434".to_string();
        model.key.kind = ArtifactKind::Model;
        model.adapter_id = "ollama".to_string();
        std::fs::write(
            dir.file(),
            serde_json::to_vec(&serde_json::json!({
                "format": HISTORY_FORMAT, "trusted_at": NOW, "records": [model, pip]
            }))
            .unwrap(),
        )
        .unwrap();
        match load(&dir.file(), NOW) {
            Loaded::Usable {
                records, pruned, ..
            } => {
                assert_eq!(records, vec![model, pip]);
                assert!(!pruned, "nothing to write again");
            }
            Loaded::Newer => panic!("the file is this format"),
        }
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
        assert_eq!(file["format"], HISTORY_FORMAT);
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
        let bytes = br#"{"format":3,"records":[{"what":"a newer shape"}]}"#;
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
