//! Settings → Updates' 「操作完成时通知」 (`Settings::notify_operations`,
//! off by default): one notification when a run of operations -- an
//! Update all, an update or an uninstall of one tool, and every operation
//! started while those were under way (`trackRun` in
//! src/lib/operations.ts) -- has finished while the user was not looking
//! at the window. Closing the window does not stop an operation: the page
//! goes on, hidden with the window, and hears it finish (src-tauri/src/window.rs).
//!
//! What is decided here is pure -- the setting, where the focus is
//! ([`Focus`], as the update notification has it), the run as the page
//! reports it ([`FinishedRun`]) and the runs already reported -- so every
//! case is tested without a notification. The shell hands them in when the
//! page reports a run (`report_finished_run` in
//! src-tauri/src/notify_ops.rs), words the notification and posts it the
//! way the update notification is posted (`notify::post`).

use crate::notify_updates::Focus;
use serde::Deserialize;

/// What a run's operations did, as the notification words it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum RunKind {
    /// Every one an update: 「已更新3个工具」.
    Upgrade,
    /// Every one an uninstall: 「已卸载2个工具」.
    Uninstall,
    /// Anything else -- updates and uninstalls together, an install:
    /// 「已完成3项操作」.
    Other,
}

/// A run of operations that has finished, as the page reports it: the
/// number of its newest operation (`OpSummary::id`; the backend numbers
/// them in the order they were submitted), what they did, and how many
/// ended each way. An operation the user cancelled is in none of the three:
/// the user was there to cancel it, and it changed nothing to tell of.
///
/// `FinishedRun` in src/lib/types.ts mirrors it; a shape test on each side
/// pins the JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct FinishedRun {
    pub last_op: u64,
    pub kind: RunKind,
    /// `Outcome::Succeeded`.
    pub succeeded: u32,
    /// `Outcome::Failed` and `Outcome::BanagerFailed`: did not happen.
    pub failed: u32,
    /// `Outcome::NeedsAttention` and `Outcome::Unconfirmed`: the tool said
    /// it worked and Banager could not confirm it, or found otherwise.
    pub attention: u32,
}

impl FinishedRun {
    /// How many of its operations the notification tells of: all but the
    /// cancelled ones.
    pub fn told(&self) -> u32 {
        self.succeeded
            .saturating_add(self.failed)
            .saturating_add(self.attention)
    }
}

/// The newest operation of the runs reported in this run of Banager. In
/// memory only.
#[derive(Debug, Default)]
pub struct ReportedRuns {
    through: Option<u64>,
}

impl ReportedRuns {
    /// Whether `run`, or a later one, has been reported: a page loaded
    /// again, or a report sent twice, posts nothing twice.
    pub fn has(&self, run: &FinishedRun) -> bool {
        self.through.is_some_and(|through| run.last_op <= through)
    }

    fn mark(&mut self, run: &FinishedRun) {
        self.through = Some(self.through.map_or(run.last_op, |t| t.max(run.last_op)));
    }
}

/// What one report of a finished run does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunNotice {
    /// Nothing is posted: the setting is off, the run was reported before,
    /// or every operation of it was cancelled.
    Nothing,
    /// The window has the focus: the user watched it finish, on the
    /// operation bar. Nothing is posted, whatever the setting.
    Watched,
    /// Banager is the app in front without its window focused
    /// ([`Focus::App`]), where macOS would show no banner: nothing is
    /// posted, as the update notification posts nothing then.
    Withheld,
    /// One notification, telling of the run.
    Post,
}

/// What the report of `run` does:
///
/// - Nothing, when it was reported before ([`ReportedRuns::has`]) or every
///   operation of it was cancelled ([`FinishedRun::told`] is 0).
/// - `Watched`, whenever the window has the focus ([`Focus::Window`]):
///   never a notification for a run the user watched finish.
/// - Nothing, while 「操作完成时通知」 is off (`on`).
/// - `Withheld`, with Banager in front and its window not focused.
/// - `Post`, with another app in front ([`Focus::Away`]).
pub fn decide(on: bool, focus: Focus, run: &FinishedRun, reported: &ReportedRuns) -> RunNotice {
    if reported.has(run) || run.told() == 0 {
        return RunNotice::Nothing;
    }
    if focus == Focus::Window {
        return RunNotice::Watched;
    }
    if !on {
        return RunNotice::Nothing;
    }
    if focus == Focus::App {
        return RunNotice::Withheld;
    }
    RunNotice::Post
}

/// A report's whole effect: what `decide` answers, carried out. `Post`
/// calls `post`, and its error is handed back for the caller to log. The
/// run is marked as reported whatever the answer, a failed post included:
/// the page reports a run once, and a run is news only as it ends.
pub fn report(
    reported: &mut ReportedRuns,
    on: bool,
    focus: Focus,
    run: &FinishedRun,
    post: impl FnOnce(&FinishedRun) -> Result<(), String>,
) -> Result<RunNotice, String> {
    let notice = decide(on, focus, run, reported);
    if reported.has(run) {
        return Ok(notice);
    }
    reported.mark(run);
    if notice == RunNotice::Post {
        post(run)?;
    }
    Ok(notice)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn run(
        last_op: u64,
        kind: RunKind,
        succeeded: u32,
        failed: u32,
        attention: u32,
    ) -> FinishedRun {
        FinishedRun {
            last_op,
            kind,
            succeeded,
            failed,
            attention,
        }
    }

    fn three_updated() -> FinishedRun {
        run(3, RunKind::Upgrade, 3, 0, 0)
    }

    #[test]
    fn test_finished_run_is_the_json_the_page_sends() {
        // `FinishedRun` in src/lib/types.ts; the shape test in
        // src/lib/types.test.ts writes exactly this string.
        let parsed: FinishedRun = serde_json::from_str(
            r#"{"last_op":7,"kind":"Upgrade","succeeded":2,"failed":1,"attention":0}"#,
        )
        .expect("the page's run");
        assert_eq!(parsed, run(7, RunKind::Upgrade, 2, 1, 0));
        for (wire, kind) in [
            (r#""Upgrade""#, RunKind::Upgrade),
            (r#""Uninstall""#, RunKind::Uninstall),
            (r#""Other""#, RunKind::Other),
        ] {
            assert_eq!(serde_json::from_str::<RunKind>(wire).unwrap(), kind);
        }
    }

    #[test]
    fn test_a_run_that_finishes_while_another_app_is_in_front_posts_with_the_setting_on() {
        let reported = ReportedRuns::default();
        assert_eq!(
            decide(true, Focus::Away, &three_updated(), &reported),
            RunNotice::Post
        );
        assert_eq!(
            decide(false, Focus::Away, &three_updated(), &reported),
            RunNotice::Nothing,
            "off by default, and off means nothing"
        );
    }

    #[test]
    fn test_a_run_the_user_watched_finish_in_the_focused_window_never_posts() {
        let reported = ReportedRuns::default();
        for on in [true, false] {
            assert_eq!(
                decide(on, Focus::Window, &three_updated(), &reported),
                RunNotice::Watched
            );
        }
    }

    #[test]
    fn test_with_banager_in_front_and_its_window_closed_nothing_is_posted() {
        let reported = ReportedRuns::default();
        assert_eq!(
            decide(true, Focus::App, &three_updated(), &reported),
            RunNotice::Withheld
        );
    }

    #[test]
    fn test_a_run_whose_every_operation_was_cancelled_tells_of_nothing() {
        let reported = ReportedRuns::default();
        let cancelled = run(4, RunKind::Upgrade, 0, 0, 0);
        assert_eq!(cancelled.told(), 0);
        assert_eq!(
            decide(true, Focus::Away, &cancelled, &reported),
            RunNotice::Nothing
        );
        assert_eq!(
            decide(
                true,
                Focus::Away,
                &run(4, RunKind::Upgrade, 0, 1, 0),
                &reported
            ),
            RunNotice::Post,
            "one that failed is news"
        );
    }

    #[test]
    fn test_a_run_is_posted_once_however_often_it_is_reported() {
        let mut reported = ReportedRuns::default();
        let posted = RefCell::new(Vec::new());
        let post = |run: &FinishedRun| {
            posted.borrow_mut().push(run.last_op);
            Ok(())
        };
        assert_eq!(
            report(&mut reported, true, Focus::Away, &three_updated(), post),
            Ok(RunNotice::Post)
        );
        let never = |_: &FinishedRun| -> Result<(), String> { panic!("posted twice") };
        assert_eq!(
            report(&mut reported, true, Focus::Away, &three_updated(), never),
            Ok(RunNotice::Nothing)
        );
        assert_eq!(
            report(
                &mut reported,
                true,
                Focus::Away,
                &run(2, RunKind::Uninstall, 1, 0, 0),
                never
            ),
            Ok(RunNotice::Nothing),
            "an older run, reported late"
        );
        let post = |run: &FinishedRun| {
            posted.borrow_mut().push(run.last_op);
            Ok(())
        };
        assert_eq!(
            report(
                &mut reported,
                true,
                Focus::Away,
                &run(5, RunKind::Uninstall, 2, 0, 0),
                post
            ),
            Ok(RunNotice::Post)
        );
        assert_eq!(*posted.borrow(), [3, 5]);
    }

    #[test]
    fn test_a_watched_or_unposted_run_is_still_reported_and_never_posted_later() {
        let mut reported = ReportedRuns::default();
        let never = |_: &FinishedRun| -> Result<(), String> { panic!("posted a watched run") };
        assert_eq!(
            report(&mut reported, true, Focus::Window, &three_updated(), never),
            Ok(RunNotice::Watched)
        );
        assert!(reported.has(&three_updated()));
        let never = |_: &FinishedRun| -> Result<(), String> { panic!("posted it later") };
        assert_eq!(
            report(&mut reported, true, Focus::Away, &three_updated(), never),
            Ok(RunNotice::Nothing)
        );
    }

    #[test]
    fn test_a_post_that_failed_is_handed_back_and_not_tried_again() {
        let mut reported = ReportedRuns::default();
        assert_eq!(
            report(&mut reported, true, Focus::Away, &three_updated(), |_| Err(
                "no".to_string()
            )),
            Err("no".to_string())
        );
        assert!(reported.has(&three_updated()));
    }
}
