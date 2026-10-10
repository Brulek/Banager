//! Settings → Updates' 「有更新时通知我」 (`Settings::notify_updates`),
//! under the daily check: one notification after a round of the daily
//! check, when the updates the Updates page offers to start include one
//! the user has not been told about. What is decided here is pure -- what
//! is known of the round ([`ReportedRound`]), the settings, where the focus
//! is ([`Focus`]), the updates the page offers and those told or seen
//! before -- so every case is tested without a notification. The shell
//! hands them in each time the page reports what it offers
//! (`report_update_set` in `src-tauri/src/notify.rs`), and posts.

use crate::auto_check::RoundTrigger;
use crate::model::UpdateCandidate;
use crate::settings::Settings;
use serde::Deserialize;
use std::collections::BTreeSet;

/// One update the Updates page offers to start -- one of the rows its
/// Update all would take -- as the page reports it: the key of its row, as
/// the page names it (`artifactKeyId` in src/store/ui.ts,
/// `instance_id|kind|name`), and the version the row offers. Compared and
/// counted here, never read: two pairs for one package differ when the
/// version does, so a newer version of a package the user was told about
/// is news again -- and so is any update offered for a row after a round
/// that offered that row none (`Notified::forget_unoffered`), which a
/// version string alone cannot always tell apart: a Homebrew cask declared
/// `version :latest` offers "latest" for every release.
///
/// `UpdatePair` in src/lib/types.ts mirrors it; a shape test on each side
/// pins the JSON.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
pub struct UpdatePair {
    pub key_id: String,
    pub target: String,
}

/// The pairs this run of Banager has told the user about in a
/// notification, or that the user saw in the window, for the rows still
/// offered (`forget_unoffered`). In memory only: after Banager is quit and
/// opened again, none has been.
#[derive(Debug, Default)]
pub struct Notified {
    pairs: BTreeSet<UpdatePair>,
}

impl Notified {
    /// Whether `pair` has been told or seen in this run.
    pub fn contains(&self, pair: &UpdatePair) -> bool {
        self.pairs.contains(pair)
    }

    /// Forgets every pair whose row none of `candidates` -- the snapshot's
    /// update candidates -- has (`pair_of`'s key): a row no longer offered,
    /// because it was updated, uninstalled, or its source has gone. An
    /// update offered for that row later is news again, whatever version
    /// it names: a greedy `brew outdated` (a person's own
    /// `HOMEBREW_UPGRADE_GREEDY`; Banager passes no `--greedy`) lists a cask declared
    /// `version :latest` again whenever its download has changed, offering
    /// "latest" each time, so that pair alone could not tell one release
    /// from the next. A row still offered keeps its pairs, whatever version
    /// it offers now, so what was told stays told while it waits; so does
    /// the row of a source that did not answer the round, which the
    /// snapshot keeps (`Session::refresh`: a failed read, an unavailable
    /// source, one an operation holds). `report_offered`
    /// (src-tauri/src/notify.rs) calls this before each report.
    pub fn forget_unoffered(&mut self, candidates: &[UpdateCandidate]) {
        let rows: BTreeSet<String> = candidates
            .iter()
            .map(|candidate| pair_of(candidate).key_id)
            .collect();
        self.pairs.retain(|pair| rows.contains(&pair.key_id));
    }

    /// Marks every pair of `updates` as told or seen.
    fn mark(&mut self, updates: &[UpdatePair]) {
        self.pairs.extend(updates.iter().cloned());
    }
}

/// Where the focus is as a report comes, which the shell asks macOS
/// (`focus` in src-tauri/src/notify.rs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    /// Banager's window has the focus: the user sees what it offers.
    Window,
    /// Banager is the active app, the one in front, but its window does not
    /// have the focus -- it is closed, or in the Dock. macOS shows no
    /// banner for a notification of the app in front, so one posted now
    /// would go unseen; nor does the user see what the window offers.
    App,
    /// Another app is in front.
    Away,
}

/// What is known of the round a report came from, which the shell reads
/// off its `RoundLog` by the round's number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReportedRound {
    /// Who asked for it (`RoundLog::trigger_of`); `None` when it is no
    /// longer remembered.
    pub trigger: Option<RoundTrigger>,
    /// Whether it reported a `brew update` still running whose follow-up
    /// refresh is the daily check's (`RoundLog::awaits_follow_up`).
    pub awaits_follow_up: bool,
}

/// What one report of the updates the page offers does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    /// Nothing is posted, and nothing marked.
    Nothing,
    /// The window has the focus, so the user sees what it offers: every
    /// pair of the report is marked, and nothing is posted.
    Seen,
    /// A notification was due, but the round reported its `brew update`
    /// still running, and the refresh that update's end sets off is the
    /// daily check's too ([`ReportedRound::awaits_follow_up`]): nothing is
    /// posted now, and nothing marked, so that the daily check posts once,
    /// when that refresh reports -- the updates offered then, which are
    /// those offered now and those the update's new catalogue adds.
    Deferred,
    /// A notification was due, but Banager is the app in front without its
    /// window focused ([`Focus::App`]), where macOS would show no banner:
    /// nothing is posted, and nothing marked, so the next round of the
    /// daily check that offers the same updates posts them.
    Withheld,
    /// One notification saying `count` tools can be updated -- every
    /// update of the report, not only the new ones.
    Post { count: usize },
}

/// Whether notifications are on: 「有更新时通知我」, and the daily check
/// it sits under. The Settings page shows the first off while the second
/// is off (src/pages/SettingsPage.tsx), whatever settings.json holds, so
/// both have to be.
pub fn notifications_on(settings: &Settings) -> bool {
    settings.auto_check && settings.notify_updates
}

/// What the page's report of `updates` does: the rows Update all would
/// take after `round`.
///
/// - Nothing, when it offers no update.
/// - `Seen`, whenever the window has the focus ([`Focus::Window`]): the
///   user is looking at what it offers -- whoever asked for the round, and
///   whether or not notifications are on.
/// - `Post`, when the round was the daily check's
///   (`RoundTrigger::Automatic`), notifications are on
///   (`notifications_on`), a pair of `updates` has been neither told nor
///   seen in this run, the round awaits no follow-up of the daily check's,
///   and another app is in front ([`Focus::Away`]).
/// - `Deferred`, when all that holds but the round awaits such a
///   follow-up ([`ReportedRound::awaits_follow_up`]).
/// - `Withheld`, when all that holds but Banager is the app in front
///   without its window focused ([`Focus::App`]).
/// - Nothing otherwise: a round the window asked for, or one no longer
///   remembered (`trigger` `None`), never posts; nor does one offering
///   only what the user was told about or saw.
pub fn decide(
    round: ReportedRound,
    notifications_on: bool,
    focus: Focus,
    updates: &[UpdatePair],
    notified: &Notified,
) -> Notice {
    if updates.is_empty() {
        return Notice::Nothing;
    }
    if focus == Focus::Window {
        return Notice::Seen;
    }
    if round.trigger != Some(RoundTrigger::Automatic) || !notifications_on {
        return Notice::Nothing;
    }
    if updates.iter().all(|pair| notified.contains(pair)) {
        return Notice::Nothing;
    }
    if round.awaits_follow_up {
        return Notice::Deferred;
    }
    if focus == Focus::App {
        return Notice::Withheld;
    }
    let whole: BTreeSet<&UpdatePair> = updates.iter().collect();
    Notice::Post { count: whole.len() }
}

/// The pair the page names `candidate` by (`updatePairOf` in
/// src/lib/updateNotification.ts): its key as `artifactKeyId` in
/// src/store/ui.ts spells it, `instance_id|kind|name`, the kind as its
/// serde name, and the version it offers.
pub fn pair_of(candidate: &UpdateCandidate) -> UpdatePair {
    let kind = serde_json::to_value(candidate.key.kind)
        .ok()
        .and_then(|kind| kind.as_str().map(str::to_string))
        .unwrap_or_default();
    UpdatePair {
        key_id: format!(
            "{}|{}|{}",
            candidate.key.instance_id, kind, candidate.key.name
        ),
        target: candidate.target.clone(),
    }
}

/// The pairs of `updates` -- as the page reports them -- that `candidates`,
/// the snapshot's update candidates, offer (`pair_of`), each once: what
/// `report_update_set` (src-tauri/src/notify.rs) hands `report`. A pair no
/// candidate offers is no update to tell of, and is neither counted in a
/// notification nor kept among those told, so what the page reports can
/// neither post news the snapshot does not have nor grow `Notified` past
/// the snapshot's own candidates.
pub fn offered(updates: &[UpdatePair], candidates: &[UpdateCandidate]) -> Vec<UpdatePair> {
    let offers: BTreeSet<UpdatePair> = candidates.iter().map(pair_of).collect();
    let reported: BTreeSet<&UpdatePair> = updates.iter().filter(|p| offers.contains(p)).collect();
    reported.into_iter().cloned().collect()
}

/// A report's whole effect on `notified`: what `decide` answers, carried
/// out. `Seen` marks every pair of `updates`; `Nothing`, `Deferred` and
/// `Withheld` mark none. `Post` calls `post` with its count, and marks
/// every pair once `post` returns `Ok`; when it fails, nothing is marked,
/// so the next round of the daily check that offers them posts again, and
/// its error is handed back for the caller to log.
pub fn report(
    notified: &mut Notified,
    round: ReportedRound,
    notifications_on: bool,
    focus: Focus,
    updates: &[UpdatePair],
    post: impl FnOnce(usize) -> Result<(), String>,
) -> Result<Notice, String> {
    let notice = decide(round, notifications_on, focus, updates, notified);
    match notice {
        Notice::Nothing | Notice::Deferred | Notice::Withheld => {}
        Notice::Seen => notified.mark(updates),
        Notice::Post { count } => {
            post(count)?;
            notified.mark(updates);
        }
    }
    Ok(notice)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn pair(key_id: &str, target: &str) -> UpdatePair {
        UpdatePair {
            key_id: key_id.to_string(),
            target: target.to_string(),
        }
    }

    fn jq() -> UpdatePair {
        pair("brew:/opt/homebrew|Formula|jq", "1.8.1")
    }

    fn gh() -> UpdatePair {
        pair("brew:/opt/homebrew|Formula|gh", "2.102.0")
    }

    const AUTOMATIC: ReportedRound = ReportedRound {
        trigger: Some(RoundTrigger::Automatic),
        awaits_follow_up: false,
    };
    const WINDOW: ReportedRound = ReportedRound {
        trigger: Some(RoundTrigger::Window),
        awaits_follow_up: false,
    };
    /// A round no longer remembered.
    const FORGOTTEN: ReportedRound = ReportedRound {
        trigger: None,
        awaits_follow_up: false,
    };
    /// A round of the daily check's whose `brew update` outlasted it.
    const AWAITING: ReportedRound = ReportedRound {
        trigger: Some(RoundTrigger::Automatic),
        awaits_follow_up: true,
    };

    #[test]
    fn test_only_pairs_a_candidate_of_the_snapshot_offers_are_reported_each_once() {
        use crate::model::{ArtifactKey, ArtifactKind, UpdateChannel};
        let candidate = |kind, name: &str, target: &str| UpdateCandidate {
            key: ArtifactKey {
                instance_id: "brew:/opt/homebrew".to_string(),
                kind,
                name: name.to_string(),
            },
            current: "1.0".to_string(),
            target: target.to_string(),
            channel: UpdateChannel::Native,
            checkable: true,
            warnings: Vec::new(),
            blocked: None,
            download_bytes: None,
        };
        let candidates = [
            candidate(ArtifactKind::Formula, "jq", "1.8.1"),
            candidate(ArtifactKind::Formula, "gh", "2.102.0"),
        ];
        // The page's spelling of a candidate (`artifactKeyId`).
        assert_eq!(pair_of(&candidates[0]), jq());
        let reported = [
            jq(),
            jq(),
            // Not offered: another version, another kind, another name.
            pair("brew:/opt/homebrew|Formula|gh", "9.9.9"),
            pair("brew:/opt/homebrew|Cask|jq", "1.8.1"),
            pair("brew:/opt/homebrew|Formula|evil", "1.0"),
        ];
        assert_eq!(offered(&reported, &candidates), [jq()]);
        assert_eq!(offered(&reported, &[]), Vec::<UpdatePair>::new());
    }

    #[test]
    fn test_update_pair_is_the_json_the_page_sends() {
        // `UpdatePair` in src/lib/types.ts; the shape test in
        // src/lib/types.test.ts writes exactly this string.
        let parsed: UpdatePair =
            serde_json::from_str(r#"{"key_id":"brew:/opt/homebrew|Formula|jq","target":"1.8.1"}"#)
                .expect("the page's pair");
        assert_eq!(parsed, jq());
    }

    #[test]
    fn test_notifications_are_on_only_with_the_daily_check_they_sit_under() {
        let with = |auto_check, notify_updates| Settings {
            auto_check,
            notify_updates,
            ..Settings::default()
        };
        assert!(notifications_on(&with(true, true)));
        assert!(!notifications_on(&with(true, false)));
        assert!(
            !notifications_on(&with(false, true)),
            "the Settings page shows the switch off while the daily check is off"
        );
        assert!(!notifications_on(&with(false, false)));
        assert!(!notifications_on(&Settings::default()), "off by default");
    }

    #[test]
    fn test_a_daily_check_that_finds_a_new_update_while_the_window_is_away_posts_the_whole_count() {
        let mut notified = Notified::default();
        notified.mark(&[jq()]);
        assert_eq!(
            decide(AUTOMATIC, true, Focus::Away, &[jq(), gh()], &notified),
            Notice::Post { count: 2 },
            "gh is new; the count is every update offered, jq included"
        );
    }

    #[test]
    fn test_a_round_the_window_asked_for_never_posts() {
        let notified = Notified::default();
        assert_eq!(
            decide(WINDOW, true, Focus::Away, &[jq()], &notified),
            Notice::Nothing
        );
        assert_eq!(
            decide(FORGOTTEN, true, Focus::Away, &[jq()], &notified),
            Notice::Nothing,
            "a round no longer remembered is not known to be the daily check's"
        );
    }

    #[test]
    fn test_nothing_is_posted_while_notifications_are_off() {
        assert_eq!(
            decide(AUTOMATIC, false, Focus::Away, &[jq()], &Notified::default()),
            Notice::Nothing
        );
    }

    #[test]
    fn test_with_the_window_focused_the_updates_are_seen_whoever_asked_and_nothing_is_posted() {
        let notified = Notified::default();
        for trigger in [AUTOMATIC, WINDOW, FORGOTTEN, AWAITING] {
            for on in [true, false] {
                assert_eq!(
                    decide(trigger, on, Focus::Window, &[jq()], &notified),
                    Notice::Seen,
                    "{trigger:?}, notifications on: {on}"
                );
            }
        }
    }

    #[test]
    fn test_with_banager_in_front_and_its_window_away_nothing_is_posted_or_marked() {
        // Banager is the active app, its window closed or in the Dock:
        // macOS would show no banner, and the user sees nothing the window
        // offers.
        let mut notified = Notified::default();
        let never = |_| -> Result<(), String> { panic!("posted while Banager was in front") };
        assert_eq!(
            report(&mut notified, AUTOMATIC, true, Focus::App, &[jq()], never),
            Ok(Notice::Withheld)
        );
        assert!(!notified.contains(&jq()), "not seen, so not marked");
        // The next daily check that offers it, with another app in front,
        // posts it.
        assert_eq!(
            decide(AUTOMATIC, true, Focus::Away, &[jq()], &notified),
            Notice::Post { count: 1 }
        );
    }

    #[test]
    fn test_banager_in_front_withholds_only_a_notification_that_was_due() {
        let mut notified = Notified::default();
        assert_eq!(
            decide(WINDOW, true, Focus::App, &[jq()], &notified),
            Notice::Nothing,
            "the window's round posts nothing anyway"
        );
        assert_eq!(
            decide(AUTOMATIC, false, Focus::App, &[jq()], &notified),
            Notice::Nothing,
            "notifications off"
        );
        assert_eq!(
            decide(AUTOMATIC, true, Focus::App, &[], &notified),
            Notice::Nothing,
            "nothing offered"
        );
        notified.mark(&[jq()]);
        assert_eq!(
            decide(AUTOMATIC, true, Focus::App, &[jq()], &notified),
            Notice::Nothing,
            "nothing new"
        );
    }

    #[test]
    fn test_a_daily_check_whose_brew_update_outlasts_it_posts_once_after_its_follow_up() {
        // The daily check's round finds jq; its `brew update` is still
        // running, and the refresh that update's end sets off is the daily
        // check's too. That round waits, the follow-up finds gh besides,
        // and one notification counts both.
        let mut notified = Notified::default();
        let posted = RefCell::new(Vec::new());
        assert_eq!(
            report(
                &mut notified,
                AWAITING,
                true,
                Focus::Away,
                &[jq()],
                recording(&posted)
            ),
            Ok(Notice::Deferred)
        );
        assert!(!notified.contains(&jq()), "deferred, so not marked");
        assert_eq!(
            report(
                &mut notified,
                AUTOMATIC,
                true,
                Focus::Away,
                &[jq(), gh()],
                recording(&posted)
            ),
            Ok(Notice::Post { count: 2 })
        );
        assert_eq!(
            *posted.borrow(),
            [2],
            "one notification for the daily check"
        );
        assert!(notified.contains(&jq()) && notified.contains(&gh()));
    }

    #[test]
    fn test_a_deferred_round_still_marks_what_the_focused_window_shows() {
        let mut notified = Notified::default();
        assert_eq!(
            report(
                &mut notified,
                AWAITING,
                true,
                Focus::Window,
                &[jq()],
                |_| { panic!("posted while the window had the focus") }
            ),
            Ok(Notice::Seen)
        );
        assert!(notified.contains(&jq()));
        // Nothing new, nothing deferred.
        assert_eq!(
            decide(AWAITING, true, Focus::Away, &[jq()], &notified),
            Notice::Nothing
        );
    }

    #[test]
    fn test_no_update_offered_is_nothing_even_with_the_window_focused() {
        let notified = Notified::default();
        assert_eq!(
            decide(AUTOMATIC, true, Focus::Away, &[], &notified),
            Notice::Nothing
        );
        assert_eq!(
            decide(AUTOMATIC, true, Focus::Window, &[], &notified),
            Notice::Nothing
        );
    }

    /// A poster that records each count it is handed, and succeeds.
    fn recording(posted: &RefCell<Vec<usize>>) -> impl FnOnce(usize) -> Result<(), String> + '_ {
        move |count| {
            posted.borrow_mut().push(count);
            Ok(())
        }
    }

    #[test]
    fn test_updates_told_about_are_not_posted_again_but_a_newer_version_is() {
        let mut notified = Notified::default();
        let posted = RefCell::new(Vec::new());
        assert_eq!(
            report(
                &mut notified,
                AUTOMATIC,
                true,
                Focus::Away,
                &[jq()],
                recording(&posted)
            ),
            Ok(Notice::Post { count: 1 })
        );
        assert_eq!(
            report(
                &mut notified,
                AUTOMATIC,
                true,
                Focus::Away,
                &[jq()],
                recording(&posted)
            ),
            Ok(Notice::Nothing),
            "the next day's check finds the same jq"
        );
        let newer = pair("brew:/opt/homebrew|Formula|jq", "1.8.2");
        assert_eq!(
            report(
                &mut notified,
                AUTOMATIC,
                true,
                Focus::Away,
                &[newer],
                recording(&posted)
            ),
            Ok(Notice::Post { count: 1 })
        );
        assert_eq!(*posted.borrow(), [1, 1]);
    }

    /// The snapshot's candidate for `pair` -- a Homebrew formula or cask of
    /// `brew:/opt/homebrew`, as `pair_of` names it.
    fn candidate_for(pair: &UpdatePair) -> UpdateCandidate {
        use crate::model::{ArtifactKey, ArtifactKind, UpdateChannel};
        let (instance_id, rest) = pair.key_id.split_once('|').unwrap();
        let (kind, name) = rest.split_once('|').unwrap();
        let candidate = UpdateCandidate {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: match kind {
                    "Cask" => ArtifactKind::Cask,
                    _ => ArtifactKind::Formula,
                },
                name: name.to_string(),
            },
            current: "1.0".to_string(),
            target: pair.target.clone(),
            channel: UpdateChannel::Native,
            checkable: true,
            warnings: Vec::new(),
            blocked: None,
            download_bytes: None,
        };
        assert_eq!(&pair_of(&candidate), pair, "the page's spelling");
        candidate
    }

    #[test]
    fn test_a_latest_cask_updated_and_offered_again_is_news_again() {
        // r38 S4: a Homebrew app declared `version :latest`, listed with
        // a greedy `brew outdated`, offers "latest" for every release.
        let chrome = pair("brew:/opt/homebrew|Cask|google-chrome", "latest");
        let mut notified = Notified::default();
        let posted = RefCell::new(Vec::new());
        // Monday's daily check: news.
        notified.forget_unoffered(&[candidate_for(&chrome)]);
        assert_eq!(
            report(
                &mut notified,
                AUTOMATIC,
                true,
                Focus::Away,
                std::slice::from_ref(&chrome),
                recording(&posted)
            ),
            Ok(Notice::Post { count: 1 })
        );
        // The next day's check, not updated yet: still told.
        notified.forget_unoffered(&[candidate_for(&chrome)]);
        assert_eq!(
            report(
                &mut notified,
                AUTOMATIC,
                true,
                Focus::Away,
                std::slice::from_ref(&chrome),
                recording(&posted)
            ),
            Ok(Notice::Nothing)
        );
        // Updated: the next round offers it no update.
        notified.forget_unoffered(&[]);
        assert_eq!(
            report(
                &mut notified,
                AUTOMATIC,
                true,
                Focus::Away,
                &[],
                recording(&posted)
            ),
            Ok(Notice::Nothing)
        );
        assert!(!notified.contains(&chrome), "forgotten with its row");
        // Next week's release: the same pair, and news.
        notified.forget_unoffered(&[candidate_for(&chrome)]);
        assert_eq!(
            report(
                &mut notified,
                AUTOMATIC,
                true,
                Focus::Away,
                std::slice::from_ref(&chrome),
                recording(&posted)
            ),
            Ok(Notice::Post { count: 1 })
        );
        assert_eq!(*posted.borrow(), [1, 1]);
    }

    #[test]
    fn test_only_the_pairs_of_rows_no_longer_offered_are_forgotten() {
        let mut notified = Notified::default();
        let newer_jq = pair("brew:/opt/homebrew|Formula|jq", "1.8.2");
        notified.mark(&[jq(), gh()]);
        // jq is offered at a newer version, gh no longer at all.
        notified.forget_unoffered(&[candidate_for(&newer_jq)]);
        assert!(
            notified.contains(&jq()),
            "its row is still offered: what was told of it stays told"
        );
        assert!(!notified.contains(&gh()), "its row is gone");
        // Seen pairs are forgotten the same way.
        notified.mark(&[gh()]);
        notified.forget_unoffered(&[]);
        assert!(!notified.contains(&jq()) && !notified.contains(&gh()));
    }

    #[test]
    fn test_updates_seen_in_the_window_are_not_posted_later() {
        let mut notified = Notified::default();
        let never = |_| -> Result<(), String> { panic!("posted while the window had the focus") };
        assert_eq!(
            report(&mut notified, WINDOW, true, Focus::Window, &[jq()], never),
            Ok(Notice::Seen)
        );
        assert!(notified.contains(&jq()));
        let never = |_| -> Result<(), String> { panic!("posted what the user saw") };
        assert_eq!(
            report(&mut notified, AUTOMATIC, true, Focus::Away, &[jq()], never),
            Ok(Notice::Nothing)
        );
    }

    #[test]
    fn test_a_notification_that_failed_marks_nothing_and_the_next_round_posts_again() {
        let mut notified = Notified::default();
        assert_eq!(
            report(
                &mut notified,
                AUTOMATIC,
                true,
                Focus::Away,
                &[jq(), gh()],
                |_| { Err("could not post".to_string()) }
            ),
            Err("could not post".to_string())
        );
        assert!(!notified.contains(&jq()) && !notified.contains(&gh()));

        let posted = RefCell::new(Vec::new());
        assert_eq!(
            report(
                &mut notified,
                AUTOMATIC,
                true,
                Focus::Away,
                &[jq(), gh()],
                recording(&posted)
            ),
            Ok(Notice::Post { count: 2 })
        );
        assert_eq!(*posted.borrow(), [2]);
        assert!(notified.contains(&jq()) && notified.contains(&gh()));
    }

    #[test]
    fn test_nothing_marks_nothing() {
        // A round of the window's while the window is away: neither told
        // nor seen, so the next daily check may still post it.
        let mut notified = Notified::default();
        assert_eq!(
            report(&mut notified, WINDOW, true, Focus::Away, &[jq()], |_| {
                panic!("the window's round posted")
            }),
            Ok(Notice::Nothing)
        );
        assert!(!notified.contains(&jq()));
        assert_eq!(
            decide(AUTOMATIC, true, Focus::Away, &[jq()], &notified),
            Notice::Post { count: 1 }
        );
    }

    #[test]
    fn test_the_count_is_of_distinct_updates() {
        assert_eq!(
            decide(
                AUTOMATIC,
                true,
                Focus::Away,
                &[jq(), jq(), gh()],
                &Notified::default()
            ),
            Notice::Post { count: 2 }
        );
    }
}
