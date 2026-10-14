//! Follow-up failures do not undo a successful version update. Keep only
//! these structured notes, never the command transcript, across launches.
use crate::events::{EventSink, LogNote, OpId, OperationEvent};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FollowUpWarning {
    OldVersionsNotCleanedUp {
        name: String,
        exit_code: Option<i32>,
    },
    NoLongerLinked {
        name: String,
        commands: Vec<String>,
    },
    /// An update installed of a formula set up to run with `brew services`
    /// when it was previewed (`Warning::HomebrewServiceMayNeedRestart`): a
    /// service still running runs the old version until it is restarted.
    ServiceMayNeedRestart {
        name: String,
    },
}

/// `HistoryRecord::follow_up_warnings` as `history.json` holds it: each
/// note this build knows, skipping any other -- one a later build added --
/// so that a record is never lost for a note it cannot name (`load` drops
/// a record that does not read, and the next write leaves it out).
pub(crate) fn known_warnings<'de, D>(deserializer: D) -> Result<Vec<FollowUpWarning>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|item| serde_json::from_value(item).ok())
            .collect(),
        _ => Vec::new(),
    })
}

pub(crate) struct WarningSink {
    inner: Arc<dyn EventSink>,
    op_id: OpId,
    warnings: Mutex<Vec<FollowUpWarning>>,
}

impl WarningSink {
    pub fn new(inner: Arc<dyn EventSink>, op_id: OpId) -> Self {
        Self {
            inner,
            op_id,
            warnings: Mutex::new(Vec::new()),
        }
    }
    pub fn warnings(&self) -> Vec<FollowUpWarning> {
        self.warnings.lock().unwrap().clone()
    }
}

impl EventSink for WarningSink {
    fn emit(&self, event: OperationEvent) {
        if let OperationEvent::Note { op_id, note } = &event {
            if *op_id == self.op_id {
                let warning = match note {
                    LogNote::OldVersionsNotCleanedUp { name, exit_code } => {
                        Some(FollowUpWarning::OldVersionsNotCleanedUp {
                            name: name.clone(),
                            exit_code: *exit_code,
                        })
                    }
                    LogNote::NoLongerLinked { name, commands } => {
                        Some(FollowUpWarning::NoLongerLinked {
                            name: name.clone(),
                            commands: commands.clone(),
                        })
                    }
                    LogNote::ServiceMayNeedRestart { name } => {
                        Some(FollowUpWarning::ServiceMayNeedRestart { name: name.clone() })
                    }
                    _ => None,
                };
                if let Some(warning) = warning {
                    let mut warnings = self.warnings.lock().unwrap();
                    // One cleanup, one relink and one service reminder per
                    // plan; repeated notes add nothing.
                    if warnings.len() < 3 && !warnings.contains(&warning) {
                        warnings.push(warning);
                    }
                }
            }
        }
        self.inner.emit(event);
    }
}
