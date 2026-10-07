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

use crate::model::{OpKind, OpStatus};
use crate::notify_updates::Focus;
use crate::ops::{Completions, Ended};
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

/// A run [`ReportedRuns::accepted`] accepts: how its operations ended
/// (`run`), and of its updates that failed, how many stopped where sudo
/// wanted the Mac's password with no way to ask (`password`;
/// `Ended::NeedsPassword`) -- which the notification says as the
/// operation bar says it, 「N个需要输入密码」, not as failures. Both are
/// counted from the same records over the same interval, so an update
/// evicted before its run is reported is told of as it ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Accepted {
    pub run: FinishedRun,
    pub password: u32,
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
    /// Every ID must be known to have finished: a record that is `Done`,
    /// or one evicted since (`Completions::evicted`). Page-supplied counts
    /// and kinds are never evidence, and a rejected boundary changes no
    /// state. Only an ID evicted and then forgotten
    /// (`Completions::forgotten_through`) is past proving how it ended:
    /// the run is accepted telling of nothing, so it cannot hold back the
    /// runs after it. An ID taken by a submission whose record is not in
    /// yet (`Completions::unrecorded`) is unfinished, wherever the
    /// forgotten ones reach.
    ///
    /// Counted with it, from the same records: how many of its updates
    /// stopped for the password ([`Accepted::password`]); none of a run
    /// accepted telling of nothing.
    pub fn accepted(&self, last_op: u64, known: &Completions) -> Option<Accepted> {
        let through = self.through();
        let first = through.checked_add(1)?;
        let count = last_op.checked_sub(first)?.checked_add(1)?;
        if known
            .unrecorded
            .iter()
            .any(|id| (first..=last_op).contains(id))
        {
            return None;
        }
        let mut ended: Vec<(u64, OpKind, Ended)> = Vec::new();
        for op in known
            .operations
            .iter()
            .filter(|op| (first..=last_op).contains(&op.id))
        {
            if op.status != OpStatus::Done {
                return None;
            }
            ended.push((op.id, op.kind, Ended::of(op.outcome.as_ref()?)));
        }
        ended.extend(
            known
                .evicted
                .range(first..=last_op)
                .map(|(id, op)| (*id, op.kind, op.ended)),
        );
        let mut ids: Vec<_> = ended.iter().map(|(id, _, _)| *id).collect();
        ids.sort_unstable();
        ids.dedup();
        if ids.len() != ended.len() {
            return None;
        }
        let mut run = FinishedRun {
            last_op,
            kind: RunKind::Other,
            succeeded: 0,
            failed: 0,
            attention: 0,
        };
        if ended.len() as u64 != count {
            // Some IDs are in neither. Accepted only if every one is at or
            // below `forgotten_through` -- evicted, and forgotten since;
            // any other is unfinished, not submitted yet, or no operation.
            let forgotten = known.forgotten_through;
            let above = ended.iter().filter(|(id, _, _)| *id > forgotten).count() as u64;
            if above != last_op.saturating_sub(forgotten.max(through)) {
                return None;
            }
            return Some(Accepted { run, password: 0 });
        }
        if ended.iter().all(|(_, kind, _)| *kind == OpKind::Upgrade) {
            run.kind = RunKind::Upgrade;
        } else if ended.iter().all(|(_, kind, _)| *kind == OpKind::Uninstall) {
            run.kind = RunKind::Uninstall;
        }
        let mut password = 0;
        for (_, kind, how) in ended {
            match how {
                Ended::Succeeded => run.succeeded += 1,
                Ended::Failed => run.failed += 1,
                Ended::NeedsPassword => {
                    run.failed += 1;
                    // Of updates alone, as the operation bar says it.
                    if kind == OpKind::Upgrade {
                        password += 1;
                    }
                }
                Ended::Attention => run.attention += 1,
                Ended::Cancelled => {}
            }
        }
        Some(Accepted { run, password })
    }

    /// The run [`accepted`](Self::accepted) accepts, without its password
    /// stops.
    pub fn completed(&self, last_op: u64, known: &Completions) -> Option<FinishedRun> {
        self.accepted(last_op, known).map(|accepted| accepted.run)
    }

    /// The newest operation of the runs accepted so far, or 0: what the
    /// next run's interval starts after (`OperationManager::completions_after`).
    pub fn through(&self) -> u64 {
        self.through.unwrap_or(0)
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
    fn summary(id: u64, kind: OpKind, outcome: Option<Outcome>) -> OpSummary {
        OpSummary {
            id,
            kind,
            instance_id: "test".into(),
            artifact_kind: ArtifactKind::Binary,
            name: "tool".into(),
            status: if outcome.is_some() {
                OpStatus::Done
            } else {
                OpStatus::Running
            },
            outcome,
            argv_preview: vec![],
            env_preview: vec![],
            cancel_policy: CancelPolicy::KillThenReconcile,
            already_updated: None,
        }
    }

    fn records(operations: &[OpSummary]) -> Completions {
        Completions {
            operations: operations.to_vec(),
            ..Completions::default()
        }
    }

    fn evicted(kind: OpKind, ended: Ended) -> EvictedOp {
        EvictedOp { kind, ended }
    }

    #[test]
    fn test_completed_reports_use_only_finished_backend_outcomes_and_contiguous_membership() {
        let mut op = summary(1, OpKind::Uninstall, None);
        let mut reported = ReportedRuns::default();
        assert!(reported.completed(1, &records(&[op.clone()])).is_none());
        op.status = OpStatus::Done;
        op.outcome = Some(Outcome::Unconfirmed);
        let real = reported.completed(1, &records(&[op.clone()])).unwrap();
        assert_eq!(real, run(1, RunKind::Uninstall, 0, 0, 1));
        assert!(reported.completed(2, &records(&[op.clone()])).is_none());
        assert!(reported.completed(0, &records(&[op.clone()])).is_none());
        report(&mut reported, true, Focus::Away, &real, |_| Ok(())).unwrap();
        assert_eq!(reported.through(), 1);
        assert!(reported.completed(1, &records(&[op.clone()])).is_none());
        op.id = 2;
        op.outcome = Some(Outcome::Succeeded);
        assert_eq!(
            reported.completed(2, &records(&[op.clone()])),
            Some(run(2, RunKind::Uninstall, 1, 0, 0))
        );
        op.id = 3;
        assert!(
            reported.completed(3, &records(&[op])).is_none(),
            "a record neither held nor evicted cannot prove completion"
        );
    }

    #[test]
    fn regression_evicted_operations_are_counted_and_never_hold_back_later_runs() {
        // Records 1 and 2 evicted, 3 still held: the whole run is counted.
        let mut reported = ReportedRuns::default();
        let mut known = records(&[summary(3, OpKind::Upgrade, Some(Outcome::Succeeded))]);
        known
            .evicted
            .insert(1, evicted(OpKind::Upgrade, Ended::Failed));
        known
            .evicted
            .insert(2, evicted(OpKind::Upgrade, Ended::Attention));
        let whole = reported.completed(3, &known).unwrap();
        assert_eq!(whole, run(3, RunKind::Upgrade, 1, 1, 1));
        let posted = RefCell::new(Vec::new());
        let post = |run: &FinishedRun| {
            posted.borrow_mut().push(*run);
            Ok(())
        };
        report(&mut reported, true, Focus::Away, &whole, post).unwrap();
        // The next run, after more evictions: told of in its turn.
        known.operations = vec![summary(5, OpKind::Uninstall, Some(Outcome::Succeeded))];
        known
            .evicted
            .insert(4, evicted(OpKind::Uninstall, Ended::Cancelled));
        let next = reported.completed(5, &known).unwrap();
        assert_eq!(next, run(5, RunKind::Uninstall, 1, 0, 0));
        report(&mut reported, true, Focus::Away, &next, post).unwrap();
        assert_eq!(*posted.borrow(), [whole, next]);
    }

    /// What sudo prints when it wants the Mac's password and has no way to
    /// ask (`history::failure_cause`'s `NeedsPassword`).
    const SUDO: &str = "==> Installing tool\nsudo: a terminal is required to read the password; either use the -S option to read from standard input or configure an askpass helper";

    /// A tool's failure, its cause read off `summary` as the runner reads
    /// it off what the tool wrote (`CommandOutput::failure_cause`).
    fn failed(summary: &str) -> Option<Outcome> {
        Some(Outcome::Failed {
            exit_code: Some(1),
            summary: summary.into(),
            cause: crate::history::failure_cause(summary),
        })
    }

    #[test]
    fn regression_password_stops_are_counted_from_the_same_records_evicted_or_held() {
        // Astra's final review, F1: an update that stopped for the password
        // whose record was evicted before the run was reported was counted
        // as failed from the eviction ledger, and its password cause was
        // read off a second, later read of the operations, which no longer
        // held it: the notification said it could not be updated.
        let reported = ReportedRuns::default();
        let mut known = records(&[
            summary(5, OpKind::Upgrade, failed(SUDO)),
            summary(4, OpKind::Upgrade, Some(Outcome::Succeeded)),
            summary(3, OpKind::Uninstall, failed(SUDO)),
        ]);
        let sudo = Ended::of(&failed(SUDO).unwrap());
        assert_eq!(sudo, Ended::NeedsPassword);
        known.evicted.insert(1, evicted(OpKind::Upgrade, sudo));
        known.evicted.insert(
            2,
            evicted(
                OpKind::Upgrade,
                Ended::of(&failed("Error: Download failed").unwrap()),
            ),
        );
        assert_eq!(
            reported.accepted(5, &known),
            Some(Accepted {
                run: run(5, RunKind::Other, 1, 4, 0),
                // The evicted update and the held one; never the uninstall,
                // which the operation bar does not say it of.
                password: 2,
            })
        );
        // Only the interval accepted: through 2, the one evicted.
        assert_eq!(reported.accepted(2, &known).map(|a| a.password), Some(1));
        // A run past proving how it ended tells of no password stop either.
        known.forgotten_through = 1;
        known.evicted.remove(&1);
        assert_eq!(
            reported.accepted(2, &known),
            Some(Accepted {
                run: run(2, RunKind::Other, 0, 0, 0),
                password: 0,
            })
        );
    }

    #[test]
    fn regression_a_run_with_a_forgotten_operation_tells_of_nothing_but_lets_the_next_through() {
        let mut reported = ReportedRuns::default();
        let mut known = records(&[summary(4, OpKind::Upgrade, Some(Outcome::Succeeded))]);
        known
            .evicted
            .insert(3, evicted(OpKind::Upgrade, Ended::Succeeded));
        known.forgotten_through = 2;
        let never =
            |_: &FinishedRun| -> Result<(), String> { panic!("told of a run it cannot count") };
        let unknown = reported.completed(4, &known).unwrap();
        assert_eq!(unknown.told(), 0, "never a count short of the run");
        assert_eq!(
            report(&mut reported, true, Focus::Away, &unknown, never),
            Ok(RunNotice::Nothing)
        );
        assert_eq!(reported.through(), 4, "the boundary moves on all the same");
        known.operations = vec![summary(5, OpKind::Upgrade, Some(Outcome::Succeeded))];
        let next = reported.completed(5, &known).unwrap();
        let posted = RefCell::new(Vec::new());
        assert_eq!(
            report(&mut reported, true, Focus::Away, &next, |run| {
                posted.borrow_mut().push(*run);
                Ok(())
            }),
            Ok(RunNotice::Post)
        );
        assert_eq!(*posted.borrow(), [run(5, RunKind::Upgrade, 1, 0, 0)]);
    }

    #[test]
    fn regression_an_id_reserved_but_not_recorded_is_never_taken_for_a_forgotten_one() {
        let reported = ReportedRuns::default();
        let mut known = records(&[summary(4, OpKind::Upgrade, Some(Outcome::Succeeded))]);
        known
            .evicted
            .insert(3, evicted(OpKind::Upgrade, Ended::Succeeded));
        known.forgotten_through = 2;
        known.unrecorded = vec![1];
        assert!(reported.completed(1, &known).is_none());
        assert!(reported.completed(4, &known).is_none());
        // Without it, the same interval is forgotten, and accepted silently.
        known.unrecorded.clear();
        assert_eq!(reported.completed(4, &known).map(|run| run.told()), Some(0));
    }

    #[test]
    fn test_eviction_never_proves_what_is_unfinished_unknown_or_counted_twice() {
        let reported = ReportedRuns::default();
        // Still running, whatever was evicted around it.
        let mut known = records(&[summary(2, OpKind::Upgrade, None)]);
        known
            .evicted
            .insert(1, evicted(OpKind::Upgrade, Ended::Succeeded));
        known.forgotten_through = 1;
        assert!(reported.completed(2, &known).is_none());
        // An id above the forgotten ones that neither holds: not proven.
        let mut known = records(&[summary(4, OpKind::Upgrade, Some(Outcome::Succeeded))]);
        known.forgotten_through = 2;
        assert!(reported.completed(4, &known).is_none());
        // A boundary beyond every operation, however far.
        assert!(reported.completed(u64::MAX, &known).is_none());
        // One id both held and evicted is a contradiction.
        let mut known = records(&[summary(1, OpKind::Upgrade, Some(Outcome::Succeeded))]);
        known
            .evicted
            .insert(1, evicted(OpKind::Upgrade, Ended::Succeeded));
        assert!(reported.completed(1, &known).is_none());
    }

    use super::*;
    use crate::model::{ArtifactKind, CancelPolicy, Outcome};
    use crate::ops::{EvictedOp, OpSummary};
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
