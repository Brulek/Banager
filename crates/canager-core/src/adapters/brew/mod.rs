pub mod parse;

use crate::adapters::{
    ensure_instance_match, reconcile_from, run_plan, validate_package_name, Adapter, AdapterError,
    AdapterMeta, CheckOptions, CheckOutcome,
};
use crate::events::{EventSink, OpId};
use crate::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, Fault, InstalledArtifact, InstanceId, InstanceNote,
    InstanceStatus, ManagerInstance, OpKind, OpRequest, Outcome, Plan, Reconciled, ResourceLock,
    Scope, SearchHit, Unavailable, Warning,
};
use crate::runner::{CommandOutput, CommandRunner, CommandSpec, HostEnv, OutputUse};
use async_trait::async_trait;
use parse::{parse_info_installed, parse_outdated, parse_search, parse_uses, parse_version};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

pub struct BrewAdapter {
    runner: Arc<dyn CommandRunner>,
    meta: AdapterMeta,
    /// Per-instance `brew update` bookkeeping: when one last succeeded
    /// (the TTL), whether one is running now, and what the task running
    /// it owes the refreshes that stopped waiting for it -- see
    /// `UpdateRecord`. Keyed by `ManagerInstance id` (e.g.
    /// `brew:/opt/homebrew` vs `brew:/usr/local`) so that two installs of
    /// Homebrew each get their own throttle instead of sharing a single
    /// adapter-wide timer.
    ///
    /// Shared (`Arc`) because the task that runs `brew update` writes its
    /// own result here, and that task can outlive the `check_updates` call
    /// that started it -- see `maybe_update`.
    updates: Arc<Mutex<HashMap<InstanceId, UpdateRecord>>>,
    /// Woken when a `brew update` some refresh stopped waiting for has
    /// ended, one way or the other. That refresh told the user the list
    /// was still being downloaded (`InstanceNote::IndexUpdating`), and
    /// nothing else would ever look again: `Session::background_change`
    /// hands this to the shell, which refreshes, so the notice clears and
    /// the fresh catalogue shows without the user having to do anything.
    background_change: Arc<tokio::sync::Notify>,
    /// Serialises `maybe_update` per instance: without this, two
    /// concurrent `check_updates` calls for the same instance could both
    /// observe "TTL expired" before either had recorded a fresh
    /// timestamp, and both run `brew update` concurrently — wasteful, and
    /// two `brew update` processes writing the same Homebrew cache
    /// directory at once is not something Homebrew is designed to
    /// tolerate. Keyed the same way as `updates`.
    ///
    /// The lock is held by the task that runs `brew update`, not by the
    /// `check_updates` call waiting on it, so it stays held until that
    /// `brew update` has exited even when the refresh stopped waiting long
    /// before. `execute` takes it too, so a user's install or upgrade on
    /// this Homebrew waits for a `brew update` still finishing in the
    /// background instead of running alongside it.
    update_locks: Mutex<HashMap<InstanceId, Arc<tokio::sync::Mutex<()>>>>,
    update_ttl: Duration,
    /// How long `check_updates` waits for the `brew update` it starts
    /// before giving up on this round's update check and returning
    /// `AdapterError::IndexUpdating` without running `brew outdated`.
    /// Waiting is all this bounds: when it runs out the update is left to
    /// finish, never killed. See `maybe_update` for why.
    update_patience: Duration,
    /// How long an install, upgrade or uninstall waits for a `brew update`
    /// still running in the background before giving up without having
    /// started (`Fault::HomebrewStillUpdating`). `OP_UPDATE_WAIT` outside
    /// tests.
    op_update_wait: Duration,
    /// How to read the *real* effective UID for the root-refusal check on
    /// every brew subprocess call (not just `detect`, which instead checks
    /// the caller-supplied `HostEnv::euid`). A plain fn pointer (rather than
    /// a boxed closure) keeps this injectable for tests without touching
    /// `BrewAdapter::new`'s public signature: production code always gets
    /// the default `|| unsafe { libc::geteuid() }`, and tests can swap in
    /// `|| 0` via the `#[cfg(test)]`-only `with_euid_fn`.
    euid_fn: fn() -> u32,
    /// How to read `SUDO_ASKPASS` for the cask install/upgrade
    /// passthrough. Same fn-pointer trick as `euid_fn`, and for the same
    /// reason: the value lives in the process environment, and a test that
    /// wants a known one used to `set_var` it. Rust runs tests in threads
    /// within one binary, so that was a write to process-global state
    /// racing every other test in the binary that reads it — including
    /// every other cask plan, which reads exactly this variable. Injecting
    /// the reader instead means no test has to touch the environment at
    /// all. Production still reads the real variable, and still reads it
    /// per plan rather than once at startup.
    askpass_fn: fn() -> Option<String>,
    /// How to ask whether one of `CANDIDATE_PATHS` is on this machine.
    /// Same fn-pointer seam as `euid_fn` and `askpass_fn`, and as
    /// `NpmAdapter::prefix_writable_fn`, for the same reason: `detect`'s
    /// whole job is probing the filesystem, so a test that cannot answer
    /// that probe can only assert whatever the machine running it happens
    /// to have. That made every `detect` test here a test of the author's
    /// Mac -- green on Apple Silicon with Homebrew in `/opt/homebrew`, red
    /// on an Intel Mac (`/usr/local`), red on a checkout with no Homebrew
    /// at all, and red again where both prefixes exist. Production always
    /// gets `|path| path.exists()`; tests hand in a layout.
    path_exists_fn: fn(&Path) -> bool,
    /// How to look at Homebrew's own `brew update` lock under a prefix,
    /// for `catalogue_stamp`. The same fn-pointer seam as
    /// `path_exists_fn`: outside this crate's unit tests it is always
    /// `probe_homebrew_update_lock` (`DEFAULT_UPDATE_LOCK_FN`); inside
    /// them it reads no lock at all unless a test installs one, so that no
    /// test here answers differently because the Mac running it happens to
    /// be in the middle of a `brew update`.
    update_lock_fn: fn(&Path) -> HomebrewUpdateLock,
}

/// `BrewAdapter::update_lock_fn` as `BrewAdapter::new` sets it: the real
/// probe in every build but this crate's unit tests.
#[cfg(not(test))]
const DEFAULT_UPDATE_LOCK_FN: fn(&Path) -> HomebrewUpdateLock = probe_homebrew_update_lock;
/// In this crate's unit tests, a Homebrew with no update lock file. Tests
/// that are about the lock install `probe_homebrew_update_lock` itself
/// against a prefix they made (`with_update_lock_fn`).
#[cfg(test)]
const DEFAULT_UPDATE_LOCK_FN: fn(&Path) -> HomebrewUpdateLock = |_| {
    HomebrewUpdateLock::Free(LockStamp {
        dir: None,
        file: None,
    })
};

impl BrewAdapter {
    pub const ENV: [(&'static str, &'static str); 4] = [
        ("HOMEBREW_NO_AUTO_UPDATE", "1"),
        ("HOMEBREW_NO_ENV_HINTS", "1"),
        ("HOMEBREW_NO_INSTALL_CLEANUP", "1"),
        ("NO_COLOR", "1"),
    ];

    /// How long a refresh waits for `brew update`. The same two minutes
    /// that used to be `brew update`'s own timeout, so a refresh on a slow
    /// network takes no longer than it did; what changed is that running
    /// out of it no longer kills the update.
    const UPDATE_PATIENCE: Duration = Duration::from_secs(120);

    /// The one bound on how long `brew update` itself may run before it is
    /// killed: long enough that only a Homebrew that has genuinely hung
    /// reaches it, the same half hour an install or upgrade gets.
    ///
    /// It has to exist. The task running `brew update` holds this
    /// instance's update lock, so a `brew update` that never exits would
    /// leave every later refresh reporting it as still running, and every
    /// install and upgrade on this Homebrew giving up after
    /// `OP_UPDATE_WAIT`, until the app is quit. Reaching it stops the update the way every timeout does:
    /// SIGTERM first, which git answers by removing its lock files, and a
    /// SIGKILL only if the update is still there a few seconds later. A
    /// hang that ignores SIGTERM too can still leave Homebrew's
    /// `.git/index.lock` behind; that is the price of getting the app back,
    /// and it is paid only by a process that has shown no sign of finishing
    /// for thirty minutes -- not, as the old two-minute timeout did, by any
    /// update that met a slow network.
    const UPDATE_BACKSTOP: Duration = Duration::from_secs(30 * 60);

    /// How long a user's install, upgrade or uninstall waits for a `brew
    /// update` still running in the background. Without it the only bound
    /// was `UPDATE_BACKSTOP`, so an update stuck on a dead connection held
    /// every operation on this Homebrew for half an hour, with a line in
    /// the log as the only sign. Ten minutes is long enough for a slow but
    /// working download to finish -- the case waiting is for -- and short
    /// enough that a stuck one ends the operation with a sentence saying
    /// so, and nothing changed. The log line the wait prints
    /// (`operations.logNote.waitingForBrewUpdate`) and the sentence it ends
    /// with (`operations.outcome.CanagerFailed.HomebrewStillUpdating`) both
    /// interpolate this number as `{{minutes}}` (via `op_update_wait_minutes`
    /// below) rather than carrying their own copy of it, so there is nothing
    /// to keep in sync by hand when it changes.
    const OP_UPDATE_WAIT: Duration = Duration::from_secs(10 * 60);

    pub const CANDIDATE_PATHS: [&'static str; 3] = [
        "/opt/homebrew/bin/brew",
        "/usr/local/bin/brew",
        "/home/linuxbrew/.linuxbrew/bin/brew",
    ];

    pub fn new(runner: Arc<dyn CommandRunner>) -> BrewAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../../adapters/meta/brew.toml"))
            .expect("adapters/meta/brew.toml must parse");
        BrewAdapter {
            runner,
            meta,
            updates: Arc::new(Mutex::new(HashMap::new())),
            background_change: Arc::new(tokio::sync::Notify::new()),
            update_locks: Mutex::new(HashMap::new()),
            update_ttl: Duration::from_secs(6 * 3600),
            update_patience: Self::UPDATE_PATIENCE,
            op_update_wait: Self::OP_UPDATE_WAIT,
            euid_fn: || unsafe { libc::geteuid() },
            askpass_fn: || std::env::var("SUDO_ASKPASS").ok(),
            path_exists_fn: |path| path.exists(),
            update_lock_fn: DEFAULT_UPDATE_LOCK_FN,
        }
    }

    pub fn with_update_ttl(mut self, ttl: Duration) -> BrewAdapter {
        self.update_ttl = ttl;
        self
    }

    /// Wakes `notify` whenever a `brew update` a refresh stopped waiting
    /// for has ended (see the `background_change` field). `Session::new`
    /// passes the one `Session::background_change` waits on.
    pub fn with_background_change(mut self, notify: Arc<tokio::sync::Notify>) -> BrewAdapter {
        self.background_change = notify;
        self
    }

    /// Test-only hook to shorten `update_patience`, so a test can watch a
    /// refresh stop waiting for a `brew update` without taking two minutes.
    /// `pub(crate)` because `session::refresh`'s tests drive a real
    /// `BrewAdapter` through a whole refresh.
    #[cfg(test)]
    pub(crate) fn with_update_patience(mut self, patience: Duration) -> BrewAdapter {
        self.update_patience = patience;
        self
    }

    /// Test-only hook to shorten `op_update_wait`.
    #[cfg(test)]
    fn with_op_update_wait(mut self, wait: Duration) -> BrewAdapter {
        self.op_update_wait = wait;
        self
    }

    /// Test-only hook to inject a fake euid, since a test cannot change its
    /// own real effective UID. Deliberately not part of the public API
    /// surface (`BrewAdapter::new`'s signature is unchanged).
    #[cfg(test)]
    fn with_euid_fn(mut self, euid_fn: fn() -> u32) -> BrewAdapter {
        self.euid_fn = euid_fn;
        self
    }

    /// Test-only hook to pin what `SUDO_ASKPASS` reads as, so no test has
    /// to mutate the process environment other tests are reading from.
    #[cfg(test)]
    fn with_askpass_fn(mut self, askpass_fn: fn() -> Option<String>) -> BrewAdapter {
        self.askpass_fn = askpass_fn;
        self
    }

    /// Test-only hook to describe the Homebrew layout `detect` should see:
    /// which of `CANDIDATE_PATHS` exist. `pub(crate)` rather than private
    /// because `session::refresh`'s own tests build a real `BrewAdapter`
    /// (deliberately -- what they exercise is brew's own root policy) and
    /// so need to pin the layout too.
    #[cfg(test)]
    pub(crate) fn with_path_exists_fn(mut self, path_exists_fn: fn(&Path) -> bool) -> BrewAdapter {
        self.path_exists_fn = path_exists_fn;
        self
    }

    /// Test-only hook to choose how `catalogue_stamp` sees Homebrew's own
    /// update lock (see `update_lock_fn`).
    #[cfg(test)]
    fn with_update_lock_fn(
        mut self,
        update_lock_fn: fn(&Path) -> HomebrewUpdateLock,
    ) -> BrewAdapter {
        self.update_lock_fn = update_lock_fn;
        self
    }

    /// The common root-refusal gate for every brew subprocess invocation
    /// except `detect` (which checks the caller-supplied `HostEnv::euid`
    /// instead, by design — see its own doc comment). Called from
    /// `run_brew` (covers inventory/check_updates/search/plan's `uses`
    /// lookup), from `maybe_update` and from `execute` (which both talk to
    /// the runner directly and so do not go through `run_brew`).
    fn refuse_if_root(&self) -> Result<(), AdapterError> {
        if (self.euid_fn)() == 0 {
            return Err(AdapterError::Refused(
                "refusing to run Homebrew as root".to_string(),
            ));
        }
        Ok(())
    }

    fn env_vec(&self) -> Vec<(String, String)> {
        Self::ENV
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn prefix_for(exe_path: &Path) -> PathBuf {
        exe_path
            .parent()
            .and_then(|bin| bin.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    fn instance_id_for(&self, prefix: &Path) -> String {
        crate::model::instance_id(&self.meta.id, Some(&prefix.display().to_string()))
    }

    async fn run_brew(
        &self,
        inst: &ManagerInstance,
        args: Vec<String>,
        timeout: Duration,
    ) -> Result<CommandOutput, AdapterError> {
        self.refuse_if_root()?;
        let spec = CommandSpec {
            program: inst.exe_path.clone(),
            args,
            env: self.env_vec(),
            cwd: None,
            timeout,
            // `inventory`, `check_updates`, `plan`'s dependent scan and
            // `search` all hand this stdout to a parser, so it must
            // arrive whole.
            output_use: OutputUse::Parsed,
        };
        Ok(self
            .runner
            .run(spec, None, CancellationToken::new())
            .await?)
    }

    fn update_lock_for(&self, inst_id: &InstanceId) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.update_locks.lock().unwrap();
        locks
            .entry(inst_id.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    /// Brings this Homebrew's catalogue up to date if the TTL says it is
    /// due, and reports whether that catalogue is current, stale, or still
    /// being rewritten -- in which case `check_updates` does not read it.
    ///
    /// `brew update` is a git operation on Homebrew's own repository. A
    /// SIGKILL partway through it -- which is how `CommandRunner` stops a
    /// command when the run future is dropped, and how a timeout or cancel
    /// ends one that has not exited within the grace period after SIGTERM
    /// -- can leave `.git/index.lock` behind, and
    /// from then on Homebrew refuses to update until someone deletes that
    /// file by hand: not something a person who does not write code can be
    /// expected to know how to do, or that Canager can explain from here.
    /// Two paths used to deliver that kill: the two-minute timeout this
    /// command had, which any slow network reached, and dropping the
    /// refresh (the only way to stop one: `Session::refresh` takes no
    /// `CancellationToken`), which since the run future learned to kill its
    /// process group on drop does so at an arbitrary point.
    ///
    /// So the waiting and the running are split. `brew update` runs in a
    /// task of its own, which holds the update lock and runs the command to
    /// completion (bounded only by `UPDATE_BACKSTOP`); this function waits
    /// for that task, for at most `update_patience`, through a plain
    /// `JoinHandle` -- which *detaches* the task when dropped, never aborts
    /// it. Whether this function returns because the patience ran out or is
    /// dropped because its refresh was, the update carries on, keeps
    /// draining its pipes, and records its result in `updates` when it
    /// ends.
    ///
    /// Running out of patience is not a failure, and is not reported as
    /// one: the download is still going, and on a slow link to GitHub it
    /// usually finishes. So it is `Updating`, which the user reads as "the
    /// list is still being downloaded", never `MayBeStale`, which tells
    /// them the download failed and to check their connection. When the
    /// update then ends, its task wakes `background_change`, and the shell
    /// refreshes once: the notice clears and the new catalogue shows, or,
    /// if the update failed after all, the refresh says *that*, without
    /// starting another `brew update` of its own (see
    /// `UpdateRecord::unreported_failure`).
    ///
    /// A refresh that arrives while an update is running does not wait for
    /// it at all, and is covered by the same follow-up. It used to wait on
    /// the lock for its own two minutes and then report the download as
    /// failed, while the first one was downloading fine. In a refresh it is
    /// `inventory`, which `Session::refresh` calls first, that finds the
    /// update running and returns `IndexUpdating`, and the refresh then
    /// skips `check_updates`; the `join_running_update` below answers a
    /// `check_updates` called on its own.
    ///
    /// The one thing this cannot prevent is the app itself exiting
    /// mid-update: quitting ends the process, and the update's pipes with
    /// it.
    async fn maybe_update(&self, inst: &ManagerInstance, opts: &CheckOptions) -> IndexFreshness {
        if self.refuse_if_root().is_err() {
            return IndexFreshness::MayBeStale;
        }
        let deadline = tokio::time::Instant::now() + self.update_patience;
        let lock = self.update_lock_for(&inst.id);
        let guard = match lock.clone().try_lock_owned() {
            Ok(guard) => guard,
            Err(_) => {
                if self.join_running_update(&inst.id) {
                    return IndexFreshness::Updating;
                }
                // Held, but not by a running update: an update's task
                // between writing its result and releasing the lock, which
                // is a moment. (An install or upgrade on this Homebrew
                // holds it too, but never while a refresh of the same
                // instance is here: both hold the instance's resource lock
                // first.)
                match tokio::time::timeout_at(deadline, lock.lock_owned()).await {
                    Ok(guard) => guard,
                    Err(_) => return IndexFreshness::MayBeStale,
                }
            }
        };
        let finish = {
            let mut updates = self.updates.lock().unwrap();
            let record = updates.entry(inst.id.clone()).or_default();
            if let Some(failed_at) = record.unreported_failure {
                // Only a round that began after the failure takes it; one
                // that began before reports it and leaves it for the
                // round the failure's wake-up sets off.
                //
                // Strictly after, because an `Instant` is no finer than the
                // clock's tick (41.67 ns on Apple Silicon, where most
                // back-to-back reads return the same value): a round that
                // began just before the failure can carry the failure's own
                // `Instant`, and taking the flag then would let the
                // wake-up's round start the `brew update` this flag is
                // there to stop. Leaving a tie costs one more "may be
                // stale" and cannot strand the flag: the wake-up's round
                // stamps its start (`round_started` in `refresh_round`)
                // only after `UpdateFinish::drop` has stamped the failure
                // and called `notify_one` and the shell's loop has woken to
                // it, so it lands on a later tick and takes it.
                // `test_a_round_stamped_in_the_failures_own_tick_leaves_the_failure_for_a_later_one`
                // sets up the tie exactly.
                if opts.round_started.is_none_or(|t| t > failed_at) {
                    record.unreported_failure = None;
                }
                return IndexFreshness::MayBeStale;
            }
            if record
                .succeeded_at
                .is_some_and(|t| t.elapsed() < self.update_ttl)
            {
                return IndexFreshness::Current;
            }
            // Set while holding the update lock, so a refresh that finds
            // the lock taken always finds this too -- and only by
            // `UpdateFinish::begin`, which hands back the guard whose drop
            // clears it, so nothing can run between setting `running` and
            // owning the guard.
            UpdateFinish::begin(
                record,
                self.updates.clone(),
                inst.id.clone(),
                self.background_change.clone(),
            )
        };
        let spec = CommandSpec {
            program: inst.exe_path.clone(),
            args: vec!["update".to_string()],
            env: self.env_vec(),
            cwd: None,
            timeout: Self::UPDATE_BACKSTOP,
            // Only the exit code is read. `Parsed` still refuses an
            // implausible 64 MiB of stdout rather than hold it, which costs
            // this caller nothing.
            output_use: OutputUse::Parsed,
        };
        let runner = self.runner.clone();
        let started = Instant::now();
        let update = tokio::spawn(async move {
            // Declared first so it is dropped last: the result is in
            // `updates` (by `finish`'s drop) before the lock is free.
            let _guard = guard;
            // Named, not just assigned through: a closure or async block
            // that only writes `finish.succeeded` captures that one `bool`
            // (edition 2021's disjoint captures), which would leave
            // `finish` -- and its drop -- behind in `maybe_update`.
            let mut finish = finish;
            finish.succeeded = matches!(
                runner.run(spec, None, CancellationToken::new()).await,
                Ok(output) if output.exit_code == Some(0)
            );
            finish.succeeded
        });
        match tokio::time::timeout_at(deadline, update).await {
            Ok(Ok(true)) => IndexFreshness::Current,
            // It ran and failed, or its task panicked.
            Ok(_) => IndexFreshness::MayBeStale,
            // Still running: `update` is dropped here, detached, not
            // aborted.
            Err(_) => {
                if self.join_running_update(&inst.id) {
                    return IndexFreshness::Updating;
                }
                // It ended between the timeout and here.
                let updates = self.updates.lock().unwrap();
                match updates.get(&inst.id).and_then(|r| r.succeeded_at) {
                    Some(t) if t >= started => IndexFreshness::Current,
                    _ => IndexFreshness::MayBeStale,
                }
            }
        }
    }

    /// If a `brew update` is running for `inst_id`, asks to be told when it
    /// ends (see `UpdateRecord::announced`) and returns true.
    ///
    /// Checked and set under the one `updates` lock the update's own task
    /// takes to write its result, so the two cannot miss each other: either
    /// this sees it still running and the task sees `announced`, or this
    /// sees it finished.
    fn join_running_update(&self, inst_id: &InstanceId) -> bool {
        let mut updates = self.updates.lock().unwrap();
        let record = updates.entry(inst_id.clone()).or_default();
        if record.running {
            record.announced = true;
        }
        record.running
    }

    /// `None` while a `brew update` is running for `inst` that either
    /// Canager started (`UpdateRecord::running`) or anyone holds Homebrew's
    /// own update lock for (`HomebrewUpdateLock::Held`), otherwise a stamp.
    /// A read of the catalogue takes one before and one after and trusts
    /// what it read only when both are the same `Some`.
    ///
    /// What two equal stamps prove:
    /// - No `brew update` Canager started overlapped the read. The stamp
    ///   carries `UpdateRecord::started`, which `UpdateFinish::begin` bumps
    ///   in the same write that sets `running`, both under the `updates`
    ///   lock this reads them under.
    /// - No `brew update` from anywhere else -- Terminal, the auto-update
    ///   Homebrew runs before a `brew install` typed in Terminal, a launchd
    ///   auto-update agent -- held Homebrew's update lock at either end,
    ///   or began in between. Each one opens the lock file with truncation
    ///   before it locks it (`exec 200>"${lock_file}"` in Homebrew's
    ///   `utils/lock.sh`), which moves the file's mtime. One that has to
    ///   make the file first, because a `brew cleanup` deleted it, adds an
    ///   entry to the `locks` directory, which moves the directory's mtime
    ///   -- and so does a `brew cleanup` that deletes the file again before
    ///   the second stamp, which would otherwise leave no file at either
    ///   end to compare. The stamp carries both (`LockStamp`).
    ///
    /// What they do not prove:
    /// - That an update which had opened the lock file but not yet locked
    ///   it when the first stamp was taken -- the few milliseconds between
    ///   `exec 200>` and `lockf -t 0 200` in `lock.sh` -- and then finished
    ///   before the second, did not overlap: the file looks the same at
    ///   both ends.
    /// - Anything about the API catalogue Homebrew downloads outside that
    ///   lock: every Ruby `brew` command typed in Terminal re-downloads it
    ///   when its copy is stale (`Homebrew::API.fetch_api_files!`, called
    ///   from `brew.rb` before the command runs), straight over the old
    ///   file (`curl --output` in `Utils::Curl.curl_download`). A read
    ///   that meets that file half-written does not come back as a
    ///   shorter list: `fetch_json_api_file` answers `JSON::ParserError`
    ///   by deleting the file and fetching it again, or stopping with an
    ///   error, and a whole file must still pass `verify_and_parse_jws`.
    /// - Whether Homebrew's lock was held, when the probe could not look
    ///   (`HomebrewUpdateLock::Unobservable` at both ends). The stamps
    ///   still compare what it did read -- the directory, and the file's
    ///   `FileId` when only `fcntl` failed -- so a `brew update` that
    ///   began in between still shows through those; one that already
    ///   held the lock at the first stamp does not.
    ///
    /// Why what is left is bounded: a dependents list that comes back short
    /// leaves names off the confirm screen, and one that comes back empty
    /// enables Confirm (`hasAffected` in `src/components/
    /// UninstallDialog.tsx`). But `brew uninstall` itself refuses to
    /// remove a formula or cask that another installed one depends on
    /// unless it is given `--ignore-dependencies`
    /// (`Uninstall.handle_unsatisfied_dependents` and
    /// `Cask::Uninstall.check_dependent_casks` in Homebrew 7.0.6), and the
    /// plan never passes that flag: its `args` are exactly `uninstall`,
    /// the kind flag and the name (`OpKind::Uninstall` in `plan`). So the
    /// realistic cost of a gap is an uninstall Homebrew refuses, with its
    /// own message, not one that breaks something.
    ///
    /// Unlike `join_running_update` this asks nobody to be told when the
    /// update ends: its one caller, the uninstall preview, refuses and
    /// leaves nothing on screen that a later refresh would have to clear.
    fn catalogue_stamp(&self, inst: &ManagerInstance) -> Option<CatalogueStamp> {
        let homebrew_lock = match (self.update_lock_fn)(&inst.prefix) {
            HomebrewUpdateLock::Held => return None,
            seen => seen,
        };
        let updates = self.updates.lock().unwrap();
        let canager_updates = match updates.get(&inst.id) {
            Some(record) if record.running => return None,
            Some(record) => record.started,
            None => 0,
        };
        Some(CatalogueStamp {
            canager_updates,
            homebrew_lock,
        })
    }

    /// `op_update_wait` rounded down to whole minutes, for the two locale
    /// sentences that name this bound (`LogNote::WaitingForBrewUpdate` and
    /// `Fault::HomebrewStillUpdating`, both built from this, never from a
    /// second copy of the number). Outside tests `op_update_wait` is
    /// `OP_UPDATE_WAIT`, an exact number of minutes, so the truncation
    /// never bites in production; a test-shortened wait below one minute
    /// rounds down to 0, which is fine since no test asserts a sentence
    /// built from it.
    fn op_update_wait_minutes(&self) -> u64 {
        self.op_update_wait.as_secs() / 60
    }

    /// Waits, cancellably and for at most `op_update_wait`, for any `brew
    /// update` still finishing for this instance, and hands back the lock
    /// so the caller can hold it for the length of its own command.
    ///
    /// The instance's `ResourceLock` does not cover a `brew update` a
    /// refresh has stopped waiting for -- the refresh worker released it
    /// when it moved on -- so without this an install or upgrade could run
    /// while Homebrew is still rewriting the catalogue it installs from.
    /// Homebrew does not stop that itself: its own `update` lock
    /// (`var/homebrew/locks/update`) only turns away a second `brew
    /// update`, and `brew install` never looks at it. Meanwhile `brew
    /// update` git-merges Homebrew's own Ruby code and `curl`s the package
    /// list straight over the file an install reads it from.
    async fn wait_for_update(
        &self,
        inst_id: &InstanceId,
        sink: &Arc<dyn EventSink>,
        op_id: OpId,
        cancel: &CancellationToken,
    ) -> UpdateWait {
        let lock = self.update_lock_for(inst_id);
        if let Ok(guard) = lock.clone().try_lock_owned() {
            return UpdateWait::Ready(guard);
        }
        sink.emit(crate::events::OperationEvent::Note {
            op_id,
            note: crate::events::LogNote::WaitingForBrewUpdate {
                minutes: self.op_update_wait_minutes(),
            },
        });
        tokio::select! {
            guard = lock.lock_owned() => UpdateWait::Ready(guard),
            _ = cancel.cancelled() => UpdateWait::Cancelled,
            _ = tokio::time::sleep(self.op_update_wait) => UpdateWait::GaveUp,
        }
    }

    /// True when `env`'s effective UID means every brew invocation this
    /// adapter would make will refuse to run.
    ///
    /// The rule lives in one named place, rather than as `env.euid == 0`
    /// inline, because two things in `detect` below turn on it: whether to
    /// run `brew --version` at all, and which `Unavailable` the detected
    /// instance carries. It is deliberately *not* public: it used to be,
    /// for a `Session::refresh` call site that no longer exists -- the root
    /// objection is Homebrew's alone and disabling the other six sources
    /// over it was the bug that call site caused.
    fn refuses_as_root(env: &HostEnv) -> bool {
        env.euid == 0
    }

    /// Every Homebrew on this Mac, each with the state it is in.
    ///
    /// Under root this still reports the installs it finds, marked
    /// `Unavailable::RefusesAsRoot`, and runs no brew process at all. An
    /// empty `Vec` would have been read as "Homebrew is not installed" --
    /// by `Session::refresh`'s `DetectOutcome` and, through it, by the
    /// user, who would be told to go and install the Homebrew they already
    /// have instead of to reopen Canager without `sudo`.
    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let as_root = Self::refuses_as_root(env);
        let mut found = Vec::new();
        for candidate in Self::CANDIDATE_PATHS {
            let path = PathBuf::from(candidate);
            if !(self.path_exists_fn)(&path) {
                continue;
            }
            let version = if as_root {
                None
            } else {
                let spec = CommandSpec {
                    program: path.clone(),
                    args: vec!["--version".to_string()],
                    env: self.env_vec(),
                    cwd: None,
                    timeout: Duration::from_secs(30),
                    output_use: OutputUse::Parsed,
                };
                let output = self.runner.run(spec, None, CancellationToken::new()).await;
                match output {
                    Ok(o) if o.exit_code == Some(0) => parse_version(&o.stdout),
                    _ => None,
                }
            };
            let unverified_version = self.meta.unverified_version(&version);
            let prefix = Self::prefix_for(&path);
            found.push(ManagerInstance {
                id: self.instance_id_for(&prefix),
                adapter_id: self.meta.id.clone(),
                exe_path: path,
                prefix,
                scope: Scope::User,
                status: InstanceStatus {
                    // The state axis. Under root nothing was asked, so the
                    // reason is the root run itself -- not `NotResponding`,
                    // which would send the user off reinstalling a Homebrew
                    // that is working perfectly well. Otherwise `version`
                    // is `None` exactly when the CLI is on PATH but
                    // `--version` would not run or could not be parsed: the
                    // tool is there, it just did not answer.
                    unavailable: if as_root {
                        Some(Unavailable::RefusesAsRoot)
                    } else {
                        version.is_none().then_some(Unavailable::NotResponding)
                    },
                    notes: Vec::new(),
                },
                version,
                unverified_version,
                read_only_reason: None,
            });
        }
        found
    }

    /// Everything installed in this Homebrew, or
    /// `AdapterError::IndexUpdating` without running anything while a
    /// `brew update` is rewriting the catalogue `brew info` would read.
    ///
    /// The check goes through `join_running_update`, so the update's end
    /// wakes `background_change` and the shell refreshes again: the
    /// refresh that got this reports `IndexUpdating`, and that notice
    /// clears itself the same way as when `check_updates` reports it.
    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        if self.join_running_update(&inst.id) {
            return Err(AdapterError::IndexUpdating);
        }
        let output = self
            .run_brew(
                inst,
                vec![
                    "info".to_string(),
                    "--installed".to_string(),
                    "--json=v2".to_string(),
                ],
                Duration::from_secs(120),
            )
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        parse_info_installed(&output.stdout, &inst.id)
    }

    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        // A failed `brew update` is a fact about this Homebrew, not about
        // any package it lists: the catalogue everything below is compared
        // against may be behind, which makes "no updates" as suspect as
        // any version number here. So it rides back on the *source*, as a
        // note, and not on the candidates.
        //
        // It used to be a sentence pushed onto every candidate's
        // `warnings`. Two things were wrong with that. `UpdatesPage`
        // renders no warning text on a checkable row -- only a "1 warning"
        // badge -- so the sentence was unreadable; and a Homebrew with
        // nothing outdated produces no candidates at all, so in the one
        // case where the caveat decides whether the page is lying, there
        // was nothing to attach it to.
        //
        // The stderr detail that sentence carried is gone, deliberately:
        // `InstanceNote` is payload-free so the hand-written TypeScript
        // mirror keeps seeing a bare string on the wire (spec §2.3).
        //
        // An update still running is different: `brew outdated` and the
        // `brew info` below would read the catalogue while it is being
        // rewritten, so neither runs, and the caller keeps last round's
        // candidates (see `AdapterError::IndexUpdating`).
        let notes = match self.maybe_update(inst, opts).await {
            IndexFreshness::Current => Vec::new(),
            IndexFreshness::Updating => return Err(AdapterError::IndexUpdating),
            IndexFreshness::MayBeStale => vec![InstanceNote::IndexMayBeStale],
        };
        let mut args = vec!["outdated".to_string(), "--json=v2".to_string()];
        if opts.include_self_updating {
            args.push("--greedy".to_string());
        }
        let output = self.run_brew(inst, args, Duration::from_secs(120)).await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        let mut candidates = parse_outdated(&output.stdout, &inst.id)?;
        // brew's two readers spell the same package differently.
        // `parse_info_installed` keys by `full_name`/`full_token`
        // (`gautham-v/tap/claudebar`); `brew outdated --json=v2` never
        // emits `full_name`, so `parse_outdated` falls back to the bare
        // `claudebar`. Both are faithful readings of what brew printed --
        // which is why the resolution belongs here and not in either
        // parser -- but `ArtifactKey` is what everything downstream joins
        // on, and while the two disagree every tapped formula and cask
        // loses its "update available" badge on the Installed page, its
        // description on the Updates page, and its place in
        // `Session::refresh`'s check that a carried-forward candidate is
        // still installed.
        //
        // The cost is one extra `brew info --installed --json=v2` per
        // check, the same local command `inventory` runs; it buys keys
        // that mean one thing across this adapter. A failed inventory is
        // not a failed check: `qualified_key` returns the key it was
        // given when nothing matches, so the worst case is the short
        // spelling that shipped before.
        let installed = self.inventory(inst).await.unwrap_or_default();
        if !installed.is_empty() {
            for candidate in &mut candidates {
                candidate.key = qualified_key(&installed, &candidate.key);
            }
        }
        Ok(CheckOutcome { candidates, notes })
    }

    pub async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        validate_package_name(query)?;
        let names_output = self
            .run_brew(
                inst,
                vec!["search".to_string(), query.to_string()],
                Duration::from_secs(30),
            )
            .await?;
        if names_output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: names_output.exit_code,
                stderr: names_output.stderr,
            });
        }
        let desc_output = self
            .run_brew(
                inst,
                vec![
                    "search".to_string(),
                    "--desc".to_string(),
                    query.to_string(),
                ],
                Duration::from_secs(30),
            )
            .await?;
        if desc_output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: desc_output.exit_code,
                stderr: desc_output.stderr,
            });
        }
        let mut hits = parse_search(&desc_output.stdout, &self.meta.id);
        if hits.is_empty() {
            hits = parse_search(&names_output.stdout, &self.meta.id);
        }
        Ok(hits)
    }
}

/// What `maybe_update` can say about this Homebrew's catalogue.
enum IndexFreshness {
    /// Updated within the TTL, or just now.
    Current,
    /// A `brew update` is still running: the refresh stopped waiting for
    /// it, or found one already running. Nothing has failed, but the
    /// catalogue is mid-rewrite, so `check_updates` reads nothing.
    Updating,
    /// `brew update` failed, or was refused.
    MayBeStale,
}

/// How `BrewAdapter::wait_for_update` ended.
enum UpdateWait {
    /// No update is running; hold this for the length of the command.
    Ready(tokio::sync::OwnedMutexGuard<()>),
    /// The user pressed Cancel while waiting. Nothing was run.
    Cancelled,
    /// The update was still running after `op_update_wait`. Nothing was
    /// run.
    GaveUp,
}

/// What `BrewAdapter::catalogue_stamp` hands a read of the catalogue to
/// compare with the one it takes after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CatalogueStamp {
    /// `UpdateRecord::started` for the instance.
    canager_updates: u64,
    /// Homebrew's own update lock, never `Held` here: `catalogue_stamp`
    /// returns `None` for that.
    homebrew_lock: HomebrewUpdateLock,
}

/// What `probe_homebrew_update_lock` saw of `<prefix>/var/homebrew/
/// locks/update`, the lock every `brew update` takes (`lock update` in
/// Homebrew's `cmd/update.sh`), whoever started it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HomebrewUpdateLock {
    /// Some process holds it: a `brew update` is running.
    Held,
    /// Nobody holds it.
    Free(LockStamp),
    /// Whether it is held could not be seen: the file is there but would
    /// not open, or `fcntl` failed. Carries what the probe read before
    /// that -- the directory, and the file's own `FileId` when `fcntl` was
    /// what failed -- so that two such probes are still compared on it.
    Unobservable(LockStamp),
}

/// What `probe_homebrew_update_lock` read of the lock's directory and
/// file, for `catalogue_stamp` to compare with a later probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LockStamp {
    /// `<prefix>/var/homebrew/locks` itself, or `None` when it could not
    /// be read (`std::fs::metadata` failed, as when it does not exist
    /// yet). Making or deleting an entry in a directory sets the
    /// directory's mtime and ctime, so this changes when the `update` file
    /// is made or deleted between two probes -- even when there is no file
    /// at either of them, because a `brew update && brew cleanup` typed in
    /// Terminal in between made it and deleted it again
    /// (`test_uninstall_preview_discards_a_brew_uses_that_an_update_and_a_cleanup_ran_during`).
    ///
    /// It changes for any other entry made or deleted there too, which
    /// makes the uninstall preview refuse and ask for a retry when nothing
    /// touched the catalogue: each curl download -- a bottle, say -- that
    /// a `brew install`, `upgrade` or `fetch` makes creates and deletes a
    /// `.download.lock` in this directory (`DownloadLock` in
    /// `CurlDownloadStrategy#fetch`, unlocked with `unlink: true`, in
    /// Homebrew 7.0.6), and a package's first `.formula.lock` is made here
    /// (`LockFile#lock`). That is the safe
    /// direction: a retry, never a list read while the catalogue was being
    /// rewritten.
    dir: Option<FileId>,
    /// The `update` file, or `None` when there is none: `brew cleanup`
    /// deletes it (`Cleanup#cleanup_lockfiles`), and the next `brew
    /// update` makes it again. `brew update` opens it with truncation
    /// before it locks it (`exec 200>"${lock_file}"` in Homebrew's
    /// `utils/lock.sh`), which sets its mtime and ctime even when it is
    /// already empty, so a `brew update` that began and ended between two
    /// probes still leaves this different.
    file: Option<FileId>,
}

/// Which file or directory this is (`dev`, `ino`) and when it last
/// changed (`mtime`, `ctime`, to the nanosecond).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileId {
    dev: u64,
    ino: u64,
    mtime: (i64, i64),
    ctime: (i64, i64),
}

impl FileId {
    fn of(meta: &std::fs::Metadata) -> FileId {
        use std::os::unix::fs::MetadataExt;
        FileId {
            dev: meta.dev(),
            ino: meta.ino(),
            mtime: (meta.mtime(), meta.mtime_nsec()),
            ctime: (meta.ctime(), meta.ctime_nsec()),
        }
    }
}

/// Looks at Homebrew's update lock under `prefix` without taking it.
///
/// Homebrew takes that lock with `flock(2)`: on macOS `lock.sh` runs
/// `lockf -t 0 200` on the descriptor it opened, and `lockf(1)` uses
/// BSD-style (`flock`) locking. Trying to take it here, even shared and
/// for an instant, would make a `brew update` typed in Terminal at that
/// same instant fail with "Another `brew update` process is already
/// running" -- Homebrew asks with a zero timeout. `F_GETLK` only asks
/// whether a lock *would* conflict and never takes one, and on macOS it
/// reports a conflicting `flock` lock too (as `F_WRLCK` with `l_pid` -1):
/// `test_update_lock_probe_sees_a_lock_taken_the_way_homebrew_takes_it`
/// holds one exactly as `lock.sh` does and checks both that this sees it
/// and that looking never stops `lockf -t 0` from taking it.
/// The file is opened read-only and never created, and the directory is
/// only `stat`ed, so looking leaves nothing behind.
///
/// On Linux `flock` and `fcntl` locks do not see each other (flock(2)),
/// so there this never reports `Held`; the `LockStamp` still changes when
/// a `brew update` begins.
fn probe_homebrew_update_lock(prefix: &Path) -> HomebrewUpdateLock {
    use std::os::unix::io::AsRawFd;

    let dir_path = prefix.join("var/homebrew/locks");
    let dir = std::fs::metadata(&dir_path)
        .ok()
        .map(|meta| FileId::of(&meta));
    let file = match std::fs::File::open(dir_path.join("update")) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return HomebrewUpdateLock::Free(LockStamp { dir, file: None })
        }
        Err(_) => return HomebrewUpdateLock::Unobservable(LockStamp { dir, file: None }),
    };
    let Ok(meta) = file.metadata() else {
        return HomebrewUpdateLock::Unobservable(LockStamp { dir, file: None });
    };
    let seen = LockStamp {
        dir,
        file: Some(FileId::of(&meta)),
    };
    // SAFETY: `flock` is a plain C struct for which all-zero bytes are a
    // valid value; every field `F_GETLK` reads is set below.
    let mut query: libc::flock = unsafe { std::mem::zeroed() };
    query.l_type = libc::F_WRLCK as libc::c_short;
    query.l_whence = libc::SEEK_SET as libc::c_short;
    query.l_start = 0;
    query.l_len = 0;
    // SAFETY: `file` is open for the whole call and `query` is a valid,
    // exclusively borrowed `flock` for `F_GETLK` to read and overwrite.
    let rc = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETLK, &mut query) };
    if rc == -1 {
        return HomebrewUpdateLock::Unobservable(seen);
    }
    if query.l_type == libc::F_UNLCK as libc::c_short {
        HomebrewUpdateLock::Free(seen)
    } else {
        HomebrewUpdateLock::Held
    }
}

/// What `BrewAdapter` knows about one instance's `brew update`s.
#[derive(Debug, Default)]
struct UpdateRecord {
    /// When the last `brew update` that exited 0 ended. The TTL runs from
    /// here.
    succeeded_at: Option<Instant>,
    /// A `brew update` is running now, in the task `maybe_update` spawned.
    running: bool,
    /// How many `brew update`s have begun for this instance, counted by
    /// `UpdateFinish::begin`, the one place `running` is set. It lets a
    /// read that must not overlap an update (`catalogue_stamp`) notice one
    /// that began *and* ended while it ran, which `running` alone, sampled
    /// before and after, cannot see.
    started: u64,
    /// A refresh has told the user the running update is still going
    /// (`AdapterError::IndexUpdating`, which the refresh shows as
    /// `InstanceNote::IndexUpdating`), so when it ends its task wakes
    /// `BrewAdapter::background_change`: without a refresh after it, that
    /// notice would stay on screen, and the old catalogue with it, until
    /// the user happened to do something.
    announced: bool,
    /// When an update a refresh announced then failed, until a refresh
    /// round that began after that moment has reported it. While it is
    /// set, `maybe_update` reports `MayBeStale` and starts no `brew
    /// update`: a failure that started a fresh attempt would, on a network
    /// that fails every one, keep Homebrew updating in a loop nobody asked
    /// for. The refresh after the one that takes it -- the notice's "Try
    /// again", or any other -- tries again.
    ///
    /// A time, not a flag the first reader takes, because the first reader
    /// can be the wrong round (F1 in the final concurrency review): a
    /// round that began before the failure but reached brew after it --
    /// held up in detection by a slow source -- used to take the flag, and
    /// the round the failure's wake-up sets off then found none and started
    /// another update. Such a round reports the failure and leaves it
    /// (`maybe_update` compares with `CheckOptions::round_started`). The
    /// wake-up's refresh gets a round that began after the wake-up
    /// (`Session::refresh`), so it is the one that takes it.
    ///
    /// Except in one window: another refresh's round that begins between
    /// the failure and the shell's loop picking the wake-up up began after
    /// the failure, so it takes the flag. The wake-up's refresh then
    /// arrives while that round is in flight and does not share it
    /// (`Session::refresh` shares only a round numbered above the count it
    /// read on arrival), so it runs a round of its own, which finds no
    /// flag and the TTL still expired -- a failure never sets
    /// `succeeded_at` (`UpdateFinish::drop`) -- and starts one fresh `brew
    /// update`. That is one extra, never a loop: if the fresh update ends
    /// within the patience, that same round reports how it went and
    /// nothing is announced; if it outlasts the patience it is announced
    /// (`join_running_update`), and when it fails its wake-up's round
    /// takes the new flag and starts nothing, unless yet another refresh
    /// lands in that same window. The window is normally microseconds. It
    /// is wider only while the shell's loop (`refresh_on_background_change`
    /// in src-tauri/src/ipc.rs) is still inside the refresh for an earlier
    /// wake-up -- on a Mac with Homebrew in both /opt/homebrew and
    /// /usr/local, say.
    unreported_failure: Option<Instant>,
}

/// Writes a finished `brew update`'s result into its `UpdateRecord` when
/// dropped, which is what makes it happen even if the run panics: a
/// `running` left `true` would report "still updating" for the life of the
/// app.
struct UpdateFinish {
    updates: Arc<Mutex<HashMap<InstanceId, UpdateRecord>>>,
    inst_id: InstanceId,
    succeeded: bool,
    background_change: Arc<tokio::sync::Notify>,
}

impl UpdateFinish {
    /// Marks `record`'s update running and returns the guard that marks
    /// it ended. The one place `running` is set to `true`, so a `running`
    /// with no guard to clear it cannot exist: whatever panics after this,
    /// in `maybe_update` or in the update's task, drops the guard.
    fn begin(
        record: &mut UpdateRecord,
        updates: Arc<Mutex<HashMap<InstanceId, UpdateRecord>>>,
        inst_id: InstanceId,
        background_change: Arc<tokio::sync::Notify>,
    ) -> UpdateFinish {
        record.running = true;
        record.started += 1;
        UpdateFinish {
            updates,
            inst_id,
            succeeded: false,
            background_change,
        }
    }
}

impl Drop for UpdateFinish {
    fn drop(&mut self) {
        let announced = {
            // Not `unwrap`: this may run while unwinding, where a second
            // panic aborts.
            let mut updates = self
                .updates
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let record = updates.entry(self.inst_id.clone()).or_default();
            record.running = false;
            if self.succeeded {
                record.succeeded_at = Some(Instant::now());
            }
            let announced = std::mem::take(&mut record.announced);
            if announced && !self.succeeded {
                record.unreported_failure = Some(Instant::now());
            }
            announced
        };
        if announced {
            // `notify_one` keeps the wake-up if nobody is waiting yet, so
            // it is not lost to a shell that is between two refreshes. One
            // that lands *during* a refresh is not lost either: the shell
            // answers it with `Session::refresh`, which never answers a
            // call with a round that started before the call arrived.
            self.background_change.notify_one();
        }
    }
}

/// The name `inventory()` lists an artifact under, given whatever spelling
/// the caller had.
///
/// Homebrew is the one source where two names address the same artifact: a
/// tapped cask is installed as `gautham-v/tap/claudebar` and is just as
/// legitimately called `claudebar`. Returns the caller's own key unchanged
/// when nothing matches, so a genuinely absent artifact stays absent.
///
/// This is the part of brew's reconcile that is *not* shared, and it is
/// deliberately kept here rather than pushed into `reconcile_from` as an
/// option: the last-segment rule would be wrong for npm, whose scoped
/// package names contain `/` (`@types/node` must never be found by
/// `node`).
fn qualified_key(artifacts: &[InstalledArtifact], key: &ArtifactKey) -> ArtifactKey {
    artifacts
        .iter()
        .find(|a| {
            a.key.kind == key.kind
                && (a.key.name == key.name
                    || a.key.name.rsplit('/').next() == Some(key.name.as_str()))
        })
        .map(|a| a.key.clone())
        .unwrap_or_else(|| key.clone())
}

impl BrewAdapter {
    pub async fn plan(
        &self,
        inst: &ManagerInstance,
        req: &OpRequest,
    ) -> Result<Plan, AdapterError> {
        ensure_instance_match(req, inst)?;
        validate_package_name(&req.name)?;
        let lock = ResourceLock(inst.id.clone());
        match req.kind {
            OpKind::Install => {
                let flag = match req.artifact_kind {
                    ArtifactKind::Cask => "--cask",
                    _ => "--formula",
                };
                let needs_password = matches!(req.artifact_kind, ArtifactKind::Cask);
                let mut env = self.env_vec();
                if let Some(askpass) = (self.askpass_fn)() {
                    env.push(("SUDO_ASKPASS".to_string(), askpass));
                }
                Ok(Plan {
                    request: req.clone(),
                    program: inst.exe_path.clone(),
                    args: vec!["install".to_string(), flag.to_string(), req.name.clone()],
                    env,
                    needs_password,
                    locks: vec![lock],
                    cancel_policy: CancelPolicy::KillThenReconcile,
                    warnings: Vec::new(),
                    affected: Vec::new(),
                    timeout_secs: 1800,
                })
            }
            OpKind::Uninstall => {
                let flag = match req.artifact_kind {
                    ArtifactKind::Cask => "--cask",
                    _ => "--formula",
                };
                // `brew uses` reads the same catalogue `brew update`
                // rewrites, and this list is what the user confirms an
                // uninstall against: a half-written read that still
                // parses would show fewer dependents than will break --
                // worse than any error. So it is not read while an update
                // runs -- one Canager started, as `inventory` also checks,
                // or one holding Homebrew's own update lock -- and a read
                // that one began during is thrown away. `catalogue_stamp`
                // says exactly what that does and does not catch, and why
                // what it misses realistically ends in an uninstall
                // Homebrew refuses, not one that breaks something. Refused
                // rather than waited for: the dialog would sit on
                // "checking" for minutes with no word of why;
                // `IndexUpdating` goes out as its own kind
                // (`plan_operation_error` in src-tauri/src/ipc.rs) and the
                // dialog says Homebrew is updating and to try again.
                let Some(stamp) = self.catalogue_stamp(inst) else {
                    return Err(AdapterError::IndexUpdating);
                };
                let uses_output = self
                    .run_brew(
                        inst,
                        vec![
                            "uses".to_string(),
                            "--installed".to_string(),
                            req.name.clone(),
                        ],
                        Duration::from_secs(120),
                    )
                    .await?;
                if self.catalogue_stamp(inst) != Some(stamp) {
                    return Err(AdapterError::IndexUpdating);
                }
                let mut warnings = Vec::new();
                let affected = if uses_output.exit_code == Some(0) {
                    parse_uses(&uses_output.stdout)
                } else {
                    // The check itself failed or timed out — this is *not*
                    // the same thing as "confirmed no dependents", and must
                    // not be presented as if it were.
                    warnings.push(Warning::DependentsUnknown);
                    Vec::new()
                };
                if !affected.is_empty() {
                    warnings.push(Warning::WouldBreak {
                        names: affected.clone(),
                    });
                }
                Ok(Plan {
                    request: req.clone(),
                    program: inst.exe_path.clone(),
                    args: vec!["uninstall".to_string(), flag.to_string(), req.name.clone()],
                    env: self.env_vec(),
                    needs_password: matches!(req.artifact_kind, ArtifactKind::Cask),
                    locks: vec![lock],
                    cancel_policy: CancelPolicy::KillThenReconcile,
                    warnings,
                    affected,
                    timeout_secs: 1800,
                })
            }
            OpKind::Upgrade => {
                let flag = match req.artifact_kind {
                    ArtifactKind::Cask => "--cask",
                    _ => "--formula",
                };
                let needs_password = matches!(req.artifact_kind, ArtifactKind::Cask);
                let mut env = self.env_vec();
                if let Some(askpass) = (self.askpass_fn)() {
                    env.push(("SUDO_ASKPASS".to_string(), askpass));
                }
                Ok(Plan {
                    request: req.clone(),
                    program: inst.exe_path.clone(),
                    args: vec!["upgrade".to_string(), flag.to_string(), req.name.clone()],
                    env,
                    needs_password,
                    locks: vec![lock],
                    cancel_policy: CancelPolicy::KillThenReconcile,
                    warnings: Vec::new(),
                    affected: Vec::new(),
                    timeout_secs: 1800,
                })
            }
        }
    }

    pub async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        // brew is the one adapter that must refuse to run at all as root
        // (Homebrew itself refuses a `sudo brew install`); every other
        // step of turning a plan into an `Outcome` -- the transcript
        // spec, the cancelled/timed-out mapping, the five-line stderr
        // summary -- is identical to every other adapter's, so it is
        // `run_plan` and not a hand-kept copy of it.
        self.refuse_if_root()?;
        let _update_guard = match self
            .wait_for_update(&plan.request.instance_id, &sink, op_id, &cancel)
            .await
        {
            UpdateWait::Ready(guard) => guard,
            UpdateWait::Cancelled => return Ok(Outcome::Cancelled),
            UpdateWait::GaveUp => {
                return Ok(Outcome::CanagerFailed(Fault::HomebrewStillUpdating {
                    minutes: self.op_update_wait_minutes(),
                }));
            }
        };
        run_plan(&self.runner, plan, sink, op_id, cancel).await
    }

    pub async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        let artifacts = self.inventory(inst).await?;
        // Inventory keys are always the fully-qualified name (see
        // `parse::parse_info_installed`), but a caller may have started the
        // operation from a short name (e.g. the user typed `claudebar`
        // rather than `gautham-v/tap/claudebar`, or an older `OpRequest` was
        // built before full names existed). Resolve the short spelling to
        // the name inventory actually uses first; the presence rule itself
        // is the shared one every adapter applies.
        let key = qualified_key(&artifacts, key);
        let mut reconciled = reconcile_from(artifacts, &key);
        // A cask declared `version :latest` is installed under the version
        // "latest": `brew info` reports as `installed` the name of the
        // version directory in its Caskroom metadata
        // (`Caskroom.cask_installed_version` in Homebrew 7.0.6), and an
        // upgrade reinstalls it under that same name. `brew outdated
        // --greedy` lists one whenever its download has changed
        // (`Cask#outdated_version`), so an upgrade of it
        // does real work while the version reads the same before and
        // after. The string cannot tell one install from another, so it is
        // not offered as one (`Reconciled::version`).
        if key.kind == ArtifactKind::Cask && reconciled.version.as_deref() == Some("latest") {
            reconciled.version = None;
        }
        Ok(reconciled)
    }
}

// Adapter is implemented by forwarding to the inherent methods above via
// fully-qualified `BrewAdapter::method(self, ...)` calls. This is
// unambiguous even though the method names match: `BrewAdapter::detect`
// can only resolve to the inherent impl (a trait method would be spelled
// `<BrewAdapter as Adapter>::detect`), so there is no risk of the trait
// method accidentally calling itself.
#[async_trait]
impl Adapter for BrewAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        BrewAdapter::detect(self, env).await
    }

    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        BrewAdapter::inventory(self, inst).await
    }

    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        BrewAdapter::check_updates(self, inst, opts).await
    }

    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        BrewAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        BrewAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        BrewAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        BrewAdapter::reconcile(self, inst, key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ArtifactKind;
    use crate::runner::MockRunner;
    use std::sync::Arc;

    fn test_instance() -> ManagerInstance {
        ManagerInstance {
            exe_path: PathBuf::from("/opt/homebrew/bin/brew"),
            prefix: PathBuf::from("/opt/homebrew"),
            version: Some("7.0.3".to_string()),
            ..crate::testing::manager_instance("brew", "brew:/opt/homebrew")
        }
    }

    /// (F5 / M7) Every brew subprocess entry point besides `detect` must
    /// also refuse to run as root — `detect` checks the *passed-in*
    /// `HostEnv::euid`, but everything else (inventory, check_updates,
    /// search, plan, execute) previously took a ready-made `ManagerInstance`
    /// and ran brew unconditionally regardless of the *real* euid. `euid_fn`
    /// is injected here (via the `#[cfg(test)]`-only `with_euid_fn`
    /// constructor) since a real test cannot change its own euid.
    #[tokio::test]
    async fn test_inventory_refuses_as_root() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone()).with_euid_fn(|| 0);
        let result = adapter.inventory(&test_instance()).await;
        match result {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
        assert_eq!(
            runner.calls().len(),
            0,
            "no brew process should ever be started as root"
        );
    }

    /// The four Homebrew layouts a contributor's Mac can be in, written as
    /// answers to the one filesystem question `detect` asks. Every detect
    /// test below picks one, so what it proves is the same everywhere the
    /// suite runs -- rather than whatever the machine running it happens
    /// to have installed.
    fn apple_silicon_layout(path: &Path) -> bool {
        path == Path::new("/opt/homebrew/bin/brew")
    }

    fn intel_layout(path: &Path) -> bool {
        path == Path::new("/usr/local/bin/brew")
    }

    fn both_prefixes_layout(path: &Path) -> bool {
        apple_silicon_layout(path) || intel_layout(path)
    }

    fn no_homebrew_layout(_path: &Path) -> bool {
        false
    }

    /// A `HostEnv` for the detect tests. Brew's `detect` reads only `euid`
    /// from it -- the candidate paths are absolute, not searched on `PATH`
    /// -- so the rest is filler.
    fn detect_env(euid: u32) -> HostEnv {
        HostEnv {
            path_dirs: vec![],
            home: if euid == 0 {
                PathBuf::from("/var/root")
            } else {
                PathBuf::from("/tmp")
            },
            euid,
            cargo_home: None,
            ollama_host: None,
        }
    }

    /// A runner that answers `--version` for one candidate path.
    fn version_runner(brew_path: &str, version_line: &str) -> Arc<MockRunner> {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![brew_path, "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: version_line.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner
    }

    #[tokio::test]
    async fn test_detect_under_root_reports_the_instance_as_refusing_rather_than_missing() {
        // Returning no instance at all here is indistinguishable from
        // "Homebrew is not installed", and the front end says exactly that:
        // `SnapshotStatus` falls through to "None of them are set up on
        // this Mac yet". Homebrew *is* set up; the one thing the user has
        // to be told -- quit and reopen without `sudo` -- is the one thing
        // an empty `Vec` cannot say. So the instance is detected and marked
        // unavailable with a reason, which `sourceNoticesFor` turns into
        // that sentence and `Session::issue_plan` turns into a refusal.
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone()).with_path_exists_fn(apple_silicon_layout);
        let env = detect_env(0);
        let instances = adapter.detect(&env).await;
        assert_eq!(instances.len(), 1, "got {instances:?}");
        assert_eq!(instances[0].id, "brew:/opt/homebrew");
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::RefusesAsRoot),
            "a root run is its own reason, not the generic NotResponding"
        );
        assert!(
            instances[0].writable(),
            "nothing about root makes the prefix read-only; the state axis carries this"
        );
        assert_eq!(
            runner.calls().len(),
            0,
            "no brew process should ever be started as root, `--version` included"
        );
    }

    #[test]
    fn test_refuses_as_root_is_true_only_for_euid_zero() {
        let root = HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/var/root"),
            euid: 0,
            cargo_home: None,
            ollama_host: None,
        };
        let user = HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            ollama_host: None,
        };
        assert!(BrewAdapter::refuses_as_root(&root));
        assert!(!BrewAdapter::refuses_as_root(&user));
    }

    #[tokio::test]
    async fn test_detect_finds_the_apple_silicon_prefix() {
        let runner = version_runner("/opt/homebrew/bin/brew", "Homebrew 7.0.3\n");
        let adapter = BrewAdapter::new(runner).with_path_exists_fn(apple_silicon_layout);
        let instances = adapter.detect(&detect_env(501)).await;
        assert_eq!(instances.len(), 1, "got {instances:?}");
        assert_eq!(instances[0].id, "brew:/opt/homebrew");
        assert_eq!(
            instances[0].exe_path,
            PathBuf::from("/opt/homebrew/bin/brew")
        );
        assert_eq!(instances[0].prefix, PathBuf::from("/opt/homebrew"));
        assert_eq!(instances[0].version, Some("7.0.3".to_string()));
        assert!(instances[0].available());
    }

    #[tokio::test]
    async fn test_detect_finds_the_intel_prefix() {
        // The same Homebrew, installed where an Intel Mac puts it. This
        // used to be untestable without owning an Intel Mac, which meant
        // the `/usr/local` half of `CANDIDATE_PATHS` -- and the
        // `prefix_for` derivation that turns the exe path back into a
        // prefix -- was never exercised at all.
        let runner = version_runner("/usr/local/bin/brew", "Homebrew 7.0.3\n");
        let adapter = BrewAdapter::new(runner).with_path_exists_fn(intel_layout);
        let instances = adapter.detect(&detect_env(501)).await;
        assert_eq!(instances.len(), 1, "got {instances:?}");
        assert_eq!(instances[0].id, "brew:/usr/local");
        assert_eq!(instances[0].exe_path, PathBuf::from("/usr/local/bin/brew"));
        assert_eq!(instances[0].prefix, PathBuf::from("/usr/local"));
        assert_eq!(instances[0].version, Some("7.0.3".to_string()));
        assert!(instances[0].available());
    }

    #[tokio::test]
    async fn test_detect_finds_both_prefixes_as_separate_instances() {
        // A Mac that has been through a Rosetta phase carries both. They
        // are two Homebrews, not one: separate ids (which is what keys the
        // per-instance `brew update` throttle and the operation lock),
        // separate exe paths, and separate version answers.
        let runner = version_runner("/opt/homebrew/bin/brew", "Homebrew 7.0.3\n");
        runner.respond(
            vec!["/usr/local/bin/brew", "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "Homebrew 99.9.9\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner).with_path_exists_fn(both_prefixes_layout);
        let instances = adapter.detect(&detect_env(501)).await;
        assert_eq!(instances.len(), 2, "got {instances:?}");
        let ids: Vec<&str> = instances.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["brew:/opt/homebrew", "brew:/usr/local"],
            "in CANDIDATE_PATHS order, so the Apple Silicon install is listed first"
        );
        assert_eq!(instances[0].version, Some("7.0.3".to_string()));
        assert_eq!(instances[1].version, Some("99.9.9".to_string()));
        assert_eq!(
            instances[1].unverified_version,
            Some("99.9.9".to_string()),
            "each instance is judged against brew.toml on its own version"
        );
        assert!(instances[0].unverified_version.is_none());
    }

    #[tokio::test]
    async fn test_detect_finds_nothing_when_homebrew_is_not_installed() {
        // A clean Mac with no Homebrew: no instance, and -- the part worth
        // pinning -- no subprocess either. `detect` must not try to run a
        // `brew` that is not there.
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone()).with_path_exists_fn(no_homebrew_layout);
        let instances = adapter.detect(&detect_env(501)).await;
        assert!(instances.is_empty(), "got {instances:?}");
        assert!(
            runner.calls().is_empty(),
            "nothing on disk means nothing to ask for a version"
        );
    }

    #[tokio::test]
    async fn test_detect_marks_an_install_that_does_not_answer_as_not_responding() {
        // The binary is on disk but `--version` fails: the tool is there,
        // it just did not answer, which is `NotResponding` and not a
        // missing install. The MockRunner has no canned response for this
        // argv, which is exactly what that failure looks like here.
        let adapter =
            BrewAdapter::new(Arc::new(MockRunner::new())).with_path_exists_fn(intel_layout);
        let instances = adapter.detect(&detect_env(501)).await;
        assert_eq!(instances.len(), 1, "got {instances:?}");
        assert_eq!(instances[0].version, None);
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::NotResponding)
        );
    }

    #[tokio::test]
    async fn test_detect_does_not_flag_a_version_listed_in_brew_toml() {
        let runner = version_runner("/opt/homebrew/bin/brew", "Homebrew 7.0.3\n");
        let adapter = BrewAdapter::new(runner).with_path_exists_fn(apple_silicon_layout);
        let instances = adapter.detect(&detect_env(501)).await;
        assert_eq!(instances.len(), 1);
        assert!(
            instances[0].unverified_version.is_none(),
            "7.0.3 is listed in adapters/meta/brew.toml's verified_versions"
        );
    }

    #[tokio::test]
    async fn test_detect_flags_an_unverified_version_not_in_brew_toml() {
        let runner = version_runner("/opt/homebrew/bin/brew", "Homebrew 99.9.9\n");
        let adapter = BrewAdapter::new(runner).with_path_exists_fn(apple_silicon_layout);
        let instances = adapter.detect(&detect_env(501)).await;
        assert_eq!(instances.len(), 1);
        assert_eq!(
            instances[0].unverified_version,
            Some("99.9.9".to_string()),
            "a version not listed in adapters/meta/brew.toml's verified_versions must be flagged"
        );
    }

    #[tokio::test]
    async fn test_inventory_parses_formula_and_cask() {
        let runner = Arc::new(MockRunner::new());
        let json = r#"{
            "formulae": [
                {"name":"jq","desc":"JSON processor","homepage":"https://jqlang.org","linked_keg":"1.7.1","installed":[{"version":"1.7.1","installed_on_request":true,"installed_as_dependency":false,"time":1700000000}]}
            ],
            "casks": [
                {"token":"claudebar","name":["ClaudeBar"],"desc":"Menu bar app","homepage":"https://example.invalid","installed":"1.0.0","auto_updates":false}
            ]
        }"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "info", "--installed", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: json.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let artifacts = adapter
            .inventory(&test_instance())
            .await
            .expect("inventory");
        assert_eq!(artifacts.len(), 2);
        assert_eq!(artifacts[0].key.kind, ArtifactKind::Formula);
        assert_eq!(artifacts[1].key.kind, ArtifactKind::Cask);
    }

    #[tokio::test]
    async fn test_check_updates_respects_ttl() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "update"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let outdated_json = r#"{"formulae":[{"name":"jq","installed_versions":["1.6"],"current_version":"1.7.1","pinned":false,"pinned_version":null}],"casks":[]}"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: outdated_json.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let mock_ref = runner.clone();
        let adapter = BrewAdapter::new(runner).with_update_ttl(Duration::from_secs(3600));
        let inst = test_instance();

        let first = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("first check_updates")
            .candidates;
        assert_eq!(first.len(), 1);
        let second = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("second check_updates")
            .candidates;
        assert_eq!(second.len(), 1);

        let calls = mock_ref.calls();
        let update_calls = calls
            .iter()
            .filter(|c| c.get(1).map(String::as_str) == Some("update"))
            .count();
        let outdated_calls = calls
            .iter()
            .filter(|c| c.get(1).map(String::as_str) == Some("outdated"))
            .count();
        assert_eq!(
            update_calls, 1,
            "brew update should run once within the TTL window"
        );
        assert_eq!(outdated_calls, 2);
    }

    /// (F7 / M6) `last_update` used to be a single `Option<Instant>` shared
    /// by the whole adapter, so checking `/opt/homebrew` first and then
    /// `/usr/local` would make the second instance skip its *own* `brew
    /// update` even though it has never run one. Each `ManagerInstance` must
    /// get its own TTL bookkeeping.
    #[tokio::test]
    async fn test_check_updates_ttl_is_tracked_per_instance() {
        let runner = Arc::new(MockRunner::new());
        let empty_outdated = r#"{"formulae":[],"casks":[]}"#;
        for brew in ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"] {
            runner.respond(
                vec![brew, "update"],
                CommandOutput {
                    exit_code: Some(0),
                    stdout: String::new(),
                    stderr: String::new(),
                    timed_out: false,
                    cancelled: false,
                },
            );
            runner.respond(
                vec![brew, "outdated", "--json=v2"],
                CommandOutput {
                    exit_code: Some(0),
                    stdout: empty_outdated.to_string(),
                    stderr: String::new(),
                    timed_out: false,
                    cancelled: false,
                },
            );
        }
        let mock_ref = runner.clone();
        let adapter = BrewAdapter::new(runner).with_update_ttl(Duration::from_secs(3600));

        let inst_opt = test_instance();
        let inst_local = ManagerInstance {
            exe_path: PathBuf::from("/usr/local/bin/brew"),
            prefix: PathBuf::from("/usr/local"),
            version: Some("7.0.3".to_string()),
            ..crate::testing::manager_instance("brew", "brew:/usr/local")
        };

        adapter
            .check_updates(&inst_opt, &CheckOptions::default())
            .await
            .expect("check_updates opt/homebrew #1");
        adapter
            .check_updates(&inst_local, &CheckOptions::default())
            .await
            .expect("check_updates usr/local #1");

        let update_calls_after_first_round = mock_ref
            .calls()
            .iter()
            .filter(|c| c.get(1).map(String::as_str) == Some("update"))
            .count();
        assert_eq!(
            update_calls_after_first_round, 2,
            "each instance must run its own `brew update` once, not share one TTL"
        );

        adapter
            .check_updates(&inst_opt, &CheckOptions::default())
            .await
            .expect("check_updates opt/homebrew #2");
        adapter
            .check_updates(&inst_local, &CheckOptions::default())
            .await
            .expect("check_updates usr/local #2");

        let update_calls_after_second_round = mock_ref
            .calls()
            .iter()
            .filter(|c| c.get(1).map(String::as_str) == Some("update"))
            .count();
        assert_eq!(
            update_calls_after_second_round, 2,
            "within the TTL window, neither instance should run `brew update` again"
        );
    }

    #[tokio::test]
    async fn test_check_updates_passes_greedy_flag_when_include_self_updating_is_true() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "update"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let empty_outdated = r#"{"formulae":[],"casks":[]}"#;
        runner.respond(
            vec![
                "/opt/homebrew/bin/brew",
                "outdated",
                "--json=v2",
                "--greedy",
            ],
            CommandOutput {
                exit_code: Some(0),
                stdout: empty_outdated.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let opts = CheckOptions {
            include_self_updating: true,
            ..CheckOptions::default()
        };
        let result = adapter
            .check_updates(&test_instance(), &opts)
            .await
            .expect("check_updates with --greedy")
            .candidates;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_check_updates_names_a_tapped_package_the_way_the_inventory_does() {
        // brew's two readers spell the same package differently: `brew
        // info --installed --json=v2` gives a cask's `full_token`
        // (`gautham-v/tap/claudebar`), while `brew outdated --json=v2`
        // never emits `full_name` at all and so falls back to the bare
        // `claudebar` -- both true of this repo's recorded fixtures.
        // `ArtifactKey` is what everything downstream joins on: the
        // Installed page's "update available" badge, the Updates page's
        // description lookup, and `Session::refresh`'s check that a
        // carried-forward candidate is still installed. All three miss
        // for every tapped formula and cask while the two spellings
        // disagree, so the candidate is renamed here, once, against the
        // inventory -- the same resolution `reconcile` already does
        // through `qualified_key`.
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "update"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let installed = r#"{"formulae":[],"casks":[{"token":"claudebar","full_token":"gautham-v/tap/claudebar","name":["ClaudeBar"],"installed":"0.1.1","desc":"Menu bar app"}]}"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "info", "--installed", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: installed.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let outdated = r#"{"formulae":[],"casks":[{"name":"claudebar","installed_versions":["0.1.1"],"current_version":"0.1.2"}]}"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: outdated.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].key.name, "gautham-v/tap/claudebar",
            "the candidate must carry the name the inventory lists, not the short one"
        );
        assert_eq!(candidates[0].key.kind, ArtifactKind::Cask);
        assert_eq!(candidates[0].target, "0.1.2");
    }

    #[tokio::test]
    async fn test_check_updates_keeps_the_short_name_when_the_inventory_cannot_be_read() {
        // Qualifying is a best effort over a second command, and that
        // command can fail. When it does, the candidates are still the
        // right candidates under brew's own short spelling -- which is
        // what shipped before -- so the check must not fail with it.
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "update"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        // No response registered for `brew info --installed --json=v2`:
        // MockRunner answers `NoMock`, i.e. the inventory errors.
        let outdated = r#"{"formulae":[{"name":"jq","installed_versions":["1.6"],"current_version":"1.7.1"}],"casks":[]}"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: outdated.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("a failed inventory must not fail the update check")
            .candidates;
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "jq");
    }

    #[tokio::test]
    async fn test_check_updates_reports_a_failed_brew_update_as_a_note_on_the_source() {
        // A failed `brew update` is a fact about Homebrew, not about jq:
        // the catalogue Canager compared against may be behind, so every
        // answer from this round -- including "nothing is outdated" -- may
        // be wrong. It used to ride along as a string on each candidate's
        // `warnings`, where `UpdatesPage` renders a "1 warning" badge on a
        // checkable row and never the warning's text: a badge whose
        // contents the user cannot read. Worse, a source with nothing
        // outdated has no candidates at all, so the one case where the
        // caveat matters most carried it nowhere.
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "update"],
            CommandOutput {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "error: brew update failed: no such remote".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let outdated_json = r#"{"formulae":[{"name":"jq","installed_versions":["1.6"],"current_version":"1.7.1","pinned":false,"pinned_version":null}],"casks":[]}"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: outdated_json.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let outcome = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("a failed `brew update` must not fail check_updates");
        assert_eq!(
            outcome.notes,
            vec![InstanceNote::IndexMayBeStale],
            "the source has to carry the caveat, because no package row can"
        );
        assert_eq!(outcome.candidates.len(), 1);
        assert!(
            outcome.candidates[0].warnings.is_empty(),
            "removed, not duplicated: leaving it on the row adds a badge \
             whose text this page never renders, got {:?}",
            outcome.candidates[0].warnings
        );
    }

    #[tokio::test]
    async fn test_check_updates_reports_no_note_when_brew_update_succeeded() {
        // The other half: a note that appears when nothing is wrong would
        // put a permanent "this may not be accurate" banner over a page
        // that is accurate, and the user would learn to ignore it.
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "update"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: r#"{"formulae":[],"casks":[]}"#.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let outcome = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("check_updates");
        assert!(outcome.notes.is_empty(), "got {:?}", outcome.notes);
        assert!(outcome.candidates.is_empty());
    }

    #[tokio::test]
    async fn test_check_updates_serialises_maybe_update_across_concurrent_callers() {
        // Before this fix, two concurrent `check_updates` calls for the same
        // instance could both observe the TTL expired and both run `brew
        // update` — this test makes the first `update` slow enough that a
        // second, concurrent call is guaranteed to reach its own TTL check
        // while the first is still in flight, and proves the fix serialises
        // them: only one `brew update` process ever runs.
        let runner = Arc::new(MockRunner::new());
        runner.delay(
            vec!["/opt/homebrew/bin/brew", "update"],
            Duration::from_millis(200),
        );
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "update"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let empty_outdated = r#"{"formulae":[],"casks":[]}"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: empty_outdated.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter =
            Arc::new(BrewAdapter::new(runner.clone()).with_update_ttl(Duration::from_secs(3600)));
        let inst = test_instance();

        let task_a = {
            let adapter = adapter.clone();
            let inst = inst.clone();
            tokio::spawn(
                async move { adapter.check_updates(&inst, &CheckOptions::default()).await },
            )
        };
        // Give task_a time to acquire the per-instance update lock and start
        // its (slow) `brew update` before task_b starts.
        tokio::time::sleep(Duration::from_millis(20)).await;
        let task_b = {
            let adapter = adapter.clone();
            let inst = inst.clone();
            tokio::spawn(
                async move { adapter.check_updates(&inst, &CheckOptions::default()).await },
            )
        };

        task_a
            .await
            .expect("task a panicked")
            .expect("check_updates a");
        // b found a's update still running, so it read nothing and said
        // so rather than start a second `brew update`.
        let b = task_b.await.expect("task b panicked");
        assert!(matches!(b, Err(AdapterError::IndexUpdating)), "got {b:?}");

        let update_calls = runner
            .calls()
            .iter()
            .filter(|c| c.get(1).map(String::as_str) == Some("update"))
            .count();
        assert_eq!(
            update_calls, 1,
            "two concurrent check_updates calls for the same instance must run `brew update` at most once"
        );
    }

    #[tokio::test]
    async fn test_search_calls_both_search_variants() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "search", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "==> Formulae\njq\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "search", "--desc", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "==> Formulae\njq: Command-line JSON processor\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let mock_ref = runner.clone();
        let adapter = BrewAdapter::new(runner);
        let hits = adapter
            .search(&test_instance(), "jq")
            .await
            .expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "jq");
        assert_eq!(
            hits[0].description.as_deref(),
            Some("Command-line JSON processor")
        );
        assert_eq!(
            mock_ref.calls(),
            vec![
                vec![
                    "/opt/homebrew/bin/brew".to_string(),
                    "search".to_string(),
                    "jq".to_string()
                ],
                vec![
                    "/opt/homebrew/bin/brew".to_string(),
                    "search".to_string(),
                    "--desc".to_string(),
                    "jq".to_string()
                ],
            ]
        );
    }
}

#[cfg(test)]
mod plan_execute_tests {
    use super::*;
    use crate::events::VecSink;
    use crate::runner::MockRunner;

    fn test_instance() -> ManagerInstance {
        ManagerInstance {
            exe_path: PathBuf::from("/opt/homebrew/bin/brew"),
            prefix: PathBuf::from("/opt/homebrew"),
            version: Some("7.0.3".to_string()),
            ..crate::testing::manager_instance("brew", "brew:/opt/homebrew")
        }
    }

    /// (F4 / M2) `plan` must refuse when the request's `instance_id` does not
    /// match the instance it was actually given — otherwise a caller could
    /// end up with a plan that runs against instance A but reconciles
    /// against instance B's request, silently operating on the wrong
    /// Homebrew installation.
    #[tokio::test]
    async fn test_plan_refuses_when_request_instance_id_does_not_match_given_instance() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone());
        let inst_a = test_instance();
        let inst_b = ManagerInstance {
            exe_path: PathBuf::from("/usr/local/bin/brew"),
            prefix: PathBuf::from("/usr/local"),
            version: Some("7.0.3".to_string()),
            ..crate::testing::manager_instance("brew", "brew:/usr/local")
        };
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst_b.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };

        let result = adapter.plan(&inst_a, &req).await;
        match result {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
        assert!(
            runner.calls().is_empty(),
            "a mismatched instance must be rejected before any brew command runs"
        );
    }

    #[tokio::test]
    async fn test_plan_install_formula() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.args, vec!["install", "--formula", "jq"]);
        assert!(!plan.needs_password);
        assert_eq!(
            plan.locks,
            vec![ResourceLock("brew:/opt/homebrew".to_string())]
        );
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);
    }

    #[tokio::test]
    async fn test_plan_install_cask_needs_password() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Cask,
            name: "claudebar".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.args, vec!["install", "--cask", "claudebar"]);
        assert!(plan.needs_password);
    }

    #[tokio::test]
    async fn test_plan_uninstall_with_dependents_warns() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "python@3.13\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.args, vec!["uninstall", "--formula", "jq"]);
        assert_eq!(plan.affected, vec!["python@3.13".to_string()]);
        assert_eq!(
            plan.warnings,
            vec![Warning::WouldBreak {
                names: vec!["python@3.13".to_string()]
            }]
        );
    }

    /// (F6 / M5) When `brew uses --installed {name}` itself fails (non-zero
    /// exit — the same shape a timeout produces, since `CommandOutput` for a
    /// timed-out run also carries `exit_code: None`), the dependents list
    /// must not be silently treated as "confirmed empty": the plan needs a
    /// distinct warning saying the check itself could not be completed, so
    /// the user is not told "nothing depends on this" when the truth is
    /// "we don't know".
    #[tokio::test]
    async fn test_plan_uninstall_warns_when_uses_check_fails() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "jq"],
            CommandOutput {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "error: some transient brew failure".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert!(plan.affected.is_empty());
        assert!(
            plan.warnings.contains(&Warning::DependentsUnknown),
            "expected a could-not-determine warning, got {:?}",
            plan.warnings
        );
    }

    #[tokio::test]
    async fn test_plan_uninstall_without_dependents_has_no_warning() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.args, vec!["uninstall", "--formula", "jq"]);
        assert!(plan.affected.is_empty());
        assert!(plan.warnings.is_empty());
    }

    /// (F10 / M8) Uninstalling a cask can require a password (e.g. a
    /// `pkgutil`/kernel-extension/launch-daemon removal invoking `sudo`),
    /// unlike a formula uninstall which never does. Before this fix every
    /// uninstall plan hardcoded `needs_password: false` regardless of
    /// artifact kind.
    #[tokio::test]
    async fn test_plan_uninstall_needs_password_matches_artifact_kind() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "docker"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();

        let cask_plan = adapter
            .plan(
                &inst,
                &OpRequest {
                    kind: OpKind::Uninstall,
                    instance_id: inst.id.clone(),
                    artifact_kind: ArtifactKind::Cask,
                    name: "docker".to_string(),
                },
            )
            .await
            .expect("cask uninstall plan");
        assert!(
            cask_plan.needs_password,
            "cask uninstalls may need sudo (e.g. pkgutil/kext removal)"
        );

        let formula_plan = adapter
            .plan(
                &inst,
                &OpRequest {
                    kind: OpKind::Uninstall,
                    instance_id: inst.id.clone(),
                    artifact_kind: ArtifactKind::Formula,
                    name: "jq".to_string(),
                },
            )
            .await
            .expect("formula uninstall plan");
        assert!(!formula_plan.needs_password);
    }

    #[tokio::test]
    async fn test_plan_uninstall_cask_passes_cask_flag() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "docker"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Cask,
            name: "docker".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        // `docker` exists as both a formula and a cask; without `--cask`
        // here `brew uninstall docker` would act on the wrong one.
        assert_eq!(plan.args, vec!["uninstall", "--cask", "docker"]);
    }

    #[tokio::test]
    async fn test_plan_upgrade_formula_passes_formula_flag() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.args, vec!["upgrade", "--formula", "jq"]);
        assert!(!plan.needs_password);
    }

    #[tokio::test]
    async fn test_plan_upgrade_cask_passes_cask_flag() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Cask,
            name: "docker".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        // Same ambiguous-name hazard as uninstall: `docker` is both a
        // formula and a cask.
        assert_eq!(plan.args, vec!["upgrade", "--cask", "docker"]);
        assert!(plan.needs_password);
    }

    /// Homebrew's `--zap` removes everything a cask's zap stanza names --
    /// for `claude-code` that is the *native* install's `~/.local/bin/claude`
    /// and `~/.local/share/claude`, and the shared `~/.claude` -- and
    /// `--force` and `--ignore-dependencies` override refusals Homebrew makes
    /// on the user's behalf. None of the three has ever been passed here, but
    /// until now that was an absence, not a promise: `docs/what-we-run.md`
    /// says Canager never passes them, and this is what keeps that sentence
    /// true when `plan` is next edited. Every plan brew builds, for both
    /// artifact kinds, is exactly the verb, the kind flag and the name.
    #[tokio::test]
    async fn test_plan_never_passes_zap_force_or_ignore_dependencies() {
        let runner = Arc::new(MockRunner::new());
        // An Uninstall plan runs `brew uses --installed {name}` first; an
        // empty answer keeps the plan free of dependents, which is not what
        // this test is about.
        for name in ["jq", "docker"] {
            runner.respond(
                vec!["/opt/homebrew/bin/brew", "uses", "--installed", name],
                CommandOutput {
                    exit_code: Some(0),
                    stdout: String::new(),
                    stderr: String::new(),
                    timed_out: false,
                    cancelled: false,
                },
            );
        }
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        const FORBIDDEN: [&str; 3] = ["--zap", "--force", "--ignore-dependencies"];
        for (artifact_kind, flag, name) in [
            (ArtifactKind::Formula, "--formula", "jq"),
            (ArtifactKind::Cask, "--cask", "docker"),
        ] {
            for (kind, verb) in [
                (OpKind::Install, "install"),
                (OpKind::Uninstall, "uninstall"),
                (OpKind::Upgrade, "upgrade"),
            ] {
                let req = OpRequest {
                    kind,
                    instance_id: inst.id.clone(),
                    artifact_kind,
                    name: name.to_string(),
                };
                let plan = adapter
                    .plan(&inst, &req)
                    .await
                    .unwrap_or_else(|e| panic!("plan {verb} {flag} {name}: {e}"));
                for forbidden in FORBIDDEN {
                    assert!(
                        !plan.args.iter().any(|arg| arg.as_str() == forbidden),
                        "brew {verb} {flag} {name} must never carry {forbidden}, got {:?}",
                        plan.args
                    );
                }
                assert_eq!(
                    plan.args,
                    vec![verb, flag, name],
                    "brew {verb} {flag} {name} is exactly the verb, the kind flag and the name"
                );
            }
        }
    }

    /// (F5 / M7) `execute` builds its `CommandSpec` and calls the runner
    /// directly rather than going through `run_brew`, so it needs its own
    /// euid gate rather than inheriting one that only lives in `run_brew`.
    #[tokio::test]
    async fn test_execute_refuses_as_root() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "install", "--formula", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner.clone()).with_euid_fn(|| 0);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        // `plan` itself does not run brew, so it is unaffected by the euid
        // gate; build the plan with a non-root adapter and only inject root
        // for the `execute` call under test.
        let plan_adapter = BrewAdapter::new(Arc::new(MockRunner::new()));
        let plan = plan_adapter.plan(&inst, &req).await.expect("plan");
        let sink = Arc::new(VecSink::new());
        let result = adapter
            .execute(&plan, sink, 1, CancellationToken::new())
            .await;
        match result {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
        assert_eq!(
            runner.calls().len(),
            0,
            "no brew process should ever be started as root"
        );
    }

    #[tokio::test]
    async fn test_execute_streams_log_events_and_succeeds() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "install", "--formula", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "Installing jq\nDone\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome = adapter
            .execute(&plan, sink.clone(), 1, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(sink.snapshot().len(), 2);
    }

    #[tokio::test]
    async fn test_execute_failure_summary_is_last_five_stderr_lines() {
        let runner = Arc::new(MockRunner::new());
        let stderr = (1..=8)
            .map(|n| format!("line{n}"))
            .collect::<Vec<_>>()
            .join("\n");
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "install", "--formula", "jq"],
            CommandOutput {
                exit_code: Some(1),
                stdout: String::new(),
                stderr,
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome = adapter
            .execute(&plan, sink, 1, CancellationToken::new())
            .await
            .expect("execute");
        match outcome {
            Outcome::Failed { exit_code, summary } => {
                assert_eq!(exit_code, Some(1));
                assert_eq!(summary, "line4\nline5\nline6\nline7\nline8");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_and_absent() {
        let runner = Arc::new(MockRunner::new());
        let json = r#"{"formulae":[{"name":"jq","desc":null,"homepage":null,"linked_keg":"1.7.1","installed":[{"version":"1.7.1","installed_on_request":true,"installed_as_dependency":false,"time":null}]}],"casks":[]}"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "info", "--installed", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: json.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();

        let present = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Formula,
                    name: "jq".to_string(),
                },
            )
            .await
            .expect("reconcile present");
        assert_eq!(
            present,
            Reconciled {
                present: true,
                version: Some("1.7.1".to_string())
            }
        );

        let absent = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Formula,
                    name: "missing".to_string(),
                },
            )
            .await
            .expect("reconcile absent");
        assert_eq!(
            absent,
            Reconciled {
                present: false,
                version: None
            }
        );
    }

    /// (F3) A cask installed from a third-party tap — the real, committed
    /// fixture `adapters/fixtures/brew/7.0.3/info-installed.json` has exactly
    /// this shape for `claudebar` (`"token": "claudebar", "full_token":
    /// "gautham-v/tap/claudebar"`). Inlining the relevant fields here as a
    /// literal is the "inline JSON string unit test" exception the fix brief
    /// calls out — it does not require hand-writing a *fixture file*.
    fn tap_cask_json() -> &'static str {
        r#"{
            "formulae": [],
            "casks": [
                {
                    "token": "claudebar",
                    "full_token": "gautham-v/tap/claudebar",
                    "name": ["Claudebar"],
                    "desc": "Claude usage limits in the menu bar",
                    "homepage": "https://github.com/gautham-v/claudebar",
                    "installed": "0.1.1",
                    "auto_updates": null
                }
            ]
        }"#
    }

    #[tokio::test]
    async fn test_inventory_uses_full_token_as_key_name_for_tapped_cask() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "info", "--installed", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: tap_cask_json().to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let artifacts = adapter
            .inventory(&test_instance())
            .await
            .expect("inventory");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].key.name, "gautham-v/tap/claudebar");
        assert_eq!(artifacts[0].display_name, "Claudebar");
    }

    #[tokio::test]
    async fn test_reconcile_matches_tapped_cask_by_full_name_and_short_name() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "info", "--installed", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: tap_cask_json().to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();

        let by_full_name = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Cask,
                    name: "gautham-v/tap/claudebar".to_string(),
                },
            )
            .await
            .expect("reconcile by full name");
        assert!(
            by_full_name.present,
            "must match on the full tap-qualified name"
        );

        let by_short_name = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Cask,
                    name: "claudebar".to_string(),
                },
            )
            .await
            .expect("reconcile by short name");
        assert!(
            by_short_name.present,
            "must also match when the request used only the short token"
        );

        let missing = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Cask,
                    name: "does-not-exist".to_string(),
                },
            )
            .await
            .expect("reconcile of a name that does not exist");
        assert!(!missing.present);
    }

    /// This used to `set_var("SUDO_ASKPASS", ...)` and then `remove_var`
    /// it. Cargo runs a binary's tests in threads by default, and the two
    /// other cask plans in this same binary read that variable while this
    /// one was writing it: a genuine data race on the process environment,
    /// the kind that fails once in fifty CI runs and costs someone an
    /// afternoon. The adapter now reads the variable through an injectable
    /// reader, so the test states the value it wants and leaves the
    /// environment alone.
    #[tokio::test]
    async fn test_plan_passes_through_sudo_askpass_for_cask_install() {
        let runner = Arc::new(MockRunner::new());
        let adapter =
            BrewAdapter::new(runner).with_askpass_fn(|| Some("/tmp/fake-askpass.sh".to_string()));
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Cask,
            name: "claudebar".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert!(plan.env.contains(&(
            "SUDO_ASKPASS".to_string(),
            "/tmp/fake-askpass.sh".to_string()
        )));
    }

    /// The other half, which the old env-mutating test could not express
    /// without unsetting a variable the developer running the tests may
    /// legitimately have set: with no `SUDO_ASKPASS` in the environment,
    /// the plan must not invent one. The command preview the user reads
    /// before confirming shows this env, so a phantom entry there is a
    /// lie about what is going to run.
    #[tokio::test]
    async fn test_plan_adds_no_askpass_when_the_variable_is_not_set() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner).with_askpass_fn(|| None);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Cask,
            name: "claudebar".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert!(
            !plan.env.iter().any(|(k, _)| k == "SUDO_ASKPASS"),
            "plan env must not carry an askpass that is not set: {:?}",
            plan.env
        );
    }

    // ---- `brew update` is waited for, never killed ----------------------
    //
    // `brew update` rewrites Homebrew's git checkout, and a SIGKILL partway
    // through can leave `.git/index.lock` behind, after which Homebrew will
    // not update again until someone deletes that file by hand. These run a
    // real process through `RealRunner` -- `MockRunner` does not kill
    // anything when its future is dropped or times out -- so a kill would
    // show up as an update that never finished.

    /// A stand-in `brew` whose `update` takes `update_secs` and leaves a line
    /// in `update-started` and `update-finished` as it begins and ends, so a
    /// test can tell an update that was killed from one that completed.
    fn slow_update_brew(label: &str, update_secs: u32) -> (PathBuf, ManagerInstance) {
        let dir = std::env::temp_dir().join(format!(
            "canager-brew-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let exe = dir.join("brew");
        let script = format!(
            "#!/bin/sh\n\
             case \"$1\" in\n\
             update)\n\
             echo started >> '{dir}/update-started'\n\
             /bin/sleep {update_secs}\n\
             echo finished >> '{dir}/update-finished'\n\
             ;;\n\
             outdated|info) echo '{{\"formulae\":[],\"casks\":[]}}' ;;\n\
             *) exit 1 ;;\n\
             esac\n",
            dir = dir.display(),
        );
        std::fs::write(&exe, script).expect("write fake brew");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake brew");
        let inst = ManagerInstance {
            exe_path: exe,
            prefix: dir.clone(),
            version: Some("7.0.3".to_string()),
            ..crate::testing::manager_instance("brew", &format!("brew:{}", dir.display()))
        };
        (dir, inst)
    }

    fn line_count(path: &Path) -> usize {
        std::fs::read_to_string(path)
            .map(|s| s.lines().count())
            .unwrap_or(0)
    }

    /// Waits for the fake `brew update` to write its `finished` line, up to
    /// a bound far past its own run time. Returns how many it wrote.
    async fn wait_for_update_to_finish(dir: &Path) -> usize {
        let finished = dir.join("update-finished");
        let give_up = Instant::now() + Duration::from_secs(15);
        while line_count(&finished) == 0 && Instant::now() < give_up {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        line_count(&finished)
    }

    #[tokio::test]
    async fn test_a_brew_update_that_outlasts_the_refresh_is_left_to_finish() {
        // The live bug: `brew update` had a two-minute timeout, and on a
        // slow network the runner SIGKILLed it wherever it had got to --
        // in the middle of git. Now the refresh stops *waiting* when its
        // patience runs out, says the list is still being downloaded, and
        // the update runs on to completion. Scaled down: patience 300 ms,
        // an update that takes 2 s.
        let (dir, inst) = slow_update_brew("outlasts", 2);
        let background_change = Arc::new(tokio::sync::Notify::new());
        let adapter = BrewAdapter::new(Arc::new(crate::runner::RealRunner::new()))
            .with_update_patience(Duration::from_millis(300))
            .with_background_change(background_change.clone());

        let started = Instant::now();
        let first = adapter.check_updates(&inst, &CheckOptions::default()).await;
        let waited = started.elapsed();
        assert!(
            matches!(first, Err(AdapterError::IndexUpdating)),
            "an update the refresh stopped waiting for is still running, not \
             failed -- `IndexMayBeStale` would tell the user the download \
             failed and to check their connection; got {first:?}"
        );
        assert!(
            waited < Duration::from_millis(1500),
            "the refresh must stop waiting at its patience, not at the end of \
             the update; took {waited:?}"
        );

        // A second refresh while that update is still running does not
        // wait for it at all -- it used to sit out its own patience on the
        // lock and then call the download failed -- and does not start a
        // second `brew update` alongside the first.
        let started = Instant::now();
        let second = adapter.check_updates(&inst, &CheckOptions::default()).await;
        assert!(
            matches!(second, Err(AdapterError::IndexUpdating)),
            "got {second:?}"
        );
        assert!(
            started.elapsed() < Duration::from_millis(250),
            "a refresh behind a running update must not wait on it; took {:?}",
            started.elapsed()
        );

        // When the update ends, whoever is listening hears about it, so the
        // notice does not sit there until the user happens to act.
        tokio::time::timeout(Duration::from_secs(15), background_change.notified())
            .await
            .expect("the end of an update a refresh reported as running must be announced");
        assert_eq!(
            wait_for_update_to_finish(&dir).await,
            1,
            "the `brew update` the refresh stopped waiting for must run to \
             completion, not be killed"
        );
        assert_eq!(
            line_count(&dir.join("update-started")),
            1,
            "two refreshes must not run two `brew update`s at once"
        );

        // The background update recorded its success, so the refresh that
        // announcement sets off gets the fresh catalogue without running
        // another one.
        let third = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("third check_updates");
        assert!(third.notes.is_empty(), "got {:?}", third.notes);
        assert_eq!(line_count(&dir.join("update-started")), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A slow `brew update` answering `output` after `delay`, and an empty
    /// `brew outdated`.
    fn runner_with_update(delay: Duration, output: CommandOutput) -> Arc<MockRunner> {
        let runner = Arc::new(MockRunner::new());
        runner.respond(vec!["/opt/homebrew/bin/brew", "update"], output);
        runner.delay(vec!["/opt/homebrew/bin/brew", "update"], delay);
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: r#"{"formulae":[],"casks":[]}"#.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner
    }

    fn update_calls(runner: &MockRunner) -> usize {
        runner
            .calls()
            .iter()
            .filter(|c| c.get(1).map(String::as_str) == Some("update"))
            .count()
    }

    #[tokio::test]
    async fn test_a_background_update_that_then_fails_is_reported_once_and_not_retried_by_itself() {
        // The update a refresh called "still downloading" fails after all.
        // The refresh its end sets off must say so -- that is when
        // "couldn't download" becomes true -- and must not start another
        // `brew update` by itself: on a network that fails every attempt,
        // that would be an update loop nobody asked for. The notice's own
        // "Try again" is what starts the next one.
        let runner = runner_with_update(
            Duration::from_millis(400),
            CommandOutput {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "fatal: unable to access GitHub".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let background_change = Arc::new(tokio::sync::Notify::new());
        let adapter = BrewAdapter::new(runner.clone())
            .with_update_patience(Duration::from_millis(100))
            .with_background_change(background_change.clone());
        let inst = test_instance();

        let first = adapter.check_updates(&inst, &CheckOptions::default()).await;
        assert!(
            matches!(first, Err(AdapterError::IndexUpdating)),
            "got {first:?}"
        );
        // While that update runs, neither reader touches the catalogue it
        // is rewriting: `check_updates` gave up before `brew outdated`,
        // and `inventory` declines before `brew info`.
        let inventory = adapter.inventory(&inst).await;
        assert!(
            matches!(inventory, Err(AdapterError::IndexUpdating)),
            "got {inventory:?}"
        );
        assert!(
            !runner.calls().iter().any(|c| matches!(
                c.get(1).map(String::as_str),
                Some("outdated") | Some("info")
            )),
            "the catalogue was read while `brew update` was rewriting it: {:?}",
            runner.calls()
        );

        tokio::time::timeout(Duration::from_secs(5), background_change.notified())
            .await
            .expect("a failed update a refresh reported as running must be announced too");
        let after = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates after");
        assert_eq!(after.notes, vec![InstanceNote::IndexMayBeStale]);
        assert_eq!(
            update_calls(&runner),
            1,
            "the refresh a failure sets off must not start another `brew update`"
        );

        // "Try again": the failure has been reported, so this one tries.
        // Its update is as slow as the first, so it too is still running
        // when the patience runs out.
        let retry = adapter.check_updates(&inst, &CheckOptions::default()).await;
        assert!(
            matches!(retry, Err(AdapterError::IndexUpdating)),
            "got {retry:?}"
        );
        assert_eq!(update_calls(&runner), 2, "Try again must try again");
    }

    #[tokio::test]
    async fn test_a_round_stamped_in_the_failures_own_tick_leaves_the_failure_for_a_later_one() {
        // Finding 3.1 of the final review. An `Instant` is no finer than
        // the clock's tick, so a round that really began just before an
        // announced update failed can carry the very same `Instant` as
        // the failure. It must report the failure and leave it, like any
        // round that began before it: taking it would leave the round the
        // failure's wake-up sets off with no flag and an expired TTL, and
        // that round would start the `brew update` nobody asked for. The
        // instants are set by hand, so the tie is exact every run.
        let runner = runner_with_update(
            Duration::ZERO,
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner.clone());
        let inst = test_instance();
        let failed_at = Instant::now();
        adapter
            .updates
            .lock()
            .unwrap()
            .entry(inst.id.clone())
            .or_default()
            .unreported_failure = Some(failed_at);

        let tied = CheckOptions {
            round_started: Some(failed_at),
            ..CheckOptions::default()
        };
        let tied = adapter
            .check_updates(&inst, &tied)
            .await
            .expect("the round in the failure's tick");
        assert_eq!(tied.notes, vec![InstanceNote::IndexMayBeStale]);

        let later = CheckOptions {
            round_started: Some(failed_at + Duration::from_nanos(1)),
            ..CheckOptions::default()
        };
        let woken = adapter
            .check_updates(&inst, &later)
            .await
            .expect("the round the failure's wake-up sets off");
        assert_eq!(
            woken.notes,
            vec![InstanceNote::IndexMayBeStale],
            "the round in the failure's tick took the failure, so the round after it \
             started a `brew update`: {:?}",
            runner.calls()
        );
        assert_eq!(update_calls(&runner), 0, "{:?}", runner.calls());

        // Taken by that round, so the one after tries again: the tie cost
        // one extra report and the flag did not stick.
        adapter
            .check_updates(&inst, &later)
            .await
            .expect("the refresh after");
        assert_eq!(update_calls(&runner), 1, "the refresh after must try again");
    }

    /// A runner whose `brew update` panics after `delay`; everything else
    /// goes to `inner`.
    struct PanickingUpdateRunner {
        inner: MockRunner,
        delay: Duration,
    }

    #[async_trait::async_trait]
    impl crate::runner::CommandRunner for PanickingUpdateRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            on_line: Option<crate::runner::LineCallback>,
            cancel: CancellationToken,
        ) -> Result<CommandOutput, crate::runner::RunnerError> {
            if spec.args.first().map(String::as_str) == Some("update") {
                tokio::time::sleep(self.delay).await;
                panic!("brew update's runner panicked on purpose");
            }
            self.inner.run(spec, on_line, cancel).await
        }
    }

    #[tokio::test]
    async fn test_a_brew_update_whose_task_panics_still_clears_running_and_wakes_the_loop() {
        // Rereview F5: `running` is cleared only by `UpdateFinish`'s drop,
        // and a panic is the case a drop guard is for. If the guard did
        // not run while unwinding, `running` would stay `true` for the
        // life of the app -- every later refresh told "still downloading"
        // and the catalogue never read again -- and nothing would wake the
        // shell's loop.
        let inner = MockRunner::new();
        inner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: r#"{"formulae":[],"casks":[]}"#.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let runner = Arc::new(PanickingUpdateRunner {
            inner,
            delay: Duration::from_millis(300),
        });
        let background_change = Arc::new(tokio::sync::Notify::new());
        let adapter = BrewAdapter::new(runner)
            .with_update_patience(Duration::from_millis(100))
            .with_background_change(background_change.clone());
        let inst = test_instance();

        let first = adapter.check_updates(&inst, &CheckOptions::default()).await;
        assert!(
            matches!(first, Err(AdapterError::IndexUpdating)),
            "got {first:?}"
        );

        tokio::time::timeout(Duration::from_secs(5), background_change.notified())
            .await
            .expect("an announced update whose task panicked must still wake the loop");
        assert!(
            !adapter.join_running_update(&inst.id),
            "the panicked update must not be left marked running"
        );
        // And the refresh that wake-up sets off reports the failure,
        // rather than "still downloading".
        let after = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates after");
        assert_eq!(after.notes, vec![InstanceNote::IndexMayBeStale]);
    }

    #[tokio::test]
    async fn test_an_update_the_refresh_waited_out_is_not_announced() {
        // The follow-up refresh exists for a notice that would otherwise
        // stay on screen. An update the refresh saw through to the end left
        // no such notice, so its end must not set off a refresh nobody
        // needs.
        let runner = runner_with_update(
            Duration::from_millis(50),
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let background_change = Arc::new(tokio::sync::Notify::new());
        let adapter = BrewAdapter::new(runner)
            .with_update_patience(Duration::from_secs(5))
            .with_background_change(background_change.clone());
        let outcome = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("check_updates");
        assert!(outcome.notes.is_empty(), "got {:?}", outcome.notes);
        assert!(
            tokio::time::timeout(Duration::from_millis(200), background_change.notified())
                .await
                .is_err(),
            "nothing was reported as still running, so nothing needs a follow-up"
        );
    }

    #[tokio::test]
    async fn test_dropping_a_refresh_does_not_kill_its_brew_update() {
        // The latent bug: `Session::refresh` takes no cancellation token,
        // so dropping it is the only way to stop one, and dropping a run
        // future SIGKILLs its command. Nothing drops a refresh today; the
        // day something wraps one in a timeout, `brew update` must still
        // not be killed mid-git.
        let (dir, inst) = slow_update_brew("dropped", 2);
        let adapter = BrewAdapter::new(Arc::new(crate::runner::RealRunner::new()));
        let dropped = tokio::time::timeout(
            Duration::from_millis(300),
            adapter.check_updates(&inst, &CheckOptions::default()),
        )
        .await;
        assert!(
            dropped.is_err(),
            "the refresh should still have been waiting"
        );
        assert_eq!(
            wait_for_update_to_finish(&dir).await,
            1,
            "dropping the refresh must detach its `brew update`, not kill it"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An update that answers `exit 0` after `delay`, plus an `install jq`
    /// that answers at once.
    fn runner_with_slow_update(delay: Duration) -> Arc<MockRunner> {
        let ok = CommandOutput {
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        };
        let runner = Arc::new(MockRunner::new());
        runner.respond(vec!["/opt/homebrew/bin/brew", "update"], ok.clone());
        runner.delay(vec!["/opt/homebrew/bin/brew", "update"], delay);
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                stdout: r#"{"formulae":[],"casks":[]}"#.to_string(),
                ..ok.clone()
            },
        );
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "install", "--formula", "jq"],
            ok,
        );
        runner
    }

    fn install_jq(inst: &ManagerInstance) -> OpRequest {
        OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        }
    }

    #[tokio::test]
    async fn test_execute_waits_for_a_brew_update_the_refresh_left_running() {
        // A `brew update` the refresh stopped waiting for is no longer under
        // the instance's resource lock -- the refresh worker released it
        // when it moved on -- so an install must wait for it by itself
        // rather than run while Homebrew is rewriting its catalogue.
        let runner = runner_with_slow_update(Duration::from_millis(600));
        let adapter =
            BrewAdapter::new(runner.clone()).with_update_patience(Duration::from_millis(100));
        let inst = test_instance();
        let checked = adapter.check_updates(&inst, &CheckOptions::default()).await;
        assert!(
            matches!(checked, Err(AdapterError::IndexUpdating)),
            "got {checked:?}"
        );

        let plan = adapter.plan(&inst, &install_jq(&inst)).await.expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome = adapter
            .execute(&plan, sink.clone(), 1, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert!(
            adapter
                .updates
                .lock()
                .unwrap()
                .get(&inst.id)
                .is_some_and(|r| r.succeeded_at.is_some()),
            "the install ran before the `brew update` it should have waited for had finished"
        );
        assert!(
            sink.snapshot().iter().any(|e| matches!(
                e,
                crate::events::OperationEvent::Note {
                    op_id: 1,
                    note: crate::events::LogNote::WaitingForBrewUpdate { minutes },
                } if *minutes == BrewAdapter::OP_UPDATE_WAIT.as_secs() / 60
            )),
            "a wait with no output would look like a hang, and its minutes must match \
             OP_UPDATE_WAIT (BrewAdapter::new does not override op_update_wait here): {:?}",
            sink.snapshot()
        );
        // The wait is Canager speaking, not Homebrew: it must arrive as a
        // note the front end localises, never as a verbatim English line.
        assert!(
            !sink.snapshot().iter().any(|e| matches!(
                e,
                crate::events::OperationEvent::Log { line, .. } if line.contains("Waiting")
            )),
            "Canager's own remark went out as tool output: {:?}",
            sink.snapshot()
        );
    }

    #[tokio::test]
    async fn test_execute_stops_waiting_for_a_stuck_brew_update_without_running_anything() {
        // A `brew update` stuck on a dead connection used to hold every
        // install, upgrade and uninstall on this Homebrew for up to
        // `UPDATE_BACKSTOP`, half an hour. The wait is bounded on its own
        // now, and ends with a reason the user can read, having run
        // nothing.
        let runner = runner_with_slow_update(Duration::from_secs(10));
        let adapter = BrewAdapter::new(runner.clone())
            .with_update_patience(Duration::from_millis(100))
            .with_op_update_wait(Duration::from_millis(200));
        let inst = test_instance();
        let checked = adapter.check_updates(&inst, &CheckOptions::default()).await;
        assert!(
            matches!(checked, Err(AdapterError::IndexUpdating)),
            "got {checked:?}"
        );

        let plan = adapter.plan(&inst, &install_jq(&inst)).await.expect("plan");
        let started = Instant::now();
        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 1, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(
            outcome,
            // `minutes` is 200ms rounded down to whole minutes, i.e. 0 --
            // proof this comes from the adapter's actual `op_update_wait`
            // (`with_op_update_wait` above), not a hard-coded 10 that
            // would happen to match the default and hide the wiring
            // being broken.
            Outcome::CanagerFailed(Fault::HomebrewStillUpdating { minutes: 0 })
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the wait must end at its own bound, took {:?}",
            started.elapsed()
        );
        assert!(
            !runner
                .calls()
                .iter()
                .any(|c| c.get(1).map(String::as_str) == Some("install")),
            "nothing may be installed alongside a running `brew update`: {:?}",
            runner.calls()
        );
    }

    #[tokio::test]
    async fn test_execute_can_be_cancelled_while_waiting_for_a_brew_update() {
        let runner = runner_with_slow_update(Duration::from_secs(10));
        let adapter =
            BrewAdapter::new(runner.clone()).with_update_patience(Duration::from_millis(100));
        let inst = test_instance();
        let checked = adapter.check_updates(&inst, &CheckOptions::default()).await;
        assert!(
            matches!(checked, Err(AdapterError::IndexUpdating)),
            "got {checked:?}"
        );

        let plan = adapter.plan(&inst, &install_jq(&inst)).await.expect("plan");
        let cancel = CancellationToken::new();
        let canceller = {
            let cancel = cancel.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                cancel.cancel();
            })
        };
        let started = Instant::now();
        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 1, cancel)
            .await
            .expect("execute");
        canceller.await.unwrap();
        assert_eq!(outcome, Outcome::Cancelled);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "cancel must be answered while waiting, took {:?}",
            started.elapsed()
        );
        assert!(
            !runner
                .calls()
                .iter()
                .any(|c| c.get(1).map(String::as_str) == Some("install")),
            "nothing may be installed after the user cancelled: {:?}",
            runner.calls()
        );
    }

    // ---- the uninstall preview does not read a catalogue mid-update ----

    fn uninstall_jq(inst: &ManagerInstance) -> OpRequest {
        OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        }
    }

    /// `runner_with_slow_update`, plus a `brew uses` that names one
    /// dependent after `uses_delay`.
    fn runner_with_update_and_uses(update: Duration, uses_delay: Duration) -> Arc<MockRunner> {
        let runner = runner_with_slow_update(update);
        let uses = vec!["/opt/homebrew/bin/brew", "uses", "--installed", "jq"];
        runner.respond(
            uses.clone(),
            CommandOutput {
                exit_code: Some(0),
                stdout: "python@3.13\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.delay(uses, uses_delay);
        runner
    }

    fn uses_calls(runner: &MockRunner) -> usize {
        runner
            .calls()
            .iter()
            .filter(|c| c.get(1).map(String::as_str) == Some("uses"))
            .count()
    }

    #[tokio::test]
    async fn test_uninstall_preview_during_a_brew_update_refuses_without_running_brew_uses() {
        // The list of what an uninstall would break is what the user
        // confirms against. Read from a catalogue `brew update` is halfway
        // through rewriting, it could parse and still be short. So while
        // an update runs the preview reads nothing and says why
        // (`IndexUpdating`, which the dialog words as "Homebrew is
        // updating, try again shortly").
        let runner = runner_with_update_and_uses(Duration::from_millis(400), Duration::ZERO);
        let adapter =
            BrewAdapter::new(runner.clone()).with_update_patience(Duration::from_millis(50));
        let inst = test_instance();
        let checked = adapter.check_updates(&inst, &CheckOptions::default()).await;
        assert!(
            matches!(checked, Err(AdapterError::IndexUpdating)),
            "setup: the refresh must have left the update running, got {checked:?}"
        );

        let planned = adapter.plan(&inst, &uninstall_jq(&inst)).await;
        assert!(
            matches!(planned, Err(AdapterError::IndexUpdating)),
            "got {planned:?}"
        );
        assert_eq!(
            uses_calls(&runner),
            0,
            "`brew uses` ran against a catalogue being rewritten: {:?}",
            runner.calls()
        );

        // Once the update is over, trying again gets the real list.
        let give_up = Instant::now() + Duration::from_secs(5);
        while adapter.catalogue_stamp(&inst).is_none() && Instant::now() < give_up {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let plan = adapter
            .plan(&inst, &uninstall_jq(&inst))
            .await
            .expect("a preview after the update has ended");
        assert_eq!(plan.affected, vec!["python@3.13".to_string()]);
    }

    #[tokio::test]
    async fn test_uninstall_preview_discards_a_brew_uses_that_a_brew_update_began_during() {
        // The check before `brew uses` finds no update running; a refresh
        // then starts one, and it both begins and ends while `brew uses`
        // is still reading. Sampling `running` before and after would see
        // `false` both times; the answer overlapped an update all the
        // same, so it is not shown.
        let runner =
            runner_with_update_and_uses(Duration::from_millis(50), Duration::from_millis(400));
        let adapter = Arc::new(BrewAdapter::new(runner.clone()));
        let inst = test_instance();

        let preview = {
            let adapter = adapter.clone();
            let inst = inst.clone();
            tokio::spawn(async move { adapter.plan(&inst, &uninstall_jq(&inst)).await })
        };
        let give_up = Instant::now() + Duration::from_secs(5);
        while uses_calls(&runner) == 0 && Instant::now() < give_up {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("the refresh's update finishes well inside its patience");
        assert!(
            adapter.catalogue_stamp(&inst).is_some(),
            "setup: the update must be over before `brew uses` answers"
        );

        let planned = preview.await.expect("preview task panicked");
        assert!(
            matches!(planned, Err(AdapterError::IndexUpdating)),
            "a dependents list read while `brew update` ran was shown: {planned:?}"
        );
        assert_eq!(update_calls(&runner), 1, "setup: {:?}", runner.calls());
    }

    // ---- ... nor one a `brew update` Canager did not start is rewriting ----
    //
    // These take Homebrew's update lock the way Homebrew's own `lock.sh`
    // does on macOS -- `exec 200>` the file, then `lockf -t 0 200` -- in a
    // real bash, so what they prove is about that lock, not a stand-in.

    /// A fresh directory to act as a Homebrew prefix.
    fn scratch_prefix(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "canager-brew-lock-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("var/homebrew/locks")).expect("create lock dir");
        dir
    }

    /// Holds `prefix`'s update lock, as a running `brew update` does, until
    /// dropped.
    #[cfg(target_os = "macos")]
    struct HomebrewUpdateLockHolder {
        child: std::process::Child,
    }

    #[cfg(target_os = "macos")]
    impl HomebrewUpdateLockHolder {
        fn hold(prefix: &Path) -> HomebrewUpdateLockHolder {
            use std::io::BufRead;
            let mut child = std::process::Command::new("/bin/bash")
                .arg("-c")
                .arg(r#"exec 200>"$1"; lockf -t 0 200 || exit 75; echo locked; read -r _"#)
                .arg("bash")
                .arg(prefix.join("var/homebrew/locks/update"))
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .expect("spawn bash");
            let mut line = String::new();
            std::io::BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut line)
                .expect("read from bash");
            assert_eq!(line.trim(), "locked", "setup: bash did not get the lock");
            HomebrewUpdateLockHolder { child }
        }
    }

    #[cfg(target_os = "macos")]
    impl Drop for HomebrewUpdateLockHolder {
        fn drop(&mut self) {
            // Closing its stdin ends `read`, so bash exits and the lock goes
            // with its descriptor.
            drop(self.child.stdin.take());
            let _ = self.child.wait();
        }
    }

    /// Takes `prefix`'s update lock and lets it go at once, as a `brew
    /// update` with nothing to do would; true when the lock was taken.
    #[cfg(target_os = "macos")]
    fn quick_homebrew_update(prefix: &Path) -> bool {
        std::process::Command::new("/bin/bash")
            .arg("-c")
            .arg(r#"exec 200>"$1"; lockf -t 0 200"#)
            .arg("bash")
            .arg(prefix.join("var/homebrew/locks/update"))
            .stderr(std::process::Stdio::null())
            .status()
            .expect("spawn bash")
            .success()
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_update_lock_probe_that_cannot_look_at_the_lock_still_compares_what_it_read() {
        // Finding 1.2 of the final review. When the lock file is there but
        // will not open, or `fcntl` fails on it, the probe cannot say
        // whether the lock is held -- but it has read the directory, and
        // the file's own stamp too when it was `fcntl` that failed. Two
        // such probes must still differ when those did.
        use std::os::unix::fs::PermissionsExt;
        let prefix = scratch_prefix("unopenable");
        let locks = prefix.join("var/homebrew/locks");
        let lock_file = locks.join("update");
        std::fs::write(&lock_file, "").expect("setup: the lock file");
        std::fs::set_permissions(&lock_file, std::fs::Permissions::from_mode(0o000))
            .expect("setup: chmod");
        if std::fs::File::open(&lock_file).is_ok() {
            // Root opens it all the same, so there is nothing to test.
            let _ = std::fs::remove_dir_all(&prefix);
            return;
        }

        let before = probe_homebrew_update_lock(&prefix);
        assert!(
            matches!(before, HomebrewUpdateLock::Unobservable { .. }),
            "got {before:?}"
        );
        let other = locks.join("jq.formula.lock");
        std::fs::write(&other, "").expect("setup: an entry made");
        std::fs::remove_file(&other).expect("setup: and deleted");
        assert_ne!(
            before,
            probe_homebrew_update_lock(&prefix),
            "an entry made and deleted beside a lock file that would not open went unseen"
        );
        let _ = std::fs::remove_dir_all(&prefix);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_update_lock_probe_sees_a_lock_taken_the_way_homebrew_takes_it() {
        let prefix = scratch_prefix("probe");
        assert!(
            matches!(
                probe_homebrew_update_lock(&prefix),
                HomebrewUpdateLock::Free(LockStamp {
                    dir: Some(_),
                    file: None
                })
            ),
            "no file (as after `brew cleanup`) is no update running"
        );
        assert!(
            !prefix.join("var/homebrew/locks/update").exists(),
            "looking must not create the file"
        );

        let holder = HomebrewUpdateLockHolder::hold(&prefix);
        assert_eq!(
            probe_homebrew_update_lock(&prefix),
            HomebrewUpdateLock::Held
        );
        drop(holder);
        let HomebrewUpdateLock::Free(LockStamp {
            file: Some(before), ..
        }) = probe_homebrew_update_lock(&prefix)
        else {
            panic!("the lock is free and its file is there once the holder has exited");
        };

        // Looking must never be what stops a `brew update` typed in
        // Terminal: Homebrew asks for the lock with a zero timeout, so a
        // probe that took it even for an instant would, now and then, make
        // that update fail with "Another `brew update` process is already
        // running". Probe as fast as possible while updates start.
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let prober = {
            let stop = stop.clone();
            let prefix = prefix.clone();
            std::thread::spawn(move || {
                let mut probes = 0u64;
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    probe_homebrew_update_lock(&prefix);
                    probes += 1;
                }
                probes
            })
        };
        let refused = (0..200).filter(|_| !quick_homebrew_update(&prefix)).count();
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let probes = prober.join().unwrap();
        assert!(probes > 1000, "setup: the prober barely ran ({probes})");
        assert_eq!(
            refused, 0,
            "probing refused {refused} of 200 `brew update`s the lock"
        );

        let HomebrewUpdateLock::Free(LockStamp {
            file: Some(after), ..
        }) = probe_homebrew_update_lock(&prefix)
        else {
            panic!("the lock is free again once every update has exited");
        };
        assert_ne!(
            before, after,
            "an update that began and ended between two looks must leave them different"
        );
        let _ = std::fs::remove_dir_all(&prefix);
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_uninstall_preview_refuses_while_homebrew_holds_its_update_lock() {
        // A `brew update` typed in Terminal, or the one Homebrew runs
        // before a Terminal `brew install`, never shows up in
        // `UpdateRecord`; it does hold Homebrew's own lock.
        let prefix = scratch_prefix("held");
        let runner = runner_with_update_and_uses(Duration::ZERO, Duration::ZERO);
        let adapter =
            BrewAdapter::new(runner.clone()).with_update_lock_fn(probe_homebrew_update_lock);
        let inst = ManagerInstance {
            prefix: prefix.clone(),
            ..test_instance()
        };

        let holder = HomebrewUpdateLockHolder::hold(&prefix);
        let planned = adapter.plan(&inst, &uninstall_jq(&inst)).await;
        assert!(
            matches!(planned, Err(AdapterError::IndexUpdating)),
            "a preview was built while Homebrew's own `brew update` ran: {planned:?}"
        );
        assert_eq!(
            uses_calls(&runner),
            0,
            "`brew uses` ran against a catalogue being rewritten: {:?}",
            runner.calls()
        );

        drop(holder);
        let plan = adapter
            .plan(&inst, &uninstall_jq(&inst))
            .await
            .expect("a preview once Homebrew's update has ended");
        assert_eq!(plan.affected, vec!["python@3.13".to_string()]);
        let _ = std::fs::remove_dir_all(&prefix);
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_uninstall_preview_discards_a_brew_uses_that_a_homebrew_update_began_during() {
        // The lock is free when `brew uses` starts and free again when it
        // answers; a `brew update` Canager did not start took it and let it
        // go in between. Looking only at whether it is held would see
        // nothing; the file's mtime has moved.
        let prefix = scratch_prefix("between");
        let runner = runner_with_update_and_uses(Duration::ZERO, Duration::from_millis(400));
        let adapter = Arc::new(
            BrewAdapter::new(runner.clone()).with_update_lock_fn(probe_homebrew_update_lock),
        );
        let inst = ManagerInstance {
            prefix: prefix.clone(),
            ..test_instance()
        };
        assert!(quick_homebrew_update(&prefix), "setup: an earlier update");

        let preview = {
            let adapter = adapter.clone();
            let inst = inst.clone();
            tokio::spawn(async move { adapter.plan(&inst, &uninstall_jq(&inst)).await })
        };
        let give_up = Instant::now() + Duration::from_secs(5);
        while uses_calls(&runner) == 0 && Instant::now() < give_up {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(
            quick_homebrew_update(&prefix),
            "setup: the update in between"
        );
        assert!(
            !preview.is_finished(),
            "setup: the update must be over before `brew uses` answers"
        );

        let planned = preview.await.expect("preview task panicked");
        assert!(
            matches!(planned, Err(AdapterError::IndexUpdating)),
            "a dependents list read while Homebrew's own `brew update` ran was shown: {planned:?}"
        );
        let _ = std::fs::remove_dir_all(&prefix);
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_uninstall_preview_discards_a_brew_uses_that_an_update_and_a_cleanup_ran_during() {
        // Finding 1.1 of the final review. After a `brew cleanup` there is
        // no update lock file: `Cleanup#cleanup_lockfiles` deletes every
        // one nobody holds. A `brew update && brew cleanup` typed in
        // Terminal while `brew uses` runs makes the file, locks it,
        // rewrites the catalogue, lets go, and deletes the file again, so
        // there is no file at either end to have changed. The directory
        // it was made and deleted in has.
        let prefix = scratch_prefix("made-and-deleted");
        let lock_file = prefix.join("var/homebrew/locks/update");
        let runner = runner_with_update_and_uses(Duration::ZERO, Duration::from_millis(400));
        let adapter = Arc::new(
            BrewAdapter::new(runner.clone()).with_update_lock_fn(probe_homebrew_update_lock),
        );
        let inst = ManagerInstance {
            prefix: prefix.clone(),
            ..test_instance()
        };
        assert!(
            !lock_file.exists(),
            "setup: no lock file, as after `brew cleanup`"
        );

        let preview = {
            let adapter = adapter.clone();
            let inst = inst.clone();
            tokio::spawn(async move { adapter.plan(&inst, &uninstall_jq(&inst)).await })
        };
        let give_up = Instant::now() + Duration::from_secs(5);
        while uses_calls(&runner) == 0 && Instant::now() < give_up {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(
            quick_homebrew_update(&prefix),
            "setup: the update in between"
        );
        std::fs::remove_file(&lock_file).expect("setup: the cleanup in between");
        assert!(
            !preview.is_finished(),
            "setup: the update and the cleanup must be over before `brew uses` answers"
        );

        let planned = preview.await.expect("preview task panicked");
        assert!(
            matches!(planned, Err(AdapterError::IndexUpdating)),
            "a dependents list read while a `brew update` made, used and a `brew cleanup` \
             deleted the lock file was shown: {planned:?}"
        );
        let _ = std::fs::remove_dir_all(&prefix);
    }
}
