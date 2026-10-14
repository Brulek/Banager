//! The history the Updates page's 「最近的更新记录」 lists after a restart
//! (`banager_core::history`): kept in `history.json` beside `settings.json`
//! in Banager's application data directory, attached to the session as
//! Banager starts (`attach`), and given to the window by two commands.
//! Neither takes anything from the window but its call, and neither runs a
//! command or connects anywhere.

use crate::state::AppState;
use banager_core::history::{HistoryStore, HistoryView};
use std::path::Path;
use std::time::Duration;
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

/// How long Banager's exit waits for the history file: a record of an
/// operation that finished just before Quit is what the file is for, and
/// one write of it takes a few milliseconds.
pub const EXIT_FLUSH: Duration = Duration::from_millis(500);

/// Called as Banager exits (`RunEvent::Exit`, `run()` in lib.rs): waits, at
/// most `EXIT_FLUSH`, for the history's own thread to write what it has
/// not written yet -- trying once more a write that failed, and no longer
/// than that try when it fails again (`HistoryStore::flush`).
pub fn flush_on_exit(state: &AppState) {
    if !state.session.flush_history(EXIT_FLUSH) {
        eprintln!("[banager] the history file may be missing the last records");
    }
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

/// The Updates page's Clear: every record already kept is marked dismissed,
/// and the page lists none of them again; what finishes later is listed
/// whatever the clock says. No record is removed; the file is written again
/// with the marks and the time, by the history's own thread. `Ok` is the
/// Clear taken in memory, not written: the view's `persistence` and
/// `unsaved_clear` say whether the file has it (r24 R74-2).
#[tauri::command]
pub async fn clear_history(state: State<'_, AppState>) -> Result<HistoryView, String> {
    Ok(clear_history_impl(&state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ChannelSink;
    use std::path::PathBuf;

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

        assert!(!get_history_impl(&state).records[0].dismissed);
        let cleared = clear_history_impl(&state);
        let at = cleared.cleared_before.expect("the time Clear was pressed");
        assert_eq!(cleared.records.len(), 1, "no record is removed");
        assert!(cleared.records[0].dismissed);

        // The next launch reads the same time back.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let next = AppState::new(dir.join("settings.json"), ChannelSink::new());
            attach(&next, &dir);
            if get_history_impl(&next).cleared_before == Some(at) {
                let records = get_history_impl(&next).records;
                assert_eq!(records.len(), 1);
                assert!(records[0].dismissed, "dismissed across launches");
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
    fn test_exit_waits_for_the_last_change_to_reach_the_file() {
        let dir = temp_dir("exit");
        let state = AppState::new(dir.join("settings.json"), ChannelSink::new());
        attach(&state, &dir);
        let at = clear_history_impl(&state).cleared_before;
        flush_on_exit(&state);
        // Written by the time exit's wait returns, with no polling.
        let file: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join(HISTORY_FILE)).unwrap()).unwrap();
        assert_eq!(file["cleared_before"].as_i64(), at);
        // And a session with none attached has nothing to wait for.
        let none = AppState::new(dir.join("settings.json"), ChannelSink::new());
        assert!(none.session.flush_history(EXIT_FLUSH));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_a_session_with_no_history_attached_answers_an_empty_one() {
        let dir = temp_dir("none");
        let state = AppState::new(dir.join("settings.json"), ChannelSink::new());
        let view = get_history_impl(&state);
        assert_eq!(view.records, vec![]);
        // Said as such, never as a history that is saved.
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["persistence"], "unattached");
        assert_eq!(json["unsaved_clear"], false);
        assert_eq!(clear_history_impl(&state).cleared_before, None);
        assert!(!dir.join(HISTORY_FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A history.json that is there but cannot be read is said to the
    /// window as such, and Clear and this launch's records leave it as it
    /// was (r24 R74-1).
    #[test]
    fn test_an_unreadable_history_is_said_to_the_window_and_left_as_it_was() {
        let dir = temp_dir("unreadable");
        std::fs::write(dir.join(HISTORY_FILE), EARLIER).unwrap();
        let state = AppState::new(dir.join("settings.json"), ChannelSink::new());
        let denied = banager_core::history::HistoryIo::default()
            .with_read(|_| Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)));
        state.session.attach_history(HistoryStore::open_with_io(
            dir.join(HISTORY_FILE),
            || 4_102_444_800_000,
            denied,
        ));
        let json = serde_json::to_value(get_history_impl(&state)).unwrap();
        assert_eq!(json["persistence"], "unreadable");
        assert_eq!(json["records"], serde_json::json!([]));
        let cleared = serde_json::to_value(clear_history_impl(&state)).unwrap();
        assert_eq!(cleared["persistence"], "unreadable");
        assert!(!state.session.flush_history(EXIT_FLUSH));
        assert_eq!(
            std::fs::read_to_string(dir.join(HISTORY_FILE)).unwrap(),
            EARLIER
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Clear's answer says whether the file has it: a Clear whose write
    /// failed is `unsaved_clear`, with the write still owed, and the next
    /// try that works says it is saved (r24 R74-2).
    #[test]
    fn test_clear_history_response_exposes_persistence_status() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let dir = temp_dir("clear-status");
        std::fs::write(dir.join(HISTORY_FILE), EARLIER).unwrap();
        let state = AppState::new(dir.join("settings.json"), ChannelSink::new());
        let failing = Arc::new(AtomicBool::new(true));
        let fails = failing.clone();
        let io = banager_core::history::HistoryIo::default().with_write(move |_, _| {
            if fails.load(Ordering::SeqCst) {
                Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            } else {
                Ok(())
            }
        });
        state.session.attach_history(HistoryStore::open_with_io(
            dir.join(HISTORY_FILE),
            || 4_102_444_800_000,
            io,
        ));
        let json = serde_json::to_value(get_history_impl(&state)).unwrap();
        assert_eq!(json["persistence"], "saved");
        assert_eq!(json["unsaved_clear"], false);

        let cleared = serde_json::to_value(clear_history_impl(&state)).unwrap();
        // Its write may have failed already, or not have ended yet.
        assert!(
            cleared["persistence"] == "pending" || cleared["persistence"] == "write_failed",
            "{}",
            cleared["persistence"]
        );
        assert_eq!(cleared["unsaved_clear"], true);
        assert!(!state.session.flush_history(EXIT_FLUSH));
        let json = serde_json::to_value(get_history_impl(&state)).unwrap();
        assert_eq!(json["persistence"], "write_failed");
        assert_eq!(json["unsaved_clear"], true);
        assert_eq!(json["records"][0]["dismissed"], true, "hidden in memory");

        failing.store(false, Ordering::SeqCst);
        assert!(state.session.flush_history(Duration::from_secs(5)));
        let json = serde_json::to_value(get_history_impl(&state)).unwrap();
        assert_eq!(json["persistence"], "saved");
        assert_eq!(json["unsaved_clear"], false);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
