//! Settings → Updates' 「操作完成时通知」 (`Settings::notify_operations`,
//! off by default): one notification when a run of operations -- an
//! Update all, an update or an uninstall of one tool, and every operation
//! started while those were under way (`trackRun` in
//! src/lib/operations.ts) -- has finished while the user was not looking
//! at the window. Closing the window does not stop an operation: the page
//! goes on, hidden with the window, and hears it finish (src-tauri/src/window.rs).
//!
//! What is decided here is pure -- the setting, where the focus is
//! ([`Focus`], as the update notification has it), the run derived from completed backend
//! records ([`FinishedRun`]) and the runs already reported -- so every
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
/// interrupted removals with possible partial changes are Unconfirmed.
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

    /// This run and `later` told of in one notification: the newer of the
    /// two newest operations, each way they ended added up, and `Other`
    /// unless both did the same.
    pub fn and(&self, later: &FinishedRun) -> FinishedRun {
        FinishedRun {
            last_op: self.last_op.max(later.last_op),
            kind: if self.kind == later.kind {
                self.kind
            } else {
                RunKind::Other
            },
            succeeded: self.succeeded.saturating_add(later.succeeded),
            failed: self.failed.saturating_add(later.failed),
            attention: self.attention.saturating_add(later.attention),
        }
    }
}

/// The newest operation of the runs reported in this run of Banager, and
/// the run withheld while Banager was in front with its window not
/// focused, which waits for Banager to leave the front
/// ([`post_withheld`]) or for the window to take the focus
/// ([`ReportedRuns::seen`]). In memory only.
#[derive(Debug, Default)]
pub struct ReportedRuns {
    through: Option<u64>,
    withheld: Option<FinishedRun>,
}

impl ReportedRuns {
    /// The backend-owned interval is (last accepted boundary, last_op].
    /// Every ID must still exist and be finished. Page-supplied counts and
    /// kinds are never evidence, and a rejected boundary changes no state.
    pub fn completed(
        &self,
        last_op: u64,
        operations: &[crate::ops::OpSummary],
    ) -> Option<FinishedRun> {
        use crate::model::{OpKind, OpStatus, Outcome};
        let first = self.through.unwrap_or(0).checked_add(1)?;
        let count = last_op.checked_sub(first)?.checked_add(1)?;
        let selected: Vec<_> = operations
            .iter()
            .filter(|op| (first..=last_op).contains(&op.id))
            .collect();
        if selected.len() as u64 != count {
            return None;
        }
        let mut ids: Vec<_> = selected.iter().map(|op| op.id).collect();
        ids.sort_unstable();
        ids.dedup();
        if ids.len() != selected.len() {
            return None;
        }
        let mut run = FinishedRun {
            last_op,
            kind: RunKind::Other,
            succeeded: 0,
            failed: 0,
            attention: 0,
        };
        if selected.iter().all(|op| op.kind == OpKind::Upgrade) {
            run.kind = RunKind::Upgrade;
        } else if selected.iter().all(|op| op.kind == OpKind::Uninstall) {
            run.kind = RunKind::Uninstall;
        }
        for op in selected {
            if op.status != OpStatus::Done {
                return None;
            }
            match op.outcome.as_ref()? {
                Outcome::Succeeded => run.succeeded += 1,
                Outcome::Failed { .. } | Outcome::BanagerFailed(_) => run.failed += 1,
                Outcome::NeedsAttention(_) | Outcome::Unconfirmed => run.attention += 1,
                Outcome::Cancelled => {}
            }
        }
        Some(run)
    }

    /// Whether `run`, or a later one, has been reported: a page loaded
    /// again, or a report sent twice, posts nothing twice.
    pub fn has(&self, run: &FinishedRun) -> bool {
        self.through.is_some_and(|through| run.last_op <= through)
    }

    fn mark(&mut self, run: &FinishedRun) {
        self.through = Some(self.through.map_or(run.last_op, |t| t.max(run.last_op)));
    }

    /// The run waiting to be posted when Banager leaves the front, if
    /// any: every run withheld since the last post, told of together.
    pub fn withheld(&self) -> Option<&FinishedRun> {
        self.withheld.as_ref()
    }

    /// The window has taken the focus: the operation bar tells the user
    /// how the withheld run went, and it is never posted.
    pub fn seen(&mut self) {
        self.withheld = None;
    }

    fn withhold(&mut self, run: &FinishedRun) {
        self.withheld = Some(self.withheld.map_or(*run, |w| w.and(run)));
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
    /// ([`Focus::App`]) -- its window closed or in the Dock -- where macOS
    /// would show no banner: nothing is posted now. The run waits
    /// ([`ReportedRuns::withheld`]), and is posted once Banager leaves the
    /// front ([`post_withheld`]), unless the window takes the focus first.
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
/// calls `post` -- with a run withheld before told of in the same
/// notification -- and its error is handed back for the caller to log.
/// `Withheld` keeps the run waiting for Banager to leave the front
/// ([`post_withheld`]); `Watched` drops a run waiting, which the user now
/// sees on the operation bar. The run is marked as reported whatever the
/// answer, a failed post included: the page reports a run once.
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
    match notice {
        RunNotice::Nothing => {}
        RunNotice::Watched => reported.seen(),
        RunNotice::Withheld => reported.withhold(run),
        RunNotice::Post => {
            let whole = reported.withheld.take().map_or(*run, |w| w.and(run));
            post(&whole)?;
        }
    }
    Ok(notice)
}

/// Banager has left the front -- another app is now in front
/// ([`Focus::Away`]) -- and a run withheld while it was there
/// ([`RunNotice::Withheld`]) is posted now, while the setting is still on
/// (`on`). `Post` when it was, else `Nothing`; the run no longer waits
/// either way, a failed post included, whose error is handed back.
pub fn post_withheld(
    reported: &mut ReportedRuns,
    on: bool,
    post: impl FnOnce(&FinishedRun) -> Result<(), String>,
) -> Result<RunNotice, String> {
    let Some(run) = reported.withheld.take() else {
        return Ok(RunNotice::Nothing);
    };
    if !on {
        return Ok(RunNotice::Nothing);
    }
    post(&run)?;
    Ok(RunNotice::Post)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_completed_reports_use_only_finished_backend_outcomes_and_contiguous_membership() {
        use crate::model::{ArtifactKind, CancelPolicy, OpKind, OpStatus, Outcome};
        let mut op = crate::ops::OpSummary {
            id: 1,
            kind: OpKind::Uninstall,
            instance_id: "test".into(),
            artifact_kind: ArtifactKind::Binary,
            name: "tool".into(),
            status: OpStatus::Running,
            outcome: None,
            argv_preview: vec![],
            env_preview: vec![],
            cancel_policy: CancelPolicy::KillThenReconcile,
        };
        let mut reported = ReportedRuns::default();
        assert!(reported.completed(1, &[op.clone()]).is_none());
        op.status = OpStatus::Done;
        op.outcome = Some(Outcome::Unconfirmed);
        let real = reported.completed(1, &[op.clone()]).unwrap();
        assert_eq!(real, run(1, RunKind::Uninstall, 0, 0, 1));
        assert!(reported.completed(2, &[op.clone()]).is_none());
        assert!(reported.completed(0, &[op.clone()]).is_none());
        report(&mut reported, true, Focus::Away, &real, |_| Ok(())).unwrap();
        assert!(reported.completed(1, &[op.clone()]).is_none());
        op.id = 2;
        op.outcome = Some(Outcome::Succeeded);
        assert_eq!(
            reported.completed(2, &[op.clone()]),
            Some(run(2, RunKind::Uninstall, 1, 0, 0))
        );
        op.id = 3;
        assert!(
            reported.completed(3, &[op]).is_none(),
            "missing/evicted records cannot prove completion"
        );
    }

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
    fn test_with_banager_in_front_and_its_window_closed_nothing_is_posted_yet() {
        let reported = ReportedRuns::default();
        assert_eq!(
            decide(true, Focus::App, &three_updated(), &reported),
            RunNotice::Withheld
        );
    }

    #[test]
    fn test_a_run_withheld_with_the_window_closed_is_posted_once_banager_leaves_the_front() {
        // The window closed, Banager still in front: the user walked away.
        let mut reported = ReportedRuns::default();
        let never = |_: &FinishedRun| -> Result<(), String> { panic!("posted while in front") };
        assert_eq!(
            report(&mut reported, true, Focus::App, &three_updated(), never),
            Ok(RunNotice::Withheld)
        );
        assert_eq!(reported.withheld(), Some(&three_updated()));
        let posted = RefCell::new(Vec::new());
        assert_eq!(
            post_withheld(&mut reported, true, |run| {
                posted.borrow_mut().push(*run);
                Ok(())
            }),
            Ok(RunNotice::Post)
        );
        assert_eq!(*posted.borrow(), [three_updated()]);
        let never = |_: &FinishedRun| -> Result<(), String> { panic!("posted twice") };
        assert_eq!(
            post_withheld(&mut reported, true, never),
            Ok(RunNotice::Nothing),
            "leaving the front again posts nothing more"
        );
        assert_eq!(
            report(&mut reported, true, Focus::Away, &three_updated(), never),
            Ok(RunNotice::Nothing),
            "nor does the same run reported again"
        );
    }

    #[test]
    fn test_a_withheld_run_is_dropped_when_the_window_takes_the_focus_or_the_setting_goes_off() {
        let never = |_: &FinishedRun| -> Result<(), String> { panic!("posted") };
        let mut reported = ReportedRuns::default();
        report(&mut reported, true, Focus::App, &three_updated(), never).unwrap();
        reported.seen();
        assert_eq!(reported.withheld(), None);
        assert_eq!(
            post_withheld(&mut reported, true, never),
            Ok(RunNotice::Nothing),
            "the user saw it on the operation bar"
        );

        let mut reported = ReportedRuns::default();
        report(&mut reported, true, Focus::App, &three_updated(), never).unwrap();
        report(
            &mut reported,
            true,
            Focus::Window,
            &run(4, RunKind::Uninstall, 1, 0, 0),
            never,
        )
        .unwrap();
        assert_eq!(reported.withheld(), None, "a run watched in the window");

        let mut reported = ReportedRuns::default();
        report(&mut reported, true, Focus::App, &three_updated(), never).unwrap();
        assert_eq!(
            post_withheld(&mut reported, false, never),
            Ok(RunNotice::Nothing),
            "turned off since"
        );
        assert_eq!(reported.withheld(), None);
    }

    #[test]
    fn test_runs_withheld_one_after_another_are_told_of_in_one_notification() {
        let never = |_: &FinishedRun| -> Result<(), String> { panic!("posted while in front") };
        let mut reported = ReportedRuns::default();
        report(&mut reported, true, Focus::App, &three_updated(), never).unwrap();
        report(
            &mut reported,
            true,
            Focus::App,
            &run(5, RunKind::Upgrade, 1, 1, 0),
            never,
        )
        .unwrap();
        assert_eq!(
            reported.withheld(),
            Some(&run(5, RunKind::Upgrade, 4, 1, 0))
        );
        // Another run ending with another app in front takes the withheld
        // one with it.
        let posted = RefCell::new(Vec::new());
        assert_eq!(
            report(
                &mut reported,
                true,
                Focus::Away,
                &run(6, RunKind::Uninstall, 1, 0, 0),
                |run| {
                    posted.borrow_mut().push(*run);
                    Ok(())
                }
            ),
            Ok(RunNotice::Post)
        );
        assert_eq!(*posted.borrow(), [run(6, RunKind::Other, 5, 1, 0)]);
        assert_eq!(reported.withheld(), None);
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
    fn test_a_watched_run_is_still_reported_and_never_posted_later() {
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
