//! Settings → Updates' 「有可更新时通知我」 (`Settings::notify_updates`),
//! under the daily check: one notification after a round of the daily
//! check, when the updates the Updates page offers to start include one
//! the user has not been told about. What is decided here is pure -- who
//! asked for the round, the settings, whether the window has the focus,
//! the updates the page offers and those told or seen before -- so every
//! case is tested without a notification. The shell hands them in each
//! time the page reports what it offers (`report_update_set` in
//! `src-tauri/src/notify.rs`), and posts.

use crate::auto_check::RoundTrigger;
use crate::settings::Settings;
use serde::Deserialize;
use std::collections::BTreeSet;

/// One update the Updates page offers to start -- one of the rows its
/// Update all would take -- as the page reports it: the key of its row, as
/// the page names it (`artifactKeyId` in src/store/ui.ts,
/// `instance_id|kind|name`), and the version the row offers. Compared and
/// counted here, never read: two pairs for one package differ when the
/// version does, so a newer version of a package the user was told about
/// is news again.
///
/// `UpdatePair` in src/lib/types.ts mirrors it; a shape test on each side
/// pins the JSON.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
pub struct UpdatePair {
    pub key_id: String,
    pub target: String,
}

/// The pairs this run of Canager has told the user about in a
/// notification, or that the user saw in the window. In memory only: after
/// Canager is quit and opened again, none has been.
#[derive(Debug, Default)]
pub struct Notified {
    pairs: BTreeSet<UpdatePair>,
}

impl Notified {
    /// Whether `pair` has been told or seen in this run.
    pub fn contains(&self, pair: &UpdatePair) -> bool {
        self.pairs.contains(pair)
    }

    /// Marks every pair of `updates` as told or seen.
    fn mark(&mut self, updates: &[UpdatePair]) {
        self.pairs.extend(updates.iter().cloned());
    }
}

/// What one report of the updates the page offers does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    /// Nothing is posted, and nothing marked.
    Nothing,
    /// The window has the focus, so the user sees what it offers: every
    /// pair of the report is marked, and nothing is posted.
    Seen,
    /// One notification saying `count` tools can be updated -- every
    /// update of the report, not only the new ones.
    Post { count: usize },
}

/// Whether notifications are on: 「有可更新时通知我」, and the daily check
/// it sits under. The Settings page shows the first off while the second
/// is off (src/pages/SettingsPage.tsx), whatever settings.json holds, so
/// both have to be.
pub fn notifications_on(settings: &Settings) -> bool {
    settings.auto_check && settings.notify_updates
}

/// What the page's report of `updates` does: the rows Update all would
/// take after a round, whose trigger is `round_trigger`.
///
/// - Nothing, when it offers no update.
/// - `Seen`, whenever the window has the focus: the user is looking at
///   what it offers -- whoever asked for the round, and whether or not
///   notifications are on.
/// - `Post`, when the round was the daily check's
///   (`RoundTrigger::Automatic`), notifications are on
///   (`notifications_on`), and a pair of `updates` has been neither told
///   nor seen in this run.
/// - Nothing otherwise: a round the window asked for, or one no longer
///   remembered (`round_trigger` `None`), never posts; nor does one
///   offering only what the user was told about or saw.
pub fn decide(
    round_trigger: Option<RoundTrigger>,
    notifications_on: bool,
    focused: bool,
    updates: &[UpdatePair],
    notified: &Notified,
) -> Notice {
    if updates.is_empty() {
        return Notice::Nothing;
    }
    if focused {
        return Notice::Seen;
    }
    if round_trigger != Some(RoundTrigger::Automatic) || !notifications_on {
        return Notice::Nothing;
    }
    if updates.iter().all(|pair| notified.contains(pair)) {
        return Notice::Nothing;
    }
    let whole: BTreeSet<&UpdatePair> = updates.iter().collect();
    Notice::Post { count: whole.len() }
}

/// A report's whole effect on `notified`: what `decide` answers, carried
/// out. `Seen` marks every pair of `updates`. `Post` calls `post` with its
/// count, and marks every pair once `post` returns `Ok`; when it fails,
/// nothing is marked, so the next round of the daily check that offers
/// them posts again, and its error is handed back for the caller to log.
pub fn report(
    notified: &mut Notified,
    round_trigger: Option<RoundTrigger>,
    notifications_on: bool,
    focused: bool,
    updates: &[UpdatePair],
    post: impl FnOnce(usize) -> Result<(), String>,
) -> Result<Notice, String> {
    let notice = decide(round_trigger, notifications_on, focused, updates, notified);
    match notice {
        Notice::Nothing => {}
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

    const AUTOMATIC: Option<RoundTrigger> = Some(RoundTrigger::Automatic);
    const WINDOW: Option<RoundTrigger> = Some(RoundTrigger::Window);

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
            decide(AUTOMATIC, true, false, &[jq(), gh()], &notified),
            Notice::Post { count: 2 },
            "gh is new; the count is every update offered, jq included"
        );
    }

    #[test]
    fn test_a_round_the_window_asked_for_never_posts() {
        let notified = Notified::default();
        assert_eq!(
            decide(WINDOW, true, false, &[jq()], &notified),
            Notice::Nothing
        );
        assert_eq!(
            decide(None, true, false, &[jq()], &notified),
            Notice::Nothing,
            "a round no longer remembered is not known to be the daily check's"
        );
    }

    #[test]
    fn test_nothing_is_posted_while_notifications_are_off() {
        assert_eq!(
            decide(AUTOMATIC, false, false, &[jq()], &Notified::default()),
            Notice::Nothing
        );
    }

    #[test]
    fn test_with_the_window_focused_the_updates_are_seen_whoever_asked_and_nothing_is_posted() {
        let notified = Notified::default();
        for trigger in [AUTOMATIC, WINDOW, None] {
            for on in [true, false] {
                assert_eq!(
                    decide(trigger, on, true, &[jq()], &notified),
                    Notice::Seen,
                    "{trigger:?}, notifications on: {on}"
                );
            }
        }
    }

    #[test]
    fn test_no_update_offered_is_nothing_even_with_the_window_focused() {
        let notified = Notified::default();
        assert_eq!(
            decide(AUTOMATIC, true, false, &[], &notified),
            Notice::Nothing
        );
        assert_eq!(
            decide(AUTOMATIC, true, true, &[], &notified),
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
                false,
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
                false,
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
                false,
                &[newer],
                recording(&posted)
            ),
            Ok(Notice::Post { count: 1 })
        );
        assert_eq!(*posted.borrow(), [1, 1]);
    }

    #[test]
    fn test_updates_seen_in_the_window_are_not_posted_later() {
        let mut notified = Notified::default();
        let never = |_| -> Result<(), String> { panic!("posted while the window had the focus") };
        assert_eq!(
            report(&mut notified, WINDOW, true, true, &[jq()], never),
            Ok(Notice::Seen)
        );
        assert!(notified.contains(&jq()));
        let never = |_| -> Result<(), String> { panic!("posted what the user saw") };
        assert_eq!(
            report(&mut notified, AUTOMATIC, true, false, &[jq()], never),
            Ok(Notice::Nothing)
        );
    }

    #[test]
    fn test_a_notification_that_failed_marks_nothing_and_the_next_round_posts_again() {
        let mut notified = Notified::default();
        assert_eq!(
            report(&mut notified, AUTOMATIC, true, false, &[jq(), gh()], |_| {
                Err("could not post".to_string())
            }),
            Err("could not post".to_string())
        );
        assert!(!notified.contains(&jq()) && !notified.contains(&gh()));

        let posted = RefCell::new(Vec::new());
        assert_eq!(
            report(
                &mut notified,
                AUTOMATIC,
                true,
                false,
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
            report(&mut notified, WINDOW, true, false, &[jq()], |_| {
                panic!("the window's round posted")
            }),
            Ok(Notice::Nothing)
        );
        assert!(!notified.contains(&jq()));
        assert_eq!(
            decide(AUTOMATIC, true, false, &[jq()], &notified),
            Notice::Post { count: 1 }
        );
    }

    #[test]
    fn test_the_count_is_of_distinct_updates() {
        assert_eq!(
            decide(
                AUTOMATIC,
                true,
                false,
                &[jq(), jq(), gh()],
                &Notified::default()
            ),
            Notice::Post { count: 2 }
        );
    }
}
