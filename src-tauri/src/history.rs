//! The history the Updates page's 「最近更新」 lists after a restart
//! (`banager_core::history`): kept in `history.json` beside `settings.json`
//! in Banager's application data directory, attached to the session as
//! Banager starts (`attach`), and given to the window by two commands.
//! Neither takes anything from the window but its call, and neither runs a
//! command or connects anywhere.

use crate::state::AppState;
use banager_core::history::{HistoryStore, HistoryView};
use std::path::Path;
use tauri::State;

/// The file's name, in the application data directory.
pub const HISTORY_FILE: &str = "history.json";

/// Reads the history in `data_dir` and keeps every operation from now on
/// there (`Session::attach_history`). Called once, from `run()`'s setup.
pub fn attach(state: &AppState, data_dir: &Path) {
    state
        .session
        .attach_history(HistoryStore::open(data_dir.join(HISTORY_FILE)));
}

pub(crate) fn get_history_impl(state: &AppState) -> HistoryView {
    state.session.history()
}

/// Every record, newest first, this launch's id and when the list was last
/// cleared (`HistoryView`). A copy under a lock: inline.
#[tauri::command]
pub async fn get_history(state: State<'_, AppState>) -> Result<HistoryView, String> {
    Ok(get_history_impl(&state))
}

pub(crate) fn clear_history_impl(state: &AppState) -> HistoryView {
    state.session.clear_history()
}

/// The Updates page's Clear: from now on the page lists nothing that
/// finished before this moment. No record is removed; the file is written
/// again with the time, by the history's own thread.
#[tauri::command]
pub async fn clear_history(state: State<'_, AppState>) -> Result<HistoryView, String> {
    Ok(clear_history_impl(&state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ChannelSink;
    use std::path::PathBuf;
    use std::time::Duration;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "banager-shell-history-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A record as a launch before this one wrote it.
    const EARLIER: &str = r#"{
  "format": 1,
  "cleared_before": null,
  "records": [
    {"run":"earlier","op_id":3,"finished_at":4102444800000,"key":{"instance_id":"brew:/opt/homebrew","kind":"Formula","name":"cmake"},"display_name":"cmake","adapter_id":"brew","kind":"Update","from_version":"3.31.6","to_version":"4.0.0","result":"Succeeded","verified":true}
  ]
}"#;

    #[test]
    fn test_get_history_hands_the_window_what_an_earlier_launch_kept_in_the_wire_shape() {
        let dir = temp_dir("get");
        std::fs::write(dir.join(HISTORY_FILE), EARLIER).unwrap();
        let state = AppState::new(dir.join("settings.json"), ChannelSink::new());
        attach(&state, &dir);

        let view = get_history_impl(&state);
        assert_ne!(view.run, "earlier", "this launch has its own id");
        // The JSON the window parses (`HistoryView` in src/lib/types.ts).
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["cleared_before"], serde_json::Value::Null);
        let record = &json["records"][0];
        assert_eq!(record["run"], "earlier");
        assert_eq!(record["display_name"], "cmake");
        assert_eq!(record["to_version"], "4.0.0");
        assert_eq!(record["result"], "Succeeded");
        assert_eq!(record["verified"], true);
        let back: HistoryView = serde_json::from_value(json).unwrap();
        assert_eq!(back, view);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_clear_history_keeps_the_time_and_every_record_across_launches() {
        let dir = temp_dir("clear");
        std::fs::write(dir.join(HISTORY_FILE), EARLIER).unwrap();
        let state = AppState::new(dir.join("settings.json"), ChannelSink::new());
        attach(&state, &dir);

        let cleared = clear_history_impl(&state);
        let at = cleared.cleared_before.expect("the time Clear was pressed");
        assert_eq!(cleared.records.len(), 1, "no record is removed");

        // The next launch reads the same time back.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let next = AppState::new(dir.join("settings.json"), ChannelSink::new());
            attach(&next, &dir);
            if get_history_impl(&next).cleared_before == Some(at) {
                assert_eq!(get_history_impl(&next).records.len(), 1);
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "Clear was never written"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_a_session_with_no_history_attached_answers_an_empty_one() {
        let dir = temp_dir("none");
        let state = AppState::new(dir.join("settings.json"), ChannelSink::new());
        let view = get_history_impl(&state);
        assert_eq!(view.records, vec![]);
        assert_eq!(clear_history_impl(&state).cleared_before, None);
        assert!(!dir.join(HISTORY_FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
