//! The notification when operations finish, Settings → Updates'
//! 「操作完成时通知」 (`Settings::notify_operations`, off by default). Once
//! every operation of a run has finished -- an Update all, one update or
//! uninstall, and whatever was started while those were under way -- the
//! page reports the run (`report_finished_run`), and
//! `banager_core::notify_operations` decides what that report does. This
//! is the shell's part: where the focus is, as the update notification
//! asks it (`notify::focus`), the words in the window's language (`body`),
//! and the notification itself, posted as the update notification is
//! (`notify::post`): titled Banager, through the same crate, with no
//! permission of its own -- turning the setting on asks the one the update
//! notification asks (`notify::request_notification_permission`).
//!
//! Closing the window does not stop an operation, nor the page, which is
//! hidden with it (window.rs): that is when this is for. A run the user
//! watched finish in the focused window is never posted.

use crate::menu::MenuLanguage;
use crate::notify::{self, Answer};
use crate::state::AppState;
use crate::window::NotificationPending;
use banager_core::notify_operations::{self, FinishedRun, ReportedRuns, RunKind, RunNotice};
use banager_core::notify_updates::Focus;
use banager_core::settings::Settings;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Runtime, State};

/// The runs reported in this run of Banager (`ReportedRuns`), managed on
/// the builder in `run()`. In memory only.
#[derive(Debug, Default)]
pub struct OperationRuns(Mutex<ReportedRuns>);

/// The page's report of a run of operations that has finished
/// (`useOperationsNotification` in src/lib/operationsNotification.ts).
/// What it does is `report`'s, with the focus as it is now and the words
/// in the window's language. A notification handed off waits on the window
/// (`NotificationPending::set_window`), which a click on it, bringing
/// Banager to the front, brings back as it was left. One that could not be
/// handed off is logged; the page is told nothing. A run withheld is
/// looked at again on the main thread (`look_again_on_main_thread`).
///
/// A run that cannot be one of the operations this Banager has started
/// (`plausible`) is dropped first, with nothing posted: its newest
/// operation is one Banager has not started, or it tells of more
/// operations than Banager has started. Each notification then stands for
/// an operation that ran, and a page that went wrong cannot post one for
/// every number it counts up to.
#[tauri::command]
pub async fn report_finished_run(
    app: AppHandle,
    state: State<'_, AppState>,
    runs: State<'_, OperationRuns>,
    run: FinishedRun,
) -> Result<(), String> {
    let newest = state.session.operations().iter().map(|op| op.id).max();
    if !plausible(&run, newest) {
        eprintln!("[banager] dropped a report of operations Banager has not started");
        return Ok(());
    }
    let focus = notify::focus(&app);
    let language = notify::language(&app, &state);
    let title = app.package_info().name.clone();
    let reported = report(&runs, &state.get_settings(), focus, &run, |run| {
        notify::post(&app, &title, &body(language, run), Answer::ShowWindow)
    });
    match reported {
        Ok(RunNotice::Post) => app.state::<NotificationPending>().set_window(),
        Ok(RunNotice::Withheld) => look_again_on_main_thread(&app),
        Ok(_) => {}
        Err(e) => {
            eprintln!("[banager] could not post the notification that operations finished: {e}")
        }
    }
    Ok(())
}

/// Whether `run` can be a run of the operations this Banager has started,
/// the newest of which is `newest` (`None` before the first): its newest
/// operation is one of them, and it tells of no more operations than there
/// are up to that one (ids count from 1, `OperationManager::submit_with`).
pub(crate) fn plausible(run: &FinishedRun, newest: Option<u64>) -> bool {
    newest.is_some_and(|newest| {
        (1..=newest).contains(&run.last_op) && u64::from(run.told()) <= run.last_op
    })
}

/// A run `report_finished_run` has just withheld, looked at again on the
/// main thread (`settle_withheld`). The focus it went by was asked off the
/// main thread, and Banager may have left the front between that question
/// and the run being withheld: `on_left_front`, finding nothing withheld
/// yet, posted nothing, and nothing else would ever post the run. On the
/// main thread, where AppKit tells of Banager leaving the front, the
/// question is in order with that: asked before, it finds Banager in front,
/// and `on_left_front` posts the run when Banager leaves; asked after, it
/// finds Banager gone and posts the run itself. Whichever posts takes the
/// run (`post_withheld`), so it is never posted twice.
#[cfg(target_os = "macos")]
fn look_again_on_main_thread<R: Runtime>(app: &AppHandle<R>) {
    let again = app.clone();
    let asked = app.run_on_main_thread(move || {
        let Some(mtm) = objc2::MainThreadMarker::new() else {
            return;
        };
        settle_withheld(&again, notify::focus_on_main_thread(&again, mtm));
    });
    if let Err(e) = asked {
        eprintln!("[banager] could not look again at a run of operations withheld: {e}");
    }
}

/// Off a Mac, Banager is never in front without its window focused
/// (`notify::focus`), so no run is withheld.
#[cfg(not(target_os = "macos"))]
fn look_again_on_main_thread<R: Runtime>(_app: &AppHandle<R>) {}

/// Banager has left the front (`window::observe_activation`): a run
/// withheld while it was in front with its window closed or in the Dock
/// is posted now, as `report_finished_run` posts one, while
/// 「操作完成时通知」 is still on. On the main thread, where AppKit tells
/// of it; the post itself is handed to a thread of its own (`notify::post`).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn on_left_front<R: Runtime>(app: &AppHandle<R>) {
    settle_withheld(app, Focus::Away);
}

/// What `recheck` does with the run withheld, if one is, and `focus`:
/// posted, with the notification in the window's language, and then
/// waiting on the window; or seen, or left waiting.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn settle_withheld<R: Runtime>(app: &AppHandle<R>, focus: Focus) {
    let runs = app.state::<OperationRuns>();
    if runs.0.lock().unwrap().withheld().is_none() {
        return;
    }
    let state = app.state::<AppState>();
    let language = notify::language(app, &state);
    let title = app.package_info().name.clone();
    let settled = recheck(&runs, &state.get_settings(), focus, |run| {
        notify::post(app, &title, &body(language, run), Answer::ShowWindow)
    });
    match settled {
        Ok(RunNotice::Post) => app.state::<NotificationPending>().set_window(),
        Ok(_) => {}
        Err(e) => {
            eprintln!("[banager] could not post the notification that operations finished: {e}")
        }
    }
}

/// The window has taken the focus (`window::on_window_event`): a run
/// withheld while it was away is on the operation bar for the user to
/// see, and is never posted.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn on_window_focused<R: Runtime>(app: &AppHandle<R>) {
    app.state::<OperationRuns>().0.lock().unwrap().seen();
}

/// What the run withheld, if one is, comes to with the focus as it is now
/// (`focus`): with another app in front, what Banager leaving the front
/// does (`post_withheld`); with the window focused, it is on the operation
/// bar for the user to see, and is never posted (`Watched`); with Banager
/// still in front and its window not focused, it goes on waiting
/// (`Withheld`, or `Nothing` when nothing waits).
pub(crate) fn recheck(
    runs: &OperationRuns,
    settings: &Settings,
    focus: Focus,
    post: impl FnOnce(&FinishedRun) -> Result<(), String>,
) -> Result<RunNotice, String> {
    match focus {
        Focus::Away => post_withheld(runs, settings, post),
        Focus::Window => {
            runs.0.lock().unwrap().seen();
            Ok(RunNotice::Watched)
        }
        Focus::App => Ok(if runs.0.lock().unwrap().withheld().is_some() {
            RunNotice::Withheld
        } else {
            RunNotice::Nothing
        }),
    }
}

/// What Banager leaving the front does: `notify_operations::post_withheld`
/// over the runs reported so far, whether 「操作完成时通知」 is on as
/// `settings` are saved, with `post` to post the notification.
pub(crate) fn post_withheld(
    runs: &OperationRuns,
    settings: &Settings,
    post: impl FnOnce(&FinishedRun) -> Result<(), String>,
) -> Result<RunNotice, String> {
    let mut reported = runs.0.lock().unwrap();
    notify_operations::post_withheld(&mut reported, settings.notify_operations, post)
}

/// A report's whole effect: `notify_operations::report` over the runs
/// reported so far, whether 「操作完成时通知」 is on as `settings` are
/// saved, and `focus`, with `post` to post the notification.
pub(crate) fn report(
    runs: &OperationRuns,
    settings: &Settings,
    focus: Focus,
    run: &FinishedRun,
    post: impl FnOnce(&FinishedRun) -> Result<(), String>,
) -> Result<RunNotice, String> {
    let mut reported = runs.0.lock().unwrap();
    notify_operations::report(&mut reported, settings.notify_operations, focus, run, post)
}

/// What the notification says under its title: how the run went, in the
/// words the operation bar uses -- 「已更新3个工具」 when every one it tells
/// of worked, else each way they ended that any did, 「2个已更新，1个未能
/// 更新」. Cancelled operations are not told of. With no space around a
/// number in Chinese: macOS spaces Chinese from digits itself.
pub fn body(language: MenuLanguage, run: &FinishedRun) -> String {
    let zh = language == MenuLanguage::ZhCn;
    let only_succeeded = run.failed == 0 && run.attention == 0;
    if only_succeeded {
        let n = run.succeeded;
        return match (zh, run.kind) {
            (true, RunKind::Upgrade) => format!("已更新{n}个工具"),
            (true, RunKind::Uninstall) => format!("已卸载{n}个工具"),
            (true, RunKind::Other) => format!("已完成{n}项操作"),
            (false, RunKind::Upgrade) => format!("Updated {n} {}", tools(n)),
            (false, RunKind::Uninstall) => format!("Uninstalled {n} {}", tools(n)),
            (false, RunKind::Other) => {
                format!(
                    "Completed {n} {}",
                    if n == 1 { "operation" } else { "operations" }
                )
            }
        };
    }
    let mut parts = Vec::new();
    if run.succeeded > 0 {
        let n = run.succeeded;
        parts.push(match (zh, run.kind) {
            (true, RunKind::Upgrade) => format!("{n}个已更新"),
            (true, RunKind::Uninstall) => format!("{n}个已卸载"),
            (true, RunKind::Other) => format!("{n}项已完成"),
            (false, RunKind::Upgrade) => format!("{n} updated"),
            (false, RunKind::Uninstall) => format!("{n} uninstalled"),
            (false, RunKind::Other) => format!("{n} completed"),
        });
    }
    if run.failed > 0 {
        let n = run.failed;
        parts.push(match (zh, run.kind) {
            (true, RunKind::Upgrade) => format!("{n}个未能更新"),
            (true, RunKind::Uninstall) => format!("{n}个未能卸载"),
            (true, RunKind::Other) => format!("{n}项未能完成"),
            (false, RunKind::Upgrade) => format!("{n} couldn't be updated"),
            (false, RunKind::Uninstall) => format!("{n} couldn't be uninstalled"),
            (false, RunKind::Other) => format!("{n} couldn't be completed"),
        });
    }
    if run.attention > 0 {
        let n = run.attention;
        parts.push(match (zh, run.kind) {
            (true, RunKind::Other) => format!("{n}项需要查看"),
            (true, _) => format!("{n}个需要查看"),
            (false, _) if n == 1 => "1 needs attention".to_string(),
            (false, _) => format!("{n} need attention"),
        });
    }
    parts.join(if zh { "，" } else { ", " })
}

fn tools(n: u32) -> &'static str {
    if n == 1 {
        "tool"
    } else {
        "tools"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn run(kind: RunKind, succeeded: u32, failed: u32, attention: u32) -> FinishedRun {
        FinishedRun {
            last_op: 3,
            kind,
            succeeded,
            failed,
            attention,
        }
    }

    fn on() -> Settings {
        Settings {
            notify_operations: true,
            ..Settings::default()
        }
    }

    #[test]
    fn test_a_run_finished_with_another_app_in_front_posts_once_with_the_setting_on() {
        let runs = OperationRuns::default();
        let posted = RefCell::new(Vec::new());
        let finished = run(RunKind::Upgrade, 3, 0, 0);
        assert_eq!(
            report(&runs, &on(), Focus::Away, &finished, |run| {
                posted.borrow_mut().push(body(MenuLanguage::ZhCn, run));
                Ok(())
            }),
            Ok(RunNotice::Post)
        );
        assert_eq!(
            report(&runs, &on(), Focus::Away, &finished, |_| panic!(
                "posted twice"
            )),
            Ok(RunNotice::Nothing)
        );
        assert_eq!(*posted.borrow(), ["已更新3个工具"]);
    }

    #[test]
    fn test_a_run_is_reported_only_as_one_of_the_operations_banager_started() {
        // Three operations started: a run of them is reported.
        assert!(plausible(&run(RunKind::Upgrade, 3, 0, 0), Some(3)));
        assert!(plausible(&run(RunKind::Uninstall, 1, 0, 0), Some(5)));
        // Before any operation, none.
        assert!(!plausible(&run(RunKind::Upgrade, 1, 0, 0), None));
        // An operation not started yet, or none at all.
        let mut later = run(RunKind::Upgrade, 1, 0, 0);
        later.last_op = 4;
        assert!(!plausible(&later, Some(3)));
        later.last_op = 0;
        assert!(!plausible(&later, Some(3)));
        // More operations than have been started up to its newest.
        assert!(!plausible(&run(RunKind::Upgrade, 3, 1, 0), Some(3)));
        assert!(!plausible(&run(RunKind::Upgrade, u32::MAX, 0, 0), Some(3)));
    }

    #[test]
    fn test_the_setting_is_read_as_saved_and_off_by_default() {
        let runs = OperationRuns::default();
        let finished = run(RunKind::Upgrade, 3, 0, 0);
        assert_eq!(
            report(&runs, &Settings::default(), Focus::Away, &finished, |_| {
                panic!("posted with the setting off")
            }),
            Ok(RunNotice::Nothing)
        );
    }

    #[test]
    fn test_a_run_the_user_watched_in_the_focused_window_posts_nothing() {
        let runs = OperationRuns::default();
        let finished = run(RunKind::Uninstall, 1, 0, 0);
        assert_eq!(
            report(&runs, &on(), Focus::Window, &finished, |_| {
                panic!("posted a run the user watched")
            }),
            Ok(RunNotice::Watched)
        );
    }

    #[test]
    fn test_a_run_finished_with_the_window_closed_and_banager_in_front_posts_when_banager_leaves_the_front(
    ) {
        let runs = OperationRuns::default();
        let finished = run(RunKind::Upgrade, 3, 0, 0);
        assert_eq!(
            report(&runs, &on(), Focus::App, &finished, |_| panic!(
                "posted while Banager was in front"
            )),
            Ok(RunNotice::Withheld)
        );
        let posted = RefCell::new(Vec::new());
        assert_eq!(
            post_withheld(&runs, &on(), |run| {
                posted.borrow_mut().push(body(MenuLanguage::ZhCn, run));
                Ok(())
            }),
            Ok(RunNotice::Post)
        );
        assert_eq!(*posted.borrow(), ["已更新3个工具"]);
        assert_eq!(
            post_withheld(&runs, &on(), |_| panic!("posted twice")),
            Ok(RunNotice::Nothing)
        );
    }

    #[test]
    fn test_a_withheld_run_is_not_posted_once_the_window_has_had_the_focus() {
        let runs = OperationRuns::default();
        let finished = run(RunKind::Uninstall, 2, 0, 0);
        report(&runs, &on(), Focus::App, &finished, |_| Ok(())).unwrap();
        runs.0.lock().unwrap().seen();
        assert_eq!(
            post_withheld(&runs, &on(), |_| panic!("posted a run the user saw")),
            Ok(RunNotice::Nothing)
        );
    }

    /// The race `look_again_on_main_thread` closes: the focus was asked
    /// with Banager in front, Banager left the front -- its observer
    /// finding nothing withheld yet -- and then the run was withheld.
    /// Looked at again with another app in front, it is posted, once.
    #[test]
    fn test_a_run_withheld_just_after_banager_left_the_front_is_posted_when_looked_at_again() {
        let runs = OperationRuns::default();
        let finished = run(RunKind::Upgrade, 3, 0, 0);
        // Banager leaves the front: nothing is withheld yet.
        assert_eq!(
            post_withheld(&runs, &on(), |_| panic!("nothing was withheld")),
            Ok(RunNotice::Nothing)
        );
        // The report, with the focus asked before Banager left.
        assert_eq!(
            report(&runs, &on(), Focus::App, &finished, |_| panic!(
                "posted while Banager was in front"
            )),
            Ok(RunNotice::Withheld)
        );
        let posted = RefCell::new(Vec::new());
        assert_eq!(
            recheck(&runs, &on(), Focus::Away, |run| {
                posted.borrow_mut().push(body(MenuLanguage::ZhCn, run));
                Ok(())
            }),
            Ok(RunNotice::Post)
        );
        assert_eq!(*posted.borrow(), ["已更新3个工具"]);
        assert_eq!(
            post_withheld(&runs, &on(), |_| panic!("posted twice")),
            Ok(RunNotice::Nothing),
            "the next time Banager leaves the front"
        );
    }

    /// Looked at again before Banager leaves the front, the run goes on
    /// waiting, and Banager leaving the front posts it; looked at again
    /// after the observer has posted it, nothing is posted twice.
    #[test]
    fn test_a_run_looked_at_again_with_banager_still_in_front_is_posted_once_when_it_leaves() {
        let runs = OperationRuns::default();
        let finished = run(RunKind::Uninstall, 2, 0, 0);
        report(&runs, &on(), Focus::App, &finished, |_| Ok(())).unwrap();
        assert_eq!(
            recheck(&runs, &on(), Focus::App, |_| panic!(
                "posted while Banager was in front"
            )),
            Ok(RunNotice::Withheld)
        );
        let posted = RefCell::new(0);
        assert_eq!(
            post_withheld(&runs, &on(), |_| {
                *posted.borrow_mut() += 1;
                Ok(())
            }),
            Ok(RunNotice::Post)
        );
        assert_eq!(
            recheck(&runs, &on(), Focus::Away, |_| panic!("posted twice")),
            Ok(RunNotice::Nothing)
        );
        assert_eq!(
            recheck(&runs, &on(), Focus::App, |_| panic!("posted twice")),
            Ok(RunNotice::Nothing)
        );
        assert_eq!(*posted.borrow(), 1);
    }

    /// Looked at again with the window focused -- it took the focus between
    /// the question and the run being withheld -- the run is seen, as the
    /// window taking the focus would have it, and never posted.
    #[test]
    fn test_a_run_looked_at_again_with_the_window_focused_is_seen_and_never_posted() {
        let runs = OperationRuns::default();
        let finished = run(RunKind::Upgrade, 1, 0, 0);
        report(&runs, &on(), Focus::App, &finished, |_| Ok(())).unwrap();
        assert_eq!(
            recheck(&runs, &on(), Focus::Window, |_| panic!(
                "posted a run the user saw"
            )),
            Ok(RunNotice::Watched)
        );
        assert_eq!(
            post_withheld(&runs, &on(), |_| panic!("posted a run the user saw")),
            Ok(RunNotice::Nothing)
        );
    }

    #[test]
    fn test_the_notification_says_how_the_run_went_in_the_windows_language() {
        use MenuLanguage::{En, ZhCn};
        let cases = [
            (
                run(RunKind::Upgrade, 3, 0, 0),
                "已更新3个工具",
                "Updated 3 tools",
            ),
            (
                run(RunKind::Upgrade, 1, 0, 0),
                "已更新1个工具",
                "Updated 1 tool",
            ),
            (
                run(RunKind::Upgrade, 2, 1, 0),
                "2个已更新，1个未能更新",
                "2 updated, 1 couldn't be updated",
            ),
            (
                run(RunKind::Upgrade, 0, 2, 1),
                "2个未能更新，1个需要查看",
                "2 couldn't be updated, 1 needs attention",
            ),
            (
                run(RunKind::Uninstall, 2, 0, 0),
                "已卸载2个工具",
                "Uninstalled 2 tools",
            ),
            (
                run(RunKind::Uninstall, 1, 1, 0),
                "1个已卸载，1个未能卸载",
                "1 uninstalled, 1 couldn't be uninstalled",
            ),
            (
                run(RunKind::Other, 3, 0, 0),
                "已完成3项操作",
                "Completed 3 operations",
            ),
            (
                run(RunKind::Other, 1, 0, 2),
                "1项已完成，2项需要查看",
                "1 completed, 2 need attention",
            ),
        ];
        for (finished, zh, en) in cases {
            assert_eq!(body(ZhCn, &finished), zh);
            assert_eq!(body(En, &finished), en);
        }
    }

    #[test]
    fn test_what_we_run_quotes_what_the_notification_says_in_both_languages() {
        // docs/what-we-run.md, "The notification when operations finish":
        // the plain update case in both languages, its count as N.
        let doc = include_str!("../../docs/what-we-run.md");
        let folded = doc.split_whitespace().collect::<Vec<_>>().join(" ");
        for language in [MenuLanguage::En, MenuLanguage::ZhCn] {
            for finished in [
                run(RunKind::Upgrade, 7, 0, 0),
                run(RunKind::Upgrade, 7, 7, 0),
            ] {
                let quoted = body(language, &finished).replace('7', "N");
                assert!(
                    folded.contains(&quoted),
                    "docs/what-we-run.md does not quote {quoted:?}, which a notification says"
                );
            }
        }
    }
}
