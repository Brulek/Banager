//! The automatic check, Settings → Updates' 「检查更新」 popup
//! (`Settings::auto_check`, and `Settings::auto_check_every`): Banager,
//! left running, refreshes by itself once a day, or once a week
//! ([`CheckEvery`]). Called "the daily check" throughout, for its first
//! and default choice: every rule here holds for the weekly one too, with
//! a week in place of the day ([`CheckEvery::due_after_secs`]). What is
//! decided here is pure -- the clock, the last check,
//! the daily checks that have failed since it and whether anything is
//! under way are handed in -- so every case can be tested without waiting
//! a day. The task that asks every [`TICK`] and runs the round is the
//! shell's (`check_automatically` in `src-tauri/src/auto_check.rs`), since
//! only the shell can announce a refresh to the window.
//!
//! Also here: [`RoundTrigger`] and [`RoundLog`], which remember who asked
//! for each refresh round, so that code after a round can tell one the
//! daily check ran from one the window asked for, when the last round
//! that counts as a check ended ([`counts_as_check`]), and how many daily
//! checks in which every source failed have run in a row since
//! ([`FailedChecks`]).

use crate::model::InstanceNote;
use crate::session::Snapshot;
pub use crate::settings::CheckEvery;
use std::collections::{BTreeMap, BTreeSet};
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

/// A week, in seconds: how long after the last round that counts as a
/// check ended the weekly check is due ([`CheckEvery::Week`]).
pub const WEEKLY_DUE_AFTER_SECS: i64 = 7 * DUE_AFTER_SECS;

impl CheckEvery {
    /// How long after the last round that counts as a check ended
    /// ([`counts_as_check`]) the check is due: [`DUE_AFTER_SECS`] for
    /// 「每天」, [`WEEKLY_DUE_AFTER_SECS`] for 「每周」. Only this differs
    /// between the two: the retries after failed checks
    /// ([`retry_after_secs`]), and everything else, are the same.
    pub fn due_after_secs(self) -> i64 {
        match self {
            CheckEvery::Day => DUE_AFTER_SECS,
            CheckEvery::Week => WEEKLY_DUE_AFTER_SECS,
        }
    }
}

/// A minute, in seconds: how far before the last check's end, or before
/// the look that started the last daily check that failed
/// ([`FailedChecks::looked_at`]), the wall clock may read and still be
/// taken as no time at all, not as a clock set back ([`tick`]). A time sync
/// can step the clock back by a little, and a round can end between a tick
/// reading `now` and reading when the last check ended.
pub const SET_BACK_SLACK_SECS: i64 = 60;

/// Fifteen minutes, in seconds: how long after the look that started a
/// daily check in which every source failed ([`counts_as_check`]) the next
/// may start, when it is the first such check in a row. Each more in a row
/// doubles the wait, up to [`RETRY_CAP_SECS`] ([`retry_after_secs`]). It is
/// one [`TICK`], so the first retry is the next look.
pub const RETRY_FIRST_SECS: i64 = 15 * 60;

/// Six hours, in seconds: the longest wait between two daily checks in a
/// row in which every source failed ([`retry_after_secs`]), reached after
/// the sixth. A Mac on which every source keeps failing -- one that stays
/// offline, say -- is then checked four times a day, not at every look.
pub const RETRY_CAP_SECS: i64 = 6 * 60 * 60;

/// A minute, in seconds: how far short of a retry's wait
/// ([`retry_after_secs`]) the time since the look that started the failed
/// check may fall, at the look meant to start the retry, and still start
/// it ([`tick`]). The looks come every [`TICK`] of the Mac being awake,
/// while the wait is measured on the wall clock, which a time sync can slow
/// or step back by a little: the look one wait after the failed check's
/// can find a second or so less than the wait gone, and would otherwise put
/// the retry off by a whole look.
pub const RETRY_SLACK_SECS: i64 = 60;

/// How long after the look that started the last of `in_a_row` daily
/// checks in a row in which every source failed ([`counts_as_check`]) the
/// next may start, in seconds: [`RETRY_FIRST_SECS`] after the first,
/// doubling with each more -- 15, 30, 60, 120, 240 minutes -- and
/// [`RETRY_CAP_SECS`] from the sixth on. `in_a_row` is 1 or more; 0 is
/// taken as 1.
pub fn retry_after_secs(in_a_row: u32) -> i64 {
    // Bounded before shifting: 15 minutes doubled 32 times is far past the
    // cap, and still far inside an `i64`.
    let doublings = in_a_row.saturating_sub(1).min(32);
    (RETRY_FIRST_SECS << doublings).min(RETRY_CAP_SECS)
}

/// The daily checks in which every source failed ([`counts_as_check`])
/// that have run in a row since the last round that counts as a check:
/// what [`tick`] spaces the next daily check out by. Kept by [`RoundLog`]
/// ([`RoundLog::failed_checks`]), in memory like the rest, so a relaunch
/// forgets them with the last check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FailedChecks {
    /// When the look that started the last of them ran: the `now` at which
    /// [`tick`] answered [`Tick::Check`], Unix seconds on the wall clock.
    pub looked_at: i64,
    /// How many have run in a row: 1 or more.
    pub in_a_row: u32,
}

/// What one tick of the daily check does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tick {
    /// `Settings::auto_check` is off, 「不自动检查」: nothing.
    Off,
    /// The last check ended less than a day ago -- a week, for the weekly
    /// check ([`CheckEvery::due_after_secs`]): nothing.
    NotDue,
    /// Due by the day, but daily checks in which every source failed have
    /// run since the last check, and the wait after the last of them
    /// ([`retry_after_secs`]) has not passed: nothing now.
    BackingOff,
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
/// still due; `failed` -- the daily checks in which every source failed
/// that have run in a row since (`RoundLog::failed_checks`) -- spaces the
/// next one out: it starts at the first tick at which the wait after as
/// many ([`retry_after_secs`]), less [`RETRY_SLACK_SECS`], has passed
/// since the look that started the last of them -- the look 15 minutes
/// after it, then 30, 60, 120 and 240 minutes after, and 360 from then on.
/// The wait is measured on the wall clock, so time the Mac spends asleep
/// counts toward it as it does toward the day. A round that counts clears
/// `failed`. `None` for `last_check_ended` -- no round has counted since
/// Banager started -- is due. Both live in memory, so after a relaunch the
/// window's check at launch is the day's.
///
/// A `now` [`SET_BACK_SLACK_SECS`] or more before `last_check_ended` is
/// due as well: the clock was set back past the last check, and waiting
/// for it to reach that check again plus a day could take as long as it
/// was set back. So is a `now` that far before the look that started the
/// last failed daily check, whatever the wait after it. The round that
/// runs then stamps the corrected time -- its end, when it counts, which
/// the day is measured from, and its look, when it fails, which the wait
/// is -- so a clock set back costs an extra check, not one per tick. A
/// `now` less than that before either is not due: the clock stepped back
/// by a little, or a round ended between the tick reading `now` and
/// reading `last_check_ended`, and in neither is a day, or a wait, gone.
///
/// `busy` wins only over a check that is due, so that `Tick` says why
/// nothing ran.
///
/// `every` is the setting as it is saved now
/// (`Settings::auto_check_schedule`): `None`, 「不自动检查」, is
/// [`Tick::Off`]; with [`CheckEvery::Week`] the day above is a week
/// ([`CheckEvery::due_after_secs`]), and nothing else changes.
pub fn tick(
    now: i64,
    last_check_ended: Option<i64>,
    failed: Option<FailedChecks>,
    busy: bool,
    every: Option<CheckEvery>,
) -> Tick {
    let Some(every) = every else {
        return Tick::Off;
    };
    if !last_check_ended.is_none_or(|ended| waited(now, ended, every.due_after_secs())) {
        return Tick::NotDue;
    }
    let retry_due = failed.is_none_or(|failed| {
        waited(
            now,
            failed.looked_at,
            retry_after_secs(failed.in_a_row) - RETRY_SLACK_SECS,
        )
    });
    if !retry_due {
        Tick::BackingOff
    } else if busy {
        Tick::Busy
    } else {
        Tick::Check
    }
}

/// When the daily check is next due, in Unix seconds on the wall clock:
/// the first `now` at or after which [`tick`] -- handed the same
/// `last_check_ended` and `failed`, nothing under way and the setting on,
/// checking `every` day or week --
/// answers [`Tick::Check`], for a clock that moves forward. That is a day
/// after the last round that counts as a check ended ([`counts_as_check`]:
/// one of the window's -- the check at launch, Check again, ⌘R -- resets
/// it as a daily one does), and, after daily checks in which every source
/// failed, no sooner than the wait after the last of them
/// ([`retry_after_secs`], less [`RETRY_SLACK_SECS`]). `None` when no round
/// has counted and none has failed: due at the next look.
///
/// The check itself starts at the first look at or after this time --
/// looks come every [`TICK`] of the Mac being awake -- and only while
/// Banager runs and nothing else is under way, so Settings says it as
/// "about" ("约").
pub fn next_check_due(
    last_check_ended: Option<i64>,
    failed: Option<FailedChecks>,
    every: CheckEvery,
) -> Option<i64> {
    let by_day = last_check_ended.map(|ended| ended.saturating_add(every.due_after_secs()));
    let by_retry = failed.map(|failed| {
        failed
            .looked_at
            .saturating_add(retry_after_secs(failed.in_a_row) - RETRY_SLACK_SECS)
    });
    match (by_day, by_retry) {
        (Some(day), Some(retry)) => Some(day.max(retry)),
        (day, retry) => day.or(retry),
    }
}

/// Whether `wait` seconds have passed on the wall clock from `then` to
/// `now`. A `now` [`SET_BACK_SLACK_SECS`] or more before `then` has them:
/// the clock was set back past `then` ([`tick`]). One less than that
/// before it has not.
fn waited(now: i64, then: i64, wait: i64) -> bool {
    if now < then {
        then.saturating_sub(now) >= SET_BACK_SLACK_SECS
    } else {
        now.saturating_sub(then) >= wait
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
/// (`every_source_failed`), after which the check is still due, but waits:
/// 15 minutes after the look that started that round, when it is the first
/// such in a row, and longer after each more ([`retry_after_secs`]). A
/// round that failed only in part counts, and so does one of the window's
/// however it went: the user saw it, and asks again when they like.
pub fn counts_as_check(trigger: RoundTrigger, snapshot: &Snapshot) -> bool {
    trigger == RoundTrigger::Window || !every_source_failed(snapshot)
}

/// Whether every source `snapshot` has failed in its round: at least one
/// source, and each with an error in `Snapshot::errors` against it or with
/// its catalogue update failed (`InstanceNote::IndexMayBeStale`,
/// Homebrew's: a `brew outdated` that answers after a failed `brew
/// update`, as it does on a Mac that is offline, reads the catalogue that
/// update could not renew, and the source carries the note and no error).
/// A source that is unavailable (`status.unavailable`) or whose catalogue
/// was being rewritten (`InstanceNote::IndexUpdating`) did not fail, and a
/// round that found no source failed nothing.
fn every_source_failed(snapshot: &Snapshot) -> bool {
    !snapshot.instances.is_empty()
        && snapshot.instances.iter().all(|inst| {
            inst.status.notes.contains(&InstanceNote::IndexMayBeStale)
                || snapshot
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
/// `Session::refresh_recording` gave it -- recorded before that round's
/// snapshot is committed, so no reader of the snapshot meets a round not
/// recorded yet -- and whose the refresh is that a `brew update` ending
/// sets off.
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
/// only an update Banager started is reported that way. Every round after
/// it that the update outlasts reports it too, and leaves the owner as it
/// is; a round that reports none ends it. So a daily check that reports
/// its `brew update` still running has its follow-up counted as automatic,
/// and a check of the user's as the window's -- and a round that reports
/// the update running while its follow-up is the daily check's waits for
/// that follow-up to say what the daily check found
/// ([`RoundLog::awaits_follow_up`]).
///
/// Who the follow-up belongs to is decided as the follow-up's own round is
/// recorded ([`RoundLog::record_follow_up`]), not as the wake-up comes.
/// The round that started the update is recorded only as it commits, and
/// the update can end before that: its Homebrew stops waiting for the
/// update after two minutes, and a slow source can keep the round going
/// after that. The follow-up's round starts after the wake-up, and one
/// round runs at a time (`Session::refresh`), so by the time the
/// follow-up's round is recorded, the round that started the update has
/// been.
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
    /// The daily checks in which every source failed that have run in a
    /// row since the newest round that counts ([`RoundLog::failed_checks`]):
    /// what [`tick`] spaces the next one out by.
    failed: Option<FailedChecks>,
    /// The rounds that reported a `brew update` still running whose
    /// follow-up is the daily check's ([`RoundLog::awaits_follow_up`]).
    awaiting: BTreeSet<u64>,
}

impl RoundLog {
    /// How many rounds before the newest one are remembered, besides the
    /// round that started the `brew update` still reported running, whose
    /// trigger the next follow-up takes ([`RoundLog::record_follow_up`]),
    /// which is kept however old it gets.
    pub const KEPT: u64 = 64;

    /// Records that round `round`, whose result is `snapshot`, was handed
    /// to a caller that asked as `trigger`. When it is the newest round, it
    /// also says whether it awaits a follow-up of the daily check's
    /// ([`RoundLog::awaits_follow_up`]) and, when it counts as a check
    /// ([`counts_as_check`]), its end is the last check's
    /// ([`RoundLog::last_check_ended`]) and the daily checks that failed
    /// before it are forgotten ([`RoundLog::failed_checks`]); an older
    /// round's record, arriving late, moves none of these.
    ///
    /// A round recorded here adds no failed daily check, whoever asked for
    /// it: the daily check's own rounds are recorded by
    /// [`RoundLog::record_daily`], which knows the look that started them.
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
        if round == self.newest {
            if counts_as_check(shared, snapshot) {
                self.last_check_ended = snapshot.refreshed_at;
                self.failed = None;
            }
            let follow_up = self
                .follow_up_owner
                .and_then(|owner| self.trigger_of(owner));
            if reports_brew_update_running(snapshot) && follow_up == Some(RoundTrigger::Automatic) {
                self.awaiting.insert(round);
            } else {
                self.awaiting.remove(&round);
            }
        }
        let oldest_kept = self.newest.saturating_sub(Self::KEPT);
        let owner = self.follow_up_owner;
        self.triggers
            .retain(|&r, _| r >= oldest_kept || Some(r) == owner);
        self.awaiting.retain(|&r| r >= oldest_kept);
    }

    /// Who asked for round `round`, when it was recorded and is still
    /// remembered.
    pub fn trigger_of(&self, round: u64) -> Option<RoundTrigger> {
        self.triggers.get(&round).copied()
    }

    /// Records round `round`, whose result is `snapshot`, as the daily
    /// check's, started by the look at `looked_at` -- the `now` at which
    /// [`tick`] answered [`Tick::Check`] (the shell's `check_automatically`)
    /// -- that is, [`RoundLog::record`] as [`RoundTrigger::Automatic`], and,
    /// when it is the newest round and does not count as a check
    /// ([`counts_as_check`]: every source failed, and no call of the
    /// window's shares the round), one more daily check failed in a row,
    /// the last started at `looked_at` ([`RoundLog::failed_checks`]).
    ///
    /// The refresh a daily check's `brew update` sets off when it ends
    /// ([`RoundLog::record_follow_up`]) is no daily check of its own, and
    /// no look started it: when it counts, it clears the failed checks, as
    /// any round that counts does, and when it does not, it adds none.
    pub fn record_daily(&mut self, round: u64, looked_at: i64, snapshot: &Snapshot) {
        self.record(round, RoundTrigger::Automatic, snapshot);
        if round == self.newest
            && self.trigger_of(round) == Some(RoundTrigger::Automatic)
            && !counts_as_check(RoundTrigger::Automatic, snapshot)
        {
            let before = self.failed.map_or(0, |failed| failed.in_a_row);
            self.failed = Some(FailedChecks {
                looked_at,
                in_a_row: before.saturating_add(1),
            });
        }
    }

    /// When the last round that counts as a check ended
    /// ([`counts_as_check`]), on the wall clock: the `last_check_ended` the
    /// shell hands [`tick`]. `None` until one has been recorded.
    pub fn last_check_ended(&self) -> Option<i64> {
        self.last_check_ended
    }

    /// When the check that runs `every` day or week is next due
    /// ([`next_check_due`] over [`RoundLog::last_check_ended`] and
    /// [`RoundLog::failed_checks`]): what Settings shows under its popup,
    /// by way of `Snapshot::next_auto_check_at`, which the shell fills in.
    pub fn next_check_due(&self, every: CheckEvery) -> Option<i64> {
        next_check_due(self.last_check_ended, self.failed, every)
    }

    /// The daily checks in which every source failed that have run in a
    /// row since the last round that counts as a check, recorded by
    /// [`RoundLog::record_daily`]: the `failed` the shell hands [`tick`].
    /// `None` when there have been none, and again once a round that counts
    /// is recorded as the newest -- any of the window's, a daily one, or a
    /// follow-up ([`RoundLog::record`]).
    pub fn failed_checks(&self) -> Option<FailedChecks> {
        self.failed
    }

    /// Whether round `round` reported a `brew update` still running whose
    /// follow-up -- the refresh that update's end sets off
    /// ([`RoundLog::record_follow_up`]) -- is the daily check's: a
    /// follow-up of the daily check's will come, and report what the daily
    /// check found, the update's new catalogue included. The update
    /// notification waits for it (`notify_updates::decide`), so that a
    /// daily check posts one notification at most. False for a round not
    /// remembered.
    pub fn awaits_follow_up(&self, round: u64) -> bool {
        self.awaiting.contains(&round)
    }

    /// Records round `round`, whose result is `snapshot`, as handed to the
    /// refresh a background change set off (the shell's
    /// `refresh_on_background_change`), and returns who that refresh was
    /// asked for by: the trigger of the round that started the `brew
    /// update` that ended, and `Window` when no round reports one running
    /// -- read now, as the follow-up's round is recorded, when the round
    /// that started the update has been recorded too, however long it ran
    /// after the update ended.
    pub fn record_follow_up(&mut self, round: u64, snapshot: &Snapshot) -> RoundTrigger {
        let trigger = self.take_follow_up_trigger();
        self.record(round, trigger, snapshot);
        trigger
    }

    /// Who the refresh a background change set off runs for
    /// ([`RoundLog::record_follow_up`]). Taken, not read: the wake-up is
    /// the update's end, and the refresh it sets off is its one follow-up.
    fn take_follow_up_trigger(&mut self) -> RoundTrigger {
        self.follow_up_owner
            .take()
            .and_then(|round| self.trigger_of(round))
            .unwrap_or(RoundTrigger::Window)
    }
}

/// Whether `snapshot` reports a `brew update` Banager started as still
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
    /// The setting on, 「每天」.
    const DAILY: Option<CheckEvery> = Some(CheckEvery::Day);
    /// The setting on, 「每周」.
    const WEEKLY: Option<CheckEvery> = Some(CheckEvery::Week);
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
                tick(NINE_AM - behind, Some(NINE_AM), None, false, DAILY),
                Tick::NotDue,
                "{behind} s before the last check"
            );
        }
    }

    #[test]
    fn test_a_now_a_minute_or_more_before_the_last_check_checks_once() {
        for behind in [SET_BACK_SLACK_SECS, 10 * 60, 365 * DAY] {
            assert_eq!(
                tick(NINE_AM - behind, Some(NINE_AM), None, false, DAILY),
                Tick::Check,
                "{behind} s before the last check"
            );
            // That check stamps the corrected clock: the ticks after it are
            // not due.
            let checked = NINE_AM - behind + 30;
            assert_eq!(
                tick(checked + 15 * 60, Some(checked), None, false, DAILY),
                Tick::NotDue
            );
        }
    }

    #[test]
    fn test_nothing_runs_while_the_setting_is_off_however_long_ago_the_last_round_was() {
        for last in [None, Some(NINE_AM - 30 * DAY), Some(NINE_AM - 60)] {
            for busy in [false, true] {
                assert_eq!(tick(NINE_AM, last, None, busy, None), Tick::Off);
            }
        }
    }

    #[test]
    fn test_a_round_that_ended_less_than_a_day_ago_is_not_due() {
        assert_eq!(
            tick(NINE_AM, Some(NINE_AM - 60), None, false, DAILY),
            Tick::NotDue
        );
        assert_eq!(
            tick(NINE_AM, Some(NINE_AM - DAY + 1), None, false, DAILY),
            Tick::NotDue,
            "one second short of a day"
        );
    }

    #[test]
    fn test_a_day_after_the_last_round_ended_the_check_is_due() {
        assert_eq!(
            tick(NINE_AM, Some(NINE_AM - DAY), None, false, DAILY),
            Tick::Check
        );
        assert_eq!(
            tick(NINE_AM, Some(NINE_AM - DAY - 1), None, false, DAILY),
            Tick::Check
        );
    }

    #[test]
    fn test_no_round_since_launch_is_due() {
        // The window's check at launch has not ended (it would be busy
        // then) and none ever ran: nothing has been checked in this run.
        assert_eq!(tick(NINE_AM, None, None, false, DAILY), Tick::Check);
    }

    #[test]
    fn test_a_mac_asleep_for_two_days_checks_at_the_first_tick_after_it_wakes_and_once() {
        // The last round ended at 9:00 on Monday; the Mac slept from 9:05
        // until Wednesday 9:00. The ticks' own clock stopped while it
        // slept, but the day is measured on the wall clock, so the first
        // tick after waking finds two days gone.
        let ended = NINE_AM;
        let woke = NINE_AM + 2 * DAY;
        assert_eq!(
            tick(woke + 60, Some(ended), None, false, DAILY),
            Tick::Check
        );
        // That round stamps its own end; the ticks after it are not due.
        let checked = woke + 90;
        assert_eq!(
            tick(woke + 15 * 60, Some(checked), None, false, DAILY),
            Tick::NotDue
        );
        assert_eq!(
            tick(woke + 30 * 60, Some(checked), None, false, DAILY),
            Tick::NotDue
        );
    }

    #[test]
    fn test_a_due_check_waits_while_something_is_under_way_and_runs_at_the_next_free_tick() {
        let last = Some(NINE_AM - 2 * DAY);
        assert_eq!(tick(NINE_AM, last, None, true, DAILY), Tick::Busy);
        assert_eq!(tick(NINE_AM + 15 * 60, last, None, true, DAILY), Tick::Busy);
        assert_eq!(
            tick(NINE_AM + 30 * 60, last, None, false, DAILY),
            Tick::Check
        );
    }

    #[test]
    fn test_busy_does_not_hide_that_nothing_was_due() {
        assert_eq!(
            tick(NINE_AM, Some(NINE_AM - 60), None, true, DAILY),
            Tick::NotDue
        );
    }

    #[test]
    fn test_a_clock_set_back_past_the_last_round_checks_once_instead_of_waiting_for_it() {
        // The last round was stamped by a clock a year fast; the clock was
        // then corrected. Waiting for `now` to pass that stamp plus a day
        // would stop the daily check for a year.
        let stamped_by_the_fast_clock = NINE_AM + 365 * DAY;
        assert_eq!(
            tick(NINE_AM, Some(stamped_by_the_fast_clock), None, false, DAILY),
            Tick::Check
        );
        // The round that runs stamps the corrected time.
        assert_eq!(
            tick(NINE_AM + 15 * 60, Some(NINE_AM + 60), None, false, DAILY),
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
            next_auto_check_at: None,
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
    fn test_a_follow_up_belongs_to_the_round_that_started_the_update_however_late_that_round_was_recorded(
    ) {
        let mut log = RoundLog::default();
        log.record(1, RoundTrigger::Window, &snapshot(false));
        // The daily check's round 2 reported its `brew update` running and
        // then waited on a slow source; the update ended, and woke the
        // shell, before round 2 committed. Round 2 is recorded as it
        // commits, and the follow-up's round 3 after it.
        log.record(2, RoundTrigger::Automatic, &snapshot(true));
        assert!(log.awaits_follow_up(2));
        assert_eq!(
            log.record_follow_up(3, &snapshot(false)),
            RoundTrigger::Automatic
        );
        assert_eq!(log.trigger_of(3), Some(RoundTrigger::Automatic));
        // Taken: the follow-up of a wake-up with no update reported
        // running is the window's.
        assert_eq!(
            log.record_follow_up(4, &snapshot(false)),
            RoundTrigger::Window
        );
        assert_eq!(log.trigger_of(4), Some(RoundTrigger::Window));
    }

    #[test]
    fn test_a_follow_up_that_shares_a_round_of_the_windows_is_the_windows() {
        let mut log = RoundLog::default();
        log.record(1, RoundTrigger::Automatic, &snapshot(true));
        // A round of the window's read the new catalogue first, and the
        // follow-up shares it.
        log.record(2, RoundTrigger::Window, &snapshot(false));
        assert_eq!(
            log.record_follow_up(2, &snapshot(false)),
            RoundTrigger::Window
        );
        assert_eq!(log.trigger_of(2), Some(RoundTrigger::Window));
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
    fn test_a_homebrew_whose_brew_update_failed_counts_as_failed_for_the_daily_check() {
        // Offline, Homebrew's `brew update` fails and its `brew outdated`
        // answers from the catalogue it already had: no error, a note.
        let mut offline = round_at(NINE_AM, &[("brew:/opt/homebrew", false)]);
        offline.instances[0]
            .status
            .notes
            .push(InstanceNote::IndexMayBeStale);
        assert!(offline.errors.is_empty(), "precondition: no error");
        assert!(!counts_as_check(RoundTrigger::Automatic, &offline));
        assert!(
            counts_as_check(RoundTrigger::Window, &offline),
            "a round of the window's counts however it went"
        );
        // With every other source failed too, the round still does not
        // count; with one that answered, it does.
        let mut with_a_failed_one =
            round_at(NINE_AM, &[("brew:/opt/homebrew", false), ("fake:1", true)]);
        with_a_failed_one.instances[0]
            .status
            .notes
            .push(InstanceNote::IndexMayBeStale);
        assert!(!counts_as_check(
            RoundTrigger::Automatic,
            &with_a_failed_one
        ));
        let mut with_one_that_answered =
            round_at(NINE_AM, &[("brew:/opt/homebrew", false), ("fake:1", false)]);
        with_one_that_answered.instances[0]
            .status
            .notes
            .push(InstanceNote::IndexMayBeStale);
        assert!(counts_as_check(
            RoundTrigger::Automatic,
            &with_one_that_answered
        ));
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
    fn test_a_mac_that_wakes_offline_checks_again_at_the_next_look_until_a_check_reaches_a_source()
    {
        // The last check ended at 9:00 on Monday; the Mac slept until
        // Wednesday 9:00, and wakes before its network does.
        let mut log = RoundLog::default();
        log.record(
            1,
            RoundTrigger::Window,
            &round_at(NINE_AM, &[("fake:1", false)]),
        );
        let look = NINE_AM + 2 * DAY + 60;
        assert_eq!(tick_over(&log, look), Tick::Check);
        // Every source fails: not the day's check.
        log.record_daily(
            2,
            look,
            &round_at(look + 30, &[("fake:1", true), ("fake:2", true)]),
        );
        assert_eq!(log.last_check_ended(), Some(NINE_AM));
        assert_eq!(
            log.failed_checks(),
            Some(FailedChecks {
                looked_at: look,
                in_a_row: 1
            })
        );
        assert_eq!(
            tick_over(&log, look + 15 * MINUTE),
            Tick::Check,
            "the next look, 15 minutes on, checks again"
        );
        // The network is back for one of them: that check counts.
        let next = look + 15 * MINUTE;
        log.record_daily(
            3,
            next,
            &round_at(next + 30, &[("fake:1", false), ("fake:2", true)]),
        );
        assert_eq!(log.failed_checks(), None);
        assert_eq!(tick_over(&log, next + 15 * MINUTE), Tick::NotDue);
        assert_eq!(
            tick_over(&log, next + 30 + DAY),
            Tick::Check,
            "and the next is a day after it"
        );
    }

    const MINUTE: i64 = 60;

    /// How long a daily round takes in these tests, from its look to its
    /// end: longer than [`RETRY_SLACK_SECS`], so that a wait measured from
    /// a failed round's end, not from its look, would miss the look it is
    /// meant for.
    const TOOK: i64 = 150;

    /// The tick at `now` over what `log` holds, with nothing under way and
    /// the daily check on: what the shell's `tick_at` hands [`tick`].
    fn tick_over(log: &RoundLog, now: i64) -> Tick {
        tick(
            now,
            log.last_check_ended(),
            log.failed_checks(),
            false,
            DAILY,
        )
    }

    /// Runs the daily check's looks over `log` as the shell's task does:
    /// one every [`TICK`] from `from` until before `until`, on the wall
    /// clock, the Mac awake and offline throughout. At each look at which
    /// [`tick`] says [`Tick::Check`], a daily round runs, numbered on from
    /// `round`, ends [`TOOK`] after its look, and fails for its one source.
    /// Returns the looks at which a round ran.
    fn run_looks(log: &mut RoundLog, round: &mut u64, from: i64, until: i64) -> Vec<i64> {
        let mut ran = Vec::new();
        let mut look = from;
        while look < until {
            if tick_over(log, look) == Tick::Check {
                *round += 1;
                log.record_daily(*round, look, &round_at(look + TOOK, &[("fake:1", true)]));
                ran.push(look);
            }
            look += TICK.as_secs() as i64;
        }
        ran
    }

    /// Minutes from `from` to each of `looks`.
    fn minutes_after(from: i64, looks: &[i64]) -> Vec<i64> {
        looks.iter().map(|look| (look - from) / MINUTE).collect()
    }

    /// A log whose last check ended a day before `NINE_AM`, then the
    /// failed daily checks started at `looks`, in that order, each ending
    /// [`TOOK`] after its look.
    fn failed_at(looks: &[i64]) -> RoundLog {
        let mut log = RoundLog::default();
        log.record(
            1,
            RoundTrigger::Window,
            &round_at(NINE_AM - DAY, &[("fake:1", false)]),
        );
        for (round, &look) in (2..).zip(looks) {
            assert_eq!(tick_over(&log, look), Tick::Check, "precondition");
            log.record_daily(round, look, &round_at(look + TOOK, &[("fake:1", true)]));
        }
        log
    }

    #[test]
    fn test_the_wait_after_a_failed_daily_check_is_15_minutes_doubling_up_to_six_hours() {
        assert_eq!(RETRY_FIRST_SECS, 15 * MINUTE);
        assert_eq!(RETRY_FIRST_SECS, TICK.as_secs() as i64, "one look");
        assert_eq!(RETRY_CAP_SECS, 6 * 60 * MINUTE);
        assert_eq!(RETRY_SLACK_SECS, MINUTE);
        let waits: Vec<i64> = (1..=8).map(|n| retry_after_secs(n) / MINUTE).collect();
        assert_eq!(waits, [15, 30, 60, 120, 240, 360, 360, 360]);
        assert_eq!(retry_after_secs(0), RETRY_FIRST_SECS, "0 is taken as 1");
        for in_a_row in [9, 32, 33, 34, 1_000, u32::MAX] {
            assert_eq!(
                retry_after_secs(in_a_row),
                RETRY_CAP_SECS,
                "{in_a_row} in a row"
            );
        }
    }

    #[test]
    fn test_failed_daily_checks_are_retried_15_30_60_120_and_240_minutes_apart_then_every_six_hours(
    ) {
        // The last check ended a day before 9:00; from 9:00 the Mac is
        // offline for two days, awake and looking every 15 minutes.
        let mut log = failed_at(&[]);
        let mut round = 1;
        let ran = run_looks(&mut log, &mut round, NINE_AM, NINE_AM + 2 * DAY);
        assert_eq!(
            minutes_after(NINE_AM, &ran),
            [0, 15, 45, 105, 225, 465, 825, 1185, 1545, 1905, 2265, 2625],
            "each wait from the look that started the last failed check: 15, 30, 60, 120, 240, then 360 minutes"
        );
        assert_eq!(
            log.failed_checks(),
            Some(FailedChecks {
                looked_at: NINE_AM + 2625 * MINUTE,
                in_a_row: 12
            })
        );
        assert_eq!(
            log.last_check_ended(),
            Some(NINE_AM - DAY),
            "none of them counted"
        );
        // Every look between two of them was told why it did nothing.
        assert_eq!(tick_over(&log, NINE_AM + 2640 * MINUTE), Tick::BackingOff);
        assert_eq!(
            tick_over(&log, NINE_AM + 2985 * MINUTE),
            Tick::Check,
            "six hours after the last"
        );
    }

    #[test]
    fn test_a_daily_check_that_counts_ends_the_backoff() {
        // Four in a row failed: the next is 120 minutes after the fourth's
        // look, at 225.
        let looks = [0, 15, 45, 105].map(|m| NINE_AM + m * MINUTE);
        let mut log = failed_at(&looks);
        assert_eq!(tick_over(&log, NINE_AM + 210 * MINUTE), Tick::BackingOff);
        let retry = NINE_AM + 225 * MINUTE;
        assert_eq!(tick_over(&log, retry), Tick::Check);
        // A source answers: that check counts, and the backoff is over.
        log.record_daily(6, retry, &round_at(retry + TOOK, &[("fake:1", false)]));
        assert_eq!(log.failed_checks(), None);
        assert_eq!(log.last_check_ended(), Some(retry + TOOK));
        assert_eq!(tick_over(&log, retry + 15 * MINUTE), Tick::NotDue);
        // The next daily check is a day after it; when that one fails, the
        // next is 15 minutes after its look, not 240.
        let next_day = retry + DAY + 15 * MINUTE;
        assert_eq!(tick_over(&log, next_day), Tick::Check);
        log.record_daily(7, next_day, &round_at(next_day + TOOK, &[("fake:1", true)]));
        assert_eq!(
            log.failed_checks(),
            Some(FailedChecks {
                looked_at: next_day,
                in_a_row: 1
            })
        );
        assert_eq!(tick_over(&log, next_day + 15 * MINUTE), Tick::Check);
    }

    #[test]
    fn test_a_check_of_the_users_between_failed_daily_checks_ends_the_backoff() {
        // Three in a row failed: the next would be 60 minutes after the
        // third's look.
        let looks = [0, 15, 45].map(|m| NINE_AM + m * MINUTE);
        let mut log = failed_at(&looks);
        assert_eq!(tick_over(&log, NINE_AM + 90 * MINUTE), Tick::BackingOff);
        // The user checks 20 minutes after the third, and every source
        // fails for them too: a check of the window's counts however it
        // went.
        let theirs = NINE_AM + 65 * MINUTE;
        log.record(
            5,
            RoundTrigger::Window,
            &round_at(theirs, &[("fake:1", true)]),
        );
        assert_eq!(log.failed_checks(), None);
        assert_eq!(log.last_check_ended(), Some(theirs));
        // So no daily check runs until a day after theirs, whatever the
        // wait stood at...
        assert_eq!(tick_over(&log, NINE_AM + 105 * MINUTE), Tick::NotDue);
        assert_eq!(tick_over(&log, theirs + DAY - 60), Tick::NotDue);
        // ... and one that fails then starts the waits from 15 minutes.
        let next = theirs + DAY;
        assert_eq!(tick_over(&log, next), Tick::Check);
        log.record_daily(6, next, &round_at(next + TOOK, &[("fake:1", true)]));
        assert_eq!(tick_over(&log, next + 15 * MINUTE), Tick::Check);
    }

    #[test]
    fn test_time_asleep_counts_toward_the_wait_after_a_failed_daily_check() {
        // Five in a row failed: the next is 240 minutes after the fifth's
        // look, at 225.
        let looks = [0, 15, 45, 105, 225].map(|m| NINE_AM + m * MINUTE);
        let fifth = NINE_AM + 225 * MINUTE;
        let mut log = failed_at(&looks);
        // The Mac sleeps for three hours right after it. The looks stop
        // while it sleeps, the wall clock does not: the first look after
        // it wakes finds three hours of the four gone, and waits.
        let woke = fifth + 3 * 60 * MINUTE;
        assert_eq!(tick_over(&log, woke + 5 * MINUTE), Tick::BackingOff);
        // Awake again, it checks at the first look four hours after the
        // fifth's -- not four hours of looks after it woke.
        let mut round = 6;
        let ran = run_looks(
            &mut log,
            &mut round,
            woke + 5 * MINUTE,
            woke + 2 * 60 * MINUTE,
        );
        assert_eq!(minutes_after(fifth, &ran), [245]);
        let sixth = ran[0];
        // That one fails too, and the Mac sleeps for two days: the first
        // look after it wakes checks, the six hours long gone.
        assert_eq!(tick_over(&log, sixth + 60 * MINUTE), Tick::BackingOff);
        assert_eq!(tick_over(&log, sixth + 2 * DAY), Tick::Check);
    }

    #[test]
    fn test_a_look_a_little_short_of_the_wait_retries_and_the_look_before_it_does_not() {
        // Two in a row failed, the last started at 9:00: the next is 30
        // minutes after. The looks come every 15 minutes the Mac is awake,
        // and the wall clock can read a second or so short at the one
        // meant to retry.
        let last = Some(NINE_AM - 2 * DAY);
        let failed = Some(FailedChecks {
            looked_at: NINE_AM,
            in_a_row: 2,
        });
        let at = |now| tick(now, last, failed, false, DAILY);
        assert_eq!(at(NINE_AM + 15 * MINUTE), Tick::BackingOff);
        assert_eq!(
            at(NINE_AM + 30 * MINUTE - RETRY_SLACK_SECS - 1),
            Tick::BackingOff
        );
        for short in [0, 1, 2, RETRY_SLACK_SECS] {
            assert_eq!(
                at(NINE_AM + 30 * MINUTE - short),
                Tick::Check,
                "{short} s short"
            );
        }
        assert_eq!(at(NINE_AM + 30 * MINUTE + 1), Tick::Check);
    }

    #[test]
    fn test_a_clock_set_back_past_a_failed_daily_checks_look_retries_once_instead_of_waiting_for_it(
    ) {
        // The last failed check's look was stamped by a clock a year fast,
        // six hours to wait after it; the clock was then corrected.
        let last = Some(NINE_AM - 2 * DAY);
        let failed = Some(FailedChecks {
            looked_at: NINE_AM + 365 * DAY,
            in_a_row: 6,
        });
        assert_eq!(tick(NINE_AM, last, failed, false, DAILY), Tick::Check);
        // Less than a minute before it is a small correction, not a clock
        // set back: the wait stands.
        let stamped = NINE_AM + 365 * DAY;
        for behind in [1, SET_BACK_SLACK_SECS - 1] {
            assert_eq!(
                tick(stamped - behind, last, failed, false, DAILY),
                Tick::BackingOff,
                "{behind} s before the look"
            );
        }
        // The retry stamps its own look, on the corrected clock: the wait
        // after it runs from there.
        let mut log = RoundLog::default();
        log.record(
            1,
            RoundTrigger::Window,
            &round_at(NINE_AM - 2 * DAY, &[("fake:1", false)]),
        );
        log.record_daily(2, stamped, &round_at(stamped + TOOK, &[("fake:1", true)]));
        assert_eq!(tick_over(&log, NINE_AM), Tick::Check);
        log.record_daily(3, NINE_AM, &round_at(NINE_AM + TOOK, &[("fake:1", true)]));
        assert_eq!(tick_over(&log, NINE_AM + 15 * MINUTE), Tick::BackingOff);
        assert_eq!(tick_over(&log, NINE_AM + 30 * MINUTE), Tick::Check);
    }

    #[test]
    fn test_off_and_not_due_say_why_before_a_backoff_does_and_a_backoff_before_busy() {
        let failed = Some(FailedChecks {
            looked_at: NINE_AM,
            in_a_row: 1,
        });
        let due = Some(NINE_AM - 2 * DAY);
        assert_eq!(tick(NINE_AM + 60, due, failed, true, None), Tick::Off);
        assert_eq!(
            tick(NINE_AM + 60, Some(NINE_AM - 60), failed, true, DAILY),
            Tick::NotDue
        );
        assert_eq!(
            tick(NINE_AM + 60, due, failed, true, DAILY),
            Tick::BackingOff
        );
        assert_eq!(
            tick(NINE_AM + 15 * MINUTE, due, failed, true, DAILY),
            Tick::Busy,
            "the wait has passed, and busy says why nothing ran"
        );
    }

    #[test]
    fn test_a_failed_daily_round_the_window_shared_is_no_failed_check_whichever_records_first() {
        let failed = round_at(NINE_AM, &[("fake:1", true)]);
        let mut log = RoundLog::default();
        log.record_daily(4, NINE_AM - 30, &failed);
        assert_eq!(
            log.failed_checks(),
            Some(FailedChecks {
                looked_at: NINE_AM - 30,
                in_a_row: 1
            })
        );
        log.record(4, RoundTrigger::Window, &failed);
        assert_eq!(log.failed_checks(), None);
        assert_eq!(log.last_check_ended(), Some(NINE_AM));

        let mut log = RoundLog::default();
        log.record(4, RoundTrigger::Window, &failed);
        log.record_daily(4, NINE_AM - 30, &failed);
        assert_eq!(log.failed_checks(), None);
        assert_eq!(log.last_check_ended(), Some(NINE_AM));
    }

    #[test]
    fn test_the_follow_up_of_a_daily_checks_brew_update_adds_no_failed_check_and_ends_them_when_it_counts(
    ) {
        // A daily round in which every source failed -- Homebrew's reading
        // of its packages included -- though its `brew update` outlasted
        // it: one failed check, and a follow-up of the daily check's to
        // come.
        let mut failing = round_at(NINE_AM, &[("brew:/opt/homebrew", true)]);
        failing.instances[0]
            .status
            .notes
            .push(InstanceNote::IndexUpdating);
        let mut log = failed_at(&[]);
        log.record_daily(2, NINE_AM - 40, &failing);
        let one = Some(FailedChecks {
            looked_at: NINE_AM - 40,
            in_a_row: 1,
        });
        assert_eq!(log.failed_checks(), one);
        // The update fails, and the follow-up with it: no look started it,
        // and it adds no failed check.
        let stale = round_at(NINE_AM + 300, &[("brew:/opt/homebrew", true)]);
        assert_eq!(log.record_follow_up(3, &stale), RoundTrigger::Automatic);
        assert_eq!(log.failed_checks(), one);
        // A follow-up that counts ends them, as any round that counts does.
        log.record_daily(4, NINE_AM + 15 * MINUTE, &failing);
        let answered = round_at(NINE_AM + 20 * MINUTE, &[("brew:/opt/homebrew", false)]);
        assert_eq!(log.record_follow_up(5, &answered), RoundTrigger::Automatic);
        assert_eq!(log.failed_checks(), None);
        assert_eq!(log.last_check_ended(), Some(NINE_AM + 20 * MINUTE));
    }

    #[test]
    fn test_a_late_record_of_an_older_daily_round_adds_no_failed_check() {
        let mut log = failed_at(&[]);
        log.record(
            3,
            RoundTrigger::Window,
            &round_at(NINE_AM, &[("fake:1", false)]),
        );
        log.record_daily(
            2,
            NINE_AM - 60,
            &round_at(NINE_AM - 30, &[("fake:1", true)]),
        );
        assert_eq!(log.failed_checks(), None);
        assert_eq!(log.last_check_ended(), Some(NINE_AM));
    }

    #[test]
    fn test_a_daily_round_whose_brew_update_outlasts_it_awaits_its_follow_up() {
        let mut log = RoundLog::default();
        log.record(1, RoundTrigger::Window, &snapshot(false));
        // The daily check starts a `brew update` that outlasts it: a
        // follow-up of the daily check's will come.
        log.record(2, RoundTrigger::Automatic, &snapshot(true));
        assert!(log.awaits_follow_up(2));
        assert_eq!(log.take_follow_up_trigger(), RoundTrigger::Automatic);
        // The follow-up reports no update running, and awaits nothing.
        log.record(3, RoundTrigger::Automatic, &snapshot(false));
        assert!(!log.awaits_follow_up(3));
        assert!(log.awaits_follow_up(2), "what round 2 was told stays");
        assert!(!log.awaits_follow_up(1));
        assert!(!log.awaits_follow_up(99), "a round never recorded");
    }

    #[test]
    fn test_a_round_whose_follow_up_is_the_windows_awaits_nothing() {
        // The user's check started the update; the daily check's round
        // that finds it still running has no follow-up of its own to wait
        // for -- the follow-up is the window's, and posts nothing.
        let mut log = RoundLog::default();
        log.record(1, RoundTrigger::Window, &snapshot(true));
        log.record(2, RoundTrigger::Automatic, &snapshot(true));
        assert!(!log.awaits_follow_up(2));
        assert_eq!(log.take_follow_up_trigger(), RoundTrigger::Window);
    }

    #[test]
    fn test_a_daily_round_the_window_shared_awaits_nothing() {
        // Shared with the window, the round and its follow-up are the
        // window's.
        let mut log = RoundLog::default();
        log.record(4, RoundTrigger::Automatic, &snapshot(true));
        assert!(log.awaits_follow_up(4));
        log.record(4, RoundTrigger::Window, &snapshot(true));
        assert!(!log.awaits_follow_up(4));
        assert_eq!(log.take_follow_up_trigger(), RoundTrigger::Window);
    }

    /// Whether `next_check_due` names the first `now` at which `tick`
    /// checks, for every `now` from `from` on, a minute apart, over two
    /// days: the line Settings shows and the task's decision agree.
    fn agrees_with_tick(last: Option<i64>, failed: Option<FailedChecks>, from: i64) {
        agrees_with_tick_every(CheckEvery::Day, last, failed, from, MINUTE);
    }

    /// `agrees_with_tick` for the check that runs `every` day or week,
    /// over two of them, `step` seconds apart.
    fn agrees_with_tick_every(
        every: CheckEvery,
        last: Option<i64>,
        failed: Option<FailedChecks>,
        from: i64,
        step_secs: i64,
    ) {
        let due = next_check_due(last, failed, every);
        for step in 0..(2 * every.due_after_secs() / step_secs) {
            let now = from + step * step_secs;
            let checks = tick(now, last, failed, false, Some(every)) == Tick::Check;
            assert_eq!(
                checks,
                due.is_none_or(|due| now >= due),
                "now {now}, due {due:?}, last {last:?}, failed {failed:?}"
            );
        }
    }

    #[test]
    fn test_the_next_check_is_due_when_tick_first_checks() {
        agrees_with_tick(None, None, NINE_AM);
        agrees_with_tick(Some(NINE_AM), None, NINE_AM);
        for in_a_row in 1..=8 {
            // Failed daily checks since a check a day ago: the retry's wait
            // decides.
            let failed = Some(FailedChecks {
                looked_at: NINE_AM,
                in_a_row,
            });
            agrees_with_tick(Some(NINE_AM - DAY), failed, NINE_AM);
            // Failed daily checks long ago, a check since: the day decides
            // whichever is later.
            agrees_with_tick(
                Some(NINE_AM + 3 * 60 * MINUTE),
                failed,
                NINE_AM + 3 * 60 * MINUTE,
            );
            // None counted since launch, only failures.
            agrees_with_tick(None, failed, NINE_AM);
        }
    }

    #[test]
    fn test_the_next_check_is_a_day_after_the_last_check_or_the_retry_after_failed_ones() {
        assert_eq!(
            next_check_due(None, None, CheckEvery::Day),
            None,
            "due at the next look"
        );
        assert_eq!(
            next_check_due(Some(NINE_AM), None, CheckEvery::Day),
            Some(NINE_AM + DAY)
        );
        let failed = FailedChecks {
            looked_at: NINE_AM,
            in_a_row: 2,
        };
        assert_eq!(
            next_check_due(Some(NINE_AM - DAY), Some(failed), CheckEvery::Day),
            Some(NINE_AM + 30 * MINUTE - RETRY_SLACK_SECS)
        );
        assert_eq!(
            next_check_due(None, Some(failed), CheckEvery::Day),
            Some(NINE_AM + 30 * MINUTE - RETRY_SLACK_SECS)
        );
    }

    #[test]
    fn test_a_check_the_user_runs_moves_the_next_daily_check_a_day_on() {
        let mut log = RoundLog::default();
        assert_eq!(log.next_check_due(CheckEvery::Day), None);
        // The window's check at launch.
        log.record(
            1,
            RoundTrigger::Window,
            &round_at(NINE_AM, &[("fake:1", false)]),
        );
        assert_eq!(log.next_check_due(CheckEvery::Day), Some(NINE_AM + DAY));
        // Check again, an hour on, however it went: the day starts again.
        let later = NINE_AM + 60 * MINUTE;
        log.record(
            2,
            RoundTrigger::Window,
            &round_at(later, &[("fake:1", true)]),
        );
        assert_eq!(log.next_check_due(CheckEvery::Day), Some(later + DAY));
        // The daily check, a day on, offline: the retry 15 minutes after
        // its look.
        let look = later + DAY;
        log.record_daily(3, look, &round_at(look + TOOK, &[("fake:1", true)]));
        assert_eq!(
            log.next_check_due(CheckEvery::Day),
            Some(look + RETRY_FIRST_SECS - RETRY_SLACK_SECS)
        );
        // A round of the window's clears the failures and starts the day.
        let mine = look + 5 * MINUTE;
        log.record(
            4,
            RoundTrigger::Window,
            &round_at(mine, &[("fake:1", false)]),
        );
        assert_eq!(log.next_check_due(CheckEvery::Day), Some(mine + DAY));
    }

    #[test]
    fn test_a_week_is_seven_days_and_only_the_wait_for_the_next_check_changes() {
        assert_eq!(CheckEvery::Day.due_after_secs(), DAY);
        assert_eq!(CheckEvery::Week.due_after_secs(), 7 * DAY);
        assert_eq!(WEEKLY_DUE_AFTER_SECS, 604_800);
    }

    #[test]
    fn test_the_weekly_check_is_due_a_week_after_the_last_check_not_a_day() {
        let last = Some(NINE_AM - DAY);
        assert_eq!(tick(NINE_AM, last, None, false, DAILY), Tick::Check);
        assert_eq!(
            tick(NINE_AM, last, None, false, WEEKLY),
            Tick::NotDue,
            "a day is not a week"
        );
        assert_eq!(
            tick(NINE_AM - DAY + 7 * DAY - 1, last, None, false, WEEKLY),
            Tick::NotDue
        );
        assert_eq!(
            tick(NINE_AM - DAY + 7 * DAY, last, None, false, WEEKLY),
            Tick::Check
        );
        assert_eq!(
            tick(NINE_AM, None, None, false, WEEKLY),
            Tick::Check,
            "nothing counted since launch: due at the next look, as daily"
        );
        assert_eq!(tick(NINE_AM, None, None, false, None), Tick::Off);
    }

    #[test]
    fn test_the_weekly_check_waits_for_what_is_under_way_and_retries_as_the_daily_one() {
        let last = Some(NINE_AM - 8 * DAY);
        assert_eq!(tick(NINE_AM, last, None, true, WEEKLY), Tick::Busy);
        // Failed weekly checks are retried 15 minutes on, then 30, as
        // daily ones are: not a week on.
        let failed = Some(FailedChecks {
            looked_at: NINE_AM,
            in_a_row: 1,
        });
        assert_eq!(
            tick(NINE_AM + 5 * MINUTE, last, failed, false, WEEKLY),
            Tick::BackingOff
        );
        assert_eq!(
            tick(NINE_AM + 15 * MINUTE, last, failed, false, WEEKLY),
            Tick::Check
        );
    }

    #[test]
    fn test_the_next_weekly_check_is_due_when_tick_first_checks() {
        // An hour apart over two weeks: every hour is a look the line and
        // the task must agree on.
        agrees_with_tick_every(CheckEvery::Week, None, None, NINE_AM, 60 * MINUTE);
        agrees_with_tick_every(CheckEvery::Week, Some(NINE_AM), None, NINE_AM, 60 * MINUTE);
        let failed = Some(FailedChecks {
            looked_at: NINE_AM,
            in_a_row: 3,
        });
        agrees_with_tick_every(
            CheckEvery::Week,
            Some(NINE_AM - 7 * DAY),
            failed,
            NINE_AM,
            MINUTE,
        );
        assert_eq!(
            next_check_due(Some(NINE_AM), None, CheckEvery::Week),
            Some(NINE_AM + 7 * DAY)
        );
    }

    #[test]
    fn test_a_check_the_user_runs_moves_the_next_weekly_check_a_week_on() {
        let mut log = RoundLog::default();
        log.record(
            1,
            RoundTrigger::Window,
            &round_at(NINE_AM, &[("fake:1", false)]),
        );
        assert_eq!(
            log.next_check_due(CheckEvery::Week),
            Some(NINE_AM + 7 * DAY)
        );
        let later = NINE_AM + 3 * DAY;
        log.record(
            2,
            RoundTrigger::Window,
            &round_at(later, &[("fake:1", false)]),
        );
        assert_eq!(log.next_check_due(CheckEvery::Week), Some(later + 7 * DAY));
        assert_eq!(
            log.next_check_due(CheckEvery::Day),
            Some(later + DAY),
            "the same log, read for the daily check"
        );
    }
}
