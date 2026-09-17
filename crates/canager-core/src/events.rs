use crate::model::{OpStatus, Outcome};
use serde::{Deserialize, Serialize};

pub type OpId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stream {
    Stdout,
    Stderr,
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
