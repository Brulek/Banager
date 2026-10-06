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
use banager_core::ops::Completions;
use banager_core::settings::Settings;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Runtime, State};

/// The runs reported in this run of Banager (`ReportedRuns`), and of their
/// failed updates, how many stopped where sudo wanted the Mac's password
/// (`Passwords`), managed on the builder in `run()`. In memory only.
#[derive(Debug, Default)]
pub struct OperationRuns(Mutex<Runs>);

/// `OperationRuns`' contents: the runs, and the password stops beside them.
#[derive(Debug, Default)]
pub(crate) struct Runs {
    reported: ReportedRuns,
    passwords: Passwords,
}

/// What `FinishedRun` does not count and the notification says: of the
/// updates that failed, those that stopped where sudo wanted the Mac's
/// password with no way to ask (`Accepted::password`) -- the rows that say
/// 「需要输入密码」, which the operation bar and the Updates page's
/// headline count apart (`failedRunWords` in src/lib/runResult.ts,
/// `updatesHeadline`). Each run's are counted with it, from the same
/// records (`ReportedRuns::accepted`); kept here are those of the run
/// withheld, taken when it is posted or seen, as `ReportedRuns` takes the
/// run.
#[derive(Debug, Default)]
struct Passwords {
    withheld: u32,
}

impl Runs {
    /// The run withheld, if one is (`ReportedRuns::withheld`).
    fn withheld(&self) -> Option<&FinishedRun> {
        self.reported.withheld()
    }

    /// The window has taken the focus (`ReportedRuns::seen`).
    fn seen(&mut self) {
        self.reported.seen();
        self.passwords.withheld = 0;
    }
}

/// The page's report of a run of operations that has finished
/// (`useOperationsNotification` in src/lib/operationsNotification.ts).
/// What it does is `report`'s, with the focus as it is now and the words
/// in the window's language. A notification handed off waits on the window
/// (`NotificationPending::set_window`), which a click on it, bringing
/// Banager to the front, brings back as it was left. One that could not be
/// handed off is logged; the page is told nothing. A run withheld is
/// looked at again on the main thread (`look_again_on_main_thread`).
///
/// The page supplies only a boundary hint. Counts and kind are derived
/// from completed Rust records in the interval since the last accepted
/// report -- with what the evicted ones left (`Session::completions_after`)
/// -- under the same mutex as reporting/deduplication (`report_known`).
#[tauri::command]
pub async fn report_finished_run(
    app: AppHandle,
    state: State<'_, AppState>,
    runs: State<'_, OperationRuns>,
    run: FinishedRun,
) -> Result<(), String> {
    let focus = notify::focus(&app);
    let language = notify::language(&app, &state);
    let title = app.package_info().name.clone();
    let reported = {
        let mut records = runs.0.lock().unwrap();
        let known = state.session.completions_after(records.reported.through());
        let reported = report_known(
            &mut records,
            &known,
            run.last_op,
            state.get_settings().notify_operations,
            focus,
            |run, password| {
                notify::post(
                    &app,
                    &title,
                    &body(language, run, password),
                    Answer::ShowWindow,
                )
            },
        );
        let Some(reported) = reported else {
            return Ok(());
        };
        reported
    };
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

/// The run through `last_op`, as the operations `known` -- one read of
/// them, `Session::completions_after` -- prove it ended
/// (`ReportedRuns::accepted`), reported over `records` (`report_counted`)
/// with its password stops counted from the same records. `None`, and
/// nothing changed, when they do not prove it: an operation of it
/// unfinished, or the run reported before.
fn report_known(
    records: &mut Runs,
    known: &Completions,
    last_op: u64,
    on: bool,
    focus: Focus,
    post: impl FnOnce(&FinishedRun, u32) -> Result<(), String>,
) -> Option<Result<RunNotice, String>> {
    let accepted = records.reported.accepted(last_op, known)?;
    Some(report_counted(
        records,
        on,
        focus,
        &accepted.run,
        accepted.password,
        post,
    ))
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
    let settled = recheck(&runs, &state.get_settings(), focus, |run, password| {
        notify::post(
            app,
            &title,
            &body(language, run, password),
            Answer::ShowWindow,
        )
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
    post: impl FnOnce(&FinishedRun, u32) -> Result<(), String>,
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
/// `settings` are saved, with `post` to post the notification -- handed
/// the run withheld and how many of its updates stopped for the password.
pub(crate) fn post_withheld(
    runs: &OperationRuns,
    settings: &Settings,
    post: impl FnOnce(&FinishedRun, u32) -> Result<(), String>,
) -> Result<RunNotice, String> {
    let mut runs = runs.0.lock().unwrap();
    // Taken with the run, whether or not it is posted: the run no longer
    // waits either way.
    let password = std::mem::take(&mut runs.passwords.withheld);
    notify_operations::post_withheld(&mut runs.reported, settings.notify_operations, |run| {
        post(run, password)
    })
}

/// `notify_operations::report` over `runs`, with `password` -- how many of
/// `run`'s updates stopped for the password (`Accepted::password`) -- kept
/// as `ReportedRuns` keeps the run: added to the run withheld, and handed
/// to `post` with a run withheld before told of in the same notification.
/// `decide` is asked first, with the same runs `report` asks it with, so
/// that what happens to the run withheld is known even when `post` fails.
fn report_counted(
    runs: &mut Runs,
    on: bool,
    focus: Focus,
    run: &FinishedRun,
    password: u32,
    post: impl FnOnce(&FinishedRun, u32) -> Result<(), String>,
) -> Result<RunNotice, String> {
    let notice = notify_operations::decide(on, focus, run, &runs.reported);
    let withheld = runs.passwords.withheld;
    let reported = notify_operations::report(&mut runs.reported, on, focus, run, |whole| {
        post(whole, withheld.saturating_add(password))
    });
    match notice {
        RunNotice::Post | RunNotice::Watched => runs.passwords.withheld = 0,
        RunNotice::Withheld => runs.passwords.withheld = withheld.saturating_add(password),
        RunNotice::Nothing => {}
    }
    reported
}

/// A report's whole effect: `report_counted` over the runs reported so
/// far, whether 「操作完成时通知」 is on as `settings` are saved, and
/// `focus`, with `post` to post the notification.
#[cfg(test)]
pub(crate) fn report(
    runs: &OperationRuns,
    settings: &Settings,
    focus: Focus,
    run: &FinishedRun,
    password: u32,
    post: impl FnOnce(&FinishedRun, u32) -> Result<(), String>,
) -> Result<RunNotice, String> {
    let mut runs = runs.0.lock().unwrap();
    report_counted(
        &mut runs,
        settings.notify_operations,
        focus,
        run,
        password,
        post,
    )
}

/// What the notification says under its title: how the run went, in the
/// words the operation bar uses -- 「已更新3个工具」 when every one it tells
/// of worked, else each way they ended that any did, 「2个已更新，1个未能
/// 更新」. Of a run of updates, those of its failed ones that stopped
/// where sudo wanted the Mac's password (`password`, `Accepted::password`) are
/// said as the bar and the Updates page say them, 「13个需要输入密码」, not
/// as failures: Terminal finishes them, with the steps their logs give.
/// Cancelled operations are not told of. With no space around a number in
/// Chinese: macOS spaces Chinese from digits itself.
pub fn body(language: MenuLanguage, run: &FinishedRun, password: u32) -> String {
    let zh = language != MenuLanguage::En;
    let password = if run.kind == RunKind::Upgrade {
        password.min(run.failed)
    } else {
        0
    };
    let only_succeeded = run.failed == 0 && run.attention == 0;
    if only_succeeded {
        let n = run.succeeded;
        return match (language, run.kind) {
            (MenuLanguage::ZhCn, RunKind::Upgrade) => format!("已更新{n}个工具"),
            (MenuLanguage::ZhHant, RunKind::Upgrade) => format!("已更新{n}個工具"),
            (MenuLanguage::ZhCn, RunKind::Uninstall) => format!("已卸载{n}个工具"),
            (MenuLanguage::ZhHant, RunKind::Uninstall) => format!("已解除安裝{n}個工具"),
            (MenuLanguage::ZhCn, RunKind::Other) => format!("已完成{n}项操作"),
            (MenuLanguage::ZhHant, RunKind::Other) => format!("已完成{n}項操作"),
            (MenuLanguage::En, RunKind::Upgrade) => format!("Updated {n} {}", tools(n)),
            (MenuLanguage::En, RunKind::Uninstall) => format!("Uninstalled {n} {}", tools(n)),
            (MenuLanguage::En, RunKind::Other) => {
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
        parts.push(match (language, run.kind) {
            (MenuLanguage::ZhCn, RunKind::Upgrade) => format!("{n}个已更新"),
            (MenuLanguage::ZhHant, RunKind::Upgrade) => format!("{n}個已更新"),
            (MenuLanguage::ZhCn, RunKind::Uninstall) => format!("{n}个已卸载"),
            (MenuLanguage::ZhHant, RunKind::Uninstall) => format!("{n}個已解除安裝"),
            (MenuLanguage::ZhCn, RunKind::Other) => format!("{n}项已完成"),
            (MenuLanguage::ZhHant, RunKind::Other) => format!("{n}項已完成"),
            (MenuLanguage::En, RunKind::Upgrade) => format!("{n} updated"),
            (MenuLanguage::En, RunKind::Uninstall) => format!("{n} uninstalled"),
            (MenuLanguage::En, RunKind::Other) => format!("{n} completed"),
        });
    }
    if run.failed > password {
        let n = run.failed - password;
        parts.push(match (language, run.kind) {
            (MenuLanguage::ZhCn, RunKind::Upgrade) => format!("{n}个未能更新"),
            (MenuLanguage::ZhHant, RunKind::Upgrade) => format!("{n}個未能更新"),
            (MenuLanguage::ZhCn, RunKind::Uninstall) => format!("{n}个未能卸载"),
            (MenuLanguage::ZhHant, RunKind::Uninstall) => format!("{n}個未能解除安裝"),
            (MenuLanguage::ZhCn, RunKind::Other) => format!("{n}项未能完成"),
            (MenuLanguage::ZhHant, RunKind::Other) => format!("{n}項未能完成"),
            (MenuLanguage::En, RunKind::Upgrade) => format!("{n} couldn't be updated"),
            (MenuLanguage::En, RunKind::Uninstall) => format!("{n} couldn't be uninstalled"),
            (MenuLanguage::En, RunKind::Other) => format!("{n} couldn't be completed"),
        });
    }
    if password > 0 {
        let n = password;
        parts.push(match language {
            MenuLanguage::ZhCn => format!("{n}个需要输入密码"),
            MenuLanguage::ZhHant => format!("{n}個需要輸入密碼"),
            MenuLanguage::En if n == 1 => "1 needs your password".to_string(),
            MenuLanguage::En => format!("{n} need your password"),
        });
    }
    if run.attention > 0 {
        let n = run.attention;
        parts.push(match (language, run.kind) {
            (MenuLanguage::ZhCn, RunKind::Other) => format!("{n}项需要查看"),
            (MenuLanguage::ZhHant, RunKind::Other) => format!("{n}項需要查看"),
            (MenuLanguage::ZhCn, _) => format!("{n}个需要查看"),
            (MenuLanguage::ZhHant, _) => format!("{n}個需要查看"),
            (MenuLanguage::En, _) if n == 1 => "1 needs attention".to_string(),
            (MenuLanguage::En, _) => format!("{n} need attention"),
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
    use banager_core::model::{OpKind, OpStatus, Outcome};
    use banager_core::ops::OpSummary;
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
            report(&runs, &on(), Focus::Away, &finished, 0, |run, password| {
                posted
                    .borrow_mut()
                    .push(body(MenuLanguage::ZhCn, run, password));
                Ok(())
            }),
            Ok(RunNotice::Post)
        );
        assert_eq!(
            report(&runs, &on(), Focus::Away, &finished, 0, |_, _| panic!(
                "posted twice"
            )),
            Ok(RunNotice::Nothing)
        );
        assert_eq!(*posted.borrow(), ["已更新3个工具"]);
    }

    #[test]
    fn test_the_setting_is_read_as_saved_and_off_by_default() {
        let runs = OperationRuns::default();
        let finished = run(RunKind::Upgrade, 3, 0, 0);
        assert_eq!(
            report(
                &runs,
                &Settings::default(),
                Focus::Away,
                &finished,
                0,
                |_, _| { panic!("posted with the setting off") }
            ),
            Ok(RunNotice::Nothing)
        );
    }

    #[test]
    fn test_a_run_the_user_watched_in_the_focused_window_posts_nothing() {
        let runs = OperationRuns::default();
        let finished = run(RunKind::Uninstall, 1, 0, 0);
        assert_eq!(
            report(&runs, &on(), Focus::Window, &finished, 0, |_, _| {
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
            report(&runs, &on(), Focus::App, &finished, 0, |_, _| panic!(
                "posted while Banager was in front"
            )),
            Ok(RunNotice::Withheld)
        );
        let posted = RefCell::new(Vec::new());
        assert_eq!(
            post_withheld(&runs, &on(), |run, password| {
                posted
                    .borrow_mut()
                    .push(body(MenuLanguage::ZhCn, run, password));
                Ok(())
            }),
            Ok(RunNotice::Post)
        );
        assert_eq!(*posted.borrow(), ["已更新3个工具"]);
        assert_eq!(
            post_withheld(&runs, &on(), |_, _| panic!("posted twice")),
            Ok(RunNotice::Nothing)
        );
    }

    #[test]
    fn test_a_withheld_run_is_not_posted_once_the_window_has_had_the_focus() {
        let runs = OperationRuns::default();
        let finished = run(RunKind::Uninstall, 2, 0, 0);
        report(&runs, &on(), Focus::App, &finished, 0, |_, _| Ok(())).unwrap();
        runs.0.lock().unwrap().seen();
        assert_eq!(
            post_withheld(&runs, &on(), |_, _| panic!("posted a run the user saw")),
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
            post_withheld(&runs, &on(), |_, _| panic!("nothing was withheld")),
            Ok(RunNotice::Nothing)
        );
        // The report, with the focus asked before Banager left.
        assert_eq!(
            report(&runs, &on(), Focus::App, &finished, 0, |_, _| panic!(
                "posted while Banager was in front"
            )),
            Ok(RunNotice::Withheld)
        );
        let posted = RefCell::new(Vec::new());
        assert_eq!(
            recheck(&runs, &on(), Focus::Away, |run, password| {
                posted
                    .borrow_mut()
                    .push(body(MenuLanguage::ZhCn, run, password));
                Ok(())
            }),
            Ok(RunNotice::Post)
        );
        assert_eq!(*posted.borrow(), ["已更新3个工具"]);
        assert_eq!(
            post_withheld(&runs, &on(), |_, _| panic!("posted twice")),
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
        report(&runs, &on(), Focus::App, &finished, 0, |_, _| Ok(())).unwrap();
        assert_eq!(
            recheck(&runs, &on(), Focus::App, |_, _| panic!(
                "posted while Banager was in front"
            )),
            Ok(RunNotice::Withheld)
        );
        let posted = RefCell::new(0);
        assert_eq!(
            post_withheld(&runs, &on(), |_, _| {
                *posted.borrow_mut() += 1;
                Ok(())
            }),
            Ok(RunNotice::Post)
        );
        assert_eq!(
            recheck(&runs, &on(), Focus::Away, |_, _| panic!("posted twice")),
            Ok(RunNotice::Nothing)
        );
        assert_eq!(
            recheck(&runs, &on(), Focus::App, |_, _| panic!("posted twice")),
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
        report(&runs, &on(), Focus::App, &finished, 0, |_, _| Ok(())).unwrap();
        assert_eq!(
            recheck(&runs, &on(), Focus::Window, |_, _| panic!(
                "posted a run the user saw"
            )),
            Ok(RunNotice::Watched)
        );
        assert_eq!(
            post_withheld(&runs, &on(), |_, _| panic!("posted a run the user saw")),
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
            assert_eq!(body(ZhCn, &finished, 0), zh);
            assert_eq!(body(En, &finished, 0), en);
        }
    }

    #[test]
    fn test_traditional_chinese_operation_notifications() {
        for (finished, expected) in [
            (run(RunKind::Upgrade, 3, 0, 0), "已更新3個工具"),
            (
                run(RunKind::Upgrade, 2, 1, 1),
                "2個已更新，1個未能更新，1個需要查看",
            ),
            (run(RunKind::Uninstall, 2, 0, 0), "已解除安裝2個工具"),
            (
                run(RunKind::Uninstall, 1, 1, 1),
                "1個已解除安裝，1個未能解除安裝，1個需要查看",
            ),
            (run(RunKind::Other, 3, 0, 0), "已完成3項操作"),
            (
                run(RunKind::Other, 1, 1, 1),
                "1項已完成，1項未能完成，1項需要查看",
            ),
        ] {
            assert_eq!(body(MenuLanguage::ZhHant, &finished, 0), expected);
        }
    }

    #[test]
    fn test_what_we_run_quotes_what_the_notification_says_in_every_language() {
        // docs/what-we-run.md, "The notification when operations finish":
        // the plain update case, one with failures, and one with updates
        // that stopped for the password, in all three languages, its
        // count as N.
        let doc = include_str!("../../docs/what-we-run.md");
        let folded = doc.split_whitespace().collect::<Vec<_>>().join(" ");
        for language in [MenuLanguage::En, MenuLanguage::ZhCn, MenuLanguage::ZhHant] {
            for (finished, password) in [
                (run(RunKind::Upgrade, 7, 0, 0), 0),
                (run(RunKind::Upgrade, 7, 7, 0), 0),
                (run(RunKind::Upgrade, 7, 7, 0), 7),
            ] {
                let quoted = body(language, &finished, password).replace('7', "N");
                assert!(
                    folded.contains(&quoted),
                    "docs/what-we-run.md does not quote {quoted:?}, which a notification says"
                );
            }
        }
    }

    /// Walk-4 W4-1's words, in the notification too: an Update all that
    /// stopped where sudo wanted the password says 「13个需要输入密码」, as
    /// the operation bar and the Updates page's headline do, never
    /// 「13个未能更新」. Other failures keep their words beside it; a run
    /// that is not all updates says what it says of failures.
    #[test]
    fn test_updates_that_stopped_for_the_password_are_said_as_the_window_says_them() {
        use MenuLanguage::{En, ZhCn, ZhHant};
        let all = run(RunKind::Upgrade, 0, 13, 0);
        assert_eq!(body(ZhCn, &all, 13), "13个需要输入密码");
        assert_eq!(body(ZhHant, &all, 13), "13個需要輸入密碼");
        assert_eq!(body(En, &all, 13), "13 need your password");
        assert_eq!(
            body(En, &run(RunKind::Upgrade, 0, 1, 0), 1),
            "1 needs your password"
        );
        let mixed = run(RunKind::Upgrade, 2, 3, 1);
        assert_eq!(
            body(ZhCn, &mixed, 2),
            "2个已更新，1个未能更新，2个需要输入密码，1个需要查看"
        );
        assert_eq!(
            body(ZhHant, &mixed, 2),
            "2個已更新，1個未能更新，2個需要輸入密碼，1個需要查看"
        );
        assert_eq!(
            body(En, &mixed, 2),
            "2 updated, 1 couldn't be updated, 2 need your password, 1 needs attention"
        );
        // Never more than failed, and only of a run of updates.
        assert_eq!(
            body(En, &run(RunKind::Upgrade, 1, 1, 0), 5),
            "1 updated, 1 needs your password"
        );
        assert_eq!(
            body(En, &run(RunKind::Other, 1, 2, 0), 2),
            "1 completed, 2 couldn't be completed"
        );
    }

    fn op(id: u64, kind: OpKind, outcome: Option<Outcome>) -> OpSummary {
        OpSummary {
            id,
            kind,
            instance_id: "brew:/opt/homebrew".into(),
            artifact_kind: banager_core::model::ArtifactKind::Cask,
            name: "tool".into(),
            status: if outcome.is_some() {
                OpStatus::Done
            } else {
                OpStatus::Running
            },
            outcome,
            argv_preview: vec![],
            env_preview: vec![],
            cancel_policy: banager_core::model::CancelPolicy::KillThenReconcile,
        }
    }

    /// A tool's failure, its cause read off `summary` as the runner reads
    /// it off what the tool wrote (`CommandOutput::failure_cause`).
    fn failed(summary: &str) -> Option<Outcome> {
        Some(Outcome::Failed {
            exit_code: Some(1),
            summary: summary.into(),
            cause: banager_core::history::failure_cause(summary),
        })
    }

    #[test]
    fn test_password_stops_are_told_of_only_of_finished_updates_in_the_run() {
        let sudo = "==> Installing tool\nsudo: a terminal is required to read the password; either use the -S option to read from standard input or configure an askpass helper";
        let known = Completions {
            operations: vec![
                op(7, OpKind::Upgrade, None),
                op(6, OpKind::Upgrade, failed(sudo)),
                op(5, OpKind::Upgrade, Some(Outcome::Succeeded)),
                op(4, OpKind::Uninstall, failed(sudo)),
                op(3, OpKind::Upgrade, failed("Error: Download failed")),
                op(2, OpKind::Upgrade, failed(sudo)),
                op(1, OpKind::Upgrade, failed(sudo)),
            ],
            ..Completions::default()
        };
        let runs = OperationRuns::default();
        let posted = RefCell::new(Vec::new());
        let report = |last_op| {
            report_known(
                &mut runs.0.lock().unwrap(),
                &known,
                last_op,
                true,
                Focus::Away,
                |run, password| {
                    posted
                        .borrow_mut()
                        .push(body(MenuLanguage::ZhCn, run, password));
                    Ok(())
                },
            )
        };
        assert_eq!(report(3), Some(Ok(RunNotice::Post)));
        // An uninstall is not said to need the password.
        assert_eq!(report(6), Some(Ok(RunNotice::Post)));
        assert_eq!(report(7), None, "still running");
        assert_eq!(
            *posted.borrow(),
            ["1个未能更新，2个需要输入密码", "1项已完成，2项未能完成"]
        );
    }

    /// Astra's final review, F1: the run is counted from one read of the
    /// operations (`Session::completions_after`), and so are its password
    /// stops -- an update evicted before the page reported its run was
    /// counted failed from what eviction kept, while its password cause
    /// was looked for in a second, later read that no longer held it, and
    /// the notification said it could not be updated.
    #[test]
    fn regression_an_evicted_update_that_stopped_for_the_password_is_told_of_as_one() {
        use banager_core::ops::{Ended, EvictedOp};
        let sudo = "==> Installing tool\nsudo: a terminal is required to read the password; either use the -S option to read from standard input or configure an askpass helper";
        let mut known = Completions {
            operations: vec![
                op(4, OpKind::Upgrade, failed(sudo)),
                op(3, OpKind::Upgrade, Some(Outcome::Succeeded)),
            ],
            ..Completions::default()
        };
        for (id, summary) in [(1, sudo), (2, "Error: Download failed")] {
            known.evicted.insert(
                id,
                EvictedOp {
                    kind: OpKind::Upgrade,
                    ended: Ended::of(&failed(summary).unwrap()),
                },
            );
        }
        let runs = OperationRuns::default();
        let posted = RefCell::new(Vec::new());
        let reported = report_known(
            &mut runs.0.lock().unwrap(),
            &known,
            4,
            true,
            Focus::Away,
            |run, password| {
                posted
                    .borrow_mut()
                    .push(body(MenuLanguage::ZhCn, run, password));
                Ok(())
            },
        );
        assert_eq!(reported, Some(Ok(RunNotice::Post)));
        assert_eq!(
            *posted.borrow(),
            ["1个已更新，1个未能更新，2个需要输入密码"]
        );
        // The same boundary again: nothing, and nothing counted twice.
        assert_eq!(
            report_known(
                &mut runs.0.lock().unwrap(),
                &known,
                4,
                true,
                Focus::Away,
                |_, _| panic!("posted twice")
            ),
            None
        );
    }

    /// A run withheld keeps its password stops until it is posted, with
    /// the run after it told of in the same notification; seen, they go
    /// with it.
    #[test]
    fn test_a_withheld_runs_password_stops_are_told_of_with_it_or_dropped_with_it() {
        let runs = OperationRuns::default();
        let first = FinishedRun {
            last_op: 2,
            ..run(RunKind::Upgrade, 0, 2, 0)
        };
        let second = FinishedRun {
            last_op: 4,
            ..run(RunKind::Upgrade, 1, 1, 0)
        };
        assert_eq!(
            report(&runs, &on(), Focus::App, &first, 2, |_, _| panic!(
                "posted while in front"
            )),
            Ok(RunNotice::Withheld)
        );
        let posted = RefCell::new(Vec::new());
        assert_eq!(
            report(&runs, &on(), Focus::Away, &second, 1, |run, password| {
                posted
                    .borrow_mut()
                    .push(body(MenuLanguage::ZhCn, run, password));
                Ok(())
            }),
            Ok(RunNotice::Post)
        );
        assert_eq!(*posted.borrow(), ["1个已更新，3个需要输入密码"]);
        // Withheld again, then seen: nothing of it is told of later.
        let third = FinishedRun {
            last_op: 5,
            ..run(RunKind::Upgrade, 0, 1, 0)
        };
        let fourth = FinishedRun {
            last_op: 6,
            ..run(RunKind::Upgrade, 0, 1, 0)
        };
        report(&runs, &on(), Focus::App, &third, 1, |_, _| Ok(())).unwrap();
        runs.0.lock().unwrap().seen();
        let later = RefCell::new(Vec::new());
        report(&runs, &on(), Focus::Away, &fourth, 0, |run, password| {
            later
                .borrow_mut()
                .push(body(MenuLanguage::En, run, password));
            Ok(())
        })
        .unwrap();
        assert_eq!(*later.borrow(), ["1 couldn't be updated"]);
    }
}
