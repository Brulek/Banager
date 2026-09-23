use crate::model::{OpStatus, Outcome};
use serde::{Deserialize, Serialize};

pub type OpId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// A line Canager itself writes into an operation's log, as opposed to a
/// line the package manager printed.
///
/// It travels as a key plus arguments, never as text, because the log
/// drawer is the one place in the app a tool's own words are shown as-is:
/// if Canager's remarks went through [`OperationEvent::Log`] they would be
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
    /// description of the failure, shown as-is like any other text Canager
    /// did not write.
    ReadFailed { stream: Stream, error: String },
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
    /// A line of Canager's own in the log. See [`LogNote`].
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
