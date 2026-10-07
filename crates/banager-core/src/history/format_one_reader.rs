//! Frozen format-1 reader from 9e683e5b^ (before GoneBeforeUpgrade).
//! Extracted types, clock and loader with comments removed; do not modernize.
//! The small rewrite harness below follows its clear/write path, so a downgrade
//! test exercises the old deserializer and numeric guard against actual new bytes.
use crate::{
    events::OpId,
    model::{AdapterId, AlreadyUpdated, ArtifactKey},
};
use serde::{Deserialize, Serialize};
use std::path::Path;
const HISTORY_FORMAT: u32 = 1;
const MAX_RECORDS: usize = 1_000;
const MAX_AGE_MS: i64 = 180 * 24 * 60 * 60 * 1_000;
const CLOCK_CONFIRM_MS: i64 = 7 * 24 * 60 * 60 * 1_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Attention {
    NotInstalledAfterInstall,
    StillInstalledAfterUninstall,
    GoneAfterUpgrade,
    UnchangedAfterUpgrade,
    BackAfterUninstall,
    NotLinkedAfterLink,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FailureCause {
    Network,
    DiskFull,
    Permission,
    Busy,
    HomebrewUpdating,
    NeedsPassword,
    PasswordNotAccepted,
    Conflict,
    NotFound,
    AppMissing,
    Unsupported,
    TimedOut,
    NotLinked,
    Changed,
    Internal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryKind {
    Update,
    Uninstall,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryResult {
    Succeeded,
    NeedsAttention(Attention),
    Failed {
        cause: Option<FailureCause>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    Unconfirmed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub run: String,
    pub op_id: OpId,
    pub finished_at: i64,
    pub key: ArtifactKey,
    pub display_name: String,
    pub adapter_id: AdapterId,
    pub kind: HistoryKind,
    pub from_version: Option<String>,
    pub to_version: Option<String>,
    pub result: HistoryResult,
    pub verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub already_updated: Option<AlreadyUpdated>,
}

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

fn age_anchor(records: &[HistoryRecord], now: i64) -> i64 {
    records
        .iter()
        .map(|r| r.finished_at)
        .max()
        .map_or(now, |newest| newest.min(now))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Clock {
    trusted: i64,
    pending: Option<i64>,
}

impl Clock {
    fn observe(&mut self, now: i64) {
        match self.pending {
            Some(pending) if now < pending => self.pending = None,
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

    fn anchor(&self, now: i64) -> i64 {
        self.trusted.min(now)
    }
}

fn bound(records: &mut Vec<HistoryRecord>, anchor: i64) {
    records.retain(|r| r.finished_at >= anchor.saturating_sub(MAX_AGE_MS));
    records.sort_by_key(|r| r.finished_at);
    if records.len() > MAX_RECORDS {
        let extra = records.len() - MAX_RECORDS;
        records.drain(..extra);
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Loaded {
    Usable {
        cleared_before: Option<i64>,
        records: Vec<HistoryRecord>,
        clock: Clock,
        pruned: bool,
    },
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
pub(super) fn clear_and_rewrite(path: &Path, now: i64) -> usize {
    match load(path, now) {
        Loaded::Newer => 0,
        Loaded::Usable { records, clock, .. } => {
            let count = records.len();
            let file = HistoryFile {
                format: HISTORY_FORMAT,
                cleared_before: Some(now),
                records,
                trusted_at: Some(clock.trusted),
                pending_at: clock.pending,
            };
            crate::atomic_file::write(path, &serde_json::to_vec_pretty(&file).unwrap()).unwrap();
            count
        }
    }
}
