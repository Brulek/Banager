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
use tauri::{AppHandle, Manager, State};

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
/// handed off is logged; the page is told nothing.
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
    let reported = report(&runs, &state.get_settings(), focus, &run, |run| {
        notify::post(&app, &title, &body(language, run), Answer::ShowWindow)
    });
    match reported {
        Ok(RunNotice::Post) => app.state::<NotificationPending>().set_window(),
        Ok(_) => {}
        Err(e) => {
            eprintln!("[banager] could not post the notification that operations finished: {e}")
        }
    }
    Ok(())
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
