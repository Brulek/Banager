//! The update notification, Settings → Updates' 「有可更新时通知我」
//! (`Settings::notify_updates`, off by default, and on only with the daily
//! check it sits under). After each snapshot the page reports the rows its
//! Update all would take, with the round the snapshot came from
//! (`report_update_set`), and `canager_core::notify_updates` decides what
//! that report does. This is the shell's part: where the focus is -- on
//! the window, on Canager without its window, or on another app -- and the
//! notification itself, titled with the app's name, Canager, and saying
//! how many tools can be updated, in the window's language. On a Mac it
//! carries a handler that would bring the window back on the Updates page
//! for a click (`OPEN_UPDATES_EVENT`), which notify-rust never hands a
//! click (`post`). What a click does instead is bring Canager to the
//! front, and from its hand-off the notification waits on the window
//! (`window::NotificationPending`), so that Canager coming to the front
//! with its window closed or in the Dock brings it back on the Updates
//! page (`window::on_activate`). The Settings page asks for permission to
//! post as the switch is turned on (`request_notification_permission`).

use crate::menu::{self, MenuBar, MenuLanguage};
use crate::state::AppState;
use crate::window::{NotificationPending, MAIN_WINDOW};
use canager_core::notify_updates::{self, Focus, Notice, ReportedRound, UpdatePair};
use tauri::plugin::PermissionState;
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_notification::NotificationExt;

/// The event `open_updates` tells the window, once it is back on screen,
/// for the update notification -- whose click `post` never hears, but
/// which brings Canager to the front (`window::on_activate`) -- and which
/// opens the Updates page: src/lib/api.ts's `OPEN_UPDATES_EVENT` spells
/// the same.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub const OPEN_UPDATES_EVENT: &str = "notification://open-updates";

/// The page's report, after each snapshot, of the updates it offers to
/// start -- the rows Update all would take, as (row, version) pairs -- and
/// of `round`, the snapshot's `Snapshot::round`. What it does is `report`'s,
/// with the focus as it is now (`focus`) and the notification in the
/// window's language. A notification handed off waits on the window
/// (`window::NotificationPending`) until the window is next in front. One
/// that could not be handed off is logged here; the page is told nothing,
/// having nothing to do about it.
#[tauri::command]
pub async fn report_update_set(
    app: AppHandle,
    state: State<'_, AppState>,
    round: u64,
    updates: Vec<UpdatePair>,
) -> Result<(), String> {
    let focus = focus(&app);
    let language = language(&app, &state);
    let title = app.package_info().name.clone();
    let reported = report(&state, round, &updates, focus, |count| {
        post(&app, &title, &body(language, count))
    });
    match reported {
        Ok(Notice::Post { .. }) => app.state::<NotificationPending>().set(),
        Ok(_) => {}
        Err(e) => eprintln!("[canager] could not post the update notification: {e}"),
    }
    Ok(())
}

/// A report's whole effect: `notify_updates::report` over what the log
/// knows of `round` (`AppState::rounds`) -- who asked for it, and whether
/// it awaits a follow-up of the daily check's -- whether notifications are
/// on as the settings are saved now (`notify_updates::notifications_on`),
/// `focus`, and what this run has told or the user has seen
/// (`AppState::notified`), with `post` to post a notification saying how
/// many tools can be updated. The lock on what has been told is held until
/// the post has returned, so two reports cannot both post the same news.
/// `post` returns once the notification is handed off (`hand_off`), and
/// that is when its pairs are marked as told: nothing confirms a delivery.
pub(crate) fn report(
    state: &AppState,
    round: u64,
    updates: &[UpdatePair],
    focus: Focus,
    post: impl FnOnce(usize) -> Result<(), String>,
) -> Result<Notice, String> {
    let round = {
        let rounds = state.rounds.lock().unwrap();
        ReportedRound {
            trigger: rounds.trigger_of(round),
            awaits_follow_up: rounds.awaits_follow_up(round),
        }
    };
    let on = notify_updates::notifications_on(&state.get_settings());
    let mut notified = state.notified.lock().unwrap();
    notify_updates::report(&mut notified, round, on, focus, updates, post)
}

/// Where the focus is now: `focus_of` over whether the window has the
/// focus and whether Canager is the active app.
fn focus<R: Runtime>(app: &AppHandle<R>) -> Focus {
    focus_of(window_focused(app), app_active())
}

/// On the window when it has the focus; else on Canager when it is the
/// active app -- the app in front, whose notifications macOS shows no
/// banner for -- with its window closed or in the Dock; else away.
fn focus_of(window_focused: bool, app_active: bool) -> Focus {
    if window_focused {
        Focus::Window
    } else if app_active {
        Focus::App
    } else {
        Focus::Away
    }
}

/// Whether Canager is the active app, the one in front, as macOS says
/// (`NSRunningApplication`'s `isActive`, for this process; safe to ask
/// off the main thread, and it runs nothing).
#[cfg(target_os = "macos")]
fn app_active() -> bool {
    objc2_app_kit::NSRunningApplication::currentApplication().isActive()
}

/// Off a Mac, the focus is only ever the window's or away.
#[cfg(not(target_os = "macos"))]
fn app_active() -> bool {
    false
}

/// Whether the window has the focus: on screen, and the window the keys go
/// to. Closed (hidden), in the Dock or behind another app's, it has not.
/// Not knowing counts as not: at worst a notification the user did not
/// need, never news kept from them.
fn window_focused<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return false;
    };
    window.is_focused().unwrap_or_else(|e| {
        eprintln!("[canager] could not ask whether the window has the focus: {e}");
        false
    })
}

/// The window's language, which the menu bar follows
/// (`menu::set_menu_language`); before the page has said which, the one
/// the menu bar was built in, or would be.
fn language<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> MenuLanguage {
    app.state::<MenuBar>().language().unwrap_or_else(|| {
        menu::initial_language(state.get_settings().language, &menu::preferred_languages())
    })
}

/// What the notification says under its title: how many tools can be
/// updated -- every update the report offers, not only the new ones.
pub fn body(language: MenuLanguage, count: usize) -> String {
    match language {
        MenuLanguage::En if count == 1 => "1 tool can be updated".to_string(),
        MenuLanguage::En => format!("{count} tools can be updated"),
        MenuLanguage::ZhCn => format!("有 {count} 个工具可更新"),
    }
}

/// Starts `deliver` on a thread of its own and returns once the thread
/// has started, not once `deliver` has run: that is when a notification
/// is handed off, and when `report` marks its pairs as told. What
/// `deliver` answers comes after, and is only logged. The error handed
/// back is the thread's that could not be started, which handed nothing
/// off: nothing is marked, and the next report that offers the same
/// updates tries again.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn hand_off(deliver: impl FnOnce() -> Result<(), String> + Send + 'static) -> Result<(), String> {
    std::thread::Builder::new()
        .name("update-notification".to_string())
        .spawn(move || {
            if let Err(e) = deliver() {
                eprintln!("[canager] the update notification: {e}");
            }
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Posts the notification through notify-rust, the crate
/// tauri-plugin-notification posts through, on the thread `hand_off`
/// starts, and returns once that thread has started.
///
/// On a Mac, notify-rust's `show` sends nothing and never fails: it wraps
/// the notification in a handle, and the handle's `wait_for_response`, on
/// that thread, is what hands it to Notification Center
/// (`deliverNotification:`). For a notification without buttons, as this
/// one is, mac-notification-sys then waits only for macOS to confirm the
/// delivery (`didDeliverNotification:`), two seconds at most, and answers
/// that the notification closed whether the confirmation came or not. So
/// the thread ends within about two seconds, and nothing it is told
/// confirms a delivery or reports one that failed: the pairs are marked
/// as told at the hand-off, and a notification macOS does not show is not
/// posted again. Nor is the handler, which would bring the window back on
/// the Updates page (`open_updates`), ever handed a click: a click only
/// brings Canager to the front, and it is Canager coming to the front
/// while the notification waits on the window that brings the window back
/// on the Updates page (`window::on_activate`). What `wait_for_response`
/// reports as an error is logged.
#[cfg(target_os = "macos")]
fn post<R: Runtime>(app: &AppHandle<R>, title: &str, body: &str) -> Result<(), String> {
    use notify_rust::error::{ApplicationError, MacOsError};
    // Which app macOS shows the notification as, set before the first
    // one, as tauri-plugin-notification sets it: Canager, by its bundle
    // identifier; under `tauri dev`, which runs no app bundle, Terminal.
    // mac-notification-sys sets it once for the life of the process, by
    // answering that identifier for the app's own bundle from then on, so
    // every later call answers `AlreadySet`, which is no failure. Should
    // LaunchServices not know the identifier, it answers Terminal's
    // instead, and the notification is still posted, as Terminal's.
    let bundle = if tauri::is_dev() {
        "com.apple.Terminal".to_string()
    } else {
        app.config().identifier.clone()
    };
    match notify_rust::set_application(&bundle) {
        Ok(()) | Err(MacOsError::Application(ApplicationError::AlreadySet(_))) => {}
        Err(e) => eprintln!("[canager] could not post notifications as {bundle}: {e}"),
    }
    let handle = notify_rust::Notification::new()
        .summary(title)
        .body(body)
        .show()
        .map_err(|e| e.to_string())?;
    let app = app.clone();
    hand_off(move || {
        handle
            .wait_for_response(|response: &notify_rust::NotificationResponse| {
                if response.is_default_action() {
                    open_updates(&app);
                }
            })
            .map_err(|e| e.to_string())
    })
}

/// Posts the notification through tauri-plugin-notification, whose `show`
/// hands it to a task of its own and returns: what becomes of it is not
/// reported. No click is heard: the plugin reports none on a desktop.
#[cfg(not(target_os = "macos"))]
fn post<R: Runtime>(app: &AppHandle<R>, title: &str, body: &str) -> Result<(), String> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|e| e.to_string())
}

/// The notification answered: the window back on screen and the page told
/// to open Updates, as the menu bar's items that act in the page are
/// carried out (`window::show_and_tell`). Called when Canager comes to the
/// front, or its Dock icon is clicked, with its window closed or in the
/// Dock while the notification waits on the window (`window::on_activate`)
/// -- which is how a click on the notification arrives -- and from
/// `post`'s handler, which notify-rust never hands a click (see there).
#[cfg(target_os = "macos")]
pub(crate) fn open_updates<R: Runtime>(app: &AppHandle<R>) {
    if let Err(e) = crate::window::show_and_tell(app, OPEN_UPDATES_EVENT) {
        eprintln!("[canager] could not open Updates for the update notification: {e}");
    }
}

/// Asks for permission to post notifications, as the Settings page turns
/// 「有可更新时通知我」 on: yes when tauri-plugin-notification's
/// `request_permission` answers `Granted`, and the page turns the switch
/// back off otherwise. On a Mac the plugin answers `Granted` without
/// asking macOS; whether macOS shows what Canager posts is then up to
/// System Settings → Notifications.
#[tauri::command]
pub async fn request_notification_permission(app: AppHandle) -> Result<bool, String> {
    app.notification()
        .request_permission()
        .map(permission_granted)
        .map_err(|e| e.to_string())
}

/// Whether `state`, the plugin's answer, grants permission: only
/// `Granted` does. `Denied`, and a prompt still to be answered, do not.
fn permission_granted(state: PermissionState) -> bool {
    matches!(state, PermissionState::Granted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ChannelSink;
    use canager_core::auto_check::RoundTrigger;
    use canager_core::model::InstanceNote;
    use canager_core::session::{DetectOutcome, Session, Snapshot};
    use canager_core::settings::Settings;
    use std::cell::RefCell;
    use std::sync::Mutex;

    fn pair(name: &str, target: &str) -> UpdatePair {
        UpdatePair {
            key_id: format!("brew:/opt/homebrew|Formula|{name}"),
            target: target.to_string(),
        }
    }

    fn snapshot(round: u64) -> Snapshot {
        Snapshot {
            generation: 1,
            round,
            detect: DetectOutcome::Found,
            instances: Vec::new(),
            artifacts: Vec::new(),
            updates: Vec::new(),
            refreshed_at: Some(1_790_586_000),
            stale: false,
            errors: Vec::new(),
        }
    }

    /// Round `round`'s snapshot, reporting a `brew update` Canager started
    /// as still running when `brew_updating`.
    fn snapshot_with_homebrew(round: u64, brew_updating: bool) -> Snapshot {
        let mut brew = canager_core::testing::manager_instance("brew", "brew:/opt/homebrew");
        if brew_updating {
            brew.status.notes.push(InstanceNote::IndexUpdating);
        }
        Snapshot {
            instances: vec![brew],
            ..snapshot(round)
        }
    }

    /// A state whose settings are `settings`, and whose round 1 the daily
    /// check asked for and round 2 the window. Nothing is ever saved: the
    /// tests only read the settings.
    fn state(settings: Settings) -> AppState {
        let sink = ChannelSink::new();
        let state = AppState {
            session: Session::with_adapters(sink.clone(), Vec::new(), None),
            settings_path: std::env::temp_dir().join("canager-notify-never-written"),
            settings: Mutex::new(settings),
            channel_sink: sink,
            last_broadcast_generation: std::sync::atomic::AtomicU64::new(0),
            rounds: Mutex::new(Default::default()),
            notified: Mutex::new(Default::default()),
        };
        {
            let mut rounds = state.rounds.lock().unwrap();
            rounds.record(1, RoundTrigger::Automatic, &snapshot(1));
            rounds.record(2, RoundTrigger::Window, &snapshot(2));
        }
        state
    }

    fn notifications_on() -> Settings {
        Settings {
            auto_check: true,
            notify_updates: true,
            ..Settings::default()
        }
    }

    const DAILY: u64 = 1;
    const WINDOW: u64 = 2;

    /// A poster that records each count it is handed, and succeeds.
    fn recording(posted: &RefCell<Vec<usize>>) -> impl FnOnce(usize) -> Result<(), String> + '_ {
        move |count| {
            posted.borrow_mut().push(count);
            Ok(())
        }
    }

    #[test]
    fn test_a_report_of_the_daily_checks_round_posts_once_with_the_whole_count() {
        let state = state(notifications_on());
        let posted = RefCell::new(Vec::new());
        let updates = [pair("jq", "1.8.1"), pair("gh", "2.102.0")];
        assert_eq!(
            report(&state, DAILY, &updates, Focus::Away, recording(&posted)),
            Ok(Notice::Post { count: 2 })
        );
        // Reported again -- a page loaded again, the next day's check
        // finding the same two: nothing new, nothing posted.
        assert_eq!(
            report(&state, DAILY, &updates, Focus::Away, recording(&posted)),
            Ok(Notice::Nothing)
        );
        assert_eq!(*posted.borrow(), [2]);
    }

    #[test]
    fn test_a_daily_check_whose_brew_update_outlasts_it_posts_once_for_it_and_its_follow_up() {
        let state = state(notifications_on());
        // Round 3, the daily check's, leaves its `brew update` running;
        // the refresh its end sets off, round 4, is the daily check's too
        // (`ipc::refresh_on_background_change`).
        state.rounds.lock().unwrap().record(
            3,
            RoundTrigger::Automatic,
            &snapshot_with_homebrew(3, true),
        );
        let posted = RefCell::new(Vec::new());
        assert_eq!(
            report(
                &state,
                3,
                &[pair("jq", "1.8.1")],
                Focus::Away,
                recording(&posted)
            ),
            Ok(Notice::Deferred)
        );
        let follow_up = state
            .rounds
            .lock()
            .unwrap()
            .record_follow_up(4, &snapshot_with_homebrew(4, false));
        assert_eq!(follow_up, RoundTrigger::Automatic);
        let offered = [pair("jq", "1.8.1"), pair("gh", "2.102.0")];
        assert_eq!(
            report(&state, 4, &offered, Focus::Away, recording(&posted)),
            Ok(Notice::Post { count: 2 })
        );
        assert_eq!(
            *posted.borrow(),
            [2],
            "one notification, counting what both rounds found"
        );
    }

    #[test]
    fn test_the_round_is_looked_up_so_a_round_of_the_windows_posts_nothing() {
        let state = state(notifications_on());
        let updates = [pair("jq", "1.8.1")];
        let never = |_| -> Result<(), String> { panic!("posted for the window's round") };
        assert_eq!(
            report(&state, WINDOW, &updates, Focus::Away, never),
            Ok(Notice::Nothing)
        );
        let never = |_| -> Result<(), String> { panic!("posted for a round never recorded") };
        assert_eq!(
            report(&state, 99, &updates, Focus::Away, never),
            Ok(Notice::Nothing)
        );
        // Neither marked anything: the daily check's round still posts.
        let posted = RefCell::new(Vec::new());
        assert_eq!(
            report(&state, DAILY, &updates, Focus::Away, recording(&posted)),
            Ok(Notice::Post { count: 1 })
        );
    }

    #[test]
    fn test_the_settings_are_read_as_saved_and_notifications_need_the_daily_check_too() {
        let updates = [pair("jq", "1.8.1")];
        for settings in [
            Settings::default(),
            Settings {
                notify_updates: true,
                ..Settings::default()
            },
            Settings {
                auto_check: true,
                ..Settings::default()
            },
        ] {
            let state = state(settings.clone());
            let never = |_| -> Result<(), String> { panic!("posted with {settings:?}") };
            assert_eq!(
                report(&state, DAILY, &updates, Focus::Away, never),
                Ok(Notice::Nothing)
            );
        }
    }

    #[test]
    fn test_with_the_window_focused_nothing_is_posted_and_what_it_offers_is_seen() {
        let state = state(notifications_on());
        let updates = [pair("jq", "1.8.1")];
        let never = |_| -> Result<(), String> { panic!("posted while the window had the focus") };
        assert_eq!(
            report(&state, DAILY, &updates, Focus::Window, never),
            Ok(Notice::Seen)
        );
        let never = |_| -> Result<(), String> { panic!("posted what the user saw") };
        assert_eq!(
            report(&state, DAILY, &updates, Focus::Away, never),
            Ok(Notice::Nothing)
        );
    }

    #[test]
    fn test_with_canager_in_front_and_its_window_closed_nothing_is_posted_and_nothing_seen() {
        let state = state(notifications_on());
        let updates = [pair("jq", "1.8.1")];
        let never = |_| -> Result<(), String> { panic!("posted while Canager was in front") };
        assert_eq!(
            report(&state, DAILY, &updates, Focus::App, never),
            Ok(Notice::Withheld)
        );
        assert!(!state.notified.lock().unwrap().contains(&updates[0]));
        // The same updates, offered again with another app in front.
        let posted = RefCell::new(Vec::new());
        assert_eq!(
            report(&state, DAILY, &updates, Focus::Away, recording(&posted)),
            Ok(Notice::Post { count: 1 })
        );
        assert_eq!(*posted.borrow(), [1]);
    }

    #[test]
    fn test_the_focus_is_the_windows_else_canagers_when_it_is_in_front_else_away() {
        assert_eq!(focus_of(true, true), Focus::Window);
        assert_eq!(
            focus_of(true, false),
            Focus::Window,
            "a focused window is enough"
        );
        assert_eq!(
            focus_of(false, true),
            Focus::App,
            "in front, the window closed or in the Dock"
        );
        assert_eq!(focus_of(false, false), Focus::Away);
    }

    #[test]
    fn test_a_post_that_failed_is_handed_back_and_tried_again_at_the_next_report() {
        // A hand-off that failed: the thread that delivers could not be
        // started (`hand_off`'s error), so nothing was handed to macOS.
        let state = state(notifications_on());
        let updates = [pair("jq", "1.8.1")];
        assert_eq!(
            report(&state, DAILY, &updates, Focus::Away, |_| Err(
                "no".to_string()
            )),
            Err("no".to_string())
        );
        let posted = RefCell::new(Vec::new());
        assert_eq!(
            report(&state, DAILY, &updates, Focus::Away, recording(&posted)),
            Ok(Notice::Post { count: 1 })
        );
        assert_eq!(*posted.borrow(), [1]);
    }

    #[test]
    fn test_a_notification_counts_as_told_once_handed_off_whatever_its_delivery_does_after() {
        // `post` on a Mac: `hand_off` returns once the thread that delivers
        // has started, and nothing that thread learns reaches `report`. So
        // the pairs are marked at the hand-off, before the delivery has
        // run, and one that then fails is not posted again.
        let state = state(notifications_on());
        let updates = [pair("jq", "1.8.1")];
        let (release, released) = std::sync::mpsc::channel::<()>();
        let (finish, finished) = std::sync::mpsc::channel::<()>();
        let notice = report(&state, DAILY, &updates, Focus::Away, |_| {
            hand_off(move || {
                released.recv().ok();
                finish.send(()).ok();
                Err("macOS never confirmed the delivery".to_string())
            })
        });
        assert_eq!(notice, Ok(Notice::Post { count: 1 }));
        assert!(
            state.notified.lock().unwrap().contains(&updates[0]),
            "marked at the hand-off, while the delivery has not run"
        );

        release.send(()).expect("the delivery is waiting");
        finished
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the delivery ran, and failed");
        let never = |_| -> Result<(), String> { panic!("posted again after the hand-off") };
        assert_eq!(
            report(&state, DAILY, &updates, Focus::Away, never),
            Ok(Notice::Nothing)
        );
    }

    #[test]
    fn test_the_notification_says_how_many_tools_can_be_updated_in_the_windows_language() {
        assert_eq!(body(MenuLanguage::En, 1), "1 tool can be updated");
        assert_eq!(body(MenuLanguage::En, 3), "3 tools can be updated");
        assert_eq!(body(MenuLanguage::ZhCn, 1), "有 1 个工具可更新");
        assert_eq!(body(MenuLanguage::ZhCn, 12), "有 12 个工具可更新");
    }

    #[test]
    fn test_what_we_run_quotes_what_the_notification_says_in_both_languages() {
        // docs/what-we-run.md, "The daily check": what `body` writes, its
        // count as N, and the title. Hard-wrapped prose: compared with the
        // line breaks folded away.
        let doc = include_str!("../../docs/what-we-run.md");
        let folded = doc.split_whitespace().collect::<Vec<_>>().join(" ");
        for language in [MenuLanguage::En, MenuLanguage::ZhCn] {
            let quoted = body(language, 7).replace('7', "N");
            assert!(
                folded.contains(&quoted),
                "docs/what-we-run.md does not quote {quoted:?}, which a notification says"
            );
        }
        assert!(
            folded.contains("The notification is titled Canager"),
            "docs/what-we-run.md does not say the notification's title"
        );
    }

    #[test]
    fn test_the_notification_is_titled_with_the_apps_name_canager() {
        // `report_update_set` titles it with `package_info().name`, which
        // tauri takes from tauri.conf.json's productName.
        let config: tauri::utils::config::Config =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config.product_name.as_deref(), Some("Canager"));
    }

    #[test]
    fn test_only_a_granted_permission_turns_notifications_on() {
        assert!(permission_granted(PermissionState::Granted));
        assert!(!permission_granted(PermissionState::Denied));
        assert!(!permission_granted(PermissionState::Prompt));
        assert!(!permission_granted(PermissionState::PromptWithRationale));
    }

    #[test]
    fn test_the_click_event_is_the_one_the_page_listens_for() {
        // `OPEN_UPDATES_EVENT` in src/lib/api.ts, which api.test.ts pins
        // to the same string.
        assert_eq!(OPEN_UPDATES_EVENT, "notification://open-updates");
    }
}
