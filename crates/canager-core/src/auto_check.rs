//! The daily check, Settings → Updates' 「每天自动检查」
//! (`Settings::auto_check`): Canager, left running, refreshes by itself
//! once a day. What is decided here is pure -- the clock, the last check
//! and whether anything is under way are handed in -- so every case can be
//! tested without waiting a day. The task that asks every [`TICK`] and runs
//! the round is the shell's (`check_automatically` in
//! `src-tauri/src/auto_check.rs`), since only the shell can announce a
//! refresh to the window.
//!
//! Also here: [`RoundTrigger`] and [`RoundLog`], which remember who asked
//! for each refresh round, so that code after a round can tell one the
//! daily check ran from one the window asked for, and when the last round
//! that counts as a check ended ([`counts_as_check`]).

use crate::model::InstanceNote;
use crate::session::Snapshot;
use std::collections::BTreeMap;
use std::time::Duration;

/// How often the shell's task asks [`tick`] whether the daily check is due.
/// Counted on tokio's clock, which is std's `Instant`: on macOS that reads
/// `CLOCK_UPTIME_RAW`, which stops while the Mac sleeps, so this is 15
/// minutes of the Mac being awake. The day itself is measured on the wall
/// clock ([`DUE_AFTER_SECS`]), so time asleep counts toward it, and a Mac
/// that slept through a day checks at the first tick after it wakes.
pub const TICK: Duration = Duration::from_secs(15 * 60);

/// A day, in seconds: how long after the last round that counts as a
/// check ended ([`counts_as_check`]) the daily check is due.
pub const DUE_AFTER_SECS: i64 = 24 * 60 * 60;

/// A minute, in seconds: how far before the last check's end the wall
/// clock may read and still be taken as no time at all, not as a clock set
/// back ([`tick`]). A time sync can step the clock back by a little, and a
/// round can end between a tick reading `now` and reading when the last
/// check ended.
pub const SET_BACK_SLACK_SECS: i64 = 60;

/// What one tick of the daily check does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tick {
    /// `Settings::auto_check` is off: nothing.
    Off,
    /// The last check ended less than [`DUE_AFTER_SECS`] ago: nothing.
    NotDue,
    /// Due, but a refresh or an operation is under way
    /// (`Session::busy`): nothing now, and the next tick asks again.
    Busy,
    /// Start a round.
    Check,
}

/// Whether the tick at `now` starts the daily check.
///
/// `now` and `last_check_ended` are Unix seconds on the wall clock, the
/// clock `Snapshot::refreshed_at` is stamped on -- and the stamp of the
/// last round that counts as a check is what the shell hands in as
/// `last_check_ended` (`RoundLog::last_check_ended`). A round of any
/// trigger counts (the window's check at launch, Check again, ⌘R, the
/// check after an operation, a daily one), stamped whether or not every
/// source answered, but for a daily one in which every source failed
/// ([`counts_as_check`]). So a check the user runs moves the next daily
/// one a day on, and a round that failed in part counts as the day's
/// check: a source that keeps failing is not asked again every 15 minutes,
/// and the window already says it failed. A daily round in which every
/// source failed leaves `last_check_ended` where it was, so the check is
/// still due at the next tick. `None` -- no round has counted since
/// Canager started -- is due. The stamp lives in memory, so after a
/// relaunch the window's check at launch is the day's.
///
/// A `now` [`SET_BACK_SLACK_SECS`] or more before `last_check_ended` is
/// due as well: the clock was set back past the last check, and waiting
/// for it to reach that check again plus a day could take as long as it
/// was set back. The round that runs then stamps the corrected time, so it
/// is one extra check, not one per tick. A `now` less than that before it
/// is not due: the clock stepped back by a little, or a round ended
/// between the tick reading `now` and reading `last_check_ended`, and in
/// neither is a day gone.
///
/// `busy` wins only over a check that is due, so that `Tick` says why
/// nothing ran.
pub fn tick(now: i64, last_check_ended: Option<i64>, busy: bool, auto_check: bool) -> Tick {
    if !auto_check {
        return Tick::Off;
    }
    let due = match last_check_ended {
        None => true,
        Some(ended) if now < ended => ended.saturating_sub(now) >= SET_BACK_SLACK_SECS,
        Some(ended) => now.saturating_sub(ended) >= DUE_AFTER_SECS,
    };
    if !due {
        Tick::NotDue
    } else if busy {
        Tick::Busy
    } else {
        Tick::Check
    }
}

/// Unix seconds now, on the wall clock: the `now` the shell's task hands
/// [`tick`] -- `std::time::SystemTime`, the clock `Session` stamps
/// `Snapshot::refreshed_at` with when no test clock is set.
pub fn wall_clock_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Whether a round counts as a check for the daily one: every round does
/// but a daily one (`RoundTrigger::Automatic`) in which every source failed
/// (`every_source_failed`) -- a round that reached nothing, after which the
/// next tick, 15 minutes on, checks again. A round that failed only in part
/// counts, and so does one of the window's however it went: the user saw
/// it, and asks again when they like.
pub fn counts_as_check(trigger: RoundTrigger, snapshot: &Snapshot) -> bool {
    trigger == RoundTrigger::Window || !every_source_failed(snapshot)
}

/// Whether every source `snapshot` has failed in its round: at least one
/// source, and an error in `Snapshot::errors` against each. A source that
/// is unavailable (`status.unavailable`) or whose catalogue was being
/// rewritten (`InstanceNote::IndexUpdating`) did not fail -- neither is an
/// error -- and a round that found no source failed nothing.
fn every_source_failed(snapshot: &Snapshot) -> bool {
    !snapshot.instances.is_empty()
        && snapshot.instances.iter().all(|inst| {
            snapshot
                .errors
                .iter()
                .any(|error| error.instance_id == inst.id)
        })
}

/// Who asked for a refresh round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundTrigger {
    /// The window, through the `refresh` command: its check at launch,
    /// Check again and ⌘R, the check after an operation, a Try again.
    Window,
    /// The daily check.
    Automatic,
}

/// Who asked for each refresh round the shell was handed, by the number
/// `Session::refresh_with_round` gave it, and whose the refresh is that a
/// `brew update` ending sets off.
///
/// A round two callers shared -- two calls that arrived before a round
/// started share it (`Session::refresh`) -- is the window's if either was
/// the window's: it is automatic only when nobody but the daily check
/// asked for it.
///
/// The refresh a background change sets off (`Session::background_change`,
/// which the shell's `refresh_on_background_change` waits on) belongs to
/// the round that started the `brew update` whose end woke it: that round
/// is the first to report the update still running
/// (`InstanceNote::IndexUpdating`) after one that reported none, since
/// only an update Canager started is reported that way. Every round after
/// it that the update outlasts reports it too, and leaves the owner as it
/// is; a round that reports none ends it. So a daily check whose `brew
/// update` outlasted it has its follow-up counted as automatic, and a
/// check of the user's as the window's.
#[derive(Debug, Default)]
pub struct RoundLog {
    triggers: BTreeMap<u64, RoundTrigger>,
    /// The newest round recorded. An older round's record, arriving late,
    /// may still add its caller to that round's trigger, but says nothing
    /// about whether a `brew update` is running now.
    newest: u64,
    /// The round that started the `brew update` still reported running.
    follow_up_owner: Option<u64>,
    /// When the newest round that counts as a check ended
    /// ([`counts_as_check`]): what [`tick`] measures the day from.
    last_check_ended: Option<i64>,
}

impl RoundLog {
    /// How many rounds before the newest one are remembered, besides the
    /// round `take_follow_up_trigger` will ask about, which is kept however
    /// old it gets.
    pub const KEPT: u64 = 64;

    /// Records that round `round`, whose result is `snapshot`, was handed
    /// to a caller that asked as `trigger`. When it is the newest round and
    /// counts as a check ([`counts_as_check`]), its end is the last check's
    /// ([`RoundLog::last_check_ended`]); an older round's record, arriving
    /// late, moves nothing.
    pub fn record(&mut self, round: u64, trigger: RoundTrigger, snapshot: &Snapshot) {
        let shared = match self.triggers.get(&round) {
            Some(RoundTrigger::Window) => RoundTrigger::Window,
            _ => trigger,
        };
        self.triggers.insert(round, shared);
        if round > self.newest {
            self.newest = round;
            if !reports_brew_update_running(snapshot) {
                self.follow_up_owner = None;
            } else if self.follow_up_owner.is_none() {
                self.follow_up_owner = Some(round);
            }
        }
        if round == self.newest && counts_as_check(shared, snapshot) {
            self.last_check_ended = snapshot.refreshed_at;
        }
        let oldest_kept = self.newest.saturating_sub(Self::KEPT);
        let owner = self.follow_up_owner;
        self.triggers
            .retain(|&r, _| r >= oldest_kept || Some(r) == owner);
    }

    /// Who asked for round `round`, when it was recorded and is still
    /// remembered.
    pub fn trigger_of(&self, round: u64) -> Option<RoundTrigger> {
        self.triggers.get(&round).copied()
    }

    /// When the last round that counts as a check ended
    /// ([`counts_as_check`]), on the wall clock: the `last_check_ended` the
    /// shell hands [`tick`]. `None` until one has been recorded.
    pub fn last_check_ended(&self) -> Option<i64> {
        self.last_check_ended
    }

    /// Who the refresh a background change is setting off runs for: the
    /// trigger of the round that started the `brew update` that ended, and
    /// `Window` when no round reports one running. Taken, not read: the
    /// wake-up is the update's end, and the refresh it sets off is its one
    /// follow-up.
    pub fn take_follow_up_trigger(&mut self) -> RoundTrigger {
        self.follow_up_owner
            .take()
            .and_then(|round| self.trigger_of(round))
            .unwrap_or(RoundTrigger::Window)
    }
}

/// Whether `snapshot` reports a `brew update` Canager started as still
/// running: an instance carrying `InstanceNote::IndexUpdating`.
fn reports_brew_update_running(snapshot: &Snapshot) -> bool {
    snapshot
        .instances
        .iter()
        .any(|inst| inst.status.notes.contains(&InstanceNote::IndexUpdating))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{InstanceStatus, ManagerInstance, Unavailable};
    use crate::session::{DetectOutcome, SourceError};

    const DAY: i64 = DUE_AFTER_SECS;
    /// 2026-09-28 09:00 UTC, an arbitrary wall-clock "now".
    const NINE_AM: i64 = 1_790_586_000;

    #[test]
    fn test_tick_is_fifteen_minutes_and_due_is_a_day() {
        assert_eq!(TICK, Duration::from_secs(900));
        assert_eq!(DUE_AFTER_SECS, 86_400);
        assert_eq!(SET_BACK_SLACK_SECS, 60);
    }

    #[test]
    fn test_a_now_less_than_a_minute_before_the_last_check_is_not_due() {
        // A time sync stepped the clock back a little, or a round ended
        // between the tick reading `now` and reading the last check.
        for behind in [1, 2, 30, SET_BACK_SLACK_SECS - 1] {
            assert_eq!(
                tick(NINE_AM - behind, Some(NINE_AM), false, true),
                Tick::NotDue,
                "{behind} s before the last check"
            );
        }
    }

    #[test]
    fn test_a_now_a_minute_or_more_before_the_last_check_checks_once() {
        for behind in [SET_BACK_SLACK_SECS, 10 * 60, 365 * DAY] {
            assert_eq!(
                tick(NINE_AM - behind, Some(NINE_AM), false, true),
                Tick::Check,
                "{behind} s before the last check"
            );
            // That check stamps the corrected clock: the ticks after it are
            // not due.
            let checked = NINE_AM - behind + 30;
            assert_eq!(
                tick(checked + 15 * 60, Some(checked), false, true),
                Tick::NotDue
            );
        }
    }

    #[test]
    fn test_nothing_runs_while_the_setting_is_off_however_long_ago_the_last_round_was() {
        for last in [None, Some(NINE_AM - 30 * DAY), Some(NINE_AM - 60)] {
            for busy in [false, true] {
                assert_eq!(tick(NINE_AM, last, busy, false), Tick::Off);
            }
        }
    }

    #[test]
    fn test_a_round_that_ended_less_than_a_day_ago_is_not_due() {
        assert_eq!(tick(NINE_AM, Some(NINE_AM - 60), false, true), Tick::NotDue);
        assert_eq!(
            tick(NINE_AM, Some(NINE_AM - DAY + 1), false, true),
            Tick::NotDue,
            "one second short of a day"
        );
    }

    #[test]
    fn test_a_day_after_the_last_round_ended_the_check_is_due() {
        assert_eq!(tick(NINE_AM, Some(NINE_AM - DAY), false, true), Tick::Check);
        assert_eq!(
            tick(NINE_AM, Some(NINE_AM - DAY - 1), false, true),
            Tick::Check
        );
    }

    #[test]
    fn test_no_round_since_launch_is_due() {
        // The window's check at launch has not ended (it would be busy
        // then) and none ever ran: nothing has been checked in this run.
        assert_eq!(tick(NINE_AM, None, false, true), Tick::Check);
    }

    #[test]
    fn test_a_mac_asleep_for_two_days_checks_at_the_first_tick_after_it_wakes_and_once() {
        // The last round ended at 9:00 on Monday; the Mac slept from 9:05
        // until Wednesday 9:00. The ticks' own clock stopped while it
        // slept, but the day is measured on the wall clock, so the first
        // tick after waking finds two days gone.
        let ended = NINE_AM;
        let woke = NINE_AM + 2 * DAY;
        assert_eq!(tick(woke + 60, Some(ended), false, true), Tick::Check);
        // That round stamps its own end; the ticks after it are not due.
        let checked = woke + 90;
        assert_eq!(
            tick(woke + 15 * 60, Some(checked), false, true),
            Tick::NotDue
        );
        assert_eq!(
            tick(woke + 30 * 60, Some(checked), false, true),
            Tick::NotDue
        );
    }

    #[test]
    fn test_a_due_check_waits_while_something_is_under_way_and_runs_at_the_next_free_tick() {
        let last = Some(NINE_AM - 2 * DAY);
        assert_eq!(tick(NINE_AM, last, true, true), Tick::Busy);
        assert_eq!(tick(NINE_AM + 15 * 60, last, true, true), Tick::Busy);
        assert_eq!(tick(NINE_AM + 30 * 60, last, false, true), Tick::Check);
    }

    #[test]
    fn test_busy_does_not_hide_that_nothing_was_due() {
        assert_eq!(tick(NINE_AM, Some(NINE_AM - 60), true, true), Tick::NotDue);
    }

    #[test]
    fn test_a_clock_set_back_past_the_last_round_checks_once_instead_of_waiting_for_it() {
        // The last round was stamped by a clock a year fast; the clock was
        // then corrected. Waiting for `now` to pass that stamp plus a day
        // would stop the daily check for a year.
        let stamped_by_the_fast_clock = NINE_AM + 365 * DAY;
        assert_eq!(
            tick(NINE_AM, Some(stamped_by_the_fast_clock), false, true),
            Tick::Check
        );
        // The round that runs stamps the corrected time.
        assert_eq!(
            tick(NINE_AM + 15 * 60, Some(NINE_AM + 60), false, true),
            Tick::NotDue
        );
    }

    #[test]
    fn test_wall_clock_now_is_the_system_clock_in_unix_seconds() {
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let now = wall_clock_now();
        assert!(now >= before && now - before < 5, "{now} vs {before}");
    }

    fn snapshot(brew_updating: bool) -> Snapshot {
        let notes = if brew_updating {
            vec![InstanceNote::IndexUpdating]
        } else {
            Vec::new()
        };
        Snapshot {
            generation: 1,
            round: 1,
            detect: DetectOutcome::Found,
            instances: vec![ManagerInstance {
                status: InstanceStatus {
                    unavailable: None,
                    notes,
                },
                ..crate::testing::manager_instance("brew", "brew:/opt/homebrew")
            }],
            artifacts: Vec::new(),
            updates: Vec::new(),
            refreshed_at: Some(NINE_AM),
            stale: false,
            errors: Vec::new(),
        }
    }

    #[test]
    fn test_a_round_is_remembered_by_its_number_with_who_asked_for_it() {
        let mut log = RoundLog::default();
        log.record(1, RoundTrigger::Window, &snapshot(false));
        log.record(2, RoundTrigger::Automatic, &snapshot(false));
        assert_eq!(log.trigger_of(1), Some(RoundTrigger::Window));
        assert_eq!(log.trigger_of(2), Some(RoundTrigger::Automatic));
        assert_eq!(log.trigger_of(3), None);
    }

    #[test]
    fn test_a_round_the_window_shared_with_the_daily_check_is_the_windows_whichever_records_first()
    {
        let mut log = RoundLog::default();
        log.record(4, RoundTrigger::Automatic, &snapshot(false));
        log.record(4, RoundTrigger::Window, &snapshot(false));
        assert_eq!(log.trigger_of(4), Some(RoundTrigger::Window));

        let mut log = RoundLog::default();
        log.record(4, RoundTrigger::Window, &snapshot(false));
        log.record(4, RoundTrigger::Automatic, &snapshot(false));
        assert_eq!(log.trigger_of(4), Some(RoundTrigger::Window));
    }

    #[test]
    fn test_the_follow_up_of_a_daily_checks_brew_update_is_automatic() {
        let mut log = RoundLog::default();
        log.record(1, RoundTrigger::Window, &snapshot(false));
        // The daily check starts a `brew update` that outlasts it.
        log.record(2, RoundTrigger::Automatic, &snapshot(true));
        // The user checks while it still runs: their round reports it too.
        log.record(3, RoundTrigger::Window, &snapshot(true));
        // The update ends; the refresh it sets off is the daily check's.
        assert_eq!(log.take_follow_up_trigger(), RoundTrigger::Automatic);
        log.record(4, RoundTrigger::Automatic, &snapshot(false));
        assert_eq!(log.trigger_of(4), Some(RoundTrigger::Automatic));
        // Taken: a second wake-up with no update reported is the window's.
        assert_eq!(log.take_follow_up_trigger(), RoundTrigger::Window);
    }

    #[test]
    fn test_the_follow_up_of_a_users_check_is_the_windows() {
        let mut log = RoundLog::default();
        log.record(1, RoundTrigger::Window, &snapshot(true));
        assert_eq!(log.take_follow_up_trigger(), RoundTrigger::Window);
    }

    #[test]
    fn test_a_round_that_reports_no_update_running_ends_the_one_that_started_it() {
        let mut log = RoundLog::default();
        log.record(1, RoundTrigger::Automatic, &snapshot(true));
        // The update ended and a round of the window's read the new
        // catalogue before the wake-up's refresh ran.
        log.record(2, RoundTrigger::Window, &snapshot(false));
        assert_eq!(log.take_follow_up_trigger(), RoundTrigger::Window);
        // A later update belongs to the round that starts it.
        log.record(3, RoundTrigger::Window, &snapshot(true));
        assert_eq!(log.take_follow_up_trigger(), RoundTrigger::Window);
    }

    #[test]
    fn test_a_late_record_of_an_older_round_says_nothing_about_the_update_now() {
        let mut log = RoundLog::default();
        log.record(5, RoundTrigger::Automatic, &snapshot(true));
        log.record(6, RoundTrigger::Window, &snapshot(true));
        // Round 4's second caller records after round 6: its snapshot, from
        // before the update started, does not end round 5's ownership.
        log.record(4, RoundTrigger::Window, &snapshot(false));
        assert_eq!(log.trigger_of(4), Some(RoundTrigger::Window));
        assert_eq!(log.take_follow_up_trigger(), RoundTrigger::Automatic);
    }

    #[test]
    fn test_only_the_last_rounds_are_remembered_but_the_follow_ups_owner_is_kept() {
        let mut log = RoundLog::default();
        log.record(1, RoundTrigger::Automatic, &snapshot(true));
        for round in 2..=200 {
            log.record(round, RoundTrigger::Window, &snapshot(true));
        }
        assert_eq!(log.trigger_of(200 - RoundLog::KEPT - 1), None);
        assert_eq!(
            log.trigger_of(200 - RoundLog::KEPT),
            Some(RoundTrigger::Window)
        );
        assert_eq!(log.trigger_of(1), Some(RoundTrigger::Automatic));
        assert!(log.triggers.len() <= RoundLog::KEPT as usize + 2);
        assert_eq!(log.take_follow_up_trigger(), RoundTrigger::Automatic);
        log.record(201, RoundTrigger::Window, &snapshot(false));
        assert_eq!(log.trigger_of(1), None, "no longer the owner, so forgotten");
    }

    /// The snapshot of a round that ended at `ended`, over `sources`: each
    /// instance's id, and whether it failed in the round.
    fn round_at(ended: i64, sources: &[(&str, bool)]) -> Snapshot {
        Snapshot {
            instances: sources
                .iter()
                .map(|(id, _)| crate::testing::manager_instance("fake", id))
                .collect(),
            refreshed_at: Some(ended),
            stale: sources.iter().any(|(_, failed)| *failed),
            errors: sources
                .iter()
                .filter(|(_, failed)| *failed)
                .map(|(id, _)| SourceError {
                    instance_id: id.to_string(),
                    message: "could not reach it".to_string(),
                })
                .collect(),
            ..snapshot(false)
        }
    }

    #[test]
    fn test_a_daily_round_in_which_every_source_failed_is_not_a_check() {
        let every_one = round_at(NINE_AM, &[("fake:1", true), ("fake:2", true)]);
        assert!(!counts_as_check(RoundTrigger::Automatic, &every_one));
        let one_of_two = round_at(NINE_AM, &[("fake:1", true), ("fake:2", false)]);
        assert!(
            counts_as_check(RoundTrigger::Automatic, &one_of_two),
            "a round that failed in part counts"
        );
        let clean = round_at(NINE_AM, &[("fake:1", false)]);
        assert!(counts_as_check(RoundTrigger::Automatic, &clean));
    }

    #[test]
    fn test_a_round_of_the_windows_counts_however_it_went() {
        let every_one = round_at(NINE_AM, &[("fake:1", true), ("fake:2", true)]);
        assert!(counts_as_check(RoundTrigger::Window, &every_one));
    }

    #[test]
    fn test_a_source_that_did_not_answer_or_no_source_at_all_is_no_failure() {
        // Unavailable (an Ollama not running) is a state the source
        // reported, not an error; nor is a round with no source a failed
        // one.
        let mut asleep = round_at(NINE_AM, &[("fake:1", true), ("fake:2", false)]);
        asleep.instances[1].status.unavailable = Some(Unavailable::NotRunning);
        assert!(counts_as_check(RoundTrigger::Automatic, &asleep));
        let none = round_at(NINE_AM, &[]);
        assert!(counts_as_check(RoundTrigger::Automatic, &none));
    }

    #[test]
    fn test_the_last_check_is_the_newest_round_that_counts() {
        let mut log = RoundLog::default();
        assert_eq!(log.last_check_ended(), None);
        log.record(
            1,
            RoundTrigger::Window,
            &round_at(NINE_AM, &[("fake:1", true)]),
        );
        assert_eq!(
            log.last_check_ended(),
            Some(NINE_AM),
            "the window's round counts even when its source failed"
        );
        log.record(
            2,
            RoundTrigger::Automatic,
            &round_at(NINE_AM + DAY, &[("fake:1", true)]),
        );
        assert_eq!(
            log.last_check_ended(),
            Some(NINE_AM),
            "a daily round in which every source failed moves nothing"
        );
        log.record(
            3,
            RoundTrigger::Automatic,
            &round_at(NINE_AM + DAY + 900, &[("fake:1", false)]),
        );
        assert_eq!(log.last_check_ended(), Some(NINE_AM + DAY + 900));
        // A late record of round 2, the window's second caller: it counts
        // now, but it is not the newest round, so it moves nothing.
        log.record(
            2,
            RoundTrigger::Window,
            &round_at(NINE_AM + DAY, &[("fake:1", true)]),
        );
        assert_eq!(log.last_check_ended(), Some(NINE_AM + DAY + 900));
    }

    #[test]
    fn test_a_failed_daily_round_the_window_shared_counts_as_the_windows() {
        let mut log = RoundLog::default();
        let failed = round_at(NINE_AM, &[("fake:1", true)]);
        log.record(4, RoundTrigger::Automatic, &failed);
        assert_eq!(log.last_check_ended(), None);
        log.record(4, RoundTrigger::Window, &failed);
        assert_eq!(log.last_check_ended(), Some(NINE_AM));
    }

    #[test]
    fn test_a_mac_that_wakes_offline_checks_at_every_tick_until_a_check_reaches_a_source() {
        // The last check ended at 9:00 on Monday; the Mac slept until
        // Wednesday 9:00, and wakes before its network does.
        let mut log = RoundLog::default();
        log.record(
            1,
            RoundTrigger::Window,
            &round_at(NINE_AM, &[("fake:1", false)]),
        );
        let woke = NINE_AM + 2 * DAY;
        assert_eq!(
            tick(woke + 60, log.last_check_ended(), false, true),
            Tick::Check
        );
        // Every source fails: not the day's check.
        log.record(
            2,
            RoundTrigger::Automatic,
            &round_at(woke + 90, &[("fake:1", true), ("fake:2", true)]),
        );
        assert_eq!(
            tick(woke + 60 + 15 * 60, log.last_check_ended(), false, true),
            Tick::Check,
            "the next tick, 15 minutes on, checks again"
        );
        // The network is back for one of them: that check counts.
        log.record(
            3,
            RoundTrigger::Automatic,
            &round_at(
                woke + 60 + 15 * 60 + 30,
                &[("fake:1", false), ("fake:2", true)],
            ),
        );
        assert_eq!(
            tick(woke + 60 + 30 * 60, log.last_check_ended(), false, true),
            Tick::NotDue
        );
        assert_eq!(
            tick(
                woke + 60 + 15 * 60 + 30 + DAY,
                log.last_check_ended(),
                false,
                true
            ),
            Tick::Check,
            "and the next is a day after it"
        );
    }
}
