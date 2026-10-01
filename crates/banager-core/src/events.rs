use crate::model::{OpStatus, Outcome};
use serde::{Deserialize, Serialize};

pub type OpId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// A line Banager itself writes into an operation's log, as opposed to a
/// line the package manager printed.
///
/// It travels as a key plus arguments, never as text, because the log
/// drawer is the one place in the app a tool's own words are shown as-is:
/// if Banager's remarks went through [`OperationEvent::Log`] they would be
/// English sentences sitting among the tool's lines, the only English a
/// Chinese user met anywhere else in a fully translated UI. The front end
/// (`LogDrawer.tsx`) looks each variant up in its locale files instead.
///
/// Every variant needs a case in `LogDrawer.tsx`'s `noteText` and copy in
/// both `en.json` and `zh-CN.json`; a variant nobody renders is a line the
/// user never sees.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogNote {
    /// An install, upgrade or uninstall is waiting for a `brew update` a
    /// refresh started to finish before running its own command -- for at
    /// most `minutes` minutes, and Cancel ends the wait at once. The copy
    /// says both, since this line is all the user sees while it lasts.
    ///
    /// `minutes` is `BrewAdapter::OP_UPDATE_WAIT` outside tests, carried
    /// here rather than hard-coded into `operations.logNote.waitingForBrewUpdate`
    /// so the two can never disagree: see `BrewAdapter::wait_for_update`,
    /// the only production call site that builds this variant.
    WaitingForBrewUpdate { minutes: u64 },
    /// Reading one of the command's streams failed, so nothing more from
    /// that stream reaches the log. `error` is the operating system's own
    /// description of the failure, shown as-is like any other text Banager
    /// did not write.
    ReadFailed { stream: Stream, error: String },
    /// A path-list uninstall moved `path` (home folder abbreviated to `~`)
    /// to the Trash; `trashed_to` is where the system put it, as
    /// `trashItemAtURL:` reported it and abbreviated the same way (a
    /// colliding name gets a suffix), so someone who wants it back knows
    /// what to look for. One per item, from `removal::execute_removal`;
    /// worded by `LogDrawer.tsx`.
    MovedToTrash { path: String, trashed_to: String },
    /// The system refused to move `path` to the Trash; `error` is its own
    /// description, shown as-is like a tool's stderr. The run stops there,
    /// and `Outcome::Failed` carries the same words as its summary. From
    /// `removal::execute_removal`; worded by `LogDrawer.tsx`.
    TrashFailed { path: String, error: String },
    /// A path-list uninstall spent its time budget between items and
    /// stopped before moving `path` (home folder abbreviated to `~`), the
    /// first listed path it had not reached: that one and every path after
    /// it are untouched, and every path moved before it has its own
    /// `MovedToTrash` line. A Cancel stops the run at the same place without
    /// this line -- the user asked for that stop, and `run_operation`
    /// reports it as `Cancelled` when it finds the launcher still there --
    /// so this is written only when the clock, not the user, ended the run.
    /// `seconds` is the budget, `Plan.timeout_secs` (`removal::TIMEOUT_SECS`
    /// outside tests), carried here rather than hard-coded into
    /// `operations.logNote.outOfTime` so the two can never disagree. From
    /// `removal::execute_removal`, which then returns `Outcome::Unconfirmed`;
    /// worded by `LogDrawer.tsx`.
    OutOfTime { path: String, seconds: u64 },
    /// A path-list uninstall moved every path on its list to the Trash,
    /// and after the pause that follows its last move `path` (home folder
    /// abbreviated to `~`), a path on that list, was there: one it moved,
    /// back again -- a copy of the tool still running can put its program
    /// folder or its download cache back -- or one it never moved, there
    /// now (`removal::left_behind`). Banager left it where it is. One per
    /// such path, in the list's order, from `removal::execute_removal`,
    /// which then returns `Outcome::NeedsAttention(Attention::
    /// BackAfterUninstall)`; worded by `LogDrawer.tsx`.
    BackAfterUninstall { path: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationEvent {
    Status {
        op_id: OpId,
        status: OpStatus,
    },
    Log {
        op_id: OpId,
        stream: Stream,
        line: String,
    },
    /// A line of Banager's own in the log. See [`LogNote`].
    Note {
        op_id: OpId,
        note: LogNote,
    },
    Finished {
        op_id: OpId,
        outcome: Outcome,
    },
}

pub trait EventSink: Send + Sync {
    fn emit(&self, event: OperationEvent);

    /// What `Session::sizes` answers has moved, for the snapshot of
    /// `round`: a round of measuring started, got further, or finished
    /// (`size::SizeMeter`). Called from the measuring thread. Nothing by
    /// default: a sink with no window to tell has nothing to do; the
    /// shell's sends the window `UiEvent::SizesChanged`.
    fn sizes_changed(&self, _round: u64) {}
}

pub struct VecSink {
    pub events: std::sync::Mutex<Vec<OperationEvent>>,
}

impl VecSink {
    pub fn new() -> VecSink {
        VecSink {
            events: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn snapshot(&self) -> Vec<OperationEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl Default for VecSink {
    fn default() -> Self {
        VecSink::new()
    }
}

impl EventSink for VecSink {
    fn emit(&self, event: OperationEvent) {
        self.events.lock().unwrap().push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_event_round_trips_through_json() {
        let event = OperationEvent::Log {
            op_id: 42,
            stream: Stream::Stdout,
            line: "hello".to_string(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        let back: OperationEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event, back);
    }

    #[test]
    fn test_note_wire_shape_is_what_the_typescript_mirror_expects() {
        // `src/lib/types.ts` hand-mirrors this: externally tagged, every
        // `LogNote` variant a one-key object carrying its data (serde's
        // external tagging of a struct variant).
        let waiting = OperationEvent::Note {
            op_id: 7,
            note: LogNote::WaitingForBrewUpdate { minutes: 10 },
        };
        assert_eq!(
            serde_json::to_string(&waiting).unwrap(),
            r#"{"Note":{"op_id":7,"note":{"WaitingForBrewUpdate":{"minutes":10}}}}"#
        );
        let failed = OperationEvent::Note {
            op_id: 7,
            note: LogNote::ReadFailed {
                stream: Stream::Stderr,
                error: "Input/output error (os error 5)".to_string(),
            },
        };
        // Same string `src/lib/types.test.ts` builds from the TS type.
        assert_eq!(
            serde_json::to_string(&failed).unwrap(),
            r#"{"Note":{"op_id":7,"note":{"ReadFailed":{"stream":"Stderr","error":"Input/output error (os error 5)"}}}}"#
        );
        // Phase 4 step C: the two lines a path-list uninstall writes, one
        // per path moved and one for a path macOS refused. Data only (a
        // path with `$HOME` abbreviated, the Trash location, the system's
        // own words); `LogDrawer.tsx` words them.
        let moved = OperationEvent::Note {
            op_id: 7,
            note: LogNote::MovedToTrash {
                path: "~/.local/share/claude".to_string(),
                trashed_to: "~/.Trash/claude".to_string(),
            },
        };
        assert_eq!(
            serde_json::to_string(&moved).unwrap(),
            r#"{"Note":{"op_id":7,"note":{"MovedToTrash":{"path":"~/.local/share/claude","trashed_to":"~/.Trash/claude"}}}}"#
        );
        let refused = OperationEvent::Note {
            op_id: 7,
            note: LogNote::TrashFailed {
                path: "~/.local/bin/claude".to_string(),
                error: "Operation not permitted".to_string(),
            },
        };
        assert_eq!(
            serde_json::to_string(&refused).unwrap(),
            r#"{"Note":{"op_id":7,"note":{"TrashFailed":{"path":"~/.local/bin/claude","error":"Operation not permitted"}}}}"#
        );
        // The third line a path-list uninstall can write: the item it
        // stopped before when its budget ran out, and that budget in
        // seconds (`Plan.timeout_secs`, threaded through like `minutes`
        // above).
        let out_of_time = OperationEvent::Note {
            op_id: 7,
            note: LogNote::OutOfTime {
                path: "~/.local/bin/claude".to_string(),
                seconds: 120,
            },
        };
        assert_eq!(
            serde_json::to_string(&out_of_time).unwrap(),
            r#"{"Note":{"op_id":7,"note":{"OutOfTime":{"path":"~/.local/bin/claude","seconds":120}}}}"#
        );
        // The fourth: a path on the list that was there when the run looked
        // once more after the pause that follows its last move.
        let back = OperationEvent::Note {
            op_id: 7,
            note: LogNote::BackAfterUninstall {
                path: "~/.local/share/claude".to_string(),
            },
        };
        assert_eq!(
            serde_json::to_string(&back).unwrap(),
            r#"{"Note":{"op_id":7,"note":{"BackAfterUninstall":{"path":"~/.local/share/claude"}}}}"#
        );
    }

    #[test]
    fn test_vec_sink_records_events_in_order() {
        let sink = VecSink::new();
        sink.emit(OperationEvent::Status {
            op_id: 1,
            status: OpStatus::Queued,
        });
        sink.emit(OperationEvent::Finished {
            op_id: 1,
            outcome: Outcome::Succeeded,
        });
        let events = sink.snapshot();
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0],
            OperationEvent::Status {
                op_id: 1,
                status: OpStatus::Queued
            }
        );
        assert_eq!(
            events[1],
            OperationEvent::Finished {
                op_id: 1,
                outcome: Outcome::Succeeded
            }
        );
    }
}
