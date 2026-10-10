mod bottles;
pub(crate) mod brew_env;
mod cask_links;
pub(crate) mod cask_receipt;
#[cfg(test)]
mod keg_link_tests;
pub(crate) mod kegs;
pub(crate) mod links;
pub mod parse;
pub(crate) mod trust;

use crate::adapters::{
    ensure_instance_match, reconcile_from, run_plan, validate_package_name, Adapter, AdapterError,
    AdapterMeta, CheckOptions, CheckOutcome,
};
use crate::dirfd::Stat;
use crate::events::{EventSink, LogNote, OpId, OperationEvent};
use crate::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, CaskStep, Fault, InstalledArtifact, InstanceId,
    InstanceNote, InstanceStatus, ManagerInstance, OpKind, OpRequest, Outcome, Plan, PlanAction,
    Reconciled, ResourceLock, Scope, SearchHit, Unavailable, UninstallScope, UpdateBlocked,
    Warning,
};
use crate::protected::{look, Protected};
use crate::runner::{CommandOutput, CommandRunner, CommandSpec, HostEnv, OutputUse};
use async_trait::async_trait;
use bottles::MacTag;
use brew_env::HomebrewSwitches;
use cask_receipt::{Classified, Recorded};
use kegs::{Kegs, Service};
use links::KegLinks;
use parse::{parse_info_installed, parse_outdated, parse_search, parse_uses, parse_version};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tokio_util::sync::CancellationToken;
use trust::TrustList;

/// The queue key shared with npm for a Homebrew prefix. `identity` tells
/// whether the prefix is one of the discovery prefixes under another
/// spelling (`PREFIX_IDENTITY_FN`, which the adapters hold as a seam).
pub(crate) fn prefix_lock(
    prefix: &Path,
    identity: fn(&Path) -> Option<(u64, u64)>,
) -> ResourceLock {
    let prefixes = BrewAdapter::CANDIDATE_PATHS.map(|exe| BrewAdapter::prefix_for(Path::new(exe)));
    matching_prefix_lock(prefix, &prefixes, identity)
}

/// How `prefix_lock` looks at a folder, as `BrewAdapter::new` and
/// `NpmAdapter::new` set it: the real directories in every build but this
/// crate's unit tests, which never inspect the host's Homebrew -- recorded
/// fixtures name real prefixes -- and supply a reader over their own trees
/// where that is what they test. Integration tests, built without
/// `cfg(test)`, turn it off with the adapters' `test-support` hooks.
#[cfg(not(test))]
pub(crate) const PREFIX_IDENTITY_FN: fn(&Path) -> Option<(u64, u64)> =
    |path| directory_identity(path, &Protected::of_this_process());
#[cfg(test)]
pub(crate) const PREFIX_IDENTITY_FN: fn(&Path) -> Option<(u64, u64)> = |_| None;

fn directory_identity(path: &Path, protected: &Protected) -> Option<(u64, u64)> {
    let (_, stat) = look::target(path, protected).ok()?;
    stat.is_dir().then_some((stat.dev(), stat.ino()))
}

fn matching_prefix_lock(
    prefix: &Path,
    prefixes: &[PathBuf],
    identity: impl Fn(&Path) -> Option<(u64, u64)>,
) -> ResourceLock {
    let key = |path: &Path| {
        ResourceLock(crate::model::instance_id(
            "brew",
            Some(&path.display().to_string()),
        ))
    };
    // Discovery uses these fixed spellings. The first matching directory
    // supplies the one key, even if two discovery prefixes are aliases.
    // Display paths and command arguments keep the user's own spelling.
    let directory = identity(prefix);
    for known in prefixes {
        if crate::protected::same_path(prefix, known)
            || directory.is_some_and(|found| identity(known) == Some(found))
        {
            return key(known);
        }
    }
    key(prefix)
}

pub struct BrewAdapter {
    #[cfg(test)]
    inspect_cask_links: bool,
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
    /// How long after a `brew update` that succeeded refreshes skip the
    /// next one: `UPDATE_TTL` unless a caller sets another
    /// (`with_update_ttl`). Measured on the wall clock (`wall_clock_fn`,
    /// `update_is_fresh`).
    update_ttl: Duration,
    /// How to read the wall clock `update_ttl` is measured on.
    /// `SystemTime::now` outside this crate's unit tests; inside them a
    /// clock that stands still unless a test installs one of its own
    /// (`with_wall_clock_fn`, `DEFAULT_WALL_CLOCK_FN`). The same fn-pointer
    /// seam as `euid_fn`: a Mac asleep moves the wall clock while `Instant`
    /// stands still, and moving this clock alone is how a test shows that
    /// time counting.
    wall_clock_fn: fn() -> SystemTime,
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
    /// gets `brew_is_there` (whether the path leads to anything, looked up
    /// one step at a time and never into a protected place); tests hand
    /// in a layout.
    path_exists_fn: fn(&Path) -> bool,
    /// In this crate's unit tests, a folder of the test's own under which
    /// `detect` puts the discovery prefixes it finds (`with_discovery_root`):
    /// `path_exists_fn` still answers for `CANDIDATE_PATHS` as spelled, and
    /// the instance found at `/opt/homebrew/bin/brew` is the one at
    /// `<root>/opt/homebrew/bin/brew`, so that a refresh a test drives
    /// reads that folder's `bin` and `sbin` (`commands::bin_folders`), and
    /// never the Mac's own Homebrew.
    #[cfg(test)]
    discovery_root: Option<PathBuf>,
    /// How to look at Homebrew's own `brew update` lock under a prefix,
    /// for `catalogue_stamp`. The same fn-pointer seam as
    /// `path_exists_fn`: outside this crate's unit tests it is always
    /// `probe_homebrew_update_lock` (`DEFAULT_UPDATE_LOCK_FN`); inside
    /// them it reads no lock at all unless a test installs one, so that no
    /// test here answers differently because the Mac running it happens to
    /// be in the middle of a `brew update`.
    update_lock_fn: fn(&Path) -> HomebrewUpdateLock,
    /// How to read one variable of Banager's own environment, which every
    /// `brew` command inherits under its plan's own: `HOME`,
    /// `XDG_CONFIG_HOME`, `HOMEBREW_XDG_CONFIG_HOME` and
    /// `HOMEBREW_SYSTEM_ENV_TAKES_PRIORITY`, from which `bin/brew` finds and
    /// orders its `brew.env` files (`brew_env::after_brew_env`). Read per
    /// plan, as `askpass_fn` is. The same fn-pointer seam as
    /// `update_lock_fn`: inside this crate's unit tests the environment is
    /// empty unless a test installs one (`with_env_var_fn`).
    env_var_fn: fn(&str) -> Option<OsString>,
    /// How to read one `brew.env` file: `brew_env::read_brew_env_file`
    /// outside this crate's unit tests; inside them there is none unless a
    /// test installs a reader (`with_brew_env_fn`), so that no test answers
    /// differently for the brew.env files of the Mac running it.
    brew_env_fn: fn(&Path) -> brew_env::EnvFile,
    /// How to read what Homebrew recorded at install about a cask's
    /// uninstall, under a prefix, for the cask uninstall preview:
    /// `cask_receipt::read_recorded` outside this crate's unit tests;
    /// inside them nothing is recorded unless a test installs the reader
    /// (`with_recorded_uninstall_fn`), so that no test answers differently
    /// for the casks installed on the Mac running it.
    recorded_uninstall_fn: fn(&Path, &str) -> Option<Recorded>,
    /// How to read the bundle id of an app on the disk, for the cask
    /// uninstall preview to name the apps a `quit:` step quits
    /// (`quit_app_names`): `cask_receipt::app_bundle_id` outside this
    /// crate's unit tests; inside them no app is there unless a test
    /// installs a reader (`with_app_bundle_id_fn`), so that no test answers
    /// differently for the apps in the `/Applications` of the Mac running
    /// it.
    app_bundle_id_fn: fn(&Path) -> Option<String>,
    /// How to read the version an app on the disk says it is
    /// (`CFBundleShortVersionString`), for the inventory to show where an
    /// app that updates itself has moved past Homebrew's record (R47-3):
    /// `cask_receipt::app_short_version` outside this crate's unit tests,
    /// none inside them unless a test installs a reader
    /// (`with_app_version_fn`), as for `app_bundle_id_fn`.
    app_version_fn: fn(&Path) -> Option<String>,
    /// How the queue key of a plan's prefix looks at folders
    /// (`prefix_lock`): `PREFIX_IDENTITY_FN`.
    prefix_identity_fn: fn(&Path) -> Option<(u64, u64)>,
    /// Homebrew's default `appdir`, where a cask's app recorded by name
    /// alone is looked for, beside `~/Applications`: `/Applications`
    /// always but in the integration tests, which give one of their own
    /// folders (`reading_only_its_prefix`) so that nothing looks at the
    /// apps of the Mac running them.
    applications: PathBuf,
    /// How to read Homebrew's trust list in the user's Homebrew config
    /// folder, for the uninstall preview (`trust::read_trust_list`):
    /// the real file outside this crate's unit tests; inside them an empty
    /// list unless a test installs a reader (`with_trust_list_fn`), so that
    /// no test answers differently for the trust list of the Mac running it.
    trust_list_fn: fn(&Path) -> Option<TrustList>,
    /// How to read which versions of a formula are installed under a
    /// prefix, and whether it is pinned, for the upgrade and uninstall
    /// previews (`kegs::read_kegs`, U9): the real Cellar outside this
    /// crate's unit tests; inside them nothing is read unless a test
    /// installs a reader (`with_kegs_fn`), so that no test answers
    /// differently for the formulae installed on the Mac running it.
    kegs_fn: fn(&Path, &str) -> Option<Kegs>,
    /// How to read the formula folders in a prefix's `Cellar`, which the
    /// inventory compares with what `brew info` listed
    /// (`kegs::read_racks`, `remember_unlisted_racks`): the real Cellar
    /// outside this crate's unit tests; inside them nothing is read unless
    /// a test installs a reader (`with_racks_fn`).
    racks_fn: fn(&Path) -> Option<Vec<String>>,
    /// How to read the bottle tag Homebrew looks for on this Mac, for the
    /// Homebrew at a prefix (`bottles::this_mac`, r18 R46-2): the real
    /// processor and macOS version outside this crate's unit tests; inside
    /// them none unless a test installs one (`with_mac_tag_fn`), so that
    /// no test answers differently on the Mac running it. With none, no
    /// update is said to compile.
    mac_tag_fn: fn(&Path) -> Option<MacTag>,
    /// How to look for a formula's `brew services` file, given the home
    /// folder and the formula's name in the Cellar, for its uninstall
    /// preview (`kegs::read_service`, r18 R46-3): the real
    /// `~/Library/LaunchAgents` and `/Library/LaunchDaemons` outside this
    /// crate's unit tests; inside them none unless a test installs a
    /// reader (`with_service_fn`).
    service_fn: fn(Option<&Path>, &str) -> Option<Service>,
    /// How to read whether a keg-only formula is linked into a prefix, and
    /// what holds its commands' places there, for its update's preview and
    /// around the update itself (`links::read_links`, y1-keg), and for the
    /// preview of the link a source's notice offers and the reading after
    /// it (`OpKind::Link`, `reconcile_link`, y2-npmwhy): the real
    /// prefix outside this crate's unit tests; inside them nothing is read
    /// unless a test installs a reader (`with_links_fn`), so that no test
    /// answers differently for the links on the Mac running it.
    links_fn: fn(&Path, &str) -> Option<KegLinks>,
    /// The keg-only formulae of each instance that `brew link --formula
    /// --force` may link -- by name in the prefix (a tap's `user/tap/name`
    /// is `name`) --
    /// as its last inventory read them (`brew info --installed --json=v2`'s
    /// `keg_only`, `CommandInputs`): an update's preview has only the
    /// request, and only that answer is Homebrew's own. One whose
    /// `keg_only_reason` is macOS's (`provided_by_macos`,
    /// `shadowed_by_macos`) is left out: `brew link` refuses to link it at
    /// Homebrew's default prefix (`cmd/link.rb`). Empty until an inventory
    /// has run, which every refresh does before a row can be updated or a
    /// source's notice can offer a link; an update of a formula not here is
    /// planned as before, and a link of one is refused.
    keg_only: Mutex<HashMap<InstanceId, HashSet<String>>>,
    /// The formula folders in each instance's `Cellar` that hold a version
    /// and that its last inventory's `brew info --installed --json=v2` did
    /// not list (`remember_unlisted_racks`), by name in the Cellar. Empty
    /// until an inventory has run, and when the Cellar could not be read
    /// in full.
    unlisted_racks: Mutex<HashMap<InstanceId, Vec<String>>>,
    /// The formulae of each instance, by name in the Cellar, that no
    /// bottle fits this Mac as its last inventory read them
    /// (`bottles::formulae_built_from_source`), whose update compiles.
    /// Empty until an inventory has run, and with no `mac_tag_fn` answer.
    source_builds: Mutex<HashMap<InstanceId, HashSet<String>>>,
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

/// `BrewAdapter::env_var_fn` and `brew_env_fn` as `BrewAdapter::new` sets
/// them: Banager's real environment and the real files in every build but
/// this crate's unit tests, where both are empty.
#[cfg(not(test))]
const DEFAULT_ENV_VAR_FN: fn(&str) -> Option<OsString> = |name| std::env::var_os(name);
#[cfg(test)]
const DEFAULT_ENV_VAR_FN: fn(&str) -> Option<OsString> = |_| None;
#[cfg(not(test))]
const DEFAULT_BREW_ENV_FN: fn(&Path) -> brew_env::EnvFile = brew_env::read_brew_env_file;
#[cfg(test)]
const DEFAULT_BREW_ENV_FN: fn(&Path) -> brew_env::EnvFile = |_| brew_env::EnvFile::Skipped;

/// `BrewAdapter::recorded_uninstall_fn` as `BrewAdapter::new` sets it: the
/// real Caskroom in every build but this crate's unit tests, where nothing
/// is recorded.
#[cfg(not(test))]
const DEFAULT_RECORDED_UNINSTALL_FN: fn(&Path, &str) -> Option<Recorded> =
    cask_receipt::read_recorded;
#[cfg(test)]
const DEFAULT_RECORDED_UNINSTALL_FN: fn(&Path, &str) -> Option<Recorded> = |_, _| None;

/// `BrewAdapter::app_bundle_id_fn` as `BrewAdapter::new` sets it: the real
/// apps in every build but this crate's unit tests, where there are none.
#[cfg(not(test))]
const DEFAULT_APP_BUNDLE_ID_FN: fn(&Path) -> Option<String> = cask_receipt::app_bundle_id;
#[cfg(test)]
const DEFAULT_APP_BUNDLE_ID_FN: fn(&Path) -> Option<String> = |_| None;

/// `BrewAdapter::app_version_fn` as `BrewAdapter::new` sets it, as
/// `DEFAULT_APP_BUNDLE_ID_FN`.
#[cfg(not(test))]
const DEFAULT_APP_VERSION_FN: fn(&Path) -> Option<String> = cask_receipt::app_short_version;
#[cfg(test)]
const DEFAULT_APP_VERSION_FN: fn(&Path) -> Option<String> = |_| None;

/// `BrewAdapter::trust_list_fn` as `BrewAdapter::new` sets it: the real
/// trust list in every build but this crate's unit tests, where it is
/// empty.
#[cfg(not(test))]
const DEFAULT_TRUST_LIST_FN: fn(&Path) -> Option<TrustList> = trust::read_trust_list;
#[cfg(test)]
const DEFAULT_TRUST_LIST_FN: fn(&Path) -> Option<TrustList> = |_| Some(TrustList::default());

/// `BrewAdapter::kegs_fn` as `BrewAdapter::new` sets it: the real Cellar in
/// every build but this crate's unit tests, where nothing is read.
#[cfg(not(test))]
const DEFAULT_KEGS_FN: fn(&Path, &str) -> Option<Kegs> = kegs::read_kegs;
#[cfg(test)]
const DEFAULT_KEGS_FN: fn(&Path, &str) -> Option<Kegs> = |_, _| None;

/// `BrewAdapter::mac_tag_fn` as `BrewAdapter::new` sets it: this Mac in
/// every build but this crate's unit tests, where there is none.
/// `BrewAdapter::service_fn` as `BrewAdapter::new` sets it: the real
/// folders in every build but this crate's unit tests, where none is read.
#[cfg(not(test))]
const DEFAULT_SERVICE_FN: fn(Option<&Path>, &str) -> Option<Service> = kegs::read_service;
#[cfg(test)]
const DEFAULT_SERVICE_FN: fn(Option<&Path>, &str) -> Option<Service> = |_, _| None;

#[cfg(not(test))]
const DEFAULT_MAC_TAG_FN: fn(&Path) -> Option<MacTag> = bottles::this_mac;
#[cfg(test)]
const DEFAULT_MAC_TAG_FN: fn(&Path) -> Option<MacTag> = |_| None;

/// `BrewAdapter::racks_fn` as `BrewAdapter::new` sets it: the real Cellar
/// in every build but this crate's unit tests, where nothing is read.
#[cfg(not(test))]
const DEFAULT_RACKS_FN: fn(&Path) -> Option<Vec<String>> = kegs::read_racks;
#[cfg(test)]
const DEFAULT_RACKS_FN: fn(&Path) -> Option<Vec<String>> = |_| None;

/// `BrewAdapter::links_fn` as `BrewAdapter::new` sets it: the real prefix
/// in every build but this crate's unit tests, where nothing is read.
#[cfg(not(test))]
const DEFAULT_LINKS_FN: fn(&Path, &str) -> Option<KegLinks> = links::read_links;
#[cfg(test)]
const DEFAULT_LINKS_FN: fn(&Path, &str) -> Option<KegLinks> = |_, _| None;

/// `BrewAdapter::wall_clock_fn` as `BrewAdapter::new` sets it: the real
/// clock in every build but this crate's unit tests, where it stands still
/// at 2026-09-29 00:00 UTC, so that no test answers differently because
/// the clock of the Mac running it was stepped between two of its checks.
/// The tests that are about the clock install one they move.
#[cfg(not(test))]
const DEFAULT_WALL_CLOCK_FN: fn() -> SystemTime = SystemTime::now;
#[cfg(test)]
const DEFAULT_WALL_CLOCK_FN: fn() -> SystemTime =
    || SystemTime::UNIX_EPOCH + Duration::from_secs(1_790_640_000);

impl BrewAdapter {
    /// The environment every `brew` command Banager runs is given, on top
    /// of Banager's own. `HOMEBREW_NO_AUTOREMOVE` keeps Homebrew's
    /// autoremove from uninstalling every formula installed only as a
    /// dependency that nothing needs any more -- packages no preview names
    /// -- after an uninstall, and in the periodic cleanup an install or
    /// upgrade runs; `HOMEBREW_NO_INSTALL_CLEANUP` keeps an install or
    /// upgrade from cleaning up at all: from deleting the older versions and
    /// old downloads of the package it names, every time, and those of
    /// every formula when the periodic cleanup is due. A `brew.env` file
    /// can take either back, which the plan then says (`brew_env_warnings`).
    pub const ENV: [(&'static str, &'static str); 5] = [
        ("HOMEBREW_NO_AUTO_UPDATE", "1"),
        ("HOMEBREW_NO_AUTOREMOVE", "1"),
        ("HOMEBREW_NO_ENV_HINTS", "1"),
        ("HOMEBREW_NO_INSTALL_CLEANUP", "1"),
        ("NO_COLOR", "1"),
    ];

    /// How long a refresh waits for `brew update`. The same two minutes
    /// that used to be `brew update`'s own timeout, so a refresh on a slow
    /// network takes no longer than it did; what changed is that running
    /// out of it no longer kills the update.
    const UPDATE_PATIENCE: Duration = Duration::from_secs(120);

    /// How long after a `brew update` that succeeded refreshes skip the
    /// next one: six hours on the wall clock, time the Mac spends asleep
    /// included (`update_is_fresh`). It used to be six hours of `Instant`,
    /// which on macOS reads `CLOCK_UPTIME_RAW` and stops while the Mac
    /// sleeps, so after a night asleep a morning check skipped `brew
    /// update` and listed updates from a catalogue fifteen hours old.
    const UPDATE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

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
    /// with (`operations.outcome.BanagerFailed.HomebrewStillUpdating`) both
    /// interpolate this number as `{{minutes}}` (via `op_update_wait_minutes`
    /// below) rather than carrying their own copy of it, so there is nothing
    /// to keep in sync by hand when it changes.
    const OP_UPDATE_WAIT: Duration = Duration::from_secs(10 * 60);

    /// How long the `brew cleanup <name>` after an upgrade may run (U9):
    /// it deletes one formula's old versions and downloads, which takes
    /// seconds; ten minutes is for a slow disk, not for a network. Public
    /// for the trust document's test, which finds it in its table.
    pub const CLEANUP_TIMEOUT_SECS: u64 = 10 * 60;

    /// How long a `brew link --formula --force <name>` may run (`link_argv`),
    /// after an upgrade (y1-keg) or on its own (`OpKind::Link`, y2-npmwhy):
    /// it makes one formula's links in the prefix, which takes seconds even
    /// for node's thousands of files. Public for the trust document's test,
    /// which finds it in its table.
    pub const LINK_TIMEOUT_SECS: u64 = 5 * 60;

    /// How long the update of a formula no bottle fits this Mac may take
    /// (r18 R46-2): it compiles, and `llvm`, `gcc`, `rust` or `node` on an
    /// older Intel Mac run well past the half hour every other update is
    /// given. Cancel stops it at any time. Public for the trust document's
    /// test, which finds it in its table.
    pub const SOURCE_BUILD_TIMEOUT_SECS: u64 = 6 * 60 * 60;

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
            update_ttl: Self::UPDATE_TTL,
            wall_clock_fn: DEFAULT_WALL_CLOCK_FN,
            update_patience: Self::UPDATE_PATIENCE,
            op_update_wait: Self::OP_UPDATE_WAIT,
            euid_fn: || unsafe { libc::geteuid() },
            askpass_fn: || std::env::var("SUDO_ASKPASS").ok(),
            path_exists_fn: brew_is_there,
            #[cfg(test)]
            discovery_root: None,
            update_lock_fn: DEFAULT_UPDATE_LOCK_FN,
            env_var_fn: DEFAULT_ENV_VAR_FN,
            brew_env_fn: DEFAULT_BREW_ENV_FN,
            recorded_uninstall_fn: DEFAULT_RECORDED_UNINSTALL_FN,
            #[cfg(test)]
            inspect_cask_links: false,
            app_bundle_id_fn: DEFAULT_APP_BUNDLE_ID_FN,
            app_version_fn: DEFAULT_APP_VERSION_FN,
            applications: PathBuf::from("/Applications"),
            prefix_identity_fn: PREFIX_IDENTITY_FN,
            trust_list_fn: DEFAULT_TRUST_LIST_FN,
            kegs_fn: DEFAULT_KEGS_FN,
            racks_fn: DEFAULT_RACKS_FN,
            mac_tag_fn: DEFAULT_MAC_TAG_FN,
            service_fn: DEFAULT_SERVICE_FN,
            links_fn: DEFAULT_LINKS_FN,
            keg_only: Mutex::new(HashMap::new()),
            unlisted_racks: Mutex::new(HashMap::new()),
            source_builds: Mutex::new(HashMap::new()),
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

    /// Test-only hook to install a wall clock the test moves (see
    /// `wall_clock_fn`).
    #[cfg(test)]
    fn with_wall_clock_fn(mut self, wall_clock_fn: fn() -> SystemTime) -> BrewAdapter {
        self.wall_clock_fn = wall_clock_fn;
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

    /// Test-only hook to find the discovery prefixes under `root` (see
    /// `discovery_root`). `pub(crate)` for `session::refresh`'s tests,
    /// which drive a whole refresh, bin folders and all.
    #[cfg(test)]
    pub(crate) fn with_discovery_root(mut self, root: &Path) -> BrewAdapter {
        self.discovery_root = Some(root.to_path_buf());
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

    /// Test-only hook to describe Banager's environment as `bin/brew` would
    /// inherit it (see `env_var_fn`).
    #[cfg(test)]
    fn with_env_var_fn(mut self, env_var_fn: fn(&str) -> Option<OsString>) -> BrewAdapter {
        self.env_var_fn = env_var_fn;
        self
    }

    /// Test-only hook to put `brew.env` files on the disk the plan reads
    /// (see `brew_env_fn`).
    #[cfg(test)]
    fn with_brew_env_fn(mut self, brew_env_fn: fn(&Path) -> brew_env::EnvFile) -> BrewAdapter {
        self.brew_env_fn = brew_env_fn;
        self
    }

    /// Test-only hook to read casks' recorded uninstalls (see
    /// `recorded_uninstall_fn`) -- the real reader, over a Caskroom a test
    /// made under its own prefix.
    #[cfg(test)]
    fn with_recorded_uninstall_fn(
        mut self,
        recorded_uninstall_fn: fn(&Path, &str) -> Option<Recorded>,
    ) -> BrewAdapter {
        self.recorded_uninstall_fn = recorded_uninstall_fn;
        self
    }

    /// Test-only hook to give the preview a trust list (see
    /// `trust_list_fn`).
    #[cfg(test)]
    fn with_trust_list_fn(mut self, trust_list_fn: fn(&Path) -> Option<TrustList>) -> BrewAdapter {
        self.trust_list_fn = trust_list_fn;
        self
    }

    /// Test-only hook to put a formula's versions on the disk the
    /// previews read (see `kegs_fn`).
    #[cfg(test)]
    fn with_kegs_fn(mut self, kegs_fn: fn(&Path, &str) -> Option<Kegs>) -> BrewAdapter {
        self.kegs_fn = kegs_fn;
        self
    }

    /// Test-only hook to put formula folders in the Cellar the inventory
    /// reads (see `racks_fn`).
    #[cfg(test)]
    fn with_racks_fn(mut self, racks_fn: fn(&Path) -> Option<Vec<String>>) -> BrewAdapter {
        self.racks_fn = racks_fn;
        self
    }

    /// Test-only hook to put a formula's `brew services` file where its
    /// uninstall preview looks (see `service_fn`).
    #[cfg(test)]
    fn with_service_fn(
        mut self,
        service_fn: fn(Option<&Path>, &str) -> Option<Service>,
    ) -> BrewAdapter {
        self.service_fn = service_fn;
        self
    }

    /// Test-only hook for the bottle tag of the Mac the inventory and the
    /// update's preview judge for (see `mac_tag_fn`).
    #[cfg(test)]
    fn with_mac_tag_fn(mut self, mac_tag_fn: fn(&Path) -> Option<MacTag>) -> BrewAdapter {
        self.mac_tag_fn = mac_tag_fn;
        self
    }

    /// Test-only hook to put a prefix's links on the disk the update's
    /// preview and the update read (see `links_fn`).
    #[cfg(test)]
    fn with_links_fn(mut self, links_fn: fn(&Path, &str) -> Option<KegLinks>) -> BrewAdapter {
        self.links_fn = links_fn;
        self
    }

    /// Test-only hook for what an inventory of `instance_id` would have
    /// said is keg-only and linkable (see `keg_only`).
    #[cfg(test)]
    fn with_keg_only(self, instance_id: &str, names: &[&str]) -> BrewAdapter {
        self.keg_only.lock().expect("keg-only lock").insert(
            instance_id.to_string(),
            names.iter().map(|name| name.to_string()).collect(),
        );
        self
    }

    /// Test-only hook to put apps on the disk the preview reads (see
    /// `app_bundle_id_fn`).
    #[cfg(test)]
    fn with_app_bundle_id_fn(
        mut self,
        app_bundle_id_fn: fn(&Path) -> Option<String>,
    ) -> BrewAdapter {
        self.app_bundle_id_fn = app_bundle_id_fn;
        self
    }

    /// Test-only hook to put apps that say their version on the disk the
    /// inventory reads (see `app_version_fn`).
    #[cfg(test)]
    fn with_app_version_fn(mut self, app_version_fn: fn(&Path) -> Option<String>) -> BrewAdapter {
        self.app_version_fn = app_version_fn;
        self
    }

    /// Test support (the `test-support` feature, for the integration tests
    /// and the shell's, which are built without `cfg(test)` and so get every
    /// real reader): an adapter that reads nothing of the Mac running the
    /// test -- no variable of its environment, no `brew.env` file, no trust
    /// list, no Caskroom, Cellar, links or update lock under any prefix, no
    /// app, no discovery prefix -- as this crate's unit tests have it. For a
    /// test whose instance names a real prefix such as `/opt/homebrew`.
    #[cfg(feature = "test-support")]
    pub fn reading_nothing_of_this_mac(mut self) -> BrewAdapter {
        self.askpass_fn = || None;
        self.update_lock_fn = |_| {
            HomebrewUpdateLock::Free(LockStamp {
                dir: None,
                file: None,
            })
        };
        self.env_var_fn = |_| None;
        self.brew_env_fn = |_| brew_env::EnvFile::Skipped;
        self.recorded_uninstall_fn = |_, _| None;
        self.app_bundle_id_fn = |_| None;
        self.app_version_fn = |_| None;
        self.trust_list_fn = |_| Some(TrustList::default());
        self.kegs_fn = |_, _| None;
        self.racks_fn = |_| None;
        self.mac_tag_fn = |_| None;
        self.service_fn = |_, _| None;
        self.links_fn = |_, _| None;
        self.prefix_identity_fn = |_| None;
        self
    }

    /// Test support (`test-support`), for a test whose instance's prefix is
    /// a folder of its own: what is under that prefix is read as Banager
    /// reads it -- its Caskroom, Cellar, links, update lock and
    /// `etc/homebrew/brew.env` -- and nothing of the Mac running the test.
    /// Banager's environment is empty (no `HOME`, so no user `brew.env`,
    /// trust list or `~/Applications`), the system's
    /// `/etc/homebrew/brew.env` is not there, apps are looked for in
    /// `applications` instead of `/Applications`, and no discovery prefix
    /// is looked at.
    #[cfg(feature = "test-support")]
    pub fn reading_only_its_prefix(mut self, applications: &Path) -> BrewAdapter {
        self.askpass_fn = || None;
        self.env_var_fn = |_| None;
        self.brew_env_fn = |path| {
            if path == Path::new(brew_env::SYSTEM_FILE) {
                brew_env::EnvFile::Skipped
            } else {
                brew_env::read_brew_env_file(path)
            }
        };
        self.trust_list_fn = |_| Some(TrustList::default());
        self.mac_tag_fn = |_| None;
        self.service_fn = |_, _| None;
        self.applications = applications.to_path_buf();
        self.prefix_identity_fn = |_| None;
        self
    }

    /// Test support (`test-support`): what this adapter's queue key makes
    /// of the folder at `path` (`prefix_lock`), so that an integration
    /// test can hold `new` to the real reader (`PREFIX_IDENTITY_FN`) on
    /// folders of its own, never a discovery prefix.
    #[cfg(feature = "test-support")]
    pub fn prefix_identity(&self, path: &Path) -> Option<(u64, u64)> {
        (self.prefix_identity_fn)(path)
    }

    /// A recorded link that cannot be confirmed as this cask's. Checked
    /// at preview and again immediately before running its uninstall.
    fn cask_link_conflict(&self, prefix: &Path, req: &OpRequest) -> Option<PathBuf> {
        #[cfg(test)]
        if !self.inspect_cask_links {
            return None;
        }
        if req.kind != OpKind::Uninstall || req.artifact_kind != ArtifactKind::Cask {
            return None;
        }
        // Nothing recorded names a link -- a cask installed before Homebrew
        // recorded its uninstall, one Banager cannot read -- so none is held
        // against it, as before links were checked (`docs/what-we-run.md`).
        let recorded = (self.recorded_uninstall_fn)(prefix, &req.name)?;
        let home = (self.env_var_fn)("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        cask_links::conflict(prefix, &req.name, &recorded, &home, &self.applications)
    }

    /// What an uninstall of `req` says under the tool
    /// (`Warning::UninstallScope`), and, for a cask whose recorded uninstall
    /// takes extra steps, one `Warning::CaskUninstallStep` per kind.
    /// `autoremoves` is whether the `brew.env` files take Homebrew's
    /// autoremove back (`brew_env_warnings`): a formula's sentence says
    /// "only", and that of a cask with steps beside what Homebrew placed
    /// "nothing else is deleted", when they do not -- unless a step runs a
    /// program or code whose deletions Banager cannot see
    /// (`cask_receipt::runs_unseen`), when a cask's sentence says so and
    /// nothing of what stays. A cask's comes from what Homebrew recorded
    /// when it installed the cask (`cask_receipt`), the home folder read
    /// from Banager's environment, as Homebrew's is (`env_var_fn`).
    fn uninstall_scope(
        &self,
        prefix: &Path,
        req: &OpRequest,
        switches: &HomebrewSwitches,
        trust: Option<&TrustList>,
    ) -> (Warning, Vec<Warning>) {
        let scope = |what| Warning::UninstallScope { what };
        if req.artifact_kind != ArtifactKind::Cask {
            let what = if switches.no_autoremove {
                UninstallScope::HomebrewFormulaOnly
            } else {
                UninstallScope::HomebrewFormula
            };
            return (scope(what), Vec::new());
        }
        let read = self.cask_steps(prefix, req, switches, trust);
        let lines = self.step_warnings(read.steps, read.recorded.as_ref(), read.home.as_deref());
        (scope(read.what), lines)
    }

    /// What a cask update of `req` says before its other notes (R47-1,
    /// r18): `brew upgrade --cask` first runs the uninstall the installed
    /// version recorded (`cask/installer.rb:666-667`, `:719-746` in
    /// Homebrew 7.0.9), each directive but `signal` -- unless the record's
    /// `on_upgrade` names it -- and `rmdir` (`cask/artifact/uninstall.rb:
    /// 25-53`), quitting each running app its `quit:` names and opening
    /// again those it quit (`abstract_uninstall.rb:91-127`,
    /// `cask/upgrade.rb:342-366`). So: `Warning::CaskUpdateRunsOldSteps`,
    /// then the lines an uninstall says of those steps (`uninstall_scope`).
    /// Nothing for a record with no such step, one Banager does not read,
    /// or one from a tap Homebrew may not trust, whose steps it does not
    /// run (`cask_steps`).
    fn update_steps(
        &self,
        prefix: &Path,
        req: &OpRequest,
        switches: &HomebrewSwitches,
        trust: Option<&TrustList>,
    ) -> Vec<Warning> {
        if req.artifact_kind != ArtifactKind::Cask {
            return Vec::new();
        }
        let read = self.cask_steps(prefix, req, switches, trust);
        if read.untrusted {
            return Vec::new();
        }
        let signals = read
            .recorded
            .as_ref()
            .is_some_and(cask_receipt::signals_on_upgrade);
        let steps: Vec<_> = read
            .steps
            .into_iter()
            .filter(|(step, _, _)| signals || *step != CaskStep::SignalsApps)
            .collect();
        if steps.is_empty() {
            return Vec::new();
        }
        let reopens = steps
            .iter()
            .any(|(step, _, _)| *step == CaskStep::QuitsApps);
        std::iter::once(Warning::CaskUpdateRunsOldSteps { reopens })
            .chain(self.step_warnings(steps, read.recorded.as_ref(), read.home.as_deref()))
            .collect()
    }

    /// A cask's recorded uninstall, as `uninstall_scope` and `update_steps`
    /// read it: the sentence an uninstall says of it, the steps it takes,
    /// the record, the home folder read from Banager's environment, as
    /// Homebrew's is (`env_var_fn`), and whether it is a Ruby record from
    /// a tap Homebrew may not trust, where trust is required.
    fn cask_steps(
        &self,
        prefix: &Path,
        req: &OpRequest,
        switches: &HomebrewSwitches,
        trust: Option<&TrustList>,
    ) -> CaskSteps {
        let autoremoves = !switches.no_autoremove;
        let home = (self.env_var_fn)("HOME")
            .filter(|home| !home.is_empty())
            .map(PathBuf::from);
        let recorded = (self.recorded_uninstall_fn)(prefix, &req.name);
        let classified = match &recorded {
            Some(recorded) => cask_receipt::classify(recorded, home.as_deref()),
            None => Classified::Unknown,
        };
        // The tap Homebrew installed it from, as Homebrew takes it
        // (`tab.tap || @cask.tap`, `cask/installer.rb:1007-1009`): the
        // receipt's, else the one in its full name.
        let token = req.name.rsplit('/').next().unwrap_or(&req.name);
        let tap = recorded
            .as_ref()
            .and_then(|recorded| recorded.tap.clone())
            .or_else(|| trust::split_full_name(&req.name).map(|(tap, _)| tap.to_string()));
        let third_party = tap.as_deref().is_some_and(|tap| !trust::official(tap));
        let ruby = recorded.as_ref().is_some_and(|recorded| recorded.ruby);
        // A Ruby record from a tap Banager cannot see Homebrew trusts, where
        // trust is required: Homebrew loads none of the Ruby and runs no
        // step (`cask/installer.rb:1010-1043`).
        let maybe_untrusted = ruby
            && switches.require_tap_trust
            && third_party
            && !tap
                .as_deref()
                .is_some_and(|tap| trust.is_some_and(|trust| trust.trusts_cask(tap, token)));
        let (what, steps) = match classified {
            Classified::Unknown => (UninstallScope::HomebrewCask, Vec::new()),
            // Loaded as Ruby, which Homebrew may not manage, and then runs
            // the cask's current definition; the record itself has no step.
            Classified::Plain(steps) if ruby && !maybe_untrusted => {
                (UninstallScope::HomebrewCaskPlainRuby, steps)
            }
            // Untrusted, Homebrew skips the `uninstall` stanza, its quits
            // and signals with it, and removes only what it placed.
            Classified::Plain(_) if maybe_untrusted => {
                (UninstallScope::HomebrewCaskPlainThirdParty, Vec::new())
            }
            Classified::Plain(steps) if third_party => {
                (UninstallScope::HomebrewCaskPlainThirdParty, steps)
            }
            Classified::Plain(steps) => (UninstallScope::HomebrewCaskPlain, steps),
            Classified::Steps(steps) if maybe_untrusted => {
                (UninstallScope::HomebrewCaskStepsIfTrusted, steps)
            }
            Classified::Steps(steps) if ruby => (UninstallScope::HomebrewCaskRuby, steps),
            Classified::OnlySteps(steps) if maybe_untrusted => {
                (UninstallScope::HomebrewCaskStepsOnlyIfTrusted, steps)
            }
            Classified::OnlySteps(steps) if ruby => {
                (UninstallScope::HomebrewCaskStepsOnlyRuby, steps)
            }
            // A program or code Banager cannot see into: no sentence says
            // what stays, with the autoremove on or off.
            Classified::Steps(steps) if cask_receipt::runs_unseen(&steps) => {
                (UninstallScope::HomebrewCaskStepsUnseen, steps)
            }
            Classified::Steps(steps) if autoremoves => {
                (UninstallScope::HomebrewCaskStepsAutoremoves, steps)
            }
            Classified::Steps(steps) => (UninstallScope::HomebrewCaskSteps, steps),
            Classified::OnlySteps(steps) if cask_receipt::runs_unseen(&steps) => {
                (UninstallScope::HomebrewCaskStepsOnlyUnseen, steps)
            }
            Classified::OnlySteps(steps) => (UninstallScope::HomebrewCaskStepsOnly, steps),
        };
        CaskSteps {
            what,
            steps,
            recorded,
            home,
            untrusted: maybe_untrusted,
        }
    }

    /// One `Warning::CaskUninstallStep` per step of `recorded`, the apps
    /// its `quit:` quits named where they were found
    /// (`quit_app_names`), the rest counted.
    fn step_warnings(
        &self,
        steps: Vec<cask_receipt::StepLine>,
        recorded: Option<&Recorded>,
        home: Option<&Path>,
    ) -> Vec<Warning> {
        // Looked for only when there is an app to quit and a line to say it.
        let apps = match recorded {
            Some(recorded)
                if steps
                    .iter()
                    .any(|(step, _, _)| *step == CaskStep::QuitsApps) =>
            {
                self.quit_app_names(recorded, home)
            }
            _ => Vec::new(),
        };
        steps
            .into_iter()
            .flat_map(|(step, only_if, items)| {
                if step != CaskStep::QuitsApps {
                    return vec![Warning::CaskUninstallStep {
                        step,
                        items,
                        only_if,
                    }];
                }
                // The apps it quits that were found, by name, and the
                // ids of the rest, which their line counts.
                let mut named: Vec<String> = Vec::new();
                let mut unfound: Vec<String> = Vec::new();
                for id in items {
                    match apps.iter().find(|(app_id, _)| *app_id == id) {
                        Some((_, name)) if !named.contains(name) => named.push(name.clone()),
                        Some(_) => {}
                        None => unfound.push(id),
                    }
                }
                [
                    (CaskStep::QuitsNamedApps, named),
                    (CaskStep::QuitsApps, unfound),
                ]
                .into_iter()
                .filter(|(_, items)| !items.is_empty())
                .map(|(step, items)| Warning::CaskUninstallStep {
                    step,
                    items,
                    only_if: None,
                })
                .collect()
            })
            .collect()
    }

    /// `Warning::HomebrewForgetsTrust` for an uninstall of `req`, when
    /// `trust` -- Homebrew's trust list -- holds an entry for it alone that
    /// `brew uninstall` will delete (`TrustList::uninstall_forgets`). The
    /// name is the one Homebrew looks up: a cask's full name, which has its
    /// tap in it unless the cask is Homebrew's own (`item.full_name`,
    /// `cmd/uninstall.rb:59`), and a formula's tap and name
    /// (`"#{keg.tab.tap.name}/#{keg.name}"`, `:63`, `:68`) -- `homebrew/core`
    /// for a formula whose full name has no tap in it.
    fn forgets_trust(trust: &TrustList, req: &OpRequest) -> Option<Warning> {
        let (kind, name) = match req.artifact_kind {
            ArtifactKind::Cask => (trust::Kind::Cask, req.name.clone()),
            _ if req.name.contains('/') => (trust::Kind::Formula, req.name.clone()),
            _ => (trust::Kind::Formula, format!("homebrew/core/{}", req.name)),
        };
        trust
            .uninstall_forgets(kind, &name)
            .then_some(Warning::HomebrewForgetsTrust { name })
    }

    /// The apps a cask's `quit:` and `signal:` steps can be said to quit by
    /// name, as (bundle id, name) pairs: each app its record puts down
    /// (`cask_receipt::app_targets`) that is on the disk where Homebrew
    /// puts it -- at its target when that is absolute or under `~/`, else
    /// in `/Applications`, Homebrew's default `appdir`, or
    /// `~/Applications` -- with the bundle id its `Info.plist` gives
    /// (`app_bundle_id_fn`), named as Finder shows its bundle, without
    /// `.app`: "Visual Studio Code". An app kept in an `appdir` of its own
    /// (`--appdir` in `HOMEBREW_CASK_OPTS`) is not found here, and a step
    /// that quits it is said by count and bundle id instead: a name is
    /// said only where the id that goes with it was read.
    fn quit_app_names(&self, recorded: &Recorded, home: Option<&Path>) -> Vec<(String, String)> {
        let mut apps = Vec::new();
        for target in cask_receipt::app_targets(recorded) {
            let candidates: Vec<PathBuf> = if let Some(rest) = target.strip_prefix("~/") {
                home.map(|home| home.join(rest)).into_iter().collect()
            } else if Path::new(&target).is_absolute() {
                vec![PathBuf::from(&target)]
            } else {
                std::iter::once(self.applications.join(&target))
                    .chain(home.map(|home| home.join("Applications").join(&target)))
                    .collect()
            };
            let found = candidates.iter().find_map(|path| {
                let id = (self.app_bundle_id_fn)(path)?;
                let name = path.file_stem()?.to_str()?.to_string();
                Some((id, name))
            });
            apps.extend(found);
        }
        apps
    }

    /// What the `brew.env` files make Homebrew do beyond a plan's command,
    /// for a plan of `kind` on `inst` whose environment is `env`, in the
    /// order it is said -- nothing when Banager's variables hold
    /// (`brew_env::after_brew_env`). An uninstall autoremoves unless
    /// `HOMEBREW_NO_AUTOREMOVE` holds. Unless `HOMEBREW_NO_INSTALL_CLEANUP`
    /// holds, an install or upgrade deletes the older versions and old
    /// downloads of the package it names, every time, and runs the
    /// periodic cleanup when one is due, which deletes those of every
    /// formula and autoremoves too unless `HOMEBREW_NO_AUTOREMOVE` holds.
    /// Then, when `HOMEBREW_NO_CLEANUP_FORMULAE` names a formula, what it
    /// leaves out of those (`Warning::HomebrewNoCleanupFormulae`).
    fn brew_env_warnings(
        &self,
        inst: &ManagerInstance,
        kind: OpKind,
        env: &[(String, String)],
    ) -> Vec<Warning> {
        Self::switch_warnings(&self.homebrew_switches(inst, env), kind)
    }

    /// The versions of the formula `req` names that a `brew cleanup` of it
    /// deletes once its upgrade has succeeded -- every version installed
    /// now, oldest first, the one the upgrade replaces among them -- or
    /// `None` when Banager runs no such cleanup (the author's decision U9,
    /// r6). Homebrew itself deletes them after every upgrade unless
    /// `HOMEBREW_NO_INSTALL_CLEANUP` is set (`Cleanup.install_clean!`,
    /// `cleanup.rb:361-389`), which Banager sets to keep its periodic
    /// clean-up of every formula from running (`ENV`); this gives that one
    /// formula back what Homebrew would have done for it, and only where
    /// the person has said nothing against it:
    ///
    /// - not when Homebrew cleans up by itself, or may: a `brew.env` that
    ///   takes `HOMEBREW_NO_INSTALL_CLEANUP` back, or one Banager cannot
    ///   read (the plan's own lines say what Homebrew does then);
    /// - not when the person turned that cleanup off: their own
    ///   `HOMEBREW_NO_INSTALL_CLEANUP`, in a `brew.env` or in Banager's
    ///   environment, which is what Homebrew would make of the switch
    ///   without Banager's `1`;
    /// - not for a formula `HOMEBREW_NO_CLEANUP_FORMULAE` names, by the
    ///   name Homebrew checks (`Cleanup.skip_clean_formula?`,
    ///   `cleanup.rb:409-415`) -- and an alias it names Banager cannot see
    ///   is refused by Homebrew's own `brew cleanup` (`cleanup.rb:511-514`);
    /// - not for a pinned formula, or one whose pin record cannot be looked
    ///   at: a pin says to keep a version;
    /// - not when the Cellar could not be read, so there are no versions
    ///   to name (`kegs_fn`).
    ///
    /// The same answer is asked for again right before the cleanup runs
    /// (`cleanup_allowed`, from `execute`).
    fn cleanup_after_upgrade(
        &self,
        inst: &ManagerInstance,
        req: &OpRequest,
        env: &[(String, String)],
    ) -> Option<Vec<String>> {
        if req.kind != OpKind::Upgrade || req.artifact_kind != ArtifactKind::Formula {
            return None;
        }
        let kegs = self.cleanup_allowed(&inst.prefix, &req.name, env)?;
        (!kegs.versions.is_empty()).then_some(kegs.versions)
    }

    /// Whether the person's settings let Banager's `brew cleanup` of the
    /// formula `name` under `prefix` run, with a plan's environment `env`
    /// (`cleanup_after_upgrade` says each condition): its kegs as read
    /// now when they do, `None` when they do not or cannot be told. Asked
    /// at the preview, and again right before the cleanup runs
    /// (review F4, r6): a `brew.env`, a pin or a Cellar can change while
    /// the confirmation is open or the update runs, and `brew cleanup` with
    /// a name ignores `HOMEBREW_NO_INSTALL_CLEANUP` (`cleanup.rb:497-519`)
    /// -- so setting the switch for it would not be enough -- and, for a
    /// pinned formula, skips only the pinned version itself: the formula's
    /// other old versions it can still delete
    /// (`Formula#eligible_kegs_for_cleanup`, `formula.rb:3760-3793`), which
    /// is why no cleanup runs for a pinned formula.
    fn cleanup_allowed(&self, prefix: &Path, name: &str, env: &[(String, String)]) -> Option<Kegs> {
        let switches = self.switches_at(prefix, env);
        if !switches.no_install_cleanup || switches.install_cleanup_unknown {
            return None;
        }
        let theirs: Vec<(String, String)> = env
            .iter()
            .filter(|(variable, _)| variable != brew_env::NO_INSTALL_CLEANUP)
            .cloned()
            .collect();
        let own = self.switches_at(prefix, &theirs);
        if own.no_install_cleanup || own.install_cleanup_unknown {
            return None;
        }
        let short = name.rsplit('/').next().unwrap_or(name);
        if switches
            .no_cleanup_formulae
            .iter()
            .any(|named| named == short)
        {
            return None;
        }
        let kegs = (self.kegs_fn)(prefix, name)?;
        (!kegs.pinned).then_some(kegs)
    }

    /// Whether the last inventory of `instance_id` listed the formula
    /// `name` as keg-only and linkable (`keg_only`).
    fn is_keg_only(&self, instance_id: &str, name: &str) -> bool {
        let short = name.rsplit('/').next().unwrap_or(name);
        self.keg_only
            .lock()
            .map(|known| {
                known
                    .get(instance_id)
                    .is_some_and(|names| names.contains(short))
            })
            .unwrap_or(false)
    }

    /// Keeps which of `artifacts`, an inventory of `instance_id`, are
    /// keg-only formulae `brew link` may link (`keg_only`).
    fn remember_keg_only(&self, instance_id: &str, artifacts: &[InstalledArtifact]) {
        let names: HashSet<String> = artifacts
            .iter()
            .filter(|artifact| {
                artifact.key.kind == ArtifactKind::Formula
                    && artifact.facts.command_inputs.keg_only
                    && !artifact.facts.command_inputs.keg_only_by_macos
            })
            .map(|artifact| {
                let name = &artifact.key.name;
                name.rsplit('/').next().unwrap_or(name).to_string()
            })
            .collect();
        if let Ok(mut known) = self.keg_only.lock() {
            known.insert(instance_id.to_string(), names);
        }
    }

    /// Keeps which formula folders in `inst`'s Cellar hold a version and
    /// are missing from `artifacts`, its inventory (r18 R46-1). Homebrew 7
    /// lists an installed formula only if it can load it from its tap, and
    /// silently drops one it cannot (`Formula.installed`,
    /// `formula.rb:2784-2790`) -- one from a tap it does not trust above
    /// all (`Trust.require_trusted_formula!`). `brew outdated` and `brew
    /// uses --installed` drop it the same way, so its updates are not
    /// checked (`InstanceNote::FormulaeNotListed`), and no uninstall
    /// preview can rule out that it needs the formula being uninstalled
    /// (`Warning::DependentsUnknown`). Only names are read: the Cellar's,
    /// and each missing one's versions (`kegs_fn`). A Cellar that cannot be
    /// read in full keeps nothing, so nothing is claimed.
    fn remember_unlisted_racks(&self, inst: &ManagerInstance, artifacts: &[InstalledArtifact]) {
        let listed: HashSet<&str> = artifacts
            .iter()
            .filter(|artifact| artifact.key.kind == ArtifactKind::Formula)
            .map(|artifact| {
                let name = artifact.key.name.as_str();
                name.rsplit('/').next().unwrap_or(name)
            })
            .collect();
        let unlisted: Vec<String> = (self.racks_fn)(&inst.prefix)
            .unwrap_or_default()
            .into_iter()
            .filter(|rack| !listed.contains(rack.as_str()))
            .filter(|rack| {
                (self.kegs_fn)(&inst.prefix, rack).is_some_and(|kegs| !kegs.versions.is_empty())
            })
            .collect();
        if let Ok(mut known) = self.unlisted_racks.lock() {
            known.insert(inst.id.clone(), unlisted);
        }
    }

    /// Keeps which formulae of `json`, `inst`'s inventory reply, no bottle
    /// fits this Mac (`bottles::formulae_built_from_source`, r18 R46-2):
    /// their update compiles. A Mac whose tag is not known keeps none.
    fn remember_source_builds(&self, inst: &ManagerInstance, json: &str) {
        let names = (self.mac_tag_fn)(&inst.prefix)
            .map(|mac| bottles::formulae_built_from_source(json, mac))
            .unwrap_or_default();
        if let Ok(mut known) = self.source_builds.lock() {
            known.insert(inst.id.clone(), names);
        }
    }

    /// Whether the update of the formula `name` compiles, as the last
    /// inventory of `instance_id` read it (`remember_source_builds`).
    fn builds_from_source(&self, instance_id: &str, name: &str) -> bool {
        let short = name.rsplit('/').next().unwrap_or(name);
        self.source_builds
            .lock()
            .map(|known| {
                known
                    .get(instance_id)
                    .is_some_and(|names| names.contains(short))
            })
            .unwrap_or(false)
    }

    /// Whether the last inventory of `instance_id` found formula folders
    /// Homebrew did not list (`remember_unlisted_racks`).
    fn has_unlisted_racks(&self, instance_id: &str) -> bool {
        self.unlisted_racks
            .lock()
            .map(|known| {
                known
                    .get(instance_id)
                    .is_some_and(|racks| !racks.is_empty())
            })
            .unwrap_or(false)
    }

    /// How the keg-only formula `name` under `prefix` stands in it, when
    /// its link there is recorded (`brew::links`, `KegLinks::recorded`):
    /// the one case in which Homebrew's update unlinks it and links it
    /// again (`upgrade.rb:268-272`, `640-643`). `None` for a formula the
    /// last inventory did not list as keg-only and linkable, one whose
    /// links cannot be read, and one with no record -- whose update
    /// unlinks and links nothing, so a link of a person's own through
    /// `opt/<name>` follows it to the new version -- whose update is
    /// planned as before.
    fn recorded_keg_only(&self, instance_id: &str, prefix: &Path, name: &str) -> Option<KegLinks> {
        if !self.is_keg_only(instance_id, name) {
            return None;
        }
        (self.links_fn)(prefix, name).filter(|links| links.recorded)
    }

    /// The commands of the keg-only formula `req` names that its update
    /// unlinks and Homebrew links back after it, which Banager checks once
    /// it is done (`brew link --formula --force` if it is not linked back,
    /// `Warning::HomebrewRelinksAfterUpdate`, y1-keg): those whose places
    /// hold Homebrew's link to it now, by name. `Ok(None)` for anything but
    /// such a formula's update (`recorded_keg_only`). Refused as
    /// `UpdateBlocked::LinkTaken` when something else is at one of its
    /// commands' places (`KegLinks::held_paths`): Homebrew's update would
    /// unlink it, stop at that place to link it back (`Keg::ConflictError`)
    /// and fail, leaving its commands out of Terminal -- and `brew link
    /// --force` would stop there too.
    fn relink_after_upgrade(
        &self,
        inst: &ManagerInstance,
        req: &OpRequest,
    ) -> Result<Option<Vec<String>>, AdapterError> {
        if req.kind != OpKind::Upgrade || req.artifact_kind != ArtifactKind::Formula {
            return Ok(None);
        }
        let Some(links) = self.recorded_keg_only(&inst.id, &inst.prefix, &req.name) else {
            return Ok(None);
        };
        if !links.held_paths().is_empty() {
            return Err(AdapterError::UpdateBlocked {
                reason: UpdateBlocked::LinkTaken,
            });
        }
        Ok(Some(links.linked_names()))
    }

    /// What Homebrew makes of a plan's environment `env` on `inst` once
    /// `bin/brew` has read the `brew.env` files (`brew_env::after_brew_env`).
    fn homebrew_switches(
        &self,
        inst: &ManagerInstance,
        env: &[(String, String)],
    ) -> HomebrewSwitches {
        self.switches_at(&inst.prefix, env)
    }

    /// `homebrew_switches` for the Homebrew under `prefix`.
    fn switches_at(&self, prefix: &Path, env: &[(String, String)]) -> HomebrewSwitches {
        brew_env::after_brew_env(env, prefix, &self.env_var_fn, &self.brew_env_fn)
    }

    /// `brew_env_warnings` for switches already read.
    fn switch_warnings(switches: &HomebrewSwitches, kind: OpKind) -> Vec<Warning> {
        // What `HOMEBREW_NO_CLEANUP_FORMULAE` leaves out of the lines before
        // it, when it names any formula.
        let except = |old_versions: bool, autoremove: bool| {
            (!switches.no_cleanup_formulae.is_empty()).then(|| Warning::HomebrewNoCleanupFormulae {
                names: switches.no_cleanup_formulae.clone(),
                old_versions,
                autoremove,
            })
        };
        // Before an install or an upgrade, where a `brew.env` Banager does
        // not read may have set `HOMEBREW_NO_AUTO_UPDATE` to nothing
        // (`require_no_auto_update` refuses a known one): first, as
        // Homebrew would do it first.
        let may_auto_update = || {
            if switches.auto_update_unknown {
                vec![Warning::HomebrewMayAutoUpdate]
            } else {
                Vec::new()
            }
        };
        // "May" where a `brew.env` Banager does not read may have taken
        // the switch back (`brew_env::EnvFile::Unknown`).
        let autoremoves = |may, will| {
            if switches.autoremove_unknown {
                may
            } else {
                will
            }
        };
        match kind {
            OpKind::Uninstall if !switches.no_autoremove => {
                let mut warnings = vec![autoremoves(
                    Warning::HomebrewMayAutoremove,
                    Warning::HomebrewAutoremoves,
                )];
                warnings.extend(except(false, true));
                warnings
            }
            OpKind::Install | OpKind::Upgrade if !switches.no_install_cleanup => {
                let mut warnings = may_auto_update();
                warnings.push(if switches.install_cleanup_unknown {
                    Warning::HomebrewMayCleanUp
                } else {
                    Warning::HomebrewPeriodicCleanup
                });
                if !switches.no_autoremove {
                    // "May" when either switch is unknown: the line rests
                    // on both, and the "will" line's detail says a
                    // brew.env takes both back.
                    warnings.push(
                        if switches.autoremove_unknown || switches.install_cleanup_unknown {
                            Warning::HomebrewCleanupMayAutoremove
                        } else {
                            Warning::HomebrewCleanupAutoremoves
                        },
                    );
                }
                warnings.extend(except(true, !switches.no_autoremove));
                warnings
            }
            OpKind::Install | OpKind::Upgrade => may_auto_update(),
            _ => Vec::new(),
        }
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
        if matches!(
            args.first().map(String::as_str),
            Some("install" | "upgrade" | "outdated")
        ) {
            self.require_no_auto_update(&inst.prefix, &self.env_vec())?;
        }
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

    /// Refuses `install`, `upgrade` and `outdated` -- the commands
    /// `bin/brew` runs `brew update --auto-update` before
    /// (`setup-auto-update`, `utils/auto-update.sh`) -- when a `brew.env`
    /// Banager read sets `HOMEBREW_NO_AUTO_UPDATE` to nothing: that update
    /// would run outside the one Banager tracks (`maybe_update`), where a
    /// timeout or a Cancel could stop it halfway and leave Homebrew's git
    /// checkout locked.
    ///
    /// A `brew.env` Banager does not read (`EnvFile::Unknown`, in or
    /// through a protected place) is not refused, as z1 intended for it:
    /// the install and upgrade previews say Homebrew "may" update itself
    /// first (`Warning::HomebrewMayAutoUpdate`), and the check runs. Such a
    /// file setting `HOMEBREW_NO_AUTO_UPDATE` to nothing is the one way it
    /// matters -- one that sets it at all sets it to something -- and even
    /// then Homebrew, by default, updates only when its last fetch is a day
    /// old (`HOMEBREW_AUTO_UPDATE_SECS`; 5 minutes with
    /// `HOMEBREW_NO_INSTALL_FROM_API` or a tap-qualified name), which the
    /// update a refresh runs every six hours normally keeps from happening. Refusing every Homebrew check and
    /// update for that, as the first fix did, cost every user whose
    /// dotfiles live in iCloud Drive or Documents all of Homebrew.
    fn require_no_auto_update(
        &self,
        prefix: &Path,
        env: &[(String, String)],
    ) -> Result<(), AdapterError> {
        let switches = brew_env::after_brew_env(env, prefix, &self.env_var_fn, &self.brew_env_fn);
        if !switches.no_auto_update && !switches.auto_update_unknown {
            return Err(AdapterError::Refused(
                "a brew.env file sets HOMEBREW_NO_AUTO_UPDATE to nothing; Homebrew would update itself outside Banager's tracked update".into()
            ));
        }
        Ok(())
    }

    /// Refuses an install or upgrade, right before it runs, after which
    /// Homebrew would now delete more by itself than the preview said --
    /// the reverse of `cleanup_allowed`'s second look (review of v1-brew's
    /// fixes, r6). A `brew.env` edited, or turned unreadable, while the
    /// confirmation is open can take Banager's
    /// `HOMEBREW_NO_INSTALL_CLEANUP=1` back, and the command then runs
    /// Homebrew's own cleanup (`Cleanup.install_clean!`,
    /// `cleanup.rb:361-389`): the package's old versions and downloads,
    /// every formula's when the periodic cleanup is due, and autoremove
    /// unless that is off -- where a preview without those lines said
    /// other software and its old versions are kept. So the switches are
    /// read again as the preview read them (`switch_warnings`): when
    /// Homebrew cleans up now, or may, the preview must have said it
    /// would or might (`Warning::HomebrewPeriodicCleanup`,
    /// `HomebrewMayCleanUp`); when that cleanup autoremoves now, or may,
    /// the preview must have said that too (`HomebrewCleanupAutoremoves`,
    /// `HomebrewCleanupMayAutoremove`); and every formula the preview said
    /// `HOMEBREW_NO_CLEANUP_FORMULAE` leaves out must still be left out.
    /// "Will" where the preview said "may", or the reverse, is no reason
    /// to stop, and nor is less than it said. Otherwise
    /// `Fault::HomebrewSettingsChanged`, and nothing runs.
    fn require_cleanup_as_previewed(
        &self,
        plan: &Plan,
        prefix: &Path,
        env: &[(String, String)],
    ) -> Result<(), Fault> {
        let now = self.switches_at(prefix, env);
        if now.no_install_cleanup {
            return Ok(());
        }
        let said = |line: fn(&Warning) -> bool| plan.warnings.iter().any(line);
        let cleans_up = said(|warning| {
            matches!(
                warning,
                Warning::HomebrewPeriodicCleanup | Warning::HomebrewMayCleanUp
            )
        });
        let autoremoves = now.no_autoremove
            || said(|warning| {
                matches!(
                    warning,
                    Warning::HomebrewCleanupAutoremoves | Warning::HomebrewCleanupMayAutoremove
                )
            });
        let left_out: &[String] = plan
            .warnings
            .iter()
            .find_map(|warning| match warning {
                Warning::HomebrewNoCleanupFormulae { names, .. } => Some(names.as_slice()),
                _ => None,
            })
            .unwrap_or_default();
        let still_left_out = left_out
            .iter()
            .all(|name| now.no_cleanup_formulae.contains(name));
        if cleans_up && autoremoves && still_left_out {
            Ok(())
        } else {
            Err(Fault::HomebrewSettingsChanged)
        }
    }

    /// Re-read the uninstall's settings after the update wait, before any
    /// command can run. Autoremove must have been disclosed, and every
    /// formula the preview excluded must still be excluded. An already
    /// disclosed "may" and "will" cover the same removal scope. A cask
    /// must still have the scope and steps the confirmation showed, read
    /// through the same receipt, trust and app-name readers as the preview.
    /// Its sentence is read with the autoremove the preview said: autoremove
    /// is the check above's to judge, and one turned off since then deletes
    /// less than the sentence shown, which is no reason to stop.
    fn require_uninstall_as_previewed(&self, plan: &Plan) -> Result<(), Fault> {
        if plan.request.kind != OpKind::Uninstall {
            return Ok(());
        }
        let PlanAction::Command { program, env, .. } = &plan.action else {
            return Ok(());
        };
        let prefix = Self::prefix_for(program);
        let now = self.switches_at(&prefix, env);
        // `switch_warnings` says one of these exactly when the preview read
        // the autoremove as on, or maybe on.
        let disclosed = plan.warnings.iter().any(|warning| {
            matches!(
                warning,
                Warning::HomebrewAutoremoves | Warning::HomebrewMayAutoremove
            )
        });
        if !now.no_autoremove {
            let exclusions_remain = plan.warnings.iter().all(|warning| match warning {
                Warning::HomebrewNoCleanupFormulae {
                    names,
                    autoremove: true,
                    ..
                } => names
                    .iter()
                    .all(|name| now.no_cleanup_formulae.contains(name)),
                _ => true,
            });
            if !disclosed || !exclusions_remain {
                return Err(Fault::HomebrewSettingsChanged);
            }
        }
        if plan.request.artifact_kind == ArtifactKind::Cask {
            let trust = now.user_config_home.as_deref().and_then(self.trust_list_fn);
            let as_previewed = HomebrewSwitches {
                no_autoremove: !disclosed,
                ..now
            };
            let (scope, steps) =
                self.uninstall_scope(&prefix, &plan.request, &as_previewed, trust.as_ref());
            let shown: Vec<_> = plan
                .warnings
                .iter()
                .filter(|warning| {
                    matches!(
                        warning,
                        Warning::UninstallScope { .. } | Warning::CaskUninstallStep { .. }
                    )
                })
                .collect();
            let current: Vec<_> = std::iter::once(&scope).chain(steps.iter()).collect();
            if shown != current {
                return Err(Fault::HomebrewSettingsChanged);
            }
        }
        Ok(())
    }

    /// Whether an uninstall of a formula whose Cellar reads `kegs` passes
    /// `--force` (U9): more than one version installed, and no pin.
    fn removes_every_version(kegs: &Kegs) -> bool {
        kegs.versions.len() > 1 && !kegs.pinned
    }

    /// For a formula's uninstall, its Cellar and pin record looked at
    /// again right before it runs.
    ///
    /// The uninstall of every version (U9's `--force`): `--force` deletes
    /// every version Homebrew finds then (`uninstall.rb:31-43`), and skips
    /// Homebrew's own refusal of a pinned formula (`uninstall.rb:45-53`),
    /// which the preview left standing by passing it only where there was
    /// no pin. So it runs only where both are still as the preview showed
    /// them: no pin, and no version the preview did not name
    /// (`Warning::HomebrewRemovesEveryVersion`) -- one an update in
    /// Terminal put in since then, with its cleanup off, say (review F3,
    /// r6). Fewer versions than it named is no reason to stop: nothing it
    /// did not name is deleted. A formula pinned since then, one with a
    /// version more, or whose Cellar or pin record cannot be looked at now
    /// is `Fault::FormulaChanged`, and nothing runs.
    ///
    /// The plain uninstall, which the preview says removes "this version":
    /// Homebrew deletes the one version `opt/` points to
    /// (`resolve_default_keg`, `cli/named_args.rb:567-578`). The preview
    /// passed no `--force` because it found one version, a pin, or a
    /// Cellar it could not read. More than one version now and no pin --
    /// what would make the preview pass `--force` now -- is one put in
    /// since, which may be the one deleted while the one the person saw
    /// stays (review of v1-brew's fixes, r6): `Fault::FormulaChanged`, and
    /// nothing runs. A pin now leaves Homebrew's own refusal standing, and
    /// a Cellar that cannot be read now says nothing either way: both run,
    /// as before.
    ///
    /// Any other plan passes.
    fn require_kegs_as_previewed(&self, plan: &Plan) -> Result<(), Fault> {
        let PlanAction::Command { program, args, .. } = &plan.action else {
            return Ok(());
        };
        if plan.request.kind != OpKind::Uninstall
            || plan.request.artifact_kind != ArtifactKind::Formula
        {
            return Ok(());
        }
        let changed = || Fault::FormulaChanged {
            name: plan.request.name.clone(),
        };
        let now = (self.kegs_fn)(&Self::prefix_for(program), &plan.request.name);
        if !args.iter().any(|arg| arg == "--force") {
            return match now {
                Some(now) if Self::removes_every_version(&now) => Err(changed()),
                _ => Ok(()),
            };
        }
        let named = plan.warnings.iter().find_map(|warning| match warning {
            Warning::HomebrewRemovesEveryVersion { versions } => Some(versions),
            _ => None,
        });
        match (named, now) {
            (Some(named), Some(now))
                if !now.pinned && now.versions.iter().all(|version| named.contains(version)) =>
            {
                Ok(())
            }
            _ => Err(changed()),
        }
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
    /// expected to know how to do, or that Banager can explain from here.
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
            if record.succeeded_at.is_some_and(|succeeded| {
                update_is_fresh((self.wall_clock_fn)(), succeeded.wall, self.update_ttl)
            }) {
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
                self.wall_clock_fn,
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
                    Some(t) if t.monotonic >= started => IndexFreshness::Current,
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
    /// Banager started (`UpdateRecord::running`) or anyone holds Homebrew's
    /// own update lock for (`HomebrewUpdateLock::Held`), otherwise a stamp.
    /// A read of the catalogue takes one before and one after and trusts
    /// what it read only when both are the same `Some`.
    ///
    /// What two equal stamps prove:
    /// - No `brew update` Banager started overlapped the read. The stamp
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
        let banager_updates = match updates.get(&inst.id) {
            Some(record) if record.running => return None,
            Some(record) => record.started,
            None => 0,
        };
        Some(CatalogueStamp {
            banager_updates,
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
    /// have instead of to reopen Banager without `sudo`.
    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let as_root = Self::refuses_as_root(env);
        let mut found = Vec::new();
        for candidate in Self::CANDIDATE_PATHS {
            let path = PathBuf::from(candidate);
            if !(self.path_exists_fn)(&path) {
                continue;
            }
            #[cfg(test)]
            let path = match &self.discovery_root {
                Some(root) => root.join(candidate.trim_start_matches('/')),
                None => path,
            };
            let (version, no_answer) = if as_root {
                (None, None)
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
                let version = match &output {
                    Ok(o) if o.exit_code == Some(0) => parse_version(&o.stdout),
                    _ => None,
                };
                let no_answer = crate::runner::no_answer::unless_answered(&version, &output);
                (version, no_answer)
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
                    no_answer,
                },
                version,
                answered_at: None,
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
        let mut artifacts = parse_info_installed(&output.stdout, &inst.id)?;
        self.read_app_versions(&mut artifacts);
        self.remember_keg_only(&inst.id, &artifacts);
        self.remember_unlisted_racks(inst, &artifacts);
        self.remember_source_builds(inst, &output.stdout);
        Ok(artifacts)
    }

    /// R47-3 (r18): the version each app of a cask that updates itself
    /// says it is (`app_version_fn`, its `CFBundleShortVersionString`),
    /// kept as `ArtifactFacts::app_version` where it is neither Homebrew's
    /// record nor that record's first comma-separated field (Docker's
    /// `4.35.0,184744` is the app's `4.35.0`). Only for an `auto_updates`
    /// cask whose app Homebrew says it put at an absolute `path`: the
    /// others stay at the version Homebrew installed.
    fn read_app_versions(&self, artifacts: &mut [InstalledArtifact]) {
        for artifact in artifacts {
            if artifact.key.kind != ArtifactKind::Cask || !artifact.auto_updates {
                continue;
            }
            let Some(app) = artifact.path.as_deref() else {
                continue;
            };
            let Some(said) = (self.app_version_fn)(app) else {
                continue;
            };
            let record = artifact.version.as_str();
            let first = record.split(',').next().unwrap_or(record);
            if !said.is_empty() && said != record && said != first {
                artifact.facts.app_version = Some(said);
            }
        }
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
        // No `--greedy` flag of any kind. R47-2 (r18): `--greedy` and
        // `--greedy-latest` have Homebrew download the whole installer of
        // each installed `version :latest` cask to hash it
        // (`cask/cask.rb:392-410`, `:437-438` in 7.0.9) -- a silent download
        // inside a check whose timeout fails the whole source. R47-3:
        // `--greedy` and `--greedy-auto-updates` list a cask that updates
        // itself whenever Homebrew's record is not the catalogue's version,
        // skipping Homebrew's look at the app's own version (`:433-452`), so
        // an app that already updated itself would be offered the version
        // it has, or an older one. Without them Homebrew lists such a cask
        // only when the app on the disk is older than the catalogue
        // (`auto_updates_bundle_outdated?`, `:819-850`).
        let args = vec!["outdated".to_string(), "--json=v2".to_string()];
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
        //
        // The same reading marks a package Homebrew disabled
        // (`UpdateBlocked::Disabled`): `brew outdated` lists it like any
        // other, and only `brew info` carries the mark. With no inventory
        // nothing is marked, and an upgrade of it fails or changes nothing
        // as before (`UnchangedAfterUpgrade`). The mark is as fresh as the
        // last `brew update` that succeeded: after `MayBeStale` this reads
        // the catalogue already on disk, like `brew outdated` above.
        let installed = self.inventory(inst).await.unwrap_or_default();
        // The same reading finds the formulae Homebrew left out of it
        // (r18 R46-1): their updates were not checked either.
        let mut notes = notes;
        if self.has_unlisted_racks(&inst.id) {
            notes.push(InstanceNote::FormulaeNotListed);
        }
        let index = InventoryIndex::new(&installed);
        for candidate in &mut candidates {
            if let Some(artifact) = index.resolve(&candidate.key) {
                candidate.key = artifact.key.clone();
                if artifact
                    .facts
                    .homebrew
                    .as_ref()
                    .is_some_and(|facts| facts.disabled.is_some())
                {
                    candidate.blocked = Some(UpdateBlocked::Disabled);
                }
            }
        }
        // A keg-only formula Homebrew links back after its update, with
        // something else at one of its commands' places: its update would
        // fail with its commands out of Terminal (`relink_after_upgrade`,
        // y1-keg). Read from the keg-only names the inventory above kept;
        // a pinned or disabled one keeps that reason, which Homebrew
        // refuses before it unlinks anything.
        // The paths go with it, for the row to name the file in the way.
        for candidate in &mut candidates {
            if candidate.blocked.is_some() || candidate.key.kind != ArtifactKind::Formula {
                continue;
            }
            let paths = self
                .recorded_keg_only(&inst.id, &inst.prefix, &candidate.key.name)
                .map(|links| links.held_paths())
                .unwrap_or_default();
            if !paths.is_empty() {
                candidate.blocked = Some(UpdateBlocked::LinkTaken);
                candidate.warnings.push(Warning::LinkPlacesHeld {
                    name: candidate.key.name.clone(),
                    paths,
                });
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

/// A cask's recorded uninstall as `BrewAdapter::cask_steps` read it.
struct CaskSteps {
    /// The sentence an uninstall of it says (`Warning::UninstallScope`).
    what: UninstallScope,
    /// The steps it takes beside removing what Homebrew placed, in
    /// `CaskStep`'s order; none where Homebrew would skip them all.
    steps: Vec<cask_receipt::StepLine>,
    recorded: Option<Recorded>,
    home: Option<PathBuf>,
    /// A Ruby record from a tap Homebrew may not trust, where trust is
    /// required: Homebrew may run none of its steps.
    untrusted: bool,
}

/// What `BrewAdapter::catalogue_stamp` hands a read of the catalogue to
/// compare with the one it takes after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CatalogueStamp {
    /// `UpdateRecord::started` for the instance.
    banager_updates: u64,
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
    fn of(meta: &Stat) -> FileId {
        FileId {
            dev: meta.dev(),
            ino: meta.ino(),
            mtime: (meta.mtime(), meta.mtime_nsec()),
            ctime: meta.ctime(),
        }
    }
}

/// Whether a candidate `brew` path leads to anything, as `exists` answers
/// but looked up one step at a time and never into or through a protected
/// place (`protected::look`): a Homebrew reached through a link into
/// `~/Documents` or onto `/Volumes` is passed over, as `resolve_exe`
/// passes over a `PATH` folder there, and never run.
fn brew_is_there(path: &Path) -> bool {
    look::target(path, &Protected::of_this_process()).is_ok()
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
/// only `stat`ed, so looking leaves nothing behind. It is opened without
/// waiting (`O_NONBLOCK`), as `read_file` opens a file a tool wrote: a
/// named pipe there would otherwise block the open until something wrote
/// to it, and the uninstall preview with it. Anything but a regular file
/// is a lock this cannot look at. Both are looked up one step at a time
/// and never into or through a protected place (`protected::look`): a
/// lock there is one this cannot look at either.
///
/// On Linux `flock` and `fcntl` locks do not see each other (flock(2)),
/// so there this never reports `Held`; the `LockStamp` still changes when
/// a `brew update` begins.
fn probe_homebrew_update_lock(prefix: &Path) -> HomebrewUpdateLock {
    use std::os::unix::io::AsRawFd;

    let protected = Protected::of_this_process();
    let dir_path = prefix.join("var/homebrew/locks");
    let dir = look::target(&dir_path, &protected)
        .ok()
        .map(|(_, meta)| FileId::of(&meta));
    let (file, meta) = match look::open(&dir_path.join("update"), &protected) {
        Ok(opened) => opened,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return HomebrewUpdateLock::Free(LockStamp { dir, file: None })
        }
        Err(_) => return HomebrewUpdateLock::Unobservable(LockStamp { dir, file: None }),
    };
    let seen = LockStamp {
        dir,
        file: Some(FileId::of(&meta)),
    };
    if !meta.is_file() {
        return HomebrewUpdateLock::Unobservable(seen);
    }
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

/// A moment read on both clocks, since each answers what the other cannot.
#[derive(Clone, Copy, Debug)]
struct Moment {
    /// On the wall clock (`BrewAdapter::wall_clock_fn`), which keeps going
    /// while the Mac sleeps: how long ago it was.
    wall: SystemTime,
    /// On `Instant`, which never goes back: whether it came after another
    /// moment of this run of the app, which a wall clock that can be set
    /// back cannot say.
    monotonic: Instant,
}

impl Moment {
    fn now(wall_clock_fn: fn() -> SystemTime) -> Moment {
        Moment {
            wall: wall_clock_fn(),
            monotonic: Instant::now(),
        }
    }
}

/// Whether a `brew update` that succeeded at `succeeded` spares a refresh
/// at `now` one of its own: less than `ttl` has passed between them on the
/// wall clock, time the Mac spent asleep included. A `now` before
/// `succeeded` does not spare one: the clock was set back past it, and how
/// long ago the update ran is then unknown. The update the refresh runs
/// instead is stamped on the corrected clock when it succeeds, so a clock
/// set back costs one more `brew update`, not one per refresh.
fn update_is_fresh(now: SystemTime, succeeded: SystemTime, ttl: Duration) -> bool {
    now.duration_since(succeeded).is_ok_and(|since| since < ttl)
}

/// What `BrewAdapter` knows about one instance's `brew update`s.
#[derive(Debug, Default)]
struct UpdateRecord {
    /// When the last `brew update` that exited 0 ended. The TTL runs from
    /// its wall-clock reading (`update_is_fresh`); `maybe_update` asks the
    /// monotonic one whether it ended after its round's own update began.
    succeeded_at: Option<Moment>,
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
    /// The adapter's `wall_clock_fn`, which a success is stamped on.
    wall_clock_fn: fn() -> SystemTime,
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
        wall_clock_fn: fn() -> SystemTime,
    ) -> UpdateFinish {
        record.running = true;
        record.started += 1;
        UpdateFinish {
            updates,
            inst_id,
            succeeded: false,
            background_change,
            wall_clock_fn,
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
                record.succeeded_at = Some(Moment::now(self.wall_clock_fn));
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

/// Round-local full and short names share a lookup. Insert in inventory
/// order so a short name keeps the old first-match rule, even when an
/// exact unqualified name occurs after a tapped package of that name.
struct InventoryIndex<'a> {
    names: HashMap<(ArtifactKind, &'a str), &'a InstalledArtifact>,
}

impl<'a> InventoryIndex<'a> {
    fn new(artifacts: impl IntoIterator<Item = &'a InstalledArtifact>) -> Self {
        let mut names = HashMap::new();
        for artifact in artifacts {
            names
                .entry((artifact.key.kind, artifact.key.name.as_str()))
                .or_insert(artifact);
            if let Some(short) = artifact.key.name.rsplit('/').next() {
                names.entry((artifact.key.kind, short)).or_insert(artifact);
            }
        }
        Self { names }
    }

    fn resolve(&self, key: &ArtifactKey) -> Option<&'a InstalledArtifact> {
        self.names.get(&(key.kind, key.name.as_str())).copied()
    }
}

/// `brew link --formula --force <name>`: the one link Banager runs, after
/// the update of a keg-only formula whose link Homebrew recorded where
/// Homebrew did not link it back (y1-keg), and on its own for the formula a
/// source's notice offers (`OpKind::Link`, y2-npmwhy). `--formula`: `brew
/// link` also takes a name as a cask of that name, whose links `--force`
/// would overwrite (`cmd/link.rb`); `--force`: what Homebrew asks before it
/// links a keg-only formula. Never `--overwrite`.
fn link_argv(name: &str) -> Vec<String> {
    vec![
        "link".to_string(),
        "--formula".to_string(),
        "--force".to_string(),
        name.to_string(),
    ]
}

impl BrewAdapter {
    pub async fn plan(
        &self,
        inst: &ManagerInstance,
        req: &OpRequest,
    ) -> Result<Plan, AdapterError> {
        ensure_instance_match(req, inst)?;
        validate_package_name(&req.name)?;
        if matches!(req.kind, OpKind::Install | OpKind::Upgrade) {
            self.require_no_auto_update(&inst.prefix, &self.env_vec())?;
        }
        let lock = prefix_lock(&inst.prefix, self.prefix_identity_fn);
        match req.kind {
            // `brew link --formula --force <formula>` (`link_argv`, the
            // same command that links a keg-only formula back after its
            // update): a keg-only formula's commands put where Terminal
            // looks, for a source whose launcher could not find one of them
            // (`NoAnswer::link_fixes`; the gate plans only one a snapshot
            // offers). Only one the last inventory listed as keg-only and
            // linkable (`is_keg_only`): not one keg-only because of macOS,
            // which `brew link` refuses at Homebrew's default prefix and
            // exits 0 having linked nothing, nor one that is not keg-only,
            // which Homebrew linked itself. `--formula` keeps the name to
            // the formula; `--force` is what Homebrew asks of a keg-only
            // one; `--overwrite` is never passed, so a file already there is
            // never replaced. Its links are read as for an update
            // (`links::read_links`): the commands it puts where Terminal
            // looks are said (`Warning::LinkPutsCommands`) -- linking
            // `node@20` changes which `node` Terminal runs -- and every
            // place `brew link` would stop at (`KegLinks::held_paths`, what
            // makes an update `UpdateBlocked::LinkTaken`) is said too
            // (`Warning::LinkConflicts`), as is every link of Homebrew's
            // already there that a stopped link would take back where its
            // link is not recorded (`KegLinks::rollback_paths`,
            // `Warning::LinkRollbackRisk`); `Session::submit` refuses to
            // run either. No password, no download, no update of Homebrew:
            // `brew link` does none of those.
            OpKind::Link => {
                if req.artifact_kind != ArtifactKind::Formula {
                    return Err(AdapterError::Unsupported(
                        "only a Homebrew formula is linked".to_string(),
                    ));
                }
                if !self.is_keg_only(&inst.id, &req.name) {
                    return Err(AdapterError::Unsupported(
                        "only a keg-only formula Homebrew links is linked".to_string(),
                    ));
                }
                let mut warnings = Vec::new();
                if let Some(links) = (self.links_fn)(&inst.prefix, &req.name) {
                    let names = links.command_names();
                    if !names.is_empty() {
                        warnings.push(Warning::LinkPutsCommands { names });
                    }
                    let paths = links.held_paths();
                    if !paths.is_empty() {
                        warnings.push(Warning::LinkConflicts { paths });
                    }
                    let paths = links.rollback_paths();
                    if !paths.is_empty() {
                        warnings.push(Warning::LinkRollbackRisk { paths });
                    }
                }
                Ok(Plan {
                    request: req.clone(),
                    action: PlanAction::Command {
                        program: inst.exe_path.clone(),
                        args: link_argv(&req.name),
                        env: self.env_vec(),
                    },
                    needs_password: false,
                    locks: vec![lock],
                    cancel_policy: CancelPolicy::KillThenReconcile,
                    warnings,
                    affected: Vec::new(),
                    basis: None,
                    timeout_secs: Self::LINK_TIMEOUT_SECS,
                })
            }
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
                let warnings = self.brew_env_warnings(inst, req.kind, &env);
                Ok(Plan {
                    request: req.clone(),
                    action: PlanAction::Command {
                        program: inst.exe_path.clone(),
                        args: vec!["install".to_string(), flag.to_string(), req.name.clone()],
                        env,
                    },
                    needs_password,
                    locks: vec![lock],
                    cancel_policy: CancelPolicy::KillThenReconcile,
                    warnings,
                    affected: Vec::new(),
                    basis: None,
                    timeout_secs: 1800,
                })
            }
            OpKind::Uninstall => {
                if let Some(path) = self.cask_link_conflict(&inst.prefix, req) {
                    return Err(AdapterError::UninstallUnsafe {
                        path: path.to_string_lossy().into_owned(),
                        reason: crate::model::UninstallUnsafeReason::CaskLinkNotOwned,
                    });
                }
                let flag = match req.artifact_kind {
                    ArtifactKind::Cask => "--cask",
                    _ => "--formula",
                };
                // `brew uses` reads the same catalogue `brew update`
                // rewrites, and this list is what the user confirms an
                // uninstall against: a half-written read that still
                // parses would show fewer dependents than will break --
                // worse than any error. So it is not read while an update
                // runs -- one Banager started, as `inventory` also checks,
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
                let env = self.env_vec();
                let switches = self.homebrew_switches(inst, &env);
                let autoremoves = Self::switch_warnings(&switches, req.kind);
                // Homebrew's trust list, where `bin/brew` will look for it;
                // `None` when Banager cannot read it.
                let trust = switches
                    .user_config_home
                    .as_deref()
                    .and_then(self.trust_list_fn);
                let (scope, cask_steps) =
                    self.uninstall_scope(&inst.prefix, req, &switches, trust.as_ref());
                let forgets = trust
                    .as_ref()
                    .and_then(|trust| Self::forgets_trust(trust, req))
                    .into_iter();
                let mut warnings = vec![scope];
                // U9: every installed version goes, so none is left to be
                // listed again once this one has: `--force`, Homebrew's own
                // way to that (`cmd/uninstall.rb:45`, `uninstall.rb:32-44`),
                // passed only where there is more than one version and no
                // pin -- a pinned formula is refused by Homebrew as it is
                // without it (`uninstall.rb:45-53`).
                let every_version = (req.artifact_kind == ArtifactKind::Formula)
                    .then(|| (self.kegs_fn)(&inst.prefix, &req.name))
                    .flatten()
                    .filter(Self::removes_every_version)
                    .map(|kegs| kegs.versions);
                let mut args = vec!["uninstall".to_string(), flag.to_string()];
                if let Some(versions) = every_version {
                    args.push("--force".to_string());
                    warnings.push(Warning::HomebrewRemovesEveryVersion { versions });
                }
                args.push(req.name.clone());
                let affected = if uses_output.exit_code == Some(0) {
                    parse_uses(&uses_output.stdout)
                } else {
                    // The check itself failed or timed out — this is *not*
                    // the same thing as "confirmed no dependents", and must
                    // not be presented as if it were.
                    warnings.push(Warning::DependentsUnknown);
                    Vec::new()
                };
                // r18 R46-1: `brew uses --installed` names none of the
                // formulae Homebrew left out of its list, which may need
                // this one, and neither does Homebrew's own check before
                // it uninstalls (`InstalledDependents`, also
                // `Formula.installed`).
                if req.artifact_kind == ArtifactKind::Formula
                    && self.has_unlisted_racks(&inst.id)
                    && !warnings.contains(&Warning::DependentsUnknown)
                {
                    warnings.push(Warning::DependentsUnknown);
                }
                if !affected.is_empty() {
                    warnings.push(Warning::WouldBreak {
                        names: affected.clone(),
                    });
                }
                // r18 R46-3: `brew uninstall` leaves a `brew services`
                // service running, and its file where macOS starts it from.
                if req.artifact_kind == ArtifactKind::Formula {
                    let home = (self.env_var_fn)("HOME")
                        .filter(|home| !home.is_empty())
                        .map(PathBuf::from);
                    if let Some(service) = (self.service_fn)(home.as_deref(), &req.name) {
                        warnings.push(Warning::HomebrewServiceStays {
                            name: req.name.rsplit('/').next().unwrap_or(&req.name).to_string(),
                            system: service.system,
                        });
                    }
                }
                warnings.extend(cask_steps);
                warnings.extend(forgets);
                warnings.extend(autoremoves);
                Ok(Plan {
                    request: req.clone(),
                    action: PlanAction::Command {
                        program: inst.exe_path.clone(),
                        args,
                        env,
                    },
                    needs_password: matches!(req.artifact_kind, ArtifactKind::Cask),
                    locks: vec![lock],
                    cancel_policy: CancelPolicy::KillThenReconcile,
                    warnings,
                    affected,
                    basis: None,
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
                let switches = self.homebrew_switches(inst, &env);
                // R47-1: what the installed version's recorded uninstall
                // does first, then what the `brew.env` files make Homebrew
                // do.
                let trust = switches
                    .user_config_home
                    .as_deref()
                    .and_then(self.trust_list_fn);
                let mut warnings = self.update_steps(&inst.prefix, req, &switches, trust.as_ref());
                warnings.extend(Self::switch_warnings(&switches, req.kind));
                let program = inst.exe_path.clone();
                let args = vec!["upgrade".to_string(), flag.to_string(), req.name.clone()];
                let mut then = Vec::new();
                // r18 R46-2: no bottle fits this Mac, so Homebrew compiles
                // the update -- on every Intel Mac, and on Apple silicon
                // with macOS 14 or older, since Homebrew 7 builds no
                // bottle for them. Said as Cargo's compiling update is, and
                // given hours instead of the half hour.
                let compiles = req.artifact_kind == ArtifactKind::Formula
                    && self.builds_from_source(&inst.id, &req.name);
                if compiles {
                    warnings.insert(0, Warning::CompilesLocally);
                }
                // U9: a formula's old versions go once it is updated, said
                // first, as the one thing this preview adds to the update.
                if let Some(versions) = self.cleanup_after_upgrade(inst, req, &env) {
                    warnings.insert(0, Warning::HomebrewCleansUpOldVersions { versions });
                    then.push(vec!["cleanup".to_string(), req.name.clone()]);
                }
                // y1-keg: a keg-only formula whose link Homebrew recorded
                // is checked once updated, and linked back if Homebrew did
                // not, before its cleanup; that is said first of all.
                if let Some(commands) = self.relink_after_upgrade(inst, req)? {
                    warnings.insert(
                        0,
                        Warning::HomebrewRelinksAfterUpdate {
                            name: req.name.clone(),
                            commands,
                        },
                    );
                    then.insert(0, link_argv(&req.name));
                }
                let action = if then.is_empty() {
                    PlanAction::Command { program, args, env }
                } else {
                    PlanAction::CommandThen {
                        program,
                        args,
                        env,
                        then,
                    }
                };
                Ok(Plan {
                    request: req.clone(),
                    action,
                    needs_password,
                    locks: vec![lock],
                    cancel_policy: CancelPolicy::KillThenReconcile,
                    warnings,
                    affected: Vec::new(),
                    basis: None,
                    timeout_secs: if compiles {
                        Self::SOURCE_BUILD_TIMEOUT_SECS
                    } else {
                        1800
                    },
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
                return Ok(Outcome::BanagerFailed(Fault::HomebrewStillUpdating {
                    minutes: self.op_update_wait_minutes(),
                }));
            }
        };
        // Recheck immediately before execution: brew.env can change since preview.
        if matches!(plan.request.kind, OpKind::Install | OpKind::Upgrade) {
            if let PlanAction::Command { program, env, .. }
            | PlanAction::CommandThen { program, env, .. } = &plan.action
            {
                let prefix = Self::prefix_for(program);
                self.require_no_auto_update(&prefix, env)?;
                if let Err(fault) = self.require_cleanup_as_previewed(plan, &prefix, env) {
                    return Ok(Outcome::BanagerFailed(fault));
                }
            }
        }
        if let PlanAction::Command { program, .. } = &plan.action {
            if let Some(path) = self.cask_link_conflict(&Self::prefix_for(program), &plan.request) {
                return Ok(Outcome::BanagerFailed(Fault::PathChanged {
                    path: path.to_string_lossy().into_owned(),
                }));
            }
        }
        if let Err(fault) = self.require_uninstall_as_previewed(plan) {
            return Ok(Outcome::BanagerFailed(fault));
        }
        if let Err(fault) = self.require_kegs_as_previewed(plan) {
            return Ok(Outcome::BanagerFailed(fault));
        }
        // A link on its own: its links read again, after any wait for
        // `brew update` and under the operation's lock. Homebrew's own
        // links that appeared since the preview, its link still not
        // recorded, would go with a link that stopped
        // (`Warning::LinkRollbackRisk`): not started, they stay. A place
        // taken since is left to Homebrew, which links nothing over it.
        if plan.request.kind == OpKind::Link {
            if let PlanAction::Command { program, .. } = &plan.action {
                if (self.links_fn)(&Self::prefix_for(program), &plan.request.name)
                    .is_some_and(|links| !links.rollback_paths().is_empty())
                {
                    return Ok(Outcome::BanagerFailed(Fault::LinkRollbackRisk {
                        name: plan.request.name.clone(),
                    }));
                }
            }
        }
        let relink = match self.require_link_places_free(plan) {
            Ok(relink) => relink,
            Err(fault) => return Ok(Outcome::BanagerFailed(fault)),
        };
        let PlanAction::CommandThen {
            program,
            args,
            env,
            then,
        } = &plan.action
        else {
            return run_plan(&self.runner, plan, sink, op_id, cancel).await;
        };
        // An upgrade, then its follow-ups (`PlanAction::CommandThen`): each
        // through `run_plan`, which streams its lines into this operation's
        // log. The upgrade's end is the operation's; a follow-up runs only
        // after it succeeded, and how it ends is said in the log, never in
        // the outcome -- the update is done either way.
        let step = |args: &[String], timeout_secs| Plan {
            action: PlanAction::Command {
                program: program.clone(),
                args: args.to_vec(),
                env: env.clone(),
            },
            timeout_secs,
            ..plan.clone()
        };
        let outcome = run_plan(
            &self.runner,
            &step(args, plan.timeout_secs),
            sink.clone(),
            op_id,
            cancel.clone(),
        )
        .await?;
        let prefix = Self::prefix_for(program);
        if outcome != Outcome::Succeeded {
            // An update that failed or stopped after Homebrew unlinked the
            // version it replaces: said, with what is no longer in Terminal.
            if relink {
                self.say_what_is_unlinked(plan, &prefix, &sink, op_id);
            }
            return Ok(outcome);
        }
        for follow_up in then {
            match follow_up.first().map(String::as_str) {
                Some("link") if relink => {
                    let link = step(follow_up, Self::LINK_TIMEOUT_SECS);
                    self.link_back(plan, &link, &prefix, &sink, op_id, &cancel)
                        .await;
                }
                Some("cleanup") => {
                    let cleanup = step(follow_up, Self::CLEANUP_TIMEOUT_SECS);
                    self.clean_up_old_versions(plan, &cleanup, &sink, op_id, &cancel)
                        .await;
                }
                _ => {}
            }
        }
        Ok(outcome)
    }

    /// For the update of a keg-only formula its preview said Homebrew links
    /// back (a `brew link --formula --force` follow-up, y1-keg), its links
    /// read again right before it runs. Something else at one of its
    /// commands' places now -- npm's own copy of itself in `bin/npm`, put
    /// there by an update of npm since the preview -- is `Fault::LinkTaken`,
    /// and nothing runs: the update would unlink the formula, and neither
    /// Homebrew's link afterwards nor `brew link --formula --force` gets past that
    /// file (`Keg::ConflictError`), so its commands would leave Terminal,
    /// as `node` did on 2026-10-07. Otherwise whether to check it after the
    /// update: not when its link is no longer recorded (unlinked since the
    /// preview: the update then unlinks and links nothing); yes when it
    /// is, or when its links cannot be read now (as the preview said). Any
    /// other plan: `false`.
    fn require_link_places_free(&self, plan: &Plan) -> Result<bool, Fault> {
        let PlanAction::CommandThen { program, then, .. } = &plan.action else {
            return Ok(false);
        };
        if !then
            .iter()
            .any(|argv| argv.first().map(String::as_str) == Some("link"))
        {
            return Ok(false);
        }
        let Some(links) = (self.links_fn)(&Self::prefix_for(program), &plan.request.name) else {
            return Ok(true);
        };
        if !links.recorded {
            return Ok(false);
        }
        let paths = links.held_paths();
        if paths.is_empty() {
            Ok(true)
        } else {
            Err(Fault::LinkTaken {
                name: plan.request.name.clone(),
                paths,
            })
        }
    }

    /// The `brew link --formula --force <name>` that follows the update of a keg-only
    /// formula whose link Homebrew recorded in `prefix`, once it has exited
    /// 0 (y1-keg): `link`,
    /// the plan of that one command. Not run where Homebrew linked it back
    /// itself (`LogNote::StillLinkedAfterUpdate`), nor after a Cancel; how
    /// it ends -- what of the formula is no longer in Terminal -- is said in
    /// the log (`say_what_is_unlinked`), never in the outcome.
    async fn link_back(
        &self,
        plan: &Plan,
        link: &Plan,
        prefix: &Path,
        sink: &Arc<dyn EventSink>,
        op_id: OpId,
        cancel: &CancellationToken,
    ) {
        let name = plan.request.name.clone();
        let note = |note| sink.emit(OperationEvent::Note { op_id, note });
        if (self.links_fn)(prefix, &name).is_some_and(|links| links.fully_linked()) {
            note(LogNote::StillLinkedAfterUpdate { name });
            return;
        }
        if !cancel.is_cancelled() {
            note(LogNote::RelinkingAfterUpdate { name });
            // How it ended is read off the links themselves below: a
            // `brew link` that exits 0 having linked nothing (macOS's own
            // software, refused) is no better than one that failed.
            let _ = run_plan(&self.runner, link, sink.clone(), op_id, cancel.clone()).await;
        }
        self.say_what_is_unlinked(plan, prefix, sink, op_id);
    }

    /// `LogNote::NoLongerLinked` for those of the commands the update's
    /// preview said were in Terminal (`Warning::HomebrewRelinksAfterUpdate`)
    /// whose places in `prefix` no longer lead into the formula; nothing
    /// when all still do, or when its links cannot be read.
    fn say_what_is_unlinked(
        &self,
        plan: &Plan,
        prefix: &Path,
        sink: &Arc<dyn EventSink>,
        op_id: OpId,
    ) {
        let Some(previewed) = plan.warnings.iter().find_map(|warning| match warning {
            Warning::HomebrewRelinksAfterUpdate { commands, .. } => Some(commands),
            _ => None,
        }) else {
            return;
        };
        let Some(links) = (self.links_fn)(prefix, &plan.request.name) else {
            return;
        };
        let linked = links.in_terminal_names();
        let commands: Vec<String> = previewed
            .iter()
            .filter(|command| !linked.contains(command))
            .cloned()
            .collect();
        if !commands.is_empty() {
            sink.emit(OperationEvent::Note {
                op_id,
                note: LogNote::NoLongerLinked {
                    name: plan.request.name.clone(),
                    commands,
                },
            });
        }
    }

    /// The `brew cleanup <name>` that follows a formula's update once it
    /// has exited 0 (U9): `cleanup`, the plan of that one command. Not run
    /// after a Cancel, nor where the person's settings, asked again at its
    /// turn, no longer allow it; how it ends is said in the log.
    async fn clean_up_old_versions(
        &self,
        plan: &Plan,
        cleanup: &Plan,
        sink: &Arc<dyn EventSink>,
        op_id: OpId,
        cancel: &CancellationToken,
    ) {
        let PlanAction::Command { program, env, .. } = &cleanup.action else {
            return;
        };
        let name = plan.request.name.clone();
        let note = |note| sink.emit(OperationEvent::Note { op_id, note });
        if cancel.is_cancelled() {
            // Stopped before it started: nothing of it ran.
            note(LogNote::OldVersionsNotCleanedUp {
                name,
                exit_code: None,
            });
            return;
        }
        // Recheck settings and the deletion set. The newest keg is kept by
        // cleanup; every older keg must have appeared in the confirmation.
        let allowed = self
            .cleanup_allowed(&Self::prefix_for(program), &name, env)
            .is_some_and(|kegs| {
                let named = plan.warnings.iter().find_map(|warning| match warning {
                    Warning::HomebrewCleansUpOldVersions { versions } => Some(versions),
                    _ => None,
                });
                named.is_some_and(|named| {
                    kegs.versions
                        .split_last()
                        .is_some_and(|(_, old)| old.iter().all(|version| named.contains(version)))
                })
            });
        if !allowed {
            note(LogNote::OldVersionsCleanupSkipped { name });
            return;
        }
        note(LogNote::CleaningUpOldVersions { name: name.clone() });
        let exit_code =
            match run_plan(&self.runner, cleanup, sink.clone(), op_id, cancel.clone()).await {
                Ok(Outcome::Succeeded) => {
                    let versions = self.versions_kept(program, plan);
                    if !versions.is_empty() {
                        note(LogNote::OldVersionsKept { name, versions });
                    }
                    return;
                }
                Ok(Outcome::Failed { exit_code, .. }) => exit_code,
                // Cancelled, out of time, or never started.
                Ok(_) | Err(_) => None,
            };
        note(LogNote::OldVersionsNotCleanedUp { name, exit_code });
    }

    /// The versions the update's preview said its `brew cleanup` deletes
    /// (`Warning::HomebrewCleansUpOldVersions`) that are still in the
    /// Cellar once that cleanup has exited 0 -- the same read as the
    /// preview's (`kegs_fn`) -- but for the newest there, the one the
    /// update put in. `brew cleanup` exits 0 and keeps some: one named by
    /// an alias in `HOMEBREW_NO_CLEANUP_FORMULAE` (`onoe`, which does not
    /// fail the command, `cleanup.rb:511-514`), or one Homebrew still
    /// needs (`Formula#eligible_kegs_for_cleanup`). Empty when none is
    /// left, or the Cellar cannot be read.
    fn versions_kept(&self, program: &Path, plan: &Plan) -> Vec<String> {
        let Some(named) = plan.warnings.iter().find_map(|warning| match warning {
            Warning::HomebrewCleansUpOldVersions { versions } => Some(versions),
            _ => None,
        }) else {
            return Vec::new();
        };
        let Some(now) = (self.kegs_fn)(&Self::prefix_for(program), &plan.request.name) else {
            return Vec::new();
        };
        let Some((_newest, older)) = now.versions.split_last() else {
            return Vec::new();
        };
        named
            .iter()
            .filter(|version| older.contains(version))
            .cloned()
            .collect()
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

    /// After a link (`OpKind::Link`): whether the formula is linked now,
    /// read off its links as after the link that follows an update
    /// (`link_back`): Homebrew's record of the link there, and every one of
    /// its commands' places holding Homebrew's link to it
    /// (`KegLinks::fully_linked`). `None` when its links cannot be read --
    /// no `opt/<name>` leading into its Cellar folder, the formula gone.
    /// Nothing runs.
    pub async fn reconcile_link(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Option<bool>, AdapterError> {
        Ok((self.links_fn)(&inst.prefix, &key.name).map(|links| links.fully_linked()))
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

    /// `brew upgrade` upgrades a formula's outdated dependencies before the
    /// formula itself (`Adapter::one_update_can_update_others`).
    fn one_update_can_update_others(&self) -> bool {
        true
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

    async fn reconcile_link(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Option<bool>, AdapterError> {
        BrewAdapter::reconcile_link(self, inst, key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ArtifactKind;
    use crate::runner::MockRunner;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_inventory_index_preserves_first_match_and_kind() {
        let mut artifacts = vec![
            crate::testing::installed_artifact("brew:1", ArtifactKind::Formula, "team/tap/tool"),
            crate::testing::installed_artifact("brew:1", ArtifactKind::Formula, "tool"),
            crate::testing::installed_artifact("brew:1", ArtifactKind::Cask, "tool"),
            crate::testing::installed_artifact("brew:1", ArtifactKind::Formula, "other/tap/tool"),
        ];
        artifacts.extend((0..5000).map(|i| {
            crate::testing::installed_artifact(
                "brew:1",
                ArtifactKind::Formula,
                &format!("team/tap/tool-{i}"),
            )
        }));
        let mut visits = 0;
        let index = InventoryIndex::new(artifacts.iter().inspect(|_| visits += 1));
        assert_eq!(
            visits,
            artifacts.len(),
            "build the lookup with one inventory pass"
        );
        let short = ArtifactKey {
            instance_id: "brew:1".into(),
            kind: ArtifactKind::Formula,
            name: "tool".into(),
        };
        assert_eq!(index.resolve(&short).unwrap().key.name, "team/tap/tool");
        assert_eq!(
            index
                .resolve(&ArtifactKey {
                    kind: ArtifactKind::Cask,
                    ..short.clone()
                })
                .unwrap()
                .key
                .kind,
            ArtifactKind::Cask
        );
        assert_eq!(
            index
                .resolve(&ArtifactKey {
                    name: "other/tap/tool".into(),
                    ..short.clone()
                })
                .unwrap()
                .key
                .name,
            "other/tap/tool"
        );
        assert!(index
            .resolve(&ArtifactKey {
                name: "absent".into(),
                ..short.clone()
            })
            .is_none());
        for i in 0..5000 {
            let key = ArtifactKey {
                name: format!("tool-{i}"),
                ..short.clone()
            };
            assert_eq!(
                index.resolve(&key).unwrap().key.name,
                format!("team/tap/tool-{i}")
            );
        }
    }

    #[test]
    fn test_f12_prefix_aliases_share_one_lock() {
        let root = crate::testing::unique_temp_path("f12-prefix");
        let prefix = root.join("homebrew");
        std::fs::create_dir_all(&prefix).unwrap();
        let alias = root.join("npm-prefix");
        std::os::unix::fs::symlink(&prefix, &alias).unwrap();
        let protected = Protected::new(&root.join("home"));
        let expected = ResourceLock(format!("brew:{}", prefix.display()));
        let identity = |path: &Path| directory_identity(path, &protected);
        let linked = matching_prefix_lock(&alias, std::slice::from_ref(&prefix), identity);
        assert_eq!(
            matching_prefix_lock(&alias, &[prefix.clone(), alias.clone()], identity),
            expected
        );
        let upper = matching_prefix_lock(
            &root.join("Homebrew"),
            std::slice::from_ref(&prefix),
            identity,
        );
        let separate = root.join("other");
        std::fs::create_dir_all(&separate).unwrap();
        let other = matching_prefix_lock(&separate, std::slice::from_ref(&prefix), identity);
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(linked, expected, "symlink alias");
        assert_eq!(upper, expected, "case variant");
        assert_ne!(other, expected, "separate directory");
    }

    /// A prefix that cannot be looked at -- not there, a dangling link, a
    /// file rather than a folder, or in a place Banager never reads --
    /// keeps the lock its own spelling names: never merged with a
    /// discovery prefix that cannot be looked at either (two unknowns are
    /// not one folder), and never a panic.
    #[test]
    fn test_f12_review_unresolvable_prefix_keeps_its_own_lock() {
        let root = crate::testing::unique_temp_path("f12-unresolvable");
        let prefix = root.join("homebrew");
        std::fs::create_dir_all(&prefix).unwrap();
        // The home folder is there before its places are known, as a
        // real one is: `Protected::new` learns where it leads.
        let documents = root.join("home/Documents/npm-global");
        std::fs::create_dir_all(&documents).unwrap();
        let protected = Protected::new(&root.join("home"));
        let identity = |path: &Path| directory_identity(path, &protected);
        let own = |path: &Path| ResourceLock(format!("brew:{}", path.display()));
        let missing = root.join("gone");
        let also_missing = root.join("also-gone");
        let file = root.join("file");
        std::fs::write(&file, b"").unwrap();
        let file_alias = root.join("file-alias");
        std::os::unix::fs::symlink(&file, &file_alias).unwrap();
        let documents_alias = root.join("documents-alias");
        std::os::unix::fs::symlink(&documents, &documents_alias).unwrap();
        let dangling = root.join("dangling");
        std::os::unix::fs::symlink(root.join("nowhere"), &dangling).unwrap();

        let cases = [
            (&missing, vec![also_missing.clone(), prefix.clone()]),
            (&dangling, vec![missing.clone(), prefix.clone()]),
            (&file_alias, vec![file.clone(), prefix.clone()]),
            (&documents_alias, vec![documents.clone(), prefix.clone()]),
        ];
        let found: Vec<ResourceLock> = cases
            .iter()
            .map(|(path, known)| matching_prefix_lock(path, known, identity))
            .collect();
        // A discovery prefix that is not there is passed over, not matched.
        let passed_over =
            matching_prefix_lock(&prefix, &[missing.clone(), prefix.clone()], identity);
        std::fs::remove_dir_all(&root).unwrap();
        for ((path, _), lock) in cases.iter().zip(found) {
            assert_eq!(lock, own(path), "{}", path.display());
        }
        assert_eq!(passed_over, own(&prefix));
    }

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
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        }
    }

    /// A runner that answers `--version` for one candidate path.
    fn version_runner(brew_path: &str, version_line: &str) -> Arc<MockRunner> {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![brew_path, "--version"],
            CommandOutput {
                stderr_cause: Default::default(),
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
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let user = HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
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
                stderr_cause: Default::default(),
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
    async fn test_detect_says_why_homebrew_did_not_answer() {
        // What `brew --version` did is the reason (`NoAnswer`): it ran and
        // failed here. Under root nothing was asked, so there is none.
        use crate::model::NoAnswerKind;
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "--version"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "Error: Homebrew's Ruby could not be found\n".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let instances = BrewAdapter::new(runner.clone())
            .with_path_exists_fn(apple_silicon_layout)
            .detect(&detect_env(501))
            .await;
        assert_eq!(
            instances[0].status.no_answer.as_ref().map(|why| why.kind),
            Some(NoAnswerKind::ExitedWithError)
        );
        let instances = BrewAdapter::new(runner)
            .with_path_exists_fn(apple_silicon_layout)
            .detect(&detect_env(0))
            .await;
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::RefusesAsRoot)
        );
        assert_eq!(instances[0].status.no_answer, None);
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
                stderr_cause: Default::default(),
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
    async fn test_inventory_shows_the_version_an_app_that_updates_itself_says_it_is() {
        // R47-3 (r18): an app with its own updater moves on while Homebrew's
        // record stays at the version it installed. The inventory reads the
        // app's own `CFBundleShortVersionString` for a cask that updates
        // itself and keeps it where it is not the record: the first field
        // of a version like Docker's `4.35.0,184744` counts as the record.
        let runner = Arc::new(MockRunner::new());
        let json = r#"{
            "formulae": [],
            "casks": [
                {"token":"firefox","name":["Mozilla Firefox"],"installed":"128.0","auto_updates":true,"artifacts":[{"app":["Firefox.app"],"target":"/Applications/Firefox.app"}]},
                {"token":"docker-desktop","name":["Docker Desktop"],"installed":"4.35.0,184744","auto_updates":true,"artifacts":[{"app":["Docker.app"],"target":"/Applications/Docker.app"}]},
                {"token":"onyx","name":["OnyX"],"installed":"4.6.2","auto_updates":null,"artifacts":[{"app":["OnyX.app"],"target":"/Applications/OnyX.app"}]},
                {"token":"zoom","name":["Zoom"],"installed":"6.2.5","auto_updates":true,"artifacts":[{"app":["zoom.us.app"],"target":"/Applications/zoom.us.app"}]}
            ]
        }"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "info", "--installed", "--json=v2"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: json.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner).with_app_version_fn(|app| {
            match app.to_str()? {
                "/Applications/Firefox.app" => Some("131.0.3"),
                "/Applications/Docker.app" => Some("4.35.0"),
                // Not one that updates itself: not read, whatever it says.
                "/Applications/OnyX.app" => Some("4.7.0"),
                // zoom.us.app says nothing.
                _ => None,
            }
            .map(str::to_string)
        });
        let artifacts = adapter
            .inventory(&test_instance())
            .await
            .expect("inventory");
        let app_versions: Vec<(&str, &str, Option<&str>)> = artifacts
            .iter()
            .map(|artifact| {
                (
                    artifact.key.name.as_str(),
                    artifact.version.as_str(),
                    artifact.facts.app_version.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            app_versions,
            vec![
                ("firefox", "128.0", Some("131.0.3")),
                ("docker-desktop", "4.35.0,184744", None),
                ("onyx", "4.6.2", None),
                ("zoom", "6.2.5", None),
            ]
        );
    }

    #[tokio::test]
    async fn test_check_updates_respects_ttl() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "update"],
            CommandOutput {
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: outdated_json.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let mock_ref = runner.clone();
        static NOW: AtomicU64 = AtomicU64::new(T0);
        fn now() -> SystemTime {
            wall_clock(&NOW)
        }
        let adapter = BrewAdapter::new(runner)
            .with_update_ttl(Duration::from_secs(3600))
            .with_wall_clock_fn(now);
        let inst = test_instance();

        let first = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("first check_updates")
            .candidates;
        assert_eq!(first.len(), 1);
        // 59 minutes later on the wall clock: within the hour.
        NOW.store(T0 + 59 * 60, Ordering::SeqCst);
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
            "brew update should run once within the TTL window on the wall clock"
        );
        assert_eq!(outdated_calls, 2);
    }

    /// 2026-09-29 00:00 UTC, in Unix seconds: where the update gate's tests
    /// start the wall clocks they move.
    const T0: u64 = 1_790_640_000;
    const HOUR: u64 = 60 * 60;

    /// The wall clock kept in `secs`, in Unix seconds, as `wall_clock_fn`
    /// reads it. A test that moves its clock keeps its own `static`: the
    /// tests run in parallel and the adapter's clock is a plain `fn`, so a
    /// clock two tests shared would be one test setting another's time.
    fn wall_clock(secs: &AtomicU64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs.load(Ordering::SeqCst))
    }

    /// A Homebrew whose `brew update` succeeds at once and whose `brew
    /// outdated` lists nothing.
    fn runner_with_quick_update() -> Arc<MockRunner> {
        let ok = CommandOutput {
            stderr_cause: Default::default(),
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        };
        let runner = Arc::new(MockRunner::new());
        runner.respond(vec!["/opt/homebrew/bin/brew", "update"], ok.clone());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                stdout: r#"{"formulae":[],"casks":[]}"#.to_string(),
                ..ok
            },
        );
        runner
    }

    /// How many `brew <subcommand>`s `runner` was asked to run.
    fn calls_to(runner: &MockRunner, subcommand: &str) -> usize {
        runner
            .calls()
            .iter()
            .filter(|c| c.get(1).map(String::as_str) == Some(subcommand))
            .count()
    }

    /// One `check_updates` of `inst`, which must answer from a catalogue
    /// it takes as current: brought up to date just now, or within the six
    /// hours.
    async fn check_current(adapter: &BrewAdapter, inst: &ManagerInstance) {
        let outcome = adapter
            .check_updates(inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert!(outcome.notes.is_empty(), "got {:?}", outcome.notes);
    }

    #[tokio::test]
    async fn test_within_six_hours_on_the_clock_of_a_brew_update_checks_skip_it_and_at_six_run_it()
    {
        static NOW: AtomicU64 = AtomicU64::new(T0);
        fn now() -> SystemTime {
            wall_clock(&NOW)
        }
        let runner = runner_with_quick_update();
        // `BrewAdapter::new`'s own six hours, not a TTL of the test's.
        let adapter = BrewAdapter::new(runner.clone()).with_wall_clock_fn(now);
        let inst = test_instance();

        check_current(&adapter, &inst).await;
        assert_eq!(calls_to(&runner, "update"), 1);
        NOW.store(T0 + 6 * HOUR - 1, Ordering::SeqCst);
        check_current(&adapter, &inst).await;
        assert_eq!(
            calls_to(&runner, "update"),
            1,
            "a second short of six hours on the clock after the last `brew update` \
             ended, a check must skip it"
        );
        NOW.store(T0 + 6 * HOUR, Ordering::SeqCst);
        check_current(&adapter, &inst).await;
        assert_eq!(
            calls_to(&runner, "update"),
            2,
            "six hours on the clock after the last `brew update` ended, a check must run it"
        );
        assert_eq!(
            calls_to(&runner, "outdated"),
            3,
            "every check reads the catalogue, whether it updated it or not"
        );
    }

    #[tokio::test]
    async fn test_a_night_asleep_counts_toward_the_six_hours_so_the_morning_check_runs_brew_update()
    {
        // The bug: the six hours were counted on `Instant`, which on macOS
        // stops while the Mac sleeps. Checked at 18:00 and asleep from
        // 18:05 to 09:00, the Mac had been awake five minutes since, so the
        // morning's check skipped `brew update` and listed updates from a
        // catalogue fifteen hours old. Sleep moves the wall clock and
        // nothing else, and so does this test: between the two checks
        // `Instant` moves by the milliseconds the test takes.
        static NOW: AtomicU64 = AtomicU64::new(T0 + 18 * HOUR);
        fn now() -> SystemTime {
            wall_clock(&NOW)
        }
        let runner = runner_with_quick_update();
        let adapter = BrewAdapter::new(runner.clone()).with_wall_clock_fn(now);
        let inst = test_instance();

        check_current(&adapter, &inst).await;
        assert_eq!(calls_to(&runner, "update"), 1);
        NOW.store(T0 + 33 * HOUR, Ordering::SeqCst);
        check_current(&adapter, &inst).await;
        assert_eq!(
            calls_to(&runner, "update"),
            2,
            "fifteen hours on the clock after the last `brew update`, all but minutes \
             of them asleep, the morning's check must run it"
        );
    }

    #[tokio::test]
    async fn test_a_clock_set_back_to_before_the_last_brew_update_ended_runs_it_once() {
        // Once the clock reads a time before the last update ended, how long
        // ago it ran is unknown, so the six hours count as gone: waiting for
        // the clock to reach that update's six hours again could take as
        // long as it was set back. The update that check runs is stamped on
        // the corrected clock, so a clock set back costs one more update,
        // not one per check.
        static NOW: AtomicU64 = AtomicU64::new(T0);
        fn now() -> SystemTime {
            wall_clock(&NOW)
        }
        let runner = runner_with_quick_update();
        let adapter = BrewAdapter::new(runner.clone()).with_wall_clock_fn(now);
        let inst = test_instance();

        check_current(&adapter, &inst).await;
        assert_eq!(calls_to(&runner, "update"), 1);
        NOW.store(T0 - 1, Ordering::SeqCst);
        check_current(&adapter, &inst).await;
        assert_eq!(
            calls_to(&runner, "update"),
            2,
            "a clock set back to before the last `brew update` ended must run it"
        );
        check_current(&adapter, &inst).await;
        assert_eq!(
            calls_to(&runner, "update"),
            2,
            "the update run then was stamped on the corrected clock, so the next \
             check within six hours of it must skip it"
        );
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
                    stderr_cause: Default::default(),
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
                    stderr_cause: Default::default(),
                    exit_code: Some(0),
                    stdout: empty_outdated.to_string(),
                    stderr: String::new(),
                    timed_out: false,
                    cancelled: false,
                },
            );
        }
        let mock_ref = runner.clone();
        static NOW: AtomicU64 = AtomicU64::new(T0);
        fn now() -> SystemTime {
            wall_clock(&NOW)
        }
        let adapter = BrewAdapter::new(runner)
            .with_update_ttl(Duration::from_secs(3600))
            .with_wall_clock_fn(now);

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

        // 59 minutes later on the wall clock: within the hour for both.
        NOW.store(T0 + 59 * 60, Ordering::SeqCst);
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

    /// The `brew outdated` argv and environment of one `check_updates` on
    /// `test_instance` with `include_self_updating` as given, Homebrew
    /// answering that nothing is outdated whichever flags it was given.
    async fn outdated_run(include_self_updating: bool) -> CommandSpec {
        let runner = Arc::new(MockRunner::new());
        let ok = |stdout: &str| CommandOutput {
            stderr_cause: Default::default(),
            exit_code: Some(0),
            stdout: stdout.to_string(),
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        };
        runner.respond(vec!["/opt/homebrew/bin/brew", "update"], ok(""));
        let empty_outdated = r#"{"formulae":[],"casks":[]}"#;
        for flag in [
            None,
            Some("--greedy"),
            Some("--greedy-auto-updates"),
            Some("--greedy-latest"),
        ] {
            let mut argv = vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"];
            argv.extend(flag);
            runner.respond(argv, ok(empty_outdated));
        }
        let adapter = BrewAdapter::new(runner.clone());
        let opts = CheckOptions {
            include_self_updating,
            ..CheckOptions::default()
        };
        let result = adapter
            .check_updates(&test_instance(), &opts)
            .await
            .expect("check_updates")
            .candidates;
        assert!(result.is_empty());
        runner
            .specs()
            .into_iter()
            .find(|spec| spec.args.first().map(String::as_str) == Some("outdated"))
            .expect("brew outdated ran")
    }

    #[tokio::test]
    async fn test_check_updates_leaves_an_app_that_updates_itself_to_homebrews_own_look_at_it() {
        // R47-3 (r18): `--greedy` and `--greedy-auto-updates` both have
        // Homebrew 7.0.9 list an `auto_updates` cask whenever its record is
        // not the catalogue's version, skipping its look at the app's own
        // version (`Cask#outdated_version`, `cask/cask.rb:433-452`): an app
        // that already updated itself is offered the version it has, or an
        // older one. Without either flag, Homebrew lists it only when the
        // app itself is older than the catalogue
        // (`auto_updates_bundle_outdated?`, `:819-850`).
        for include_self_updating in [false, true] {
            let spec = outdated_run(include_self_updating).await;
            assert_eq!(
                spec.args,
                vec!["outdated", "--json=v2"],
                "{include_self_updating}"
            );
        }
    }

    #[tokio::test]
    async fn test_check_updates_never_has_brew_download_a_latest_casks_installer() {
        // R47-2 (r18): with `--greedy` or `--greedy-latest`, Homebrew 7.0.9
        // downloads the whole installer of each installed `version :latest`
        // cask to hash it (`cask/cask.rb:392-410`, `:437-438`), inside a
        // check of 120 s whose timeout fails the whole source. Neither is
        // passed, with the setting on or off.
        for include_self_updating in [false, true] {
            let spec = outdated_run(include_self_updating).await;
            assert!(
                !spec
                    .args
                    .iter()
                    .any(|arg| arg == "--greedy" || arg == "--greedy-latest"),
                "{include_self_updating}: {:?}",
                spec.args
            );
        }
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
    async fn test_check_updates_marks_what_homebrew_disabled_and_nothing_else() {
        // `brew outdated --json=v2` lists a disabled formula or cask like
        // any other (its five keys say nothing of the mark), and a named
        // `brew upgrade` of it then fails (a formula) or changes nothing
        // and exits 0 (a cask). The mark is in `brew info --installed
        // --json=v2`, which this check already reads, so the row is held
        // back before anyone presses Update: `UpdateBlocked::Disabled`.
        // Inline JSON: no recorded fixture has a disabled package.
        //
        // - `oldtool`: a formula Homebrew disabled -- Disabled.
        // - `jq`: disabled *and* pinned -- Disabled, since unpinning it
        //   would not make it updatable.
        // - `twin`: a formula and a cask of one name, only the cask
        //   disabled -- only the cask is held back.
        // - `quickjot`: a tapped cask Homebrew disabled, listed by
        //   `outdated` under its short name -- found once qualified.
        // - `git`: nothing of the kind -- free to update.
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "update"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let installed = r#"{
            "formulae": [
                {"name": "oldtool", "installed": [{"version": "1.0", "installed_on_request": true}],
                 "disabled": true, "disable_date": "2026-09-01", "disable_reason": "unmaintained"},
                {"name": "jq", "installed": [{"version": "1.6", "installed_on_request": true}],
                 "pinned": true, "disabled": true},
                {"name": "twin", "installed": [{"version": "1.0", "installed_on_request": true}],
                 "disabled": false},
                {"name": "git", "installed": [{"version": "2.55.0", "installed_on_request": true}]}
            ],
            "casks": [
                {"token": "twin", "installed": "1.0", "disabled": true},
                {"token": "quickjot", "full_token": "acme/tap/quickjot", "installed": "2.3.1",
                 "disabled": true, "disable_reason": "fails_gatekeeper_check"}
            ]
        }"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "info", "--installed", "--json=v2"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: installed.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let outdated = r#"{
            "formulae": [
                {"name": "oldtool", "installed_versions": ["1.0"], "current_version": "1.1", "pinned": false, "pinned_version": null},
                {"name": "jq", "installed_versions": ["1.6"], "current_version": "1.7.1", "pinned": true, "pinned_version": "1.6"},
                {"name": "twin", "installed_versions": ["1.0"], "current_version": "1.1", "pinned": false, "pinned_version": null},
                {"name": "git", "installed_versions": ["2.55.0"], "current_version": "2.55.1", "pinned": false, "pinned_version": null}
            ],
            "casks": [
                {"name": "twin", "installed_versions": ["1.0"], "current_version": "1.1"},
                {"name": "quickjot", "installed_versions": ["2.3.1"], "current_version": "2.4.0"}
            ]
        }"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                stderr_cause: Default::default(),
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
        let blocked: Vec<(&str, ArtifactKind, Option<UpdateBlocked>)> = candidates
            .iter()
            .map(|c| (c.key.name.as_str(), c.key.kind, c.blocked))
            .collect();
        assert_eq!(
            blocked,
            vec![
                (
                    "oldtool",
                    ArtifactKind::Formula,
                    Some(UpdateBlocked::Disabled)
                ),
                ("jq", ArtifactKind::Formula, Some(UpdateBlocked::Disabled)),
                ("twin", ArtifactKind::Formula, None),
                ("git", ArtifactKind::Formula, None),
                ("twin", ArtifactKind::Cask, Some(UpdateBlocked::Disabled)),
                (
                    "acme/tap/quickjot",
                    ArtifactKind::Cask,
                    Some(UpdateBlocked::Disabled)
                ),
            ]
        );
    }

    #[tokio::test]
    async fn test_check_updates_marks_nothing_disabled_when_the_inventory_cannot_be_read() {
        // Marking is the same best effort as qualifying: with no inventory
        // the candidates are what `brew outdated` said, `pinned` and all,
        // and no row is held back for a mark Banager did not read.
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "update"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let outdated = r#"{"formulae":[{"name":"jq","installed_versions":["1.6"],"current_version":"1.7.1","pinned":true,"pinned_version":"1.6"},{"name":"git","installed_versions":["2.55.0"],"current_version":"2.55.1"}],"casks":[]}"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                stderr_cause: Default::default(),
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
        assert_eq!(candidates[0].blocked, Some(UpdateBlocked::Pinned));
        assert_eq!(candidates[1].blocked, None);
    }

    #[tokio::test]
    async fn test_check_updates_reports_a_failed_brew_update_as_a_note_on_the_source() {
        // A failed `brew update` is a fact about Homebrew, not about jq:
        // the catalogue Banager compared against may be behind, so every
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
    use crate::model::{CaskStep, RemoveCheck};
    use crate::runner::MockRunner;
    use crate::testing::{command_args, command_env};

    fn test_instance() -> ManagerInstance {
        ManagerInstance {
            exe_path: PathBuf::from("/opt/homebrew/bin/brew"),
            prefix: PathBuf::from("/opt/homebrew"),
            version: Some("7.0.3".to_string()),
            ..crate::testing::manager_instance("brew", "brew:/opt/homebrew")
        }
    }

    /// `node@22` under `/opt/homebrew`, keg-only and not linked, as
    /// `links::read_links` reads it on the author's Mac after 2026-10-07:
    /// npm's own copy of itself in `bin/npm` and `bin/npx`, `bin/node` and
    /// `bin/corepack` free.
    fn node_22_unlinked_with_npms_own(prefix: &Path, name: &str) -> Option<KegLinks> {
        assert_eq!((prefix, name), (Path::new("/opt/homebrew"), "node@22"));
        let command = |name: &str, place| links::CommandLink {
            name: name.to_string(),
            path: prefix.join("bin").join(name),
            place,
        };
        Some(KegLinks {
            recorded: false,
            commands: vec![
                command("corepack", links::Place::Free),
                command("node", links::Place::Free),
                command("npm", links::Place::Taken),
                command("npx", links::Place::Taken),
            ],
        })
    }

    #[tokio::test]
    async fn test_a_link_plans_brew_link_formula_force_and_says_what_is_in_the_way() {
        // Finding (1) of the 2026-10-07 run: the fix for a source that
        // could not find `node` is the link of `node@22`, previewed and
        // run as any operation is -- the same `brew link --formula --force`
        // that links a keg-only formula back after its update (y1-keg).
        // Nothing runs to plan it.
        let runner = Arc::new(MockRunner::new());
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Link,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "node@22".into(),
        };
        let keg_only = || BrewAdapter::new(runner.clone()).with_keg_only(&inst.id, &["node@22"]);
        let plan = keg_only()
            .plan(&inst, &req)
            .await
            .expect("a link is planned");
        assert_eq!(
            crate::testing::command_program(&plan),
            inst.exe_path.as_path()
        );
        assert_eq!(
            command_args(&plan),
            ["link", "--formula", "--force", "node@22"]
        );
        assert_eq!(
            command_env(&plan),
            BrewAdapter::new(runner.clone()).env_vec()
        );
        assert_eq!(plan.timeout_secs, BrewAdapter::LINK_TIMEOUT_SECS);
        assert!(!plan.needs_password);
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);
        assert_eq!(plan.locks, vec![ResourceLock(inst.id.clone())]);
        // Links that cannot be read: nothing said, Homebrew's own refusal
        // says what it meets.
        assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
        assert!(plan.affected.is_empty());
        assert!(runner.calls().is_empty(), "planning runs nothing");

        // The formula's commands, which the link puts where Terminal
        // looks; npm's own `npm` and `npx` in `bin`: Homebrew would link
        // nothing (`KegLinks::held_paths`).
        let plan = keg_only()
            .with_links_fn(node_22_unlinked_with_npms_own)
            .plan(&inst, &req)
            .await
            .expect("still planned, with what is in the way");
        assert_eq!(
            plan.warnings,
            vec![
                Warning::LinkPutsCommands {
                    names: ["corepack", "node", "npm", "npx"]
                        .map(String::from)
                        .to_vec(),
                },
                Warning::LinkConflicts {
                    paths: vec![
                        "/opt/homebrew/bin/npm".to_string(),
                        "/opt/homebrew/bin/npx".to_string(),
                    ],
                },
            ]
        );

        // Only a formula is linked.
        let cask = OpRequest {
            artifact_kind: ArtifactKind::Cask,
            ..req.clone()
        };
        assert!(matches!(
            keg_only().plan(&inst, &cask).await,
            Err(AdapterError::Unsupported(_))
        ));
        // ... and only one the last inventory listed as keg-only and
        // linkable (y1-keg's rule): not one keg-only because of macOS,
        // which `brew link` refuses at Homebrew's default prefix and exits
        // 0 having linked nothing, nor one before any inventory.
        let sqlite = OpRequest {
            name: "sqlite".into(),
            ..req.clone()
        };
        assert!(matches!(
            keg_only().plan(&inst, &sqlite).await,
            Err(AdapterError::Unsupported(_))
        ));
        assert!(matches!(
            BrewAdapter::new(runner.clone()).plan(&inst, &req).await,
            Err(AdapterError::Unsupported(_))
        ));
        assert!(runner.calls().is_empty());
    }

    #[tokio::test]
    async fn regression_brew_env_cannot_reenable_an_untracked_auto_update() {
        let runner = Arc::new(MockRunner::new());
        let safe = BrewAdapter::new(runner.clone());
        let blocked = BrewAdapter::new(runner.clone()).with_brew_env_fn(|path| {
            if path == Path::new(brew_env::SYSTEM_FILE) {
                brew_env::EnvFile::Read(b"HOMEBREW_NO_AUTO_UPDATE=\n".to_vec())
            } else {
                brew_env::EnvFile::Skipped
            }
        });
        let inst = test_instance();
        for kind in [OpKind::Install, OpKind::Upgrade] {
            let req = OpRequest {
                kind,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Formula,
                name: "jq".into(),
            };
            assert!(matches!(
                blocked.plan(&inst, &req).await,
                Err(AdapterError::Refused(_))
            ));
            let plan = safe.plan(&inst, &req).await.unwrap();
            assert!(matches!(
                blocked
                    .execute(&plan, Arc::new(VecSink::new()), 1, CancellationToken::new())
                    .await,
                Err(AdapterError::Refused(_))
            ));
        }
        assert!(matches!(
            blocked
                .run_brew(&inst, vec!["outdated".into()], Duration::from_secs(1))
                .await,
            Err(AdapterError::Refused(_))
        ));
        assert!(runner.calls().is_empty());
        // Shell non-empty values such as "0" still disable auto-update.
        let zero = BrewAdapter::new(runner)
            .with_brew_env_fn(|_| brew_env::EnvFile::Read(b"HOMEBREW_NO_AUTO_UPDATE=0\n".to_vec()));
        assert!(zero
            .require_no_auto_update(&inst.prefix, &zero.env_vec())
            .is_ok());
        // Opus review finding 7: a brew.env in a protected place, which
        // Banager does not read, is not refused, as z1 intended -- every
        // Homebrew check and update stopped for it. The check runs, and an
        // install's or upgrade's preview says Homebrew may update itself
        // first, in z1's protected-place words.
        let unknown = BrewAdapter::new(Arc::new(MockRunner::new()))
            .with_brew_env_fn(|_| brew_env::EnvFile::Unknown);
        assert!(unknown
            .require_no_auto_update(&inst.prefix, &unknown.env_vec())
            .is_ok());
        for kind in [OpKind::Install, OpKind::Upgrade] {
            let req = OpRequest {
                kind,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Formula,
                name: "jq".into(),
            };
            let plan = unknown
                .plan(&inst, &req)
                .await
                .expect("planned, not refused");
            assert_eq!(plan.warnings.first(), Some(&Warning::HomebrewMayAutoUpdate));
        }
        // No brew.env at all: nothing said.
        let none = BrewAdapter::new(Arc::new(MockRunner::new()))
            .with_brew_env_fn(|_| brew_env::EnvFile::Skipped);
        let req = OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".into(),
        };
        let plan = none.plan(&inst, &req).await.unwrap();
        assert!(!plan.warnings.contains(&Warning::HomebrewMayAutoUpdate));
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
        assert_eq!(command_args(&plan), vec!["install", "--formula", "jq"]);
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
        assert_eq!(command_args(&plan), vec!["install", "--cask", "claudebar"]);
        assert!(plan.needs_password);
    }

    #[tokio::test]
    async fn test_plan_uninstall_with_dependents_warns() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "jq"],
            CommandOutput {
                stderr_cause: Default::default(),
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
        assert_eq!(command_args(&plan), vec!["uninstall", "--formula", "jq"]);
        assert_eq!(plan.affected, vec!["python@3.13".to_string()]);
        assert_eq!(
            plan.warnings,
            vec![
                Warning::UninstallScope {
                    what: UninstallScope::HomebrewFormulaOnly
                },
                Warning::WouldBreak {
                    names: vec!["python@3.13".to_string()]
                }
            ]
        );
    }

    /// A Homebrew whose `brew update` succeeds, whose `brew outdated` lists
    /// nothing, whose `brew info --installed` lists `jq` alone, and whose
    /// `brew uses --installed oniguruma` names nothing: what Homebrew 7
    /// answers when `speedtest`, from a tap it does not trust, is in the
    /// Cellar (`Formula.installed` drops it from all three, r18 R46-1).
    fn runner_listing_jq_alone() -> Arc<MockRunner> {
        let ok = |stdout: &str| CommandOutput {
            stderr_cause: Default::default(),
            exit_code: Some(0),
            stdout: stdout.to_string(),
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        };
        let runner = Arc::new(MockRunner::new());
        runner.respond(vec!["/opt/homebrew/bin/brew", "update"], ok(""));
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            ok(r#"{"formulae":[],"casks":[]}"#),
        );
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "info", "--installed", "--json=v2"],
            ok(r#"{"formulae":[{"name":"jq","linked_keg":"1.8.2","installed":[{"version":"1.8.2","installed_on_request":true}]},{"name":"oniguruma","linked_keg":"6.9.10","installed":[{"version":"6.9.10","installed_on_request":false}]}],"casks":[]}"#),
        );
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "oniguruma"],
            ok(""),
        );
        runner
    }

    /// Each formula folder of the Cellar `runner_listing_jq_alone` stands
    /// for has a version in it but `empty`, which has none.
    fn kegs_but_in_empty(_: &Path, name: &str) -> Option<Kegs> {
        Some(Kegs {
            versions: if name == "empty" {
                Vec::new()
            } else {
                vec!["1.0".to_string()]
            },
            pinned: false,
        })
    }

    fn uninstall_oniguruma(inst: &ManagerInstance) -> OpRequest {
        OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "oniguruma".to_string(),
        }
    }

    /// r18 R46-1: a formula in the Cellar that `brew info --installed` did
    /// not list makes the check say Homebrew left some out, and an
    /// uninstall preview say what needs the formula is not known -- even
    /// with `brew uses --installed` naming nothing, as it names nothing of
    /// what Homebrew dropped.
    #[tokio::test]
    async fn a_formula_homebrew_did_not_list_is_noted_and_leaves_dependents_unknown() {
        let adapter = BrewAdapter::new(runner_listing_jq_alone())
            .with_racks_fn(|_| {
                Some(vec![
                    "jq".to_string(),
                    "oniguruma".to_string(),
                    "speedtest".to_string(),
                ])
            })
            .with_kegs_fn(kegs_but_in_empty);
        let inst = test_instance();
        let outcome = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert_eq!(outcome.notes, [InstanceNote::FormulaeNotListed]);
        let plan = adapter
            .plan(&inst, &uninstall_oniguruma(&inst))
            .await
            .expect("plan");
        assert!(plan.affected.is_empty());
        assert!(
            plan.warnings.contains(&Warning::DependentsUnknown),
            "got {:?}",
            plan.warnings
        );
    }

    /// A Cellar whose every formula folder with a version Homebrew listed
    /// -- a folder with none it does not list either (`Formula.racks`) --
    /// or that could not be read, gives no note, and the preview trusts
    /// `brew uses --installed` as before.
    #[tokio::test]
    async fn a_cellar_homebrew_listed_in_full_or_could_not_be_read_changes_nothing() {
        for racks in [
            (|_: &Path| {
                Some(vec![
                    "jq".to_string(),
                    "oniguruma".to_string(),
                    "empty".to_string(),
                ])
            }) as fn(&Path) -> Option<Vec<String>>,
            |_: &Path| None,
        ] {
            let adapter = BrewAdapter::new(runner_listing_jq_alone())
                .with_racks_fn(racks)
                .with_kegs_fn(kegs_but_in_empty);
            let inst = test_instance();
            let outcome = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("check_updates");
            assert!(outcome.notes.is_empty(), "got {:?}", outcome.notes);
            let plan = adapter
                .plan(&inst, &uninstall_oniguruma(&inst))
                .await
                .expect("plan");
            assert!(
                !plan.warnings.contains(&Warning::DependentsUnknown),
                "got {:?}",
                plan.warnings
            );
        }
    }

    /// r18 R46-3: a formula `brew services start` set up to run in the
    /// background keeps running after `brew uninstall`, which neither
    /// stops it nor removes its service file, so its uninstall preview
    /// says so with the command that stops it.
    #[tokio::test]
    async fn a_formulas_uninstall_says_its_background_service_stays() {
        fn ollama_service(home: Option<&Path>, name: &str) -> Option<Service> {
            assert_eq!(home, Some(Path::new("/Users/someone")));
            (name == "ollama").then_some(Service { system: false })
        }
        let runner = Arc::new(MockRunner::new());
        for name in ["ollama", "jq"] {
            runner.respond(
                vec!["/opt/homebrew/bin/brew", "uses", "--installed", name],
                CommandOutput {
                    stderr_cause: Default::default(),
                    exit_code: Some(0),
                    stdout: String::new(),
                    stderr: String::new(),
                    timed_out: false,
                    cancelled: false,
                },
            );
        }
        let adapter = BrewAdapter::new(runner)
            .with_env_var_fn(|name| (name == "HOME").then(|| OsString::from("/Users/someone")))
            .with_service_fn(ollama_service);
        let inst = test_instance();
        let uninstall = |name: &str| OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: name.to_string(),
        };
        let ollama = adapter
            .plan(&inst, &uninstall("ollama"))
            .await
            .expect("plan");
        assert!(
            ollama.warnings.contains(&Warning::HomebrewServiceStays {
                name: "ollama".to_string(),
                system: false,
            }),
            "got {:?}",
            ollama.warnings
        );
        let jq = adapter.plan(&inst, &uninstall("jq")).await.expect("plan");
        assert!(
            !jq.warnings
                .iter()
                .any(|w| matches!(w, Warning::HomebrewServiceStays { .. })),
            "got {:?}",
            jq.warnings
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
                stderr_cause: Default::default(),
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
    async fn test_plan_uninstall_without_dependents_says_only_what_goes() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "jq"],
            CommandOutput {
                stderr_cause: Default::default(),
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
        assert_eq!(command_args(&plan), vec!["uninstall", "--formula", "jq"]);
        assert!(plan.affected.is_empty());
        // Nothing to warn of: only the sentence under the tool.
        assert_eq!(
            plan.warnings,
            vec![Warning::UninstallScope {
                what: UninstallScope::HomebrewFormulaOnly
            }]
        );
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
        assert_eq!(command_args(&plan), vec!["uninstall", "--cask", "docker"]);
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
        assert_eq!(command_args(&plan), vec!["upgrade", "--formula", "jq"]);
        assert!(!plan.needs_password);
    }

    /// A Homebrew whose `brew info --installed` lists `node`, with bottles
    /// for Apple silicon on Sequoia and Tahoe only, as Homebrew 7 builds
    /// them, and `jq`, with one bottle for every Mac.
    fn runner_with_node_bottled_for_apple_silicon_only() -> Arc<MockRunner> {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "info", "--installed", "--json=v2"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: r#"{"formulae":[{"name":"node","linked_keg":"25.1.0","installed":[{"version":"25.1.0","installed_on_request":true}],"bottle":{"stable":{"files":{"arm64_tahoe":{},"arm64_sequoia":{}}}}},{"name":"jq","linked_keg":"1.8.2","installed":[{"version":"1.8.2","installed_on_request":true}],"bottle":{"stable":{"files":{"all":{}}}}}],"casks":[]}"#.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner
    }

    fn upgrade_formula(inst: &ManagerInstance, name: &str) -> OpRequest {
        OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: name.to_string(),
        }
    }

    /// r18 R46-2: on a Mac no bottle of a formula fits -- an Intel Mac, or
    /// Apple silicon on macOS 14 or older -- its update compiles: the
    /// preview says so, as Cargo's does, and the update is given hours,
    /// not the half hour that stops a long build every time.
    #[tokio::test]
    async fn an_update_no_bottle_fits_says_it_compiles_and_gets_hours() {
        for mac in [
            (|_: &Path| {
                Some(MacTag {
                    arm: false,
                    macos: (26, 0),
                })
            }) as fn(&Path) -> Option<MacTag>,
            |_: &Path| {
                Some(MacTag {
                    arm: true,
                    macos: (14, 7),
                })
            },
        ] {
            let adapter = BrewAdapter::new(runner_with_node_bottled_for_apple_silicon_only())
                .with_mac_tag_fn(mac);
            let inst = test_instance();
            adapter.inventory(&inst).await.expect("inventory");
            let node = adapter
                .plan(&inst, &upgrade_formula(&inst, "node"))
                .await
                .expect("plan");
            assert_eq!(node.warnings, [Warning::CompilesLocally]);
            assert_eq!(node.timeout_secs, BrewAdapter::SOURCE_BUILD_TIMEOUT_SECS);
            let jq = adapter
                .plan(&inst, &upgrade_formula(&inst, "jq"))
                .await
                .expect("plan");
            assert!(jq.warnings.is_empty(), "got {:?}", jq.warnings);
            assert_eq!(jq.timeout_secs, 1800);
        }
    }

    /// Where a bottle fits, or the Mac's tag is not known, the update is
    /// planned as before.
    #[tokio::test]
    async fn an_update_a_bottle_fits_or_on_an_unknown_mac_is_planned_as_before() {
        for mac in [
            (|_: &Path| {
                Some(MacTag {
                    arm: true,
                    macos: (26, 1),
                })
            }) as fn(&Path) -> Option<MacTag>,
            |_: &Path| None,
        ] {
            let adapter = BrewAdapter::new(runner_with_node_bottled_for_apple_silicon_only())
                .with_mac_tag_fn(mac);
            let inst = test_instance();
            adapter.inventory(&inst).await.expect("inventory");
            let node = adapter
                .plan(&inst, &upgrade_formula(&inst, "node"))
                .await
                .expect("plan");
            assert!(node.warnings.is_empty(), "got {:?}", node.warnings);
            assert_eq!(node.timeout_secs, 1800);
        }
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
        assert_eq!(command_args(&plan), vec!["upgrade", "--cask", "docker"]);
        assert!(plan.needs_password);
    }

    /// Homebrew's `--zap` removes everything a cask's zap stanza names --
    /// for `claude-code` that is the *native* install's `~/.local/bin/claude`
    /// and `~/.local/share/claude`, and the shared `~/.claude` -- and
    /// `--force` and `--ignore-dependencies` override refusals Homebrew makes
    /// on the user's behalf. `docs/what-we-run.md` says Banager never passes
    /// them, but for the one `--force` the author's decision U9 (r6) asks
    /// for -- an uninstall of a formula with more than one version
    /// installed and no pin, to delete every version
    /// (`old_versions::an_uninstall_of_a_formula_deletes_every_installed_version`,
    /// `an_uninstall_passes_no_force_for_one_version_a_pinned_formula_or_a_cask`)
    /// -- and this is what keeps that sentence true when `plan` is next
    /// edited. Where no second version is read, every plan brew builds, for
    /// both artifact kinds, is exactly the verb, the kind flag and the name.
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
                    stderr_cause: Default::default(),
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
                        !command_args(&plan)
                            .iter()
                            .any(|arg| arg.as_str() == forbidden),
                        "brew {verb} {flag} {name} must never carry {forbidden}, got {:?}",
                        command_args(&plan)
                    );
                }
                assert_eq!(
                    command_args(&plan),
                    vec![verb, flag, name],
                    "brew {verb} {flag} {name} is exactly the verb, the kind flag and the name"
                );
            }
        }
    }

    /// Every plan brew builds -- jq, a formula, and docker, a cask, each to
    /// install, uninstall and upgrade -- by `adapter` on `test_instance()`,
    /// with `brew uses` answering that nothing depends on either.
    async fn every_plan(runner: &MockRunner, adapter: &BrewAdapter) -> Vec<Plan> {
        for name in ["jq", "docker"] {
            runner.respond(
                vec!["/opt/homebrew/bin/brew", "uses", "--installed", name],
                CommandOutput {
                    stderr_cause: Default::default(),
                    exit_code: Some(0),
                    stdout: String::new(),
                    stderr: String::new(),
                    timed_out: false,
                    cancelled: false,
                },
            );
        }
        let inst = test_instance();
        let mut plans = Vec::new();
        for (artifact_kind, name) in [
            (ArtifactKind::Formula, "jq"),
            (ArtifactKind::Cask, "docker"),
        ] {
            for kind in [OpKind::Install, OpKind::Uninstall, OpKind::Upgrade] {
                let req = OpRequest {
                    kind,
                    instance_id: inst.id.clone(),
                    artifact_kind,
                    name: name.to_string(),
                };
                plans.push(adapter.plan(&inst, &req).await.expect("plan"));
            }
        }
        plans
    }

    /// The sentence an uninstall of `artifact_kind` says under the tool
    /// when nothing is recorded for a cask (these tests' default) and
    /// `autoremoves` says whether a brew.env took autoremove back.
    fn scope_of(artifact_kind: ArtifactKind, autoremoves: bool) -> Warning {
        let what = match (artifact_kind, autoremoves) {
            (ArtifactKind::Cask, _) => UninstallScope::HomebrewCask,
            (_, false) => UninstallScope::HomebrewFormulaOnly,
            (_, true) => UninstallScope::HomebrewFormula,
        };
        Warning::UninstallScope { what }
    }

    /// Homebrew autoremoves after every `brew uninstall`, formula or cask,
    /// unless `HOMEBREW_NO_AUTOREMOVE` is set (`cmd/uninstall.rb:129-136`
    /// in Homebrew 7.0.6-70), and in the cleanup an install or upgrade
    /// runs (`cleanup.rb:471`): every plan's command says it is set, and
    /// the preview shows it with the rest of the environment.
    #[tokio::test]
    async fn test_every_plan_runs_brew_with_autoremove_off() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone());
        for plan in every_plan(&runner, &adapter).await {
            assert!(
                command_env(&plan)
                    .contains(&("HOMEBREW_NO_AUTOREMOVE".to_string(), "1".to_string())),
                "{:?} {:?} {} runs without HOMEBREW_NO_AUTOREMOVE=1: {:?}",
                plan.request.kind,
                plan.request.artifact_kind,
                plan.request.name,
                command_env(&plan)
            );
            // With no brew.env, nothing more happens: an uninstall says
            // only what goes -- for a formula, "only" this version -- and
            // an install or upgrade says nothing.
            let expected = match plan.request.kind {
                OpKind::Link => unreachable!("every_plan plans no link"),
                OpKind::Uninstall => vec![scope_of(plan.request.artifact_kind, false)],
                OpKind::Install | OpKind::Upgrade => vec![],
            };
            assert_eq!(
                plan.warnings, expected,
                "{:?} {}",
                plan.request.kind, plan.request.name
            );
        }
    }

    /// A Mac whose `/etc/homebrew/brew.env` turns autoremove back on.
    fn system_brew_env_autoremoves(path: &Path) -> brew_env::EnvFile {
        (path == Path::new("/etc/homebrew/brew.env"))
            .then(|| b"# set by an administrator\nHOMEBREW_NO_AUTOREMOVE=0\n".to_vec())
            .into()
    }

    #[tokio::test]
    async fn test_an_uninstall_plan_says_homebrew_will_autoremove_when_a_brew_env_turns_it_back_on()
    {
        // `bin/brew` exports the file's line over the plan's
        // `HOMEBREW_NO_AUTOREMOVE=1`, so after the uninstall Homebrew
        // removes what else nothing needs any more: formula and cask alike.
        // An install or upgrade autoremoves only in a cleanup, which
        // `HOMEBREW_NO_INSTALL_CLEANUP=1` still keeps from running.
        let runner = Arc::new(MockRunner::new());
        let adapter =
            BrewAdapter::new(runner.clone()).with_brew_env_fn(system_brew_env_autoremoves);
        for plan in every_plan(&runner, &adapter).await {
            // A formula's sentence loses its "only".
            let expected = match plan.request.kind {
                OpKind::Link => unreachable!("every_plan plans no link"),
                OpKind::Uninstall => vec![
                    scope_of(plan.request.artifact_kind, true),
                    Warning::HomebrewAutoremoves,
                ],
                OpKind::Install | OpKind::Upgrade => vec![],
            };
            assert_eq!(
                plan.warnings, expected,
                "{:?} {}",
                plan.request.kind, plan.request.name
            );
        }
    }

    #[tokio::test]
    async fn test_an_install_or_upgrade_plan_says_so_when_brew_env_turns_cleanup_and_autoremove_back_on(
    ) {
        // `brew upgrade` and `brew install` end in `Install.finish_installation`
        // (`cmd/upgrade.rb:363`, `cmd/install.rb:504`), which deletes the
        // older versions and old downloads of the package they name
        // (`cleanup.rb:361-389`), and whose periodic cleanup
        // (`cleanup.rb:431-445`) deletes those of every formula
        // (`cleanup.rb:448-473`) and autoremoves (`cleanup.rb:471`) -- once
        // neither of Banager's two variables holds.
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone()).with_brew_env_fn(|path| {
            (path == Path::new("/etc/homebrew/brew.env"))
                .then(|| b"HOMEBREW_NO_INSTALL_CLEANUP=\nHOMEBREW_NO_AUTOREMOVE=off\n".to_vec())
                .into()
        });
        for plan in every_plan(&runner, &adapter).await {
            let expected = match plan.request.kind {
                OpKind::Link => unreachable!("every_plan plans no link"),
                OpKind::Uninstall => vec![
                    scope_of(plan.request.artifact_kind, true),
                    Warning::HomebrewAutoremoves,
                ],
                OpKind::Install | OpKind::Upgrade => vec![
                    Warning::HomebrewPeriodicCleanup,
                    Warning::HomebrewCleanupAutoremoves,
                ],
            };
            assert_eq!(
                plan.warnings, expected,
                "{:?} {}",
                plan.request.kind, plan.request.name
            );
        }
    }

    #[tokio::test]
    async fn test_an_install_or_upgrade_plan_says_homebrew_cleans_up_when_brew_env_turns_only_its_cleanup_back_on(
    ) {
        // With `HOMEBREW_NO_INSTALL_CLEANUP` set to nothing and
        // `HOMEBREW_NO_AUTOREMOVE=1` still in force, every install or
        // upgrade deletes the older versions and old downloads of the
        // package it names (`Cleanup.install_clean!`,
        // `cleanup.rb:361-389`), and the periodic cleanup, when it is due,
        // those of every installed formula (`Cleanup#clean!`,
        // `cleanup.rb:448-473`), without its autoremove; an uninstall runs
        // no cleanup, so its plan says nothing more.
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone()).with_brew_env_fn(|path| {
            (path == Path::new("/etc/homebrew/brew.env"))
                .then(|| b"HOMEBREW_NO_INSTALL_CLEANUP=\n".to_vec())
                .into()
        });
        for plan in every_plan(&runner, &adapter).await {
            let expected = match plan.request.kind {
                OpKind::Link => unreachable!("every_plan plans no link"),
                OpKind::Uninstall => vec![scope_of(plan.request.artifact_kind, false)],
                OpKind::Install | OpKind::Upgrade => vec![Warning::HomebrewPeriodicCleanup],
            };
            assert_eq!(
                plan.warnings, expected,
                "{:?} {}",
                plan.request.kind, plan.request.name
            );
        }
    }

    #[tokio::test]
    async fn test_a_plan_says_what_homebrew_no_cleanup_formulae_leaves_out_of_the_cleanup_it_names()
    {
        // `Cleanup.skip_clean_formula?` (`cleanup.rb:409-415`) keeps the
        // listed formulae out of every clean-up, and `Cleanup.autoremove`
        // keeps them and what they need at run time (`:1051-1055`): said
        // right after the lines it leaves them out of, in the list's order.
        let names = vec!["python@3.13".to_string(), "node".to_string()];
        let except = |old_versions, autoremove| Warning::HomebrewNoCleanupFormulae {
            names: names.clone(),
            old_versions,
            autoremove,
        };
        type Files = fn(&Path) -> brew_env::EnvFile;
        let both: Files = |path| {
            (path == Path::new("/etc/homebrew/brew.env")).then(|| {
                b"HOMEBREW_NO_INSTALL_CLEANUP=\nHOMEBREW_NO_AUTOREMOVE=off\nHOMEBREW_NO_CLEANUP_FORMULAE=python@3.13,node\n"
                    .to_vec()
            })
            .into()
        };
        let cleanup_only: Files = |path| {
            (path == Path::new("/etc/homebrew/brew.env"))
                .then(|| {
                    b"HOMEBREW_NO_INSTALL_CLEANUP=\nHOMEBREW_NO_CLEANUP_FORMULAE=python@3.13,node\n"
                        .to_vec()
                })
                .into()
        };
        for (files, autoremoves) in [(both, true), (cleanup_only, false)] {
            let runner = Arc::new(MockRunner::new());
            let adapter = BrewAdapter::new(runner.clone()).with_brew_env_fn(files);
            for plan in every_plan(&runner, &adapter).await {
                let expected = match (plan.request.kind, autoremoves) {
                    (OpKind::Uninstall, true) => vec![
                        scope_of(plan.request.artifact_kind, true),
                        Warning::HomebrewAutoremoves,
                        except(false, true),
                    ],
                    (OpKind::Uninstall, false) => vec![scope_of(plan.request.artifact_kind, false)],
                    (_, true) => vec![
                        Warning::HomebrewPeriodicCleanup,
                        Warning::HomebrewCleanupAutoremoves,
                        except(true, true),
                    ],
                    (_, false) => vec![Warning::HomebrewPeriodicCleanup, except(true, false)],
                };
                assert_eq!(
                    plan.warnings, expected,
                    "{:?} {}",
                    plan.request.kind, plan.request.name
                );
            }
        }

        // With Banager's two variables standing, the list changes nothing:
        // there is no clean-up for it to leave anything out of.
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone()).with_brew_env_fn(|path| {
            (path == Path::new("/etc/homebrew/brew.env"))
                .then(|| b"HOMEBREW_NO_CLEANUP_FORMULAE=python@3.13\n".to_vec())
                .into()
        });
        for plan in every_plan(&runner, &adapter).await {
            assert!(
                !plan
                    .warnings
                    .iter()
                    .any(|w| matches!(w, Warning::HomebrewNoCleanupFormulae { .. })),
                "{:?} {}",
                plan.request.kind,
                plan.request.name
            );
        }
    }

    #[tokio::test]
    async fn test_uninstall_preview_finds_the_prefix_and_xdg_brew_env_files() {
        // The prefix's file is this instance's (`<prefix>/etc/homebrew`),
        // and the user's is under `XDG_CONFIG_HOME` when Banager's
        // environment sets it -- then `~/.homebrew/brew.env` is not read.
        fn off_at(path: &Path, at: &str) -> brew_env::EnvFile {
            (path == Path::new(at))
                .then(|| b"HOMEBREW_NO_AUTOREMOVE=false\n".to_vec())
                .into()
        }
        let with_xdg = |name: &str| match name {
            "HOME" => Some(OsString::from("/Users/someone")),
            "XDG_CONFIG_HOME" => Some(OsString::from("/Users/someone/.config")),
            _ => None,
        };
        let without_xdg = |name: &str| (name == "HOME").then(|| OsString::from("/Users/someone"));
        type Files = fn(&Path) -> brew_env::EnvFile;
        type Env = fn(&str) -> Option<OsString>;
        let cases: [(Files, Env, bool); 4] = [
            (
                |p| off_at(p, "/opt/homebrew/etc/homebrew/brew.env"),
                without_xdg,
                true,
            ),
            (
                |p| off_at(p, "/Users/someone/.config/homebrew/brew.env"),
                with_xdg,
                true,
            ),
            (
                |p| off_at(p, "/Users/someone/.homebrew/brew.env"),
                with_xdg,
                false,
            ),
            (
                |p| off_at(p, "/Users/someone/.homebrew/brew.env"),
                without_xdg,
                true,
            ),
        ];
        for (files, env, autoremoves) in cases {
            let runner = Arc::new(MockRunner::new());
            runner.respond(
                vec!["/opt/homebrew/bin/brew", "uses", "--installed", "jq"],
                CommandOutput {
                    stderr_cause: Default::default(),
                    exit_code: Some(0),
                    stdout: String::new(),
                    stderr: String::new(),
                    timed_out: false,
                    cancelled: false,
                },
            );
            let adapter = BrewAdapter::new(runner)
                .with_brew_env_fn(files)
                .with_env_var_fn(env);
            let inst = test_instance();
            let req = OpRequest {
                kind: OpKind::Uninstall,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Formula,
                name: "jq".to_string(),
            };
            let plan = adapter.plan(&inst, &req).await.expect("plan");
            assert_eq!(
                plan.warnings.contains(&Warning::HomebrewAutoremoves),
                autoremoves,
                "{:?}",
                plan.warnings
            );
        }
    }

    /// A Homebrew prefix of a test's own whose `Caskroom` holds each cask's
    /// install receipt beside a saved caskfile of `{}`, as Homebrew 7 leaves
    /// them (`cask_receipt`). Removed when dropped.
    struct CaskroomPrefix(PathBuf);

    impl CaskroomPrefix {
        fn new(label: &str, receipts: &[(&str, &str)]) -> CaskroomPrefix {
            // A count of its own as well as the clock: two tests that ask
            // for the same label within one tick of the clock, as parallel
            // tests can, would otherwise share a folder and each other's
            // receipts.
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "banager-brew-caskroom-{label}-{}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            for (token, receipt) in receipts {
                let metadata = dir.join("Caskroom").join(token).join(".metadata");
                let casks = metadata
                    .join("1.0")
                    .join("20260928000000.000")
                    .join("Casks");
                std::fs::create_dir_all(&casks).expect("create the Caskroom");
                std::fs::write(metadata.join("INSTALL_RECEIPT.json"), receipt).unwrap();
                std::fs::write(casks.join(format!("{token}.json")), "{}").unwrap();
            }
            CaskroomPrefix(dir)
        }
    }

    impl Drop for CaskroomPrefix {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// `adapter`'s uninstall plan for the cask `name` on `inst`, with
    /// `brew uses` answering that nothing depends on it.
    async fn cask_uninstall(
        runner: &MockRunner,
        adapter: &BrewAdapter,
        inst: &ManagerInstance,
        name: &str,
    ) -> Plan {
        runner.respond(
            vec![inst.exe_path.to_str().unwrap(), "uses", "--installed", name],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Cask,
            name: name.to_string(),
        };
        adapter.plan(inst, &req).await.expect("plan")
    }

    #[tokio::test]
    async fn test_plain_cask_preserves_quit_and_signal_warnings() {
        let receipts = [
            ("zed", r#"{"uninstall_artifacts":[{"app":["Zed.app"]},{"binary":["Zed.app/Contents/MacOS/cli",{"target":"zed"}]},{"uninstall":[{"quit":"dev.zed.Zed"}]}]}"#),
            ("dbeaver-community", include_str!("../../../../../adapters/fixtures-derived/brew/7.0.6/receipts/dbeaver-community.json")),
        ];
        let prefix = CaskroomPrefix::new("plain-running-apps", &receipts);
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone())
            .with_recorded_uninstall_fn(cask_receipt::read_recorded)
            .with_env_var_fn(someones_home)
            .with_app_bundle_id_fn(|_| None);
        let inst = ManagerInstance {
            prefix: prefix.0.clone(),
            exe_path: prefix.0.join("bin/brew"),
            ..test_instance()
        };
        for (name, expected_step) in [("zed", "QuitsApps"), ("dbeaver-community", "SignalsApps")] {
            let plan = cask_uninstall(&runner, &adapter, &inst, name).await;
            assert!(plan.warnings.contains(&Warning::UninstallScope {
                what: UninstallScope::HomebrewCaskPlain
            }));
            assert!(
                plan.warnings.iter().any(|warning| {
                    let json = serde_json::to_value(warning).unwrap();
                    json["CaskUninstallStep"]["step"] == expected_step
                }),
                "{name}: {:?}",
                plan.warnings
            );
        }
    }

    #[tokio::test]
    async fn test_cask_uninstall_refuses_npm_link_and_rechecks_before_dispatch() {
        use std::os::unix::fs::symlink;
        let prefix = CaskroomPrefix::new(
            "foreign-link",
            &[(
                "codex",
                r#"{"uninstall_artifacts":[{"binary":["codex-aarch64-apple-darwin",{"target":"codex"}]}]}"#,
            )],
        );
        let bin = prefix.0.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let own = prefix
            .0
            .join("Caskroom/codex/1.0/codex-aarch64-apple-darwin");
        std::fs::create_dir_all(own.parent().unwrap()).unwrap();
        std::fs::write(&own, "cask binary").unwrap();
        let foreign = prefix.0.join("lib/node_modules/@openai/codex/bin/codex.js");
        std::fs::create_dir_all(foreign.parent().unwrap()).unwrap();
        std::fs::write(&foreign, "npm binary").unwrap();
        let link = bin.join("codex");
        symlink(&foreign, &link).unwrap();
        let runner = Arc::new(MockRunner::new());
        let mut adapter = BrewAdapter::new(runner.clone())
            .with_recorded_uninstall_fn(cask_receipt::read_recorded);
        adapter.inspect_cask_links = true;
        let inst = ManagerInstance {
            prefix: prefix.0.clone(),
            exe_path: bin.join("brew"),
            ..test_instance()
        };
        runner.respond(
            vec![
                inst.exe_path.to_str().unwrap(),
                "uses",
                "--installed",
                "codex",
            ],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                stderr_cause: Default::default(),
                timed_out: false,
                cancelled: false,
            },
        );
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Cask,
            name: "codex".into(),
        };
        assert!(matches!(
            adapter.plan(&inst, &req).await,
            Err(AdapterError::UninstallUnsafe { .. })
        ));
        std::fs::remove_file(&link).unwrap();
        symlink(&own, &link).unwrap();
        let plan = adapter.plan(&inst, &req).await.unwrap();
        std::fs::remove_file(&link).unwrap();
        symlink(&foreign, &link).unwrap();
        let outcome = adapter
            .execute(
                &plan,
                Arc::new(VecSink::new()),
                901,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(matches!(
            outcome,
            Outcome::BanagerFailed(Fault::PathChanged { .. })
        ));
        assert!(runner
            .calls()
            .iter()
            .all(|argv| !argv.iter().any(|arg| arg == "uninstall")));
        assert_eq!(std::fs::read_link(&link).unwrap(), foreign);

        // A recorded command_wrapper also owns a command path. Replacing
        // its link with npm's must not bypass the binary-link check.
        let receipt: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../adapters/fixtures/brew/7.0.6/receipts/libreoffice.json"
        ))
        .unwrap();
        let wrapper_record = Recorded {
            artifacts: receipt["uninstall_artifacts"].as_array().unwrap().clone(),
            ..Default::default()
        };
        let wrapper = bin.join("soffice");
        symlink(&foreign, &wrapper).unwrap();
        assert_eq!(
            cask_links::conflict(
                &prefix.0,
                "libreoffice",
                &wrapper_record,
                &prefix.0,
                &prefix.0.join("Applications")
            ),
            Some(wrapper)
        );
    }

    /// A cask installed before Homebrew recorded its uninstall artifacts
    /// (a receipt with no `uninstall_artifacts`, a caskfile with no
    /// `artifacts`): nothing recorded names a link, so none is held
    /// against the uninstall, which goes ahead with the general sentence
    /// as it did before links were checked -- even with its command now
    /// npm's, which only a record could tell.
    #[tokio::test]
    async fn test_a_cask_with_no_record_of_its_links_still_uninstalls() {
        use std::os::unix::fs::symlink;
        let prefix = CaskroomPrefix::new(
            "no-record",
            &[(
                "google-chrome",
                r#"{"homebrew_version":"3.6.0","source":{"tap":"homebrew/cask"}}"#,
            )],
        );
        let npm = prefix
            .0
            .join("lib/node_modules/chrome/bin/google-chrome.js");
        std::fs::create_dir_all(npm.parent().unwrap()).unwrap();
        std::fs::write(&npm, "npm").unwrap();
        std::fs::create_dir_all(prefix.0.join("bin")).unwrap();
        symlink(&npm, prefix.0.join("bin/google-chrome")).unwrap();
        let runner = Arc::new(MockRunner::new());
        let mut adapter = BrewAdapter::new(runner.clone())
            .with_recorded_uninstall_fn(cask_receipt::read_recorded);
        adapter.inspect_cask_links = true;
        // Its own brew, so that what the run reads again before the
        // uninstall (`require_uninstall_as_previewed`, from the program's
        // prefix) is this Caskroom too, never the Mac's own Homebrew.
        let brew = prefix.0.join("bin/brew");
        let inst = ManagerInstance {
            prefix: prefix.0.clone(),
            exe_path: brew.clone(),
            ..test_instance()
        };
        assert_eq!(
            cask_receipt::read_recorded(&prefix.0, "google-chrome"),
            None
        );
        let plan = cask_uninstall(&runner, &adapter, &inst, "google-chrome").await;
        assert!(plan.warnings.contains(&Warning::UninstallScope {
            what: UninstallScope::HomebrewCask
        }));
        // The mock runner answers the uninstall: nothing real runs.
        let uninstall = vec![
            brew.to_str().unwrap(),
            "uninstall",
            "--cask",
            "google-chrome",
        ];
        runner.respond(
            uninstall.clone(),
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let outcome = adapter
            .execute(
                &plan,
                Arc::new(VecSink::new()),
                902,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(!matches!(outcome, Outcome::BanagerFailed(_)), "{outcome:?}");
        assert!(runner.calls().iter().any(|argv| *argv == uninstall));
    }

    /// r34 U1. Flutter's cask, recorded and laid out as Homebrew 7 leaves
    /// it (homebrew/cask 3.47.6, from the Homebrew API: `suite "flutter",
    /// target: "#{HOMEBREW_PREFIX}/share/flutter"`, `binary
    /// "flutter/bin/dart"`, `binary "flutter/bin/flutter"`): the suite
    /// moved to `<prefix>/share/flutter`, a link to it where it was staged,
    /// and each command linked to its staged path. Its uninstall is offered
    /// and runs -- it was refused as "may now belong to another tool" --
    /// and npm's link put at `bin/dart` after the preview still stops it
    /// before Homebrew starts.
    #[tokio::test]
    async fn test_flutters_cask_uninstalls_with_its_commands_through_its_moved_suite() {
        use std::os::unix::fs::symlink;
        let prefix = CaskroomPrefix::new("flutter", &[("flutter", "{}")]);
        let suite = prefix.0.join("share/flutter");
        let receipt = serde_json::json!({
            "homebrew_version": "7.0.8",
            "loaded_from_api": true,
            "source": { "tap": "homebrew/cask", "version": "3.47.6" },
            "uninstall_artifacts": [
                { "suite": ["flutter", { "target": suite.to_str().unwrap() }] },
                { "binary": ["flutter/bin/dart"] },
                { "binary": ["flutter/bin/flutter"] },
                { "zap": [{ "trash": "~/.flutter" }] }
            ]
        });
        std::fs::write(
            prefix
                .0
                .join("Caskroom/flutter/.metadata/INSTALL_RECEIPT.json"),
            receipt.to_string(),
        )
        .unwrap();
        std::fs::create_dir_all(suite.join("bin")).unwrap();
        let staged = prefix.0.join("Caskroom/flutter/3.47.6/flutter");
        std::fs::create_dir_all(staged.parent().unwrap()).unwrap();
        symlink(&suite, &staged).unwrap();
        let bin = prefix.0.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        for command in ["dart", "flutter"] {
            std::fs::write(suite.join("bin").join(command), "flutter").unwrap();
            symlink(staged.join("bin").join(command), bin.join(command)).unwrap();
        }
        let runner = Arc::new(MockRunner::new());
        let mut adapter = BrewAdapter::new(runner.clone())
            .with_recorded_uninstall_fn(cask_receipt::read_recorded)
            .with_env_var_fn(someones_home);
        adapter.inspect_cask_links = true;
        // Its own brew, so that what the run reads again is this prefix.
        let brew = bin.join("brew");
        let inst = ManagerInstance {
            prefix: prefix.0.clone(),
            exe_path: brew.clone(),
            ..test_instance()
        };
        let plan = cask_uninstall(&runner, &adapter, &inst, "flutter").await;
        assert!(plan.warnings.contains(&Warning::UninstallScope {
            what: UninstallScope::HomebrewCaskPlain
        }));
        let uninstall = vec![brew.to_str().unwrap(), "uninstall", "--cask", "flutter"];
        runner.respond(
            uninstall.clone(),
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let outcome = adapter
            .execute(
                &plan,
                Arc::new(VecSink::new()),
                903,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(
            runner
                .calls()
                .iter()
                .filter(|argv| **argv == uninstall)
                .count(),
            1
        );

        // npm's `dart` put over Flutter's after the preview: not started.
        let plan = cask_uninstall(&runner, &adapter, &inst, "flutter").await;
        let npm = prefix.0.join("lib/node_modules/dart/bin/dart.js");
        std::fs::create_dir_all(npm.parent().unwrap()).unwrap();
        std::fs::write(&npm, "npm").unwrap();
        std::fs::remove_file(bin.join("dart")).unwrap();
        symlink(&npm, bin.join("dart")).unwrap();
        let outcome = adapter
            .execute(
                &plan,
                Arc::new(VecSink::new()),
                904,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(
            outcome,
            Outcome::BanagerFailed(Fault::PathChanged {
                path: bin.join("dart").to_string_lossy().into_owned()
            })
        );
        assert_eq!(
            runner
                .calls()
                .iter()
                .filter(|argv| **argv == uninstall)
                .count(),
            1
        );
        // And no preview is offered with it there.
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Cask,
            name: "flutter".into(),
        };
        assert!(matches!(
            adapter.plan(&inst, &req).await,
            Err(AdapterError::UninstallUnsafe {
                reason: crate::model::UninstallUnsafeReason::CaskLinkNotOwned,
                ..
            })
        ));
    }

    /// The home folder the constructed receipts were built for
    /// (`adapters/fixtures-derived/brew/7.0.6/README.md`).
    fn someones_home(name: &str) -> Option<OsString> {
        (name == "HOME").then(|| OsString::from("/Users/someone"))
    }

    const CLAUDEBAR_RECEIPT: &str =
        include_str!("../../../../../adapters/fixtures/brew/7.0.6/receipts/claudebar.json");
    const WORD_RECEIPT: &str = include_str!(
        "../../../../../adapters/fixtures-derived/brew/7.0.6/receipts/microsoft-word.json"
    );
    const TWELITE_RECEIPT: &str = include_str!(
        "../../../../../adapters/fixtures-derived/brew/7.0.6/receipts/twelite-stage.json"
    );
    const PYCHARM_EDU_RECEIPT: &str = include_str!(
        "../../../../../adapters/fixtures-derived/brew/7.0.6/receipts/pycharm-edu.json"
    );
    const PLAYDATE_RECEIPT: &str = include_str!(
        "../../../../../adapters/fixtures-derived/brew/7.0.6/receipts/playdate-simulator.json"
    );

    /// Charles, where Homebrew puts it: `/Applications/Charles.app`, whose
    /// Info.plist says it is `com.xk72.Charles`.
    fn charles_in_applications(app: &Path) -> Option<String> {
        (app == Path::new("/Applications/Charles.app")).then(|| "com.xk72.Charles".to_string())
    }

    /// Charles, where the home folder's Applications keeps it.
    fn charles_in_home_applications(app: &Path) -> Option<String> {
        (app == Path::new("/Users/someone/Applications/Charles.app"))
            .then(|| "com.xk72.Charles".to_string())
    }

    #[tokio::test]
    async fn test_a_cask_uninstall_names_the_app_it_quits_where_it_found_it() {
        // Charles's record quits `com.xk72.Charles` and puts down
        // `Charles.app`. Found where Homebrew puts apps, with that bundle
        // id in its Info.plist, the line names it as Finder does --
        // 「还会退出正在运行的 Charles」 -- where it said the bundle id.
        let charles = include_str!(
            "../../../../../adapters/fixtures-derived/brew/7.0.6/receipts/charles.json"
        );
        let prefix = CaskroomPrefix::new(
            "quit-names",
            &[("charles", charles), ("microsoft-word", WORD_RECEIPT)],
        );
        let runner = Arc::new(MockRunner::new());
        let inst = ManagerInstance {
            prefix: prefix.0.clone(),
            exe_path: prefix.0.join("bin/brew"),
            ..test_instance()
        };
        let step = |step, items: &[&str]| Warning::CaskUninstallStep {
            step,
            items: items.iter().map(|item| item.to_string()).collect(),
            only_if: None,
        };
        let quits = |plan: &Plan| -> Vec<Warning> {
            plan.warnings
                .iter()
                .filter(|warning| {
                    matches!(
                        warning,
                        Warning::CaskUninstallStep {
                            step: CaskStep::QuitsApps | CaskStep::QuitsNamedApps,
                            ..
                        }
                    )
                })
                .cloned()
                .collect()
        };

        for found in [charles_in_applications, charles_in_home_applications] {
            let adapter = BrewAdapter::new(runner.clone())
                .with_recorded_uninstall_fn(cask_receipt::read_recorded)
                .with_env_var_fn(someones_home)
                .with_app_bundle_id_fn(found);
            let plan = cask_uninstall(&runner, &adapter, &inst, "charles").await;
            assert_eq!(
                quits(&plan),
                vec![step(CaskStep::QuitsNamedApps, &["Charles"])]
            );
            // Its other lines are as the record has them.
            assert!(plan.warnings.contains(&step(
                CaskStep::RemovesServices,
                &["com.xk72.Charles.ProxyHelper"]
            )));

            // Word's record puts down no app for `com.microsoft.autoupdate2`:
            // nothing to name it by, so its line counts it.
            let plan = cask_uninstall(&runner, &adapter, &inst, "microsoft-word").await;
            assert_eq!(
                quits(&plan),
                vec![step(CaskStep::QuitsApps, &["com.microsoft.autoupdate2"])]
            );
        }

        // Not on the disk, or another app under that name: the bundle id,
        // counted.
        let adapter = BrewAdapter::new(runner.clone())
            .with_recorded_uninstall_fn(cask_receipt::read_recorded)
            .with_env_var_fn(someones_home)
            .with_app_bundle_id_fn(|app| {
                (app == Path::new("/Applications/Charles.app"))
                    .then(|| "com.example.not-charles".to_string())
            });
        let plan = cask_uninstall(&runner, &adapter, &inst, "charles").await;
        assert_eq!(
            quits(&plan),
            vec![step(CaskStep::QuitsApps, &["com.xk72.Charles"])]
        );
    }

    #[tokio::test]
    async fn test_a_cask_update_says_the_old_versions_recorded_uninstall_steps_homebrew_runs_first()
    {
        // R47-1 (r18): `brew upgrade --cask` first runs the uninstall the
        // installed version recorded, every directive but `signal` unless
        // its `on_upgrade` names it (Homebrew 7.0.9
        // `cask/artifact/uninstall.rb:10`, `:38-53`), and opens again each
        // app its `quit:` quit (`abstract_uninstall.rb:91-127`,
        // `cask/upgrade.rb:342-366`). The update's confirmation says the
        // same lines the uninstall's does, after one that says why.
        let charles = include_str!(
            "../../../../../adapters/fixtures-derived/brew/7.0.6/receipts/charles.json"
        );
        let dbeaver = include_str!(
            "../../../../../adapters/fixtures-derived/brew/7.0.6/receipts/dbeaver-community.json"
        );
        let prefix = CaskroomPrefix::new(
            "update-steps",
            &[
                ("charles", charles),
                ("microsoft-word", WORD_RECEIPT),
                ("dbeaver-community", dbeaver),
                ("claudebar", CLAUDEBAR_RECEIPT),
            ],
        );
        let runner = Arc::new(MockRunner::new());
        let inst = ManagerInstance {
            prefix: prefix.0.clone(),
            exe_path: prefix.0.join("bin/brew"),
            ..test_instance()
        };
        let adapter = BrewAdapter::new(runner.clone())
            .with_recorded_uninstall_fn(cask_receipt::read_recorded)
            .with_env_var_fn(someones_home)
            .with_app_bundle_id_fn(charles_in_applications);
        let steps_of = |plan: &Plan| -> Vec<Warning> {
            plan.warnings
                .iter()
                .filter(|warning| {
                    matches!(
                        warning,
                        Warning::CaskUninstallStep { .. } | Warning::CaskUpdateRunsOldSteps { .. }
                    )
                })
                .cloned()
                .collect()
        };
        let update = |name: &str| OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Cask,
            name: name.to_string(),
        };

        for name in ["charles", "microsoft-word", "claudebar"] {
            let uninstall = cask_uninstall(&runner, &adapter, &inst, name).await;
            let plan = adapter.plan(&inst, &update(name)).await.expect("plan");
            let mut expected = vec![Warning::CaskUpdateRunsOldSteps { reopens: true }];
            expected.extend(steps_of(&uninstall));
            assert_eq!(steps_of(&plan), expected, "{name}");
        }
        // Charles is quit by name, and opened again after.
        let plan = adapter.plan(&inst, &update("charles")).await.expect("plan");
        assert!(plan.warnings.contains(&Warning::CaskUninstallStep {
            step: CaskStep::QuitsNamedApps,
            items: vec!["Charles".to_string()],
            only_if: None,
        }));

        // DBeaver's record only signals its app, which an update skips:
        // nothing to say.
        let plan = adapter
            .plan(&inst, &update("dbeaver-community"))
            .await
            .expect("plan");
        assert_eq!(steps_of(&plan), Vec::<Warning>::new());
    }

    #[tokio::test]
    async fn test_an_uninstall_says_homebrew_deletes_the_trust_list_entry_it_holds_for_it_alone() {
        // `brew uninstall` deletes the trust list's entry for each package
        // it names whose tap is not on the list (`cmd/uninstall.rb:122-127`):
        // a cask by its full name, a formula by its tap and name.
        fn list(home: &Path) -> Option<TrustList> {
            (home == Path::new("/Users/someone/.homebrew")).then(|| TrustList {
                casks: vec!["someone/tap/thing".to_string()],
                formulae: vec!["homebrew/core/jq".to_string()],
                ..TrustList::default()
            })
        }
        let runner = Arc::new(MockRunner::new());
        let inst = test_instance();
        let adapter = BrewAdapter::new(runner.clone())
            .with_env_var_fn(someones_home)
            .with_trust_list_fn(list);
        let forgets = |name: &str| Warning::HomebrewForgetsTrust {
            name: name.to_string(),
        };
        let plan = cask_uninstall(&runner, &adapter, &inst, "someone/tap/thing").await;
        assert_eq!(
            plan.warnings,
            vec![
                Warning::UninstallScope {
                    what: UninstallScope::HomebrewCask
                },
                forgets("someone/tap/thing"),
            ]
        );
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "jq"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert!(plan.warnings.contains(&forgets("homebrew/core/jq")));

        // Another cask, and the same cask with no list to read: nothing.
        let plan = cask_uninstall(&runner, &adapter, &inst, "someone/tap/other").await;
        assert!(!plan
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::HomebrewForgetsTrust { .. })));
        let unread = BrewAdapter::new(runner.clone())
            .with_env_var_fn(someones_home)
            .with_trust_list_fn(|_| None);
        let plan = cask_uninstall(&runner, &unread, &inst, "someone/tap/thing").await;
        assert!(!plan
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::HomebrewForgetsTrust { .. })));
    }

    /// A record of `artifacts` (JSON), as `read_recorded` reads one.
    fn record(
        artifacts: serde_json::Value,
        flight_blocks: bool,
        ruby: bool,
        tap: &str,
    ) -> Recorded {
        Recorded {
            artifacts: artifacts.as_array().expect("a list").clone(),
            flight_blocks,
            ruby,
            tap: Some(tap.to_string()),
        }
    }

    #[tokio::test]
    async fn test_a_cask_uninstall_says_less_where_homebrew_may_not_run_what_it_recorded() {
        // The sentence under a cask, by its record's caskfile (JSON or Ruby),
        // its tap, Homebrew's trust list and HOMEBREW_NO_REQUIRE_TAP_TRUST.
        fn plain_from_a_tap(_: &Path, _: &str) -> Option<Recorded> {
            Some(record(
                serde_json::json!([{ "app": ["Thing.app"] }]),
                false,
                false,
                "someone/tap",
            ))
        }
        fn plain_from_homebrew(_: &Path, _: &str) -> Option<Recorded> {
            Some(record(
                serde_json::json!([{ "app": ["Thing.app"] }]),
                false,
                false,
                "homebrew/cask",
            ))
        }
        fn plain_ruby_from_a_tap(_: &Path, _: &str) -> Option<Recorded> {
            Some(record(
                serde_json::json!([{ "app": ["Thing.app"] }]),
                false,
                true,
                "someone/tap",
            ))
        }
        fn plain_ruby_from_homebrew(_: &Path, _: &str) -> Option<Recorded> {
            Some(record(
                serde_json::json!([{ "app": ["Thing.app"] }]),
                false,
                true,
                "homebrew/cask",
            ))
        }
        fn ruby_steps(tap: &str) -> Option<Recorded> {
            Some(record(
                serde_json::json!([{ "app": ["Thing.app"] }, { "uninstall": [{ "delete": "/Library/Thing" }] }]),
                true,
                true,
                tap,
            ))
        }
        fn ruby_steps_from_a_tap(_: &Path, _: &str) -> Option<Recorded> {
            ruby_steps("someone/tap")
        }
        fn ruby_steps_from_homebrew(_: &Path, _: &str) -> Option<Recorded> {
            ruby_steps("homebrew/cask")
        }
        fn ruby_only_steps_from_a_tap(_: &Path, _: &str) -> Option<Recorded> {
            Some(record(
                serde_json::json!([{ "uninstall": [{ "pkgutil": "com.someone.thing" }] }]),
                true,
                true,
                "someone/tap",
            ))
        }
        fn trusts_the_cask(_: &Path) -> Option<TrustList> {
            Some(TrustList {
                casks: vec!["someone/tap/thing".to_string()],
                ..TrustList::default()
            })
        }
        fn trusts_the_tap(_: &Path) -> Option<TrustList> {
            Some(TrustList {
                taps: vec!["someone/tap".to_string()],
                ..TrustList::default()
            })
        }
        fn no_trust_required(path: &Path) -> brew_env::EnvFile {
            (path == Path::new("/etc/homebrew/brew.env"))
                .then(|| b"HOMEBREW_NO_REQUIRE_TAP_TRUST=1\n".to_vec())
                .into()
        }
        type Recorder = fn(&Path, &str) -> Option<Recorded>;
        type Lister = fn(&Path) -> Option<TrustList>;
        type Files = fn(&Path) -> brew_env::EnvFile;
        let empty: Lister = |_| Some(TrustList::default());
        let unread: Lister = |_| None;
        let none: Files = |_| brew_env::EnvFile::Skipped;
        let cases: [(Recorder, Lister, Files, UninstallScope); 13] = [
            // JSON records: what Homebrew records runs. A tap's plain cask
            // may have an installer beside what Homebrew placed.
            (
                plain_from_a_tap,
                empty,
                none,
                UninstallScope::HomebrewCaskPlainThirdParty,
            ),
            (
                plain_from_homebrew,
                empty,
                none,
                UninstallScope::HomebrewCaskPlain,
            ),
            // Ruby: what Homebrew cannot load, it may run as the cask is
            // defined today.
            (
                ruby_steps_from_homebrew,
                empty,
                none,
                UninstallScope::HomebrewCaskRuby,
            ),
            (
                ruby_steps_from_a_tap,
                trusts_the_cask,
                none,
                UninstallScope::HomebrewCaskRuby,
            ),
            (
                ruby_steps_from_a_tap,
                trusts_the_tap,
                none,
                UninstallScope::HomebrewCaskRuby,
            ),
            (
                ruby_steps_from_a_tap,
                empty,
                no_trust_required,
                UninstallScope::HomebrewCaskRuby,
            ),
            // Ruby from a tap Banager cannot see Homebrew trusts: the steps
            // run only if it does.
            (
                ruby_steps_from_a_tap,
                empty,
                none,
                UninstallScope::HomebrewCaskStepsIfTrusted,
            ),
            (
                ruby_steps_from_a_tap,
                unread,
                none,
                UninstallScope::HomebrewCaskStepsIfTrusted,
            ),
            (
                ruby_only_steps_from_a_tap,
                empty,
                none,
                UninstallScope::HomebrewCaskStepsOnlyIfTrusted,
            ),
            (
                ruby_only_steps_from_a_tap,
                trusts_the_cask,
                none,
                UninstallScope::HomebrewCaskStepsOnlyRuby,
            ),
            // A plain Ruby record: untrusted, only what Homebrew placed goes,
            // as a tap's plain cask says; trusted, or Homebrew's own (as a
            // Homebrew before 7 could save it), the files it placed, and the
            // current definition where Homebrew cannot read the record --
            // no steps, since the record lists none.
            (
                plain_ruby_from_a_tap,
                empty,
                none,
                UninstallScope::HomebrewCaskPlainThirdParty,
            ),
            (
                plain_ruby_from_a_tap,
                trusts_the_tap,
                none,
                UninstallScope::HomebrewCaskPlainRuby,
            ),
            (
                plain_ruby_from_homebrew,
                empty,
                none,
                UninstallScope::HomebrewCaskPlainRuby,
            ),
        ];
        let runner = Arc::new(MockRunner::new());
        let inst = test_instance();
        for (i, (recorder, lister, files, expected)) in cases.into_iter().enumerate() {
            let adapter = BrewAdapter::new(runner.clone())
                .with_recorded_uninstall_fn(recorder)
                .with_trust_list_fn(lister)
                .with_brew_env_fn(files)
                .with_env_var_fn(someones_home);
            let plan = cask_uninstall(&runner, &adapter, &inst, "someone/tap/thing").await;
            assert_eq!(
                plan.warnings.first(),
                Some(&Warning::UninstallScope { what: expected }),
                "case {i}"
            );
        }
    }

    /// A plain Ruby record from a tap Homebrew may not trust, with a
    /// `quit`: untrusted, Homebrew skips the `uninstall` stanza and removes
    /// only what it placed (`load_installed_caskfile!`, `cask/installer.rb`
    /// 7.0.7-9: `Artifact::Uninstall` is left out), so no app is said to
    /// quit; trusted, the record runs and the quit is said.
    #[tokio::test]
    async fn test_an_untrusted_plain_casks_quit_is_not_said() {
        fn quits_from_a_tap(_: &Path, _: &str) -> Option<Recorded> {
            Some(record(
                serde_json::json!([{ "app": ["Thing.app"] }, { "uninstall": [{ "quit": "com.someone.thing" }] }]),
                false,
                true,
                "someone/tap",
            ))
        }
        let runner = Arc::new(MockRunner::new());
        let inst = test_instance();
        let quits = |plan: &Plan| {
            plan.warnings.iter().any(|warning| {
                matches!(
                    warning,
                    Warning::CaskUninstallStep {
                        step: CaskStep::QuitsApps,
                        ..
                    }
                )
            })
        };
        for (trust, said) in [
            (
                (|_| Some(TrustList::default())) as fn(&Path) -> Option<TrustList>,
                false,
            ),
            (
                |_| {
                    Some(TrustList {
                        taps: vec!["someone/tap".to_string()],
                        ..TrustList::default()
                    })
                },
                true,
            ),
        ] {
            let adapter = BrewAdapter::new(runner.clone())
                .with_recorded_uninstall_fn(quits_from_a_tap)
                .with_trust_list_fn(trust)
                .with_brew_env_fn(|_| brew_env::EnvFile::Skipped)
                .with_env_var_fn(someones_home);
            let plan = cask_uninstall(&runner, &adapter, &inst, "someone/tap/thing").await;
            assert_eq!(quits(&plan), said, "{:?}", plan.warnings);
        }
    }

    #[tokio::test]
    async fn test_a_cask_uninstall_says_what_its_install_receipt_records() {
        // What `brew uninstall --cask` runs is what Homebrew recorded at
        // install, not what `brew info` says of the cask today: the plan
        // reads the receipt under the prefix it runs against.
        let prefix = CaskroomPrefix::new(
            "scope",
            &[
                ("claudebar", CLAUDEBAR_RECEIPT),
                ("microsoft-word", WORD_RECEIPT),
                ("twelite-stage", TWELITE_RECEIPT),
            ],
        );
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone())
            .with_recorded_uninstall_fn(cask_receipt::read_recorded)
            .with_env_var_fn(someones_home);
        let inst = ManagerInstance {
            prefix: prefix.0.clone(),
            exe_path: prefix.0.join("bin/brew"),
            ..test_instance()
        };
        let scope = |what| Warning::UninstallScope { what };
        let step = |step, items: &[&str]| Warning::CaskUninstallStep {
            step,
            items: items.iter().map(|item| item.to_string()).collect(),
            only_if: None,
        };

        // Recorded on this Mac: `quit`, `app`, `zap`. The tapped cask's
        // full name finds its Caskroom folder; from a tap that is not
        // Homebrew's own, the sentence leaves room for an installer's files.
        let plan = cask_uninstall(&runner, &adapter, &inst, "gautham-v/tap/claudebar").await;
        assert_eq!(
            plan.warnings,
            vec![
                scope(UninstallScope::HomebrewCaskPlainThirdParty),
                step(CaskStep::QuitsApps, &["com.gauthamv.claudebar"])
            ]
        );

        // One line per kind of extra step, in `CaskStep`'s order. Word
        // installs with a `pkg`, which the record leaves out: nothing but
        // those steps deletes what it put down.
        let plan = cask_uninstall(&runner, &adapter, &inst, "microsoft-word").await;
        let word = vec![
            scope(UninstallScope::HomebrewCaskStepsOnly),
            step(
                CaskStep::RemovesPackages,
                &[
                    "com.microsoft.package.Microsoft_Word.app",
                    "com.microsoft.pkg.licensing",
                ],
            ),
            step(
                CaskStep::RemovesServices,
                &["com.microsoft.office.licensingV2.helper"],
            ),
            step(CaskStep::QuitsApps, &["com.microsoft.autoupdate2"]),
        ];
        assert_eq!(plan.warnings, word);

        // An `artifact` Homebrew placed in the home folder.
        let plan = cask_uninstall(&runner, &adapter, &inst, "twelite-stage").await;
        assert_eq!(
            plan.warnings,
            vec![
                scope(UninstallScope::HomebrewCaskSteps),
                step(CaskStep::Deletes, &["~/MWSTAGE"]),
            ]
        );

        // Nothing recorded for it: says so rather than guess.
        let plan = cask_uninstall(&runner, &adapter, &inst, "docker").await;
        assert_eq!(plan.warnings, vec![scope(UninstallScope::HomebrewCask)]);

        // With a brew.env that brings autoremove back, that comes last.
        let adapter = BrewAdapter::new(runner.clone())
            .with_recorded_uninstall_fn(cask_receipt::read_recorded)
            .with_env_var_fn(someones_home)
            .with_brew_env_fn(system_brew_env_autoremoves);
        let plan = cask_uninstall(&runner, &adapter, &inst, "microsoft-word").await;
        assert_eq!(
            plan.warnings,
            word.into_iter()
                .chain([Warning::HomebrewAutoremoves])
                .collect::<Vec<_>>()
        );
        // And a cask whose files Homebrew placed no longer has "nothing else
        // is deleted" under it (`cmd/uninstall.rb:133-136`): only that its
        // own other files stay.
        let plan = cask_uninstall(&runner, &adapter, &inst, "twelite-stage").await;
        assert_eq!(
            plan.warnings,
            vec![
                scope(UninstallScope::HomebrewCaskStepsAutoremoves),
                step(CaskStep::Deletes, &["~/MWSTAGE"]),
                Warning::HomebrewAutoremoves,
            ]
        );
        // The plain sentence and the steps-only one claim nothing about
        // other packages, so they stay as they are.
        let plan = cask_uninstall(&runner, &adapter, &inst, "gautham-v/tap/claudebar").await;
        assert_eq!(
            plan.warnings,
            vec![
                scope(UninstallScope::HomebrewCaskPlainThirdParty),
                step(CaskStep::QuitsApps, &["com.gauthamv.claudebar"]),
                Warning::HomebrewAutoremoves,
            ]
        );
    }

    #[tokio::test]
    async fn test_a_cask_uninstall_whose_recorded_steps_remove_paths_says_they_go_for_good() {
        // An uninstall step of type `remove` deletes for good
        // (`install_steps.rb:1049-1070`): its paths are said with what
        // `delete:` names, and one Homebrew finds only when it runs the step
        // is said without a name -- each with the check the step makes of
        // a path before it deletes it, on a line of its own, when it
        // records one (`install_steps.rb:1051-1060`).
        let prefix = CaskroomPrefix::new(
            "remove",
            &[
                ("pycharm-edu", PYCHARM_EDU_RECEIPT),
                ("playdate-simulator", PLAYDATE_RECEIPT),
            ],
        );
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone())
            .with_recorded_uninstall_fn(cask_receipt::read_recorded)
            .with_env_var_fn(someones_home);
        let inst = ManagerInstance {
            prefix: prefix.0.clone(),
            exe_path: prefix.0.join("bin/brew"),
            ..test_instance()
        };
        let scope = |what| Warning::UninstallScope { what };
        let step =
            |step, only_if: Option<RemoveCheck>, items: &[&str]| Warning::CaskUninstallStep {
                step,
                items: items.iter().map(|item| item.to_string()).collect(),
                only_if,
            };

        // `charm` in each folder Homebrew looks for commands in, only where
        // it is a file whose contents hold that line.
        let plan = cask_uninstall(&runner, &adapter, &inst, "pycharm-edu").await;
        assert_eq!(
            plan.warnings,
            vec![
                scope(UninstallScope::HomebrewCaskSteps),
                step(
                    CaskStep::DeletesUnnamed,
                    Some(RemoveCheck::ContentContains(
                        "# see com.intellij.idea.SocketLock for the server side of this interface"
                            .to_string()
                    )),
                    &[]
                ),
            ]
        );

        // A `pkg` cask: only its steps delete what it put down. `delete:`
        // takes `/usr/local/playdate` whatever it is; the `remove` step
        // takes `/usr/local/bin/arm-*` only where it is a link whose target
        // contains `playdate`.
        let plan = cask_uninstall(&runner, &adapter, &inst, "playdate-simulator").await;
        assert_eq!(
            plan.warnings,
            vec![
                scope(UninstallScope::HomebrewCaskStepsOnly),
                step(CaskStep::Deletes, None, &["/usr/local/playdate"]),
                step(
                    CaskStep::Deletes,
                    Some(RemoveCheck::LinkTargetContains("playdate".to_string())),
                    &["/usr/local/bin/arm-*"]
                ),
                step(CaskStep::Trashes, None, &["~/Developer/PlaydateSDK"]),
                step(CaskStep::RemovesPackages, None, &["date.play.sdk"]),
            ]
        );
    }

    #[tokio::test]
    async fn test_a_cask_uninstall_says_only_the_steps_go_when_its_record_lists_nothing_homebrew_put_down(
    ) {
        // little-snitch@4 installs with `installer manual:`, which the
        // record leaves out (`cask/cask.rb:709-732`), so the sentence must
        // not say Homebrew deletes what it installed: only its one step,
        // `launchctl`, removes anything.
        const LITTLE_SNITCH_RECEIPT: &str = include_str!(
            "../../../../../adapters/fixtures-derived/brew/7.0.6/receipts/little-snitch@4.json"
        );
        let prefix = CaskroomPrefix::new(
            "steps-only",
            &[
                ("little-snitch@4", LITTLE_SNITCH_RECEIPT),
                ("nothing-recorded", CLAUDEBAR_RECEIPT),
            ],
        );
        // Homebrew saves `"artifacts": []` for a cask with nothing to
        // uninstall (`save_caskfile`, `cask/installer.rb:594-607`).
        std::fs::write(
            prefix.0.join(
                "Caskroom/nothing-recorded/.metadata/1.0/20260928000000.000/Casks/nothing-recorded.json",
            ),
            r#"{"artifacts": []}"#,
        )
        .unwrap();
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone())
            .with_recorded_uninstall_fn(cask_receipt::read_recorded)
            .with_env_var_fn(someones_home);
        let inst = ManagerInstance {
            prefix: prefix.0.clone(),
            exe_path: prefix.0.join("bin/brew"),
            ..test_instance()
        };

        let plan = cask_uninstall(&runner, &adapter, &inst, "little-snitch@4").await;
        assert_eq!(
            plan.warnings,
            vec![
                Warning::UninstallScope {
                    what: UninstallScope::HomebrewCaskStepsOnly
                },
                Warning::CaskUninstallStep {
                    step: CaskStep::RemovesServices,
                    items: vec![
                        "at.obdev.littlesnitchd".to_string(),
                        "at.obdev.LittleSnitchHelper".to_string(),
                        "at.obdev.LittleSnitchUIAgent".to_string(),
                    ],
                    only_if: None,
                },
            ]
        );

        // An empty list: not plain, since it cannot tell what the install
        // left.
        let plan = cask_uninstall(&runner, &adapter, &inst, "nothing-recorded").await;
        assert_eq!(
            plan.warnings,
            vec![Warning::UninstallScope {
                what: UninstallScope::HomebrewCask
            }]
        );
    }

    #[tokio::test]
    async fn test_a_cask_uninstall_that_runs_a_script_or_code_says_nothing_of_what_stays() {
        // A `script:`, an `early_script:` or Ruby around the uninstall:
        // Banager knows the step is there, and names the program, but not
        // what it deletes -- a vendor's uninstaller may take settings and
        // data -- so the sentence says that, not that the rest stays, and
        // the step's own line still names what runs.
        const GPT4ALL_RECEIPT: &str = include_str!(
            "../../../../../adapters/fixtures-derived/brew/7.0.6/receipts/gpt4all.json"
        );
        const CHMODBPF_RECEIPT: &str = include_str!(
            "../../../../../adapters/fixtures-derived/brew/7.0.6/receipts/wireshark-chmodbpf.json"
        );
        const FLIGHT_BLOCK_RECEIPT: &str = include_str!(
            "../../../../../adapters/fixtures-derived/brew/7.0.6/receipts/uninstall-flight-block.json"
        );
        let prefix = CaskroomPrefix::new(
            "unseen",
            &[
                ("gpt4all", GPT4ALL_RECEIPT),
                ("wireshark-chmodbpf", CHMODBPF_RECEIPT),
                ("uninstall-flight-block", FLIGHT_BLOCK_RECEIPT),
            ],
        );
        // Homebrew saves a cask with Ruby blocks as `.rb` (`save_caskfile`,
        // `cask/installer.rb:594-607`), which is read through the receipt.
        let casks = prefix
            .0
            .join("Caskroom/uninstall-flight-block/.metadata/1.0/20260928000000.000/Casks");
        std::fs::remove_file(casks.join("uninstall-flight-block.json")).unwrap();
        std::fs::write(
            casks.join("uninstall-flight-block.rb"),
            "cask \"uninstall-flight-block\" do\nend\n",
        )
        .unwrap();
        let runner = Arc::new(MockRunner::new());
        let inst = ManagerInstance {
            prefix: prefix.0.clone(),
            exe_path: prefix.0.join("bin/brew"),
            ..test_instance()
        };
        let scope = |what| Warning::UninstallScope { what };
        let step = |step, items: &[&str]| Warning::CaskUninstallStep {
            step,
            items: items.iter().map(|item| item.to_string()).collect(),
            only_if: None,
        };
        // `script:`: gpt4all installs with a `pkg`, and its maintenance tool
        // is what uninstalls it.
        let gpt4all = vec![
            scope(UninstallScope::HomebrewCaskStepsOnlyUnseen),
            step(
                CaskStep::Deletes,
                &["~/Library/Application Support/nomic.ai/GPT4All"],
            ),
            step(
                CaskStep::RunsScript,
                &["/Applications/gpt4all/maintenancetool.app/Contents/MacOS/maintenancetool"],
            ),
        ];
        // `early_script:`: the vendor's uninstaller package, run by
        // `installer`.
        let chmodbpf = vec![
            scope(UninstallScope::HomebrewCaskStepsOnlyUnseen),
            step(CaskStep::RemovesPackages, &["org.wireshark.ChmodBPF.pkg"]),
            step(CaskStep::RunsScript, &["/usr/sbin/installer"]),
        ];
        // An `uninstall_preflight` block beside the app Homebrew placed,
        // saved as Ruby, from `someone/tap`, which these tests' empty trust
        // list does not name: Homebrew runs it only if it trusts the tap.
        let flight_block = vec![
            scope(UninstallScope::HomebrewCaskStepsIfTrusted),
            step(CaskStep::RunsOwnSteps, &[]),
        ];

        let adapter = BrewAdapter::new(runner.clone())
            .with_recorded_uninstall_fn(cask_receipt::read_recorded)
            .with_env_var_fn(someones_home);
        for (token, expected) in [
            ("gpt4all", &gpt4all),
            ("wireshark-chmodbpf", &chmodbpf),
            ("uninstall-flight-block", &flight_block),
        ] {
            let plan = cask_uninstall(&runner, &adapter, &inst, token).await;
            assert_eq!(&plan.warnings, expected, "{token}");
        }

        // With a brew.env that brings the autoremove back, the same
        // sentences, which claim nothing about other files, and the
        // autoremove's own line last.
        let adapter = BrewAdapter::new(runner.clone())
            .with_recorded_uninstall_fn(cask_receipt::read_recorded)
            .with_env_var_fn(someones_home)
            .with_brew_env_fn(system_brew_env_autoremoves);
        for (token, expected) in [
            ("gpt4all", gpt4all),
            ("wireshark-chmodbpf", chmodbpf),
            ("uninstall-flight-block", flight_block),
        ] {
            let plan = cask_uninstall(&runner, &adapter, &inst, token).await;
            assert_eq!(
                plan.warnings,
                expected
                    .into_iter()
                    .chain([Warning::HomebrewAutoremoves])
                    .collect::<Vec<_>>(),
                "{token}"
            );
        }
    }

    #[tokio::test]
    async fn test_a_cask_whose_record_is_empty_or_missing_gets_the_could_not_read_sentence() {
        // Every way Banager ends up with no list to go by, or an empty one,
        // gets the one sentence that claims no deletion (`HomebrewCask`).
        const FLIGHT_BLOCK_RECEIPT: &str = include_str!(
            "../../../../../adapters/fixtures-derived/brew/7.0.6/receipts/uninstall-flight-block.json"
        );
        let prefix = CaskroomPrefix::new(
            "empty-or-missing",
            &[
                ("caskfile-empty", CLAUDEBAR_RECEIPT),
                ("caskfile-empty-with-blocks", FLIGHT_BLOCK_RECEIPT),
                ("receipt-empty", r#"{"uninstall_artifacts": []}"#),
            ],
        );
        let casks = |token: &str| {
            prefix
                .0
                .join("Caskroom")
                .join(token)
                .join(".metadata/1.0/20260928000000.000/Casks")
        };
        // The saved caskfile's own empty list (`save_caskfile` writes one for
        // a cask with nothing to uninstall), beside a receipt that lists an
        // app, and beside one that says the cask has Ruby blocks, which a
        // `.json` caskfile cannot carry.
        for token in ["caskfile-empty", "caskfile-empty-with-blocks"] {
            std::fs::write(
                casks(token).join(format!("{token}.json")),
                r#"{"artifacts": []}"#,
            )
            .unwrap();
        }
        // No list in the caskfile and none in the receipt: Homebrew would
        // use the cask's current definition.
        std::fs::create_dir_all(casks("both-missing")).unwrap();
        std::fs::write(casks("both-missing").join("both-missing.json"), "{}").unwrap();
        // Nothing saved at all.
        std::fs::create_dir_all(prefix.0.join("Caskroom/nothing-saved/.metadata")).unwrap();

        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone())
            .with_recorded_uninstall_fn(cask_receipt::read_recorded)
            .with_env_var_fn(someones_home);
        let inst = ManagerInstance {
            prefix: prefix.0.clone(),
            exe_path: prefix.0.join("bin/brew"),
            ..test_instance()
        };
        for token in [
            "caskfile-empty",
            "caskfile-empty-with-blocks",
            "receipt-empty",
            "both-missing",
            "nothing-saved",
            "not-in-the-caskroom",
        ] {
            let plan = cask_uninstall(&runner, &adapter, &inst, token).await;
            assert_eq!(
                plan.warnings,
                vec![Warning::UninstallScope {
                    what: UninstallScope::HomebrewCask
                }],
                "{token}"
            );
        }
    }

    #[tokio::test]
    async fn test_no_install_or_upgrade_plan_says_what_an_uninstall_would() {
        // The sentence is an uninstall's; a receipt that would give a cask
        // steps changes nothing about installing it, or a formula of the
        // same name. A cask's update says the steps it runs, without the
        // sentence (R47-1, r18:
        // `test_a_cask_update_says_the_old_versions_recorded_uninstall_steps_homebrew_runs_first`).
        let prefix = CaskroomPrefix::new("no-scope", &[("microsoft-word", WORD_RECEIPT)]);
        let adapter = BrewAdapter::new(Arc::new(MockRunner::new()))
            .with_recorded_uninstall_fn(cask_receipt::read_recorded);
        let inst = ManagerInstance {
            prefix: prefix.0.clone(),
            exe_path: prefix.0.join("bin/brew"),
            ..test_instance()
        };
        for kind in [OpKind::Install, OpKind::Upgrade] {
            for artifact_kind in [ArtifactKind::Cask, ArtifactKind::Formula] {
                let req = OpRequest {
                    kind,
                    instance_id: inst.id.clone(),
                    artifact_kind,
                    name: "microsoft-word".to_string(),
                };
                let plan = adapter.plan(&inst, &req).await.expect("plan");
                assert!(
                    !plan
                        .warnings
                        .iter()
                        .any(|warning| matches!(warning, Warning::UninstallScope { .. })),
                    "{kind:?}: {:?}",
                    plan.warnings
                );
                if kind == OpKind::Install || artifact_kind == ArtifactKind::Formula {
                    assert!(plan.warnings.is_empty(), "{kind:?}: {:?}", plan.warnings);
                }
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
            Outcome::Failed {
                exit_code, summary, ..
            } => {
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
                stderr_cause: Default::default(),
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

    #[tokio::test]
    async fn test_reconcile_link_reads_whether_the_formula_is_linked_now() {
        // After a link, its links as after the link that follows an update
        // (`KegLinks::fully_linked`): recorded with every command Homebrew's
        // link, linked; not recorded -- Homebrew refused -- not linked;
        // links that cannot be read, not known to be installed. No command
        // runs.
        fn read(prefix: &Path, name: &str) -> Option<KegLinks> {
            let place = match name {
                "node@22" => links::Place::Linked,
                "openssl@3" => links::Place::Taken,
                _ => return None,
            };
            Some(KegLinks {
                recorded: name == "node@22",
                commands: vec![links::CommandLink {
                    name: "node".to_string(),
                    path: prefix.join("bin/node"),
                    place,
                }],
            })
        }
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone()).with_links_fn(read);
        let inst = test_instance();
        let formula = |name: &str| ArtifactKey {
            instance_id: inst.id.clone(),
            kind: ArtifactKind::Formula,
            name: name.to_string(),
        };
        assert_eq!(
            adapter
                .reconcile_link(&inst, &formula("node@22"))
                .await
                .unwrap(),
            Some(true)
        );
        assert_eq!(
            adapter
                .reconcile_link(&inst, &formula("openssl@3"))
                .await
                .unwrap(),
            Some(false)
        );
        assert_eq!(
            adapter
                .reconcile_link(&inst, &formula("node@20"))
                .await
                .unwrap(),
            None
        );
        assert!(runner.calls().is_empty(), "the reading runs nothing");
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
        assert!(command_env(&plan).contains(&(
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
            !command_env(&plan).iter().any(|(k, _)| k == "SUDO_ASKPASS"),
            "plan env must not carry an askpass that is not set: {:?}",
            command_env(&plan)
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
            "banager-brew-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        // The `brew` sits in the prefix's own `bin`, as detection finds
        // it, so that whatever is read from the program's prefix
        // (`prefix_for`) is this folder too.
        std::fs::create_dir_all(dir.join("bin")).expect("create temp dir");
        let exe = dir.join("bin/brew");
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
                stderr_cause: Default::default(),
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
        runner_with_slow_update_at("/opt/homebrew/bin/brew", delay)
    }

    /// `runner_with_slow_update`, for the `brew` at `brew`.
    fn runner_with_slow_update_at(brew: &str, delay: Duration) -> Arc<MockRunner> {
        let ok = CommandOutput {
            stderr_cause: Default::default(),
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        };
        let runner = Arc::new(MockRunner::new());
        runner.respond(vec![brew, "update"], ok.clone());
        runner.delay(vec![brew, "update"], delay);
        runner.respond(
            vec![brew, "outdated", "--json=v2"],
            CommandOutput {
                stdout: r#"{"formulae":[],"casks":[]}"#.to_string(),
                ..ok.clone()
            },
        );
        runner.respond(vec![brew, "install", "--formula", "jq"], ok);
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
        // The wait is Banager speaking, not Homebrew: it must arrive as a
        // note the front end localises, never as a verbatim English line.
        assert!(
            !sink.snapshot().iter().any(|e| matches!(
                e,
                crate::events::OperationEvent::Log { line, .. } if line.contains("Waiting")
            )),
            "Banager's own remark went out as tool output: {:?}",
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
            Outcome::BanagerFailed(Fault::HomebrewStillUpdating { minutes: 0 })
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
        runner_with_update_and_uses_at("/opt/homebrew/bin/brew", update, uses_delay)
    }

    /// `runner_with_update_and_uses`, for the `brew` at `brew`.
    fn runner_with_update_and_uses_at(
        brew: &str,
        update: Duration,
        uses_delay: Duration,
    ) -> Arc<MockRunner> {
        let runner = runner_with_slow_update_at(brew, update);
        let uses = vec![brew, "uses", "--installed", "jq"];
        runner.respond(
            uses.clone(),
            CommandOutput {
                stderr_cause: Default::default(),
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

    // ---- ... nor one a `brew update` Banager did not start is rewriting ----
    //
    // These take Homebrew's update lock the way Homebrew's own `lock.sh`
    // does on macOS -- `exec 200>` the file, then `lockf -t 0 200` -- in a
    // real bash, so what they prove is about that lock, not a stand-in.

    /// A fresh directory to act as a Homebrew prefix.
    fn scratch_prefix(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "banager-brew-lock-{}-{}-{}",
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

    #[test]
    fn regression_update_lock_probe_does_not_wait_on_a_named_pipe() {
        // The uninstall preview looks at `<prefix>/var/homebrew/locks/update`.
        // A named pipe there, which nothing writes to, would block a plain
        // `open` until a writer came -- the preview would never end. It is
        // opened without waiting, and anything but a regular file is a lock
        // Banager cannot look at.
        use crate::adapters::read_file::tests::{finishes, make_fifo};
        let prefix = scratch_prefix("fifo");
        make_fifo(&prefix.join("var/homebrew/locks/update"));
        let probed = {
            let prefix = prefix.clone();
            finishes(move || probe_homebrew_update_lock(&prefix))
        };
        assert!(
            matches!(
                probed,
                HomebrewUpdateLock::Unobservable(LockStamp {
                    dir: Some(_),
                    file: Some(_)
                })
            ),
            "got {probed:?}"
        );
        let _ = std::fs::remove_dir_all(&prefix);
    }

    #[test]
    fn test_each_brew_env_line_says_only_what_is_known_when_one_file_was_not_read() {
        // z1's re-check, R1: a managed Mac's `/etc/homebrew/brew.env` asks
        // to be read last and turns autoremove back on; the user's
        // `~/.homebrew` is in iCloud Drive, not read. Autoremove is then
        // known, the clean-up is not: the clean-up line says "may", and
        // so does the autoremove-in-the-clean-up line, whose "will" detail
        // says a brew.env takes both switches back -- not known here.
        let env: Vec<(String, String)> = BrewAdapter::ENV
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let switches = |system: &'static [u8], user_unknown: bool| {
            brew_env::after_brew_env(
                &env,
                Path::new("/opt/homebrew"),
                &|name| (name == "HOME").then(|| OsString::from("/Users/someone")),
                &|path| {
                    if path == Path::new(brew_env::SYSTEM_FILE) {
                        brew_env::EnvFile::Read(system.to_vec())
                    } else if user_unknown && path == Path::new("/Users/someone/.homebrew/brew.env")
                    {
                        brew_env::EnvFile::Unknown
                    } else {
                        brew_env::EnvFile::Skipped
                    }
                },
            )
        };
        let managed: &[u8] = b"HOMEBREW_SYSTEM_ENV_TAKES_PRIORITY=1\nHOMEBREW_NO_AUTOREMOVE=0\n";
        let mixed = switches(managed, true);
        assert!(
            !mixed.autoremove_unknown && mixed.install_cleanup_unknown,
            "{mixed:?}"
        );
        // The user's file, not read, may also set HOMEBREW_NO_AUTO_UPDATE to
        // nothing: Homebrew "may" update itself first.
        assert_eq!(
            BrewAdapter::switch_warnings(&mixed, OpKind::Upgrade),
            vec![
                Warning::HomebrewMayAutoUpdate,
                Warning::HomebrewMayCleanUp,
                Warning::HomebrewCleanupMayAutoremove
            ]
        );
        // Autoremove known back on: the uninstall says it will.
        assert_eq!(
            BrewAdapter::switch_warnings(&mixed, OpKind::Uninstall),
            vec![Warning::HomebrewAutoremoves]
        );
        // The other way round: the clean-up known back on, autoremove not.
        let other = switches(
            b"HOMEBREW_NO_INSTALL_CLEANUP=\nHOMEBREW_XDG_CONFIG_HOME=\n",
            true,
        );
        let other = HomebrewSwitches {
            install_cleanup_unknown: false,
            no_install_cleanup: false,
            ..other
        };
        assert!(other.autoremove_unknown, "{other:?}");
        assert_eq!(
            BrewAdapter::switch_warnings(&other, OpKind::Upgrade),
            vec![
                Warning::HomebrewMayAutoUpdate,
                Warning::HomebrewPeriodicCleanup,
                Warning::HomebrewCleanupMayAutoremove
            ]
        );
        // Every file read: "will" throughout, as before.
        let known = switches(managed, false);
        assert_eq!(
            BrewAdapter::switch_warnings(&known, OpKind::Upgrade),
            Vec::<Warning>::new(),
            "the clean-up stays off: only autoremove was turned back on"
        );
    }

    #[test]
    fn test_a_brew_env_in_a_protected_place_keeps_the_autoremove_and_cleanup_warnings() {
        // `~/.homebrew` a dotfiles link into `~/Documents` (the folder
        // standing in for the home folder), its `brew.env` turning
        // autoremove back on: Banager never reads it, and `brew`, which it
        // runs, may -- so the uninstall preview says Homebrew will
        // autoremove, and an install's says it will clean up, as they say
        // when Banager can read the file. Where it can, it reads it.
        let home = std::fs::canonicalize(scratch_prefix("brew-env-protected")).unwrap();
        let kept = home.join("Documents/hb");
        std::fs::create_dir_all(&kept).unwrap();
        std::fs::write(kept.join("brew.env"), b"HOMEBREW_NO_AUTOREMOVE=0\n").unwrap();
        std::os::unix::fs::symlink(&kept, home.join(".homebrew")).unwrap();
        let env: Vec<(String, String)> = BrewAdapter::ENV
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        // The real reader over this test's own tree only: the system file
        // `/etc/homebrew/brew.env` is answered as absent, so that an
        // administrator's file on the Mac running the test cannot change
        // what it reads, and the prefix is one of its own folders.
        let prefix = home.join("prefix");
        let read = |home_var: &std::path::Path| {
            let home_var = home_var.as_os_str().to_os_string();
            brew_env::after_brew_env(
                &env,
                &prefix,
                &|name| (name == "HOME").then(|| home_var.clone()),
                &|path| {
                    if path == Path::new(brew_env::SYSTEM_FILE) {
                        brew_env::EnvFile::Skipped
                    } else {
                        brew_env::read_brew_env_file(path)
                    }
                },
            )
        };
        let readable = read(&home);
        assert!(!readable.no_autoremove && readable.no_install_cleanup);
        let _home = crate::protected::as_if_home(&home);
        let unknown = read(&home);
        assert_eq!(
            BrewAdapter::switch_warnings(&unknown, OpKind::Uninstall),
            vec![Warning::HomebrewMayAutoremove]
        );
        assert_eq!(
            BrewAdapter::switch_warnings(&unknown, OpKind::Upgrade),
            vec![
                Warning::HomebrewMayAutoUpdate,
                Warning::HomebrewMayCleanUp,
                Warning::HomebrewCleanupMayAutoremove
            ]
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn test_neither_brew_nor_its_lock_is_looked_at_in_a_protected_place() {
        // A Homebrew reached through a link into `~/Documents` (the
        // folder standing in for the home folder): not found, so never
        // run, and its lock never opened -- each found where nothing is
        // protected.
        let home = std::fs::canonicalize(scratch_prefix("brew-in-documents")).unwrap();
        let kept = home.join("Documents/homebrew");
        std::fs::create_dir_all(kept.join("bin")).unwrap();
        std::fs::write(kept.join("bin/brew"), b"#!/bin/sh\n").unwrap();
        std::fs::create_dir_all(kept.join("var/homebrew/locks")).unwrap();
        std::fs::write(kept.join("var/homebrew/locks/update"), b"").unwrap();
        let linked = home.join("homebrew");
        std::os::unix::fs::symlink(&kept, &linked).unwrap();
        assert!(brew_is_there(&linked.join("bin/brew")));
        assert!(matches!(
            probe_homebrew_update_lock(&linked),
            HomebrewUpdateLock::Free(LockStamp {
                dir: Some(_),
                file: Some(_)
            })
        ));
        let _home = crate::protected::as_if_home(&home);
        assert!(!brew_is_there(&linked.join("bin/brew")));
        assert!(!brew_is_there(&kept.join("bin/brew")));
        assert_eq!(
            probe_homebrew_update_lock(&linked),
            HomebrewUpdateLock::Unobservable(LockStamp {
                dir: None,
                file: None
            })
        );
        let _ = std::fs::remove_dir_all(&home);
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
        let brew = prefix.join("bin/brew");
        let runner =
            runner_with_update_and_uses_at(brew.to_str().unwrap(), Duration::ZERO, Duration::ZERO);
        let adapter =
            BrewAdapter::new(runner.clone()).with_update_lock_fn(probe_homebrew_update_lock);
        let inst = ManagerInstance {
            prefix: prefix.clone(),
            exe_path: prefix.join("bin/brew"),
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
        // answers; a `brew update` Banager did not start took it and let it
        // go in between. Looking only at whether it is held would see
        // nothing; the file's mtime has moved.
        let prefix = scratch_prefix("between");
        let brew = prefix.join("bin/brew");
        let runner = runner_with_update_and_uses_at(
            brew.to_str().unwrap(),
            Duration::ZERO,
            Duration::from_millis(400),
        );
        let adapter = Arc::new(
            BrewAdapter::new(runner.clone()).with_update_lock_fn(probe_homebrew_update_lock),
        );
        let inst = ManagerInstance {
            prefix: prefix.clone(),
            exe_path: prefix.join("bin/brew"),
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
        let brew = prefix.join("bin/brew");
        let runner = runner_with_update_and_uses_at(
            brew.to_str().unwrap(),
            Duration::ZERO,
            Duration::from_millis(400),
        );
        let adapter = Arc::new(
            BrewAdapter::new(runner.clone()).with_update_lock_fn(probe_homebrew_update_lock),
        );
        let inst = ManagerInstance {
            prefix: prefix.clone(),
            exe_path: prefix.join("bin/brew"),
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

    /// Promise 3 of docs/what-we-run.md for Homebrew, which
    /// `tests/safety_refresh_commands_test.rs` cannot reach (Homebrew is
    /// found at fixed paths): every command a refresh asks of Homebrew --
    /// `detect`, `inventory` and `check_updates` with the setting on and
    /// off, and `search` -- is one the Homebrew section shows outside its
    /// "Needs a password" table, with `{name}` standing for one argument,
    /// and none is one of that table's. `brew update` is the one command
    /// that changes Homebrew itself; it is checked apart: it runs once,
    /// and the section gives it a table of its own.
    #[tokio::test]
    async fn test_a_refresh_runs_only_the_read_only_commands_homebrews_section_shows() {
        const BREW: &str = "/opt/homebrew/bin/brew";
        let ok = |stdout: &str| CommandOutput {
            stderr_cause: Default::default(),
            exit_code: Some(0),
            stdout: stdout.to_string(),
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        };
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![BREW, "--version"],
            ok(include_str!(
                "../../../../../adapters/fixtures/brew/7.0.3/version.txt"
            )),
        );
        runner.respond(vec![BREW, "update"], ok(""));
        runner.respond(
            vec![BREW, "info", "--installed", "--json=v2"],
            ok(include_str!(
                "../../../../../adapters/fixtures/brew/7.0.3/info-installed.json"
            )),
        );
        let outdated = include_str!("../../../../../adapters/fixtures/brew/7.0.3/outdated.json");
        runner.respond(vec![BREW, "outdated", "--json=v2"], ok(outdated));
        let adapter = BrewAdapter::new(runner.clone())
            .with_path_exists_fn(|path| path == Path::new("/opt/homebrew/bin/brew"));
        let env = HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };

        let instances = adapter.detect(&env).await;
        assert_eq!(instances.len(), 1, "{instances:?}");
        let inst = &instances[0];
        assert!(inst.status.unavailable.is_none(), "{inst:?}");
        // Their answers are not asserted: a command the runner was not
        // given an answer for fails the call, and the check below must
        // still see the command it was asked.
        let inventory = adapter.inventory(inst).await;
        let mut checks = Vec::new();
        for greedy in [false, true] {
            let opts = CheckOptions {
                include_self_updating: greedy,
                ..CheckOptions::default()
            };
            checks.push(adapter.check_updates(inst, &opts).await.map(|_| ()));
        }
        // Search, which runs without a plan as a refresh does, though
        // nothing in the window asks for it yet.
        runner.respond(
            vec![BREW, "search", "jq"],
            ok(include_str!(
                "../../../../../adapters/fixtures/brew/7.0.3/search-jq.txt"
            )),
        );
        runner.respond(
            vec![BREW, "search", "--desc", "jq"],
            ok(include_str!(
                "../../../../../adapters/fixtures/brew/7.0.3/search-desc-jq.txt"
            )),
        );
        checks.push(adapter.search(inst, "jq").await.map(|_| ()));

        let doc = include_str!("../../../../../docs/what-we-run.md");
        let start = doc.find("\n## Homebrew\n").expect("a section ## Homebrew") + 1;
        let section = &doc[start..];
        let section = &section[..section[3..]
            .find("\n## ")
            .map_or(section.len(), |at| at + 3)];
        let mut reads: Vec<String> = Vec::new();
        let mut writes: Vec<String> = Vec::new();
        let mut in_write_table = false;
        for line in section.lines() {
            if !line.starts_with('|') {
                in_write_table = false;
            } else if line.contains("Needs a password") {
                in_write_table = true;
            }
            for (index, span) in line.split('`').enumerate() {
                if index % 2 == 1 && span.starts_with("<brew> ") {
                    if in_write_table {
                        writes.push(span.to_string());
                    } else {
                        reads.push(span.to_string());
                    }
                }
            }
        }
        assert!(
            section.contains("| Update Homebrew and its local package index (`maybe_update`) | `<brew> update` |"),
            "`brew update` has its own row"
        );
        assert!(!writes.is_empty(), "the section's write table was read");
        let is = |argv: &[String], written: &str| {
            let words: Vec<&str> = written.split_whitespace().collect();
            words.len() == argv.len()
                && words.iter().zip(argv).all(|(word, arg)| {
                    *word == arg || (word.starts_with('{') && word.ends_with('}'))
                })
        };

        f08_assert_brew_env(&runner);
        let calls = runner.calls();
        let mut updates = 0;
        for call in &calls {
            assert_eq!(call[0], BREW, "{call:?}");
            let mut argv = vec!["<brew>".to_string()];
            argv.extend(call[1..].iter().cloned());
            assert!(
                writes.iter().all(|write| !is(&argv, write)),
                "a refresh ran {argv:?}, a write command of Homebrew"
            );
            if argv[1..] == ["update"] {
                updates += 1;
                continue;
            }
            assert!(
                reads.iter().any(|read| is(&argv, read)),
                "a refresh ran {argv:?}, which ## Homebrew does not show as read-only: {reads:?}"
            );
        }
        assert_eq!(updates, 1, "`brew update` ran once: {calls:?}");
        assert!(inventory.is_ok_and(|found| !found.is_empty()));
        assert!(checks.iter().all(Result::is_ok), "{checks:?}");
        // Not a test that ran nothing: each kind of command was asked.
        for wanted in [
            vec!["--version"],
            vec!["info", "--installed", "--json=v2"],
            vec!["outdated", "--json=v2"],
            vec!["search", "--desc", "jq"],
        ] {
            assert!(
                calls.iter().any(|call| call[1..] == wanted[..]),
                "{wanted:?} ran: {calls:?}"
            );
        }
    }

    fn f08_assert_brew_env(runner: &MockRunner) {
        let specs = runner.specs();
        assert!(!specs.is_empty(), "must inspect actual dispatches");
        for spec in specs {
            for switch in ["HOMEBREW_NO_AUTOREMOVE", "HOMEBREW_NO_INSTALL_CLEANUP"] {
                assert!(
                    spec.env.iter().any(|(k, v)| k == switch && v == "1"),
                    "missing {switch} on {spec:?}"
                );
            }
        }
    }

    fn f08_ok() -> CommandOutput {
        CommandOutput {
            stderr_cause: Default::default(),
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        }
    }

    async fn f08_uninstall_env_changes(unknown: bool) {
        let runner = Arc::new(MockRunner::new());
        let mut adapter =
            BrewAdapter::new(runner.clone()).with_brew_env_fn(|_| brew_env::EnvFile::Skipped);
        let inst = test_instance();
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "wget"],
            f08_ok(),
        );
        let plan = adapter
            .plan(
                &inst,
                &OpRequest {
                    kind: OpKind::Uninstall,
                    instance_id: inst.id.clone(),
                    artifact_kind: ArtifactKind::Formula,
                    name: "wget".into(),
                },
            )
            .await
            .unwrap();
        assert!(!plan
            .warnings
            .iter()
            .any(|warning| matches!(warning, Warning::HomebrewAutoremoves)));
        // Unknown: brew.env now sits in a protected place Banager never
        // looks into, so what it says is not known (brew_env::EnvFile).
        adapter.brew_env_fn = if unknown {
            |_| brew_env::EnvFile::Unknown
        } else {
            |_| brew_env::EnvFile::Read(b"HOMEBREW_NO_AUTOREMOVE=0\n".to_vec())
        };
        let PlanAction::Command { program, args, .. } = &plan.action else {
            panic!("command");
        };
        let mut argv = vec![program.to_str().unwrap()];
        argv.extend(args.iter().map(String::as_str));
        runner.respond(argv, f08_ok());
        let before = runner.calls().len();
        let result = adapter
            .execute(&plan, Arc::new(VecSink::new()), 1, CancellationToken::new())
            .await;
        assert_eq!(
            runner.calls().len(),
            before,
            "refusal runs no command: {result:?}"
        );
        assert_eq!(
            result.unwrap(),
            Outcome::BanagerFailed(Fault::HomebrewSettingsChanged)
        );
    }

    #[tokio::test]
    async fn f08_g05_autoremove_enabled_after_uninstall_preview() {
        f08_uninstall_env_changes(false).await;
    }
    #[tokio::test]
    async fn f08_g05_brew_env_unknown_after_uninstall_preview() {
        f08_uninstall_env_changes(true).await;
    }

    #[tokio::test]
    async fn f30a_g05_autoremove_limits_and_unchanged_controls() {
        type Files = fn(&Path) -> brew_env::EnvFile;
        let off: Files = |_| brew_env::EnvFile::Skipped;
        let on: Files = |_| brew_env::EnvFile::Read(b"HOMEBREW_NO_AUTOREMOVE=0\n".to_vec());
        let unknown: Files = |_| brew_env::EnvFile::Unknown;
        let except: Files = |_| {
            brew_env::EnvFile::Read(
                b"HOMEBREW_NO_AUTOREMOVE=0\nHOMEBREW_NO_CLEANUP_FORMULAE=keep-me\n".to_vec(),
            )
        };
        for (before, after, refused) in [
            (off, off, false),
            (off, on, true),
            (off, unknown, true),
            (on, on, false),
            (on, unknown, false),
            (unknown, on, false),
            (except, on, true),
            (except, unknown, true),
            (on, except, false),
            (except, off, false),
        ] {
            for kind in [ArtifactKind::Formula, ArtifactKind::Cask] {
                let runner = Arc::new(MockRunner::new());
                let mut adapter = BrewAdapter::new(runner.clone()).with_brew_env_fn(before);
                let inst = test_instance();
                runner.respond(
                    vec!["/opt/homebrew/bin/brew", "uses", "--installed", "wget"],
                    f08_ok(),
                );
                let plan = adapter
                    .plan(
                        &inst,
                        &OpRequest {
                            kind: OpKind::Uninstall,
                            instance_id: inst.id.clone(),
                            artifact_kind: kind,
                            name: "wget".into(),
                        },
                    )
                    .await
                    .unwrap();
                adapter.brew_env_fn = after;
                let PlanAction::Command { program, args, .. } = &plan.action else {
                    panic!("command")
                };
                let mut argv = vec![program.to_str().unwrap()];
                argv.extend(args.iter().map(String::as_str));
                runner.respond(argv, f08_ok());
                let calls = runner.calls().len();
                let result = adapter
                    .execute(&plan, Arc::new(VecSink::new()), 1, CancellationToken::new())
                    .await
                    .unwrap();
                if refused {
                    assert_eq!(
                        result,
                        Outcome::BanagerFailed(Fault::HomebrewSettingsChanged)
                    );
                    assert_eq!(runner.calls().len(), calls, "refusal runs no command");
                } else {
                    assert_eq!(result, Outcome::Succeeded);
                    assert_eq!(runner.calls().len(), calls + 1);
                }
            }
        }
    }

    const F30A_PLAIN_CASK: &str = r#"{"source":{"tap":"homebrew/cask"},"uninstall_artifacts":[{"app":["Example.app"]}],"uninstall_flight_blocks":false}"#;
    const F30A_DELETE_CASK: &str = r#"{"source":{"tap":"homebrew/cask"},"uninstall_artifacts":[{"app":["Example.app"]},{"uninstall":[{"delete":"~/Library/Example"}]}],"uninstall_flight_blocks":false}"#;

    async fn f30a_cask_receipt_recheck(
        before_receipt: &str,
        after_receipt: Option<&str>,
        saved_caskfile: bool,
    ) {
        let prefix = CaskroomPrefix::new("f30a", &[("example", before_receipt)]);
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner.clone())
            .with_recorded_uninstall_fn(cask_receipt::read_recorded);
        // Keep the executable and the receipt in the same synthetic prefix,
        // exactly as detect constructs an instance. No real brew is run.
        let inst = ManagerInstance {
            exe_path: prefix.0.join("bin/brew"),
            prefix: prefix.0.clone(),
            ..test_instance()
        };
        let plan = cask_uninstall(&runner, &adapter, &inst, "example").await;
        assert_eq!(
            plan.warnings
                .iter()
                .any(|w| matches!(w, Warning::CaskUninstallStep { .. })),
            before_receipt == F30A_DELETE_CASK,
            "precondition: the preview shows the receipt's own steps: {:?}",
            plan.warnings
        );
        if let Some(after) = after_receipt {
            let path = if saved_caskfile {
                prefix
                    .0
                    .join("Caskroom/example/.metadata/1.0/20260928000000.000/Casks/example.json")
            } else {
                prefix
                    .0
                    .join("Caskroom/example/.metadata/INSTALL_RECEIPT.json")
            };
            std::fs::write(path, after).unwrap();
            let fresh = cask_uninstall(&runner, &adapter, &inst, "example").await;
            assert_ne!(
                fresh.warnings, plan.warnings,
                "precondition: a new preview changes the removal details"
            );
            if after == F30A_DELETE_CASK {
                assert!(fresh
                    .warnings
                    .iter()
                    .any(|w| matches!(w, Warning::CaskUninstallStep { .. })));
            }
        }
        runner.respond(
            vec![
                inst.exe_path.to_str().unwrap(),
                "uninstall",
                "--cask",
                "example",
            ],
            f08_ok(),
        );
        let before = runner.calls().len();
        let result = adapter
            .execute(&plan, Arc::new(VecSink::new()), 1, CancellationToken::new())
            .await;
        if after_receipt.is_some() {
            assert_eq!(
                runner.calls().len(),
                before,
                "changed receipt must run no command: {result:?}"
            );
            assert_eq!(
                result.unwrap(),
                Outcome::BanagerFailed(Fault::HomebrewSettingsChanged)
            );
        } else {
            assert_eq!(result.unwrap(), Outcome::Succeeded);
            assert_eq!(runner.calls().len(), before + 1);
            f08_assert_brew_env(&runner);
        }
    }

    async fn f08_cask_receipt_changes(expands: bool) {
        f30a_cask_receipt_recheck(F30A_PLAIN_CASK, expands.then_some(F30A_DELETE_CASK), false)
            .await;
    }

    #[tokio::test]
    async fn f30a_g08_changed_targets_unreadable_and_saved_caskfile() {
        // A step of the same kind with a different target also needs consent.
        let moved = F30A_DELETE_CASK.replace("~/Library/Example", "~/Library/Different");
        f30a_cask_receipt_recheck(F30A_DELETE_CASK, Some(&moved), false).await;
        // Losing the receipt makes the old promise unknowable.
        f30a_cask_receipt_recheck(F30A_PLAIN_CASK, Some("not JSON"), false).await;
        // Homebrew prefers artifacts in the saved caskfile over the receipt.
        let saved = r#"{"artifacts":[{"app":["Example.app"]},{"uninstall":[{"delete":"~/Library/Example"}]}]}"#;
        f30a_cask_receipt_recheck(F30A_PLAIN_CASK, Some(saved), true).await;
        // A disclosed delete step, unchanged, is still executable.
        f30a_cask_receipt_recheck(F30A_DELETE_CASK, None, false).await;
    }

    #[tokio::test]
    async fn f08_g08_expanded_cask_receipt_refuses_saved_uninstall() {
        f08_cask_receipt_changes(true).await;
    }
    #[tokio::test]
    async fn f08_g08_unchanged_cask_receipt_executes() {
        f08_cask_receipt_changes(false).await;
    }

    /// Less than the preview said runs: a cask with a delete step, shown
    /// while a `brew.env` turned Homebrew's autoremove on, still uninstalls
    /// once that file is gone -- its sentence then says nothing else is
    /// deleted, which is less, not more, than the one confirmed.
    #[tokio::test]
    async fn f30a_g08_cask_steps_run_after_autoremove_turned_off() {
        let prefix = CaskroomPrefix::new("f30a-off", &[("example", F30A_DELETE_CASK)]);
        let runner = Arc::new(MockRunner::new());
        let mut adapter = BrewAdapter::new(runner.clone())
            .with_recorded_uninstall_fn(cask_receipt::read_recorded)
            .with_brew_env_fn(|_| brew_env::EnvFile::Read(b"HOMEBREW_NO_AUTOREMOVE=0\n".to_vec()));
        let inst = ManagerInstance {
            exe_path: prefix.0.join("bin/brew"),
            prefix: prefix.0.clone(),
            ..test_instance()
        };
        let plan = cask_uninstall(&runner, &adapter, &inst, "example").await;
        assert!(
            plan.warnings.contains(&Warning::UninstallScope {
                what: UninstallScope::HomebrewCaskStepsAutoremoves
            }) && plan.warnings.contains(&Warning::HomebrewAutoremoves),
            "precondition: the preview said autoremove runs: {:?}",
            plan.warnings
        );
        adapter.brew_env_fn = |_| brew_env::EnvFile::Skipped;
        let uninstall = vec![
            inst.exe_path.to_str().unwrap(),
            "uninstall",
            "--cask",
            "example",
        ];
        runner.respond(uninstall.clone(), f08_ok());
        let before = runner.calls().len();
        let result = adapter
            .execute(&plan, Arc::new(VecSink::new()), 1, CancellationToken::new())
            .await;
        assert_eq!(result.unwrap(), Outcome::Succeeded);
        assert_eq!(runner.calls()[before..], [uninstall]);
    }

    /// U9 (r6): an update deletes the old versions of the formula it
    /// updated, and an uninstall deletes every version installed.
    mod old_versions {
        use super::*;
        use crate::adapters::brew::kegs::Kegs;
        use crate::events::LogNote;

        /// A Cellar with wget's 1.24.0 and 1.25.0 in it, and no pin.
        fn two_versions(_prefix: &Path, name: &str) -> Option<Kegs> {
            (name.rsplit('/').next() == Some("wget")).then(|| Kegs {
                versions: vec!["1.24.0".to_string(), "1.25.0".to_string()],
                pinned: false,
            })
        }

        /// wget's Cellar once its update installed 1.26.0 and the cleanup
        /// after it deleted every older version.
        fn cleaned(_prefix: &Path, name: &str) -> Option<Kegs> {
            (name.rsplit('/').next() == Some("wget")).then(|| Kegs {
                versions: vec!["1.26.0".to_string()],
                pinned: false,
            })
        }

        /// What `sink` got, a line each: a command's line as it printed
        /// it, a note as its `Debug`.
        fn log_lines(sink: &VecSink) -> Vec<String> {
            sink.snapshot()
                .into_iter()
                .map(|event| match event {
                    crate::events::OperationEvent::Log { line, .. } => line,
                    crate::events::OperationEvent::Note { note, .. } => format!("{note:?}"),
                    other => format!("{other:?}"),
                })
                .collect()
        }

        /// A runner that answers as `inner` and then, once it has answered
        /// `after`, cancels `token`: a Cancel that lands between two
        /// commands of one operation.
        struct CancelAfter {
            inner: Arc<MockRunner>,
            after: Vec<String>,
            token: CancellationToken,
        }

        #[async_trait]
        impl CommandRunner for CancelAfter {
            async fn run(
                &self,
                spec: CommandSpec,
                on_line: Option<crate::runner::LineCallback>,
                cancel: CancellationToken,
            ) -> Result<CommandOutput, crate::runner::RunnerError> {
                let mut argv = vec![spec.program.to_string_lossy().into_owned()];
                argv.extend(spec.args.iter().cloned());
                let output = self.inner.run(spec, on_line, cancel).await;
                if argv == self.after {
                    self.token.cancel();
                }
                output
            }
        }

        fn request(kind: OpKind, artifact_kind: ArtifactKind, name: &str) -> OpRequest {
            OpRequest {
                kind,
                instance_id: test_instance().id,
                artifact_kind,
                name: name.to_string(),
            }
        }

        fn ok(stdout: &str, stderr: &str, exit_code: i32) -> CommandOutput {
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(exit_code),
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
                timed_out: false,
                cancelled: false,
            }
        }

        /// `brew uses --installed {name}` answering that nothing needs it.
        fn nothing_uses(runner: &MockRunner, name: &str) {
            runner.respond(
                vec!["/opt/homebrew/bin/brew", "uses", "--installed", name],
                ok("", "", 0),
            );
        }

        /// The update's argv and the `brew cleanup` that follows it, when
        /// one does.
        fn upgrade_then_cleanup(plan: &Plan) -> Option<(&[String], &[String])> {
            match &plan.action {
                PlanAction::CommandThen { args, then, .. } => then
                    .iter()
                    .find(|argv| argv.first().map(String::as_str) == Some("cleanup"))
                    .map(|cleanup| (args.as_slice(), cleanup.as_slice())),
                _ => None,
            }
        }

        #[tokio::test]
        async fn an_update_of_a_formula_cleans_up_its_old_versions_once_it_succeeds() {
            let runner = Arc::new(MockRunner::new());
            let adapter = BrewAdapter::new(runner).with_kegs_fn(two_versions);
            let inst = test_instance();
            let plan = adapter
                .plan(
                    &inst,
                    &request(OpKind::Upgrade, ArtifactKind::Formula, "wget"),
                )
                .await
                .expect("plan");
            let (args, then) = upgrade_then_cleanup(&plan).expect("a cleanup follows");
            assert_eq!(args, ["upgrade", "--formula", "wget"]);
            assert_eq!(then, ["cleanup", "wget"]);
            // Under the same environment: Banager's switches all stand.
            assert!(command_env(&plan)
                .contains(&("HOMEBREW_NO_AUTOREMOVE".to_string(), "1".to_string())));
            assert_eq!(
                plan.warnings,
                vec![Warning::HomebrewCleansUpOldVersions {
                    versions: vec!["1.24.0".to_string(), "1.25.0".to_string()],
                }]
            );
            // A tap's formula is cleaned up by the name it is updated by.
            let plan = adapter
                .plan(
                    &inst,
                    &request(OpKind::Upgrade, ArtifactKind::Formula, "someone/tap/wget"),
                )
                .await
                .expect("plan");
            assert_eq!(
                upgrade_then_cleanup(&plan).map(|(_, then)| then),
                Some(&["cleanup".to_string(), "someone/tap/wget".to_string()][..])
            );
        }

        #[tokio::test]
        async fn an_update_cleans_up_nothing_for_a_cask_an_install_or_where_no_version_was_read() {
            let runner = Arc::new(MockRunner::new());
            let adapter = BrewAdapter::new(runner.clone()).with_kegs_fn(two_versions);
            let inst = test_instance();
            for req in [
                request(OpKind::Upgrade, ArtifactKind::Cask, "wget"),
                request(OpKind::Install, ArtifactKind::Formula, "wget"),
            ] {
                let plan = adapter.plan(&inst, &req).await.expect("plan");
                assert!(matches!(plan.action, PlanAction::Command { .. }), "{req:?}");
                assert_eq!(plan.warnings, vec![], "{req:?}");
            }
            // Nothing read of the formula: no line to say which versions,
            // so no cleanup either.
            let plan = BrewAdapter::new(runner)
                .plan(
                    &inst,
                    &request(OpKind::Upgrade, ArtifactKind::Formula, "wget"),
                )
                .await
                .expect("plan");
            assert!(matches!(plan.action, PlanAction::Command { .. }));
            assert_eq!(plan.warnings, vec![]);
        }

        #[tokio::test]
        async fn an_update_keeps_every_cleanup_setting_the_person_made() {
            type Files = fn(&Path) -> brew_env::EnvFile;
            fn system(bytes: &'static [u8]) -> impl Fn(&Path) -> brew_env::EnvFile {
                move |path| {
                    (path == Path::new(brew_env::SYSTEM_FILE))
                        .then(|| bytes.to_vec())
                        .into()
                }
            }
            let cases: [(&str, Files); 5] = [
                // Their own HOMEBREW_NO_INSTALL_CLEANUP: no cleanup after
                // an install or upgrade, Banager's included.
                ("opted out", |p| {
                    system(b"HOMEBREW_NO_INSTALL_CLEANUP=1\n")(p)
                }),
                // Taken back: Homebrew cleans up by itself, and says so.
                ("Homebrew's own", |p| {
                    system(b"HOMEBREW_NO_INSTALL_CLEANUP=\n")(p)
                }),
                // Kept out of every clean-up by name.
                ("named", |p| {
                    system(b"HOMEBREW_NO_CLEANUP_FORMULAE=jq,wget\n")(p)
                }),
                // A brew.env Banager does not read: it may say either.
                ("unknown", |p| {
                    if p == Path::new(brew_env::SYSTEM_FILE) {
                        brew_env::EnvFile::Unknown
                    } else {
                        brew_env::EnvFile::Skipped
                    }
                }),
                // Another formula named: wget is still cleaned up.
                ("another", |p| {
                    system(b"HOMEBREW_NO_CLEANUP_FORMULAE=jq\n")(p)
                }),
            ];
            let inst = test_instance();
            for (case, files) in cases {
                let adapter = BrewAdapter::new(Arc::new(MockRunner::new()))
                    .with_kegs_fn(two_versions)
                    .with_brew_env_fn(files);
                for name in ["wget", "someone/tap/wget"] {
                    let plan = adapter
                        .plan(
                            &inst,
                            &request(OpKind::Upgrade, ArtifactKind::Formula, name),
                        )
                        .await
                        .expect("plan");
                    let cleans = plan
                        .warnings
                        .iter()
                        .any(|w| matches!(w, Warning::HomebrewCleansUpOldVersions { .. }));
                    assert_eq!(
                        upgrade_then_cleanup(&plan).is_some(),
                        case == "another",
                        "{case} {name}"
                    );
                    assert_eq!(cleans, case == "another", "{case} {name}");
                }
            }
            // Their own variable in Banager's environment, which every brew
            // command inherits under the plan's own.
            let adapter = BrewAdapter::new(Arc::new(MockRunner::new()))
                .with_kegs_fn(two_versions)
                .with_env_var_fn(|name| {
                    (name == "HOMEBREW_NO_INSTALL_CLEANUP").then(|| OsString::from("1"))
                });
            let plan = adapter
                .plan(
                    &inst,
                    &request(OpKind::Upgrade, ArtifactKind::Formula, "wget"),
                )
                .await
                .expect("plan");
            assert!(upgrade_then_cleanup(&plan).is_none());
            // A pin -- or a pin record that cannot be looked at -- keeps a
            // version: no cleanup either (the same question execute asks
            // again at the cleanup's turn, `cleanup_allowed`).
            let adapter = BrewAdapter::new(Arc::new(MockRunner::new())).with_kegs_fn(|_, _| {
                Some(Kegs {
                    versions: vec!["1.24.0".to_string(), "1.25.0".to_string()],
                    pinned: true,
                })
            });
            let plan = adapter
                .plan(
                    &inst,
                    &request(OpKind::Upgrade, ArtifactKind::Formula, "wget"),
                )
                .await
                .expect("plan");
            assert!(upgrade_then_cleanup(&plan).is_none());
            assert_eq!(plan.warnings, vec![]);
        }

        #[tokio::test]
        async fn an_uninstall_of_a_formula_deletes_every_installed_version() {
            let runner = Arc::new(MockRunner::new());
            nothing_uses(&runner, "wget");
            let adapter = BrewAdapter::new(runner).with_kegs_fn(two_versions);
            let plan = adapter
                .plan(
                    &test_instance(),
                    &request(OpKind::Uninstall, ArtifactKind::Formula, "wget"),
                )
                .await
                .expect("plan");
            assert_eq!(
                command_args(&plan),
                ["uninstall", "--formula", "--force", "wget"]
            );
            assert_eq!(
                plan.warnings,
                vec![
                    scope_of(ArtifactKind::Formula, false),
                    Warning::HomebrewRemovesEveryVersion {
                        versions: vec!["1.24.0".to_string(), "1.25.0".to_string()],
                    },
                ]
            );
        }

        #[tokio::test]
        async fn an_uninstall_passes_no_force_for_one_version_a_pinned_formula_or_a_cask() {
            let runner = Arc::new(MockRunner::new());
            nothing_uses(&runner, "wget");
            let inst = test_instance();
            type Read = fn(&Path, &str) -> Option<Kegs>;
            let one: Read = |_, _| {
                Some(Kegs {
                    versions: vec!["1.25.0".to_string()],
                    pinned: false,
                })
            };
            let pinned: Read = |_, _| {
                Some(Kegs {
                    versions: vec!["1.24.0".to_string(), "1.25.0".to_string()],
                    pinned: true,
                })
            };
            let none: Read = |_, _| None;
            for read in [one, pinned, none] {
                let adapter = BrewAdapter::new(runner.clone()).with_kegs_fn(read);
                let plan = adapter
                    .plan(
                        &inst,
                        &request(OpKind::Uninstall, ArtifactKind::Formula, "wget"),
                    )
                    .await
                    .expect("plan");
                assert_eq!(command_args(&plan), ["uninstall", "--formula", "wget"]);
                assert_eq!(plan.warnings, vec![scope_of(ArtifactKind::Formula, false)]);
            }
            let adapter = BrewAdapter::new(runner).with_kegs_fn(two_versions);
            let plan = adapter
                .plan(
                    &inst,
                    &request(OpKind::Uninstall, ArtifactKind::Cask, "wget"),
                )
                .await
                .expect("plan");
            assert_eq!(command_args(&plan), ["uninstall", "--cask", "wget"]);
        }

        #[tokio::test]
        async fn the_cleanup_runs_once_the_update_succeeds_and_its_end_decides_nothing() {
            let upgrade = vec!["/opt/homebrew/bin/brew", "upgrade", "--formula", "wget"];
            let cleanup = vec!["/opt/homebrew/bin/brew", "cleanup", "wget"];
            let inst = test_instance();
            let req = request(OpKind::Upgrade, ArtifactKind::Formula, "wget");
            for (cleanup_exit, note) in [
                (0, None),
                (
                    1,
                    Some(LogNote::OldVersionsNotCleanedUp {
                        name: "wget".to_string(),
                        exit_code: Some(1),
                    }),
                ),
            ] {
                let runner = Arc::new(MockRunner::new());
                runner.respond(upgrade.clone(), ok("==> Upgrading wget\n", "", 0));
                runner.respond(
                    cleanup.clone(),
                    ok(
                        "Removing: /opt/homebrew/Cellar/wget/1.24.0... (52 files, 4.1MB)\n",
                        if cleanup_exit == 0 {
                            ""
                        } else {
                            "Error: Permission denied\n"
                        },
                        cleanup_exit,
                    ),
                );
                let adapter = BrewAdapter::new(runner.clone()).with_kegs_fn(two_versions);
                let plan = adapter.plan(&inst, &req).await.expect("plan");
                let sink = Arc::new(VecSink::new());
                // The Cellar as the cleanup leaves it: the new version only.
                let outcome = BrewAdapter::new(runner.clone())
                    .with_kegs_fn(cleaned)
                    .execute(&plan, sink.clone(), 7, CancellationToken::new())
                    .await
                    .expect("execute");
                assert_eq!(outcome, Outcome::Succeeded, "cleanup exited {cleanup_exit}");
                assert_eq!(runner.calls(), vec![upgrade.clone(), cleanup.clone()]);
                f08_assert_brew_env(&runner);
                let PlanAction::CommandThen { env, .. } = &plan.action else {
                    panic!("upgrade and follow-up commands");
                };
                assert_eq!(
                    runner.specs()[0].env,
                    *env,
                    "upgrade keeps the confirmed environment"
                );
                // The log: the update's lines, where the cleanup starts, its
                // own lines, and how it ended when it did not finish.
                let lines: Vec<String> = sink
                    .snapshot()
                    .into_iter()
                    .map(|event| match event {
                        crate::events::OperationEvent::Log { line, .. } => line,
                        crate::events::OperationEvent::Note { note, .. } => format!("{note:?}"),
                        other => format!("{other:?}"),
                    })
                    .collect();
                let mut expected = vec![
                    "==> Upgrading wget".to_string(),
                    format!(
                        "{:?}",
                        LogNote::CleaningUpOldVersions {
                            name: "wget".to_string()
                        }
                    ),
                    "Removing: /opt/homebrew/Cellar/wget/1.24.0... (52 files, 4.1MB)".to_string(),
                ];
                if let Some(note) = note {
                    expected.push("Error: Permission denied".to_string());
                    expected.push(format!("{note:?}"));
                }
                assert_eq!(lines, expected);
            }
        }

        #[tokio::test]
        async fn no_cleanup_runs_after_an_update_that_did_not_succeed() {
            let runner = Arc::new(MockRunner::new());
            runner.respond(
                vec!["/opt/homebrew/bin/brew", "upgrade", "--formula", "wget"],
                ok("", "Error: wget 1.26.0 did not build\n", 1),
            );
            let adapter = BrewAdapter::new(runner.clone()).with_kegs_fn(two_versions);
            let inst = test_instance();
            let plan = adapter
                .plan(
                    &inst,
                    &request(OpKind::Upgrade, ArtifactKind::Formula, "wget"),
                )
                .await
                .expect("plan");
            let outcome = adapter
                .execute(&plan, Arc::new(VecSink::new()), 7, CancellationToken::new())
                .await
                .expect("execute");
            assert!(matches!(
                outcome,
                Outcome::Failed {
                    exit_code: Some(1),
                    ..
                }
            ));
            assert_eq!(runner.calls().len(), 1, "{:?}", runner.calls());
        }

        #[tokio::test]
        async fn a_cleanup_that_exits_0_but_leaves_versions_the_preview_named_says_which_in_the_log(
        ) {
            // `brew cleanup` exits 0 and still keeps a version: one named by
            // an alias in HOMEBREW_NO_CLEANUP_FORMULAE (`onoe`, which does
            // not fail the command, `cleanup.rb:511-514`), or one Homebrew
            // still needs -- linked, kept by a keepme, the newest HEAD
            // (`Formula#eligible_kegs_for_cleanup`). The Cellar is read again
            // and the log says which of the versions the preview named are
            // still there; the newest there is the one the update put in.
            let upgrade = vec!["/opt/homebrew/bin/brew", "upgrade", "--formula", "wget"];
            let cleanup = vec!["/opt/homebrew/bin/brew", "cleanup", "wget"];
            let inst = test_instance();
            let req = request(OpKind::Upgrade, ArtifactKind::Formula, "wget");
            type Read = fn(&Path, &str) -> Option<Kegs>;
            let refused: Read = |_, _| {
                Some(Kegs {
                    versions: vec![
                        "1.24.0".to_string(),
                        "1.25.0".to_string(),
                        "1.26.0".to_string(),
                    ],
                    pinned: false,
                })
            };
            let linked: Read = |_, _| {
                Some(Kegs {
                    versions: vec!["1.25.0".to_string(), "1.26.0".to_string()],
                    pinned: false,
                })
            };
            // Readable when the cleanup's turn comes (its settings are asked
            // again then, and a Cellar that cannot be read stops it), not
            // once it has run: this execute reads it twice, before and
            // after, and no other test uses this reader.
            fn unread_after(_: &Path, _: &str) -> Option<Kegs> {
                use std::sync::atomic::{AtomicUsize, Ordering};
                static READS: AtomicUsize = AtomicUsize::new(0);
                READS
                    .fetch_add(1, Ordering::SeqCst)
                    .is_multiple_of(2)
                    .then(|| Kegs {
                        versions: vec!["1.24.0".to_string(), "1.25.0".to_string()],
                        pinned: false,
                    })
            }
            let unread: Read = unread_after;
            let kept = |versions: &[&str]| {
                Some(LogNote::OldVersionsKept {
                    name: "wget".to_string(),
                    versions: versions.iter().map(|v| v.to_string()).collect(),
                })
            };
            for (read, note) in [
                (refused, kept(&["1.24.0", "1.25.0"])),
                (linked, kept(&["1.25.0"])),
                (cleaned as Read, None),
                (unread, None),
            ] {
                let runner = Arc::new(MockRunner::new());
                runner.respond(upgrade.clone(), ok("", "", 0));
                runner.respond(
                    cleanup.clone(),
                    ok("", "Error: Refusing to clean up wget\n", 0),
                );
                let plan = BrewAdapter::new(runner.clone())
                    .with_kegs_fn(two_versions)
                    .plan(&inst, &req)
                    .await
                    .expect("plan");
                let sink = Arc::new(VecSink::new());
                let outcome = BrewAdapter::new(runner.clone())
                    .with_kegs_fn(read)
                    .execute(&plan, sink.clone(), 7, CancellationToken::new())
                    .await
                    .expect("execute");
                assert_eq!(outcome, Outcome::Succeeded);
                let mut expected = vec![
                    format!(
                        "{:?}",
                        LogNote::CleaningUpOldVersions {
                            name: "wget".to_string()
                        }
                    ),
                    "Error: Refusing to clean up wget".to_string(),
                ];
                expected.extend(note.map(|note| format!("{note:?}")));
                assert_eq!(log_lines(&sink), expected);
            }
        }

        #[tokio::test]
        async fn a_cleanup_that_is_stopped_or_cannot_start_leaves_the_update_succeeded() {
            // Cancelled while it runs, out of time, or not started at all
            // (the runner could not spawn it): the update stands, and the
            // log says the cleanup did not finish.
            let upgrade = vec!["/opt/homebrew/bin/brew", "upgrade", "--formula", "wget"];
            let cleanup = vec!["/opt/homebrew/bin/brew", "cleanup", "wget"];
            let inst = test_instance();
            let req = request(OpKind::Upgrade, ArtifactKind::Formula, "wget");
            let stopped = |cancelled: bool| CommandOutput {
                stderr_cause: Default::default(),
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: !cancelled,
                cancelled,
            };
            for ending in [Some(stopped(true)), Some(stopped(false)), None] {
                let runner = Arc::new(MockRunner::new());
                runner.respond(upgrade.clone(), ok("", "", 0));
                if let Some(output) = ending.clone() {
                    runner.respond(cleanup.clone(), output);
                }
                let adapter = BrewAdapter::new(runner.clone()).with_kegs_fn(two_versions);
                let plan = adapter.plan(&inst, &req).await.expect("plan");
                let sink = Arc::new(VecSink::new());
                let outcome = adapter
                    .execute(&plan, sink.clone(), 7, CancellationToken::new())
                    .await
                    .expect("execute");
                assert_eq!(outcome, Outcome::Succeeded, "{ending:?}");
                assert_eq!(runner.calls(), vec![upgrade.clone(), cleanup.clone()]);
                f08_assert_brew_env(&runner);
                let PlanAction::CommandThen { env, .. } = &plan.action else {
                    panic!("upgrade and follow-up commands");
                };
                assert_eq!(
                    runner.specs()[0].env,
                    *env,
                    "upgrade keeps the confirmed environment"
                );
                assert_eq!(
                    log_lines(&sink),
                    vec![
                        format!(
                            "{:?}",
                            LogNote::CleaningUpOldVersions {
                                name: "wget".to_string()
                            }
                        ),
                        format!(
                            "{:?}",
                            LogNote::OldVersionsNotCleanedUp {
                                name: "wget".to_string(),
                                exit_code: None,
                            }
                        ),
                    ],
                    "{ending:?}"
                );
            }
        }

        #[tokio::test]
        async fn a_cancel_after_the_update_and_before_the_cleanup_runs_no_cleanup() {
            // The update has succeeded; the Cancel lands before the cleanup
            // starts. Nothing more runs, the log says the old versions were
            // not cleaned up -- without saying the cleanup started -- and
            // the outcome is the update's.
            let upgrade = vec!["/opt/homebrew/bin/brew", "upgrade", "--formula", "wget"];
            let inner = Arc::new(MockRunner::new());
            inner.respond(upgrade.clone(), ok("==> Upgrading wget\n", "", 0));
            let token = CancellationToken::new();
            let runner = Arc::new(CancelAfter {
                inner: inner.clone(),
                after: upgrade.iter().map(|s| s.to_string()).collect(),
                token: token.clone(),
            });
            let adapter = BrewAdapter::new(runner).with_kegs_fn(two_versions);
            let plan = adapter
                .plan(
                    &test_instance(),
                    &request(OpKind::Upgrade, ArtifactKind::Formula, "wget"),
                )
                .await
                .expect("plan");
            let sink = Arc::new(VecSink::new());
            let outcome = adapter
                .execute(&plan, sink.clone(), 7, token)
                .await
                .expect("execute");
            assert_eq!(outcome, Outcome::Succeeded);
            assert_eq!(inner.calls(), vec![upgrade]);
            assert_eq!(
                log_lines(&sink),
                vec![
                    "==> Upgrading wget".to_string(),
                    format!(
                        "{:?}",
                        LogNote::OldVersionsNotCleanedUp {
                            name: "wget".to_string(),
                            exit_code: None,
                        }
                    ),
                ]
            );
        }

        #[tokio::test]
        async fn regression_f01_cleanup_skips_unconfirmed_old_kegs() {
            let runner = Arc::new(MockRunner::new());
            runner.respond(
                vec!["/opt/homebrew/bin/brew", "upgrade", "--formula", "wget"],
                ok("", "", 0),
            );
            runner.respond(
                vec!["/opt/homebrew/bin/brew", "cleanup", "wget"],
                ok("", "", 0),
            );
            let plan = BrewAdapter::new(runner.clone())
                .with_kegs_fn(two_versions)
                .plan(
                    &test_instance(),
                    &request(OpKind::Upgrade, ArtifactKind::Formula, "wget"),
                )
                .await
                .unwrap();
            let sink = Arc::new(VecSink::new());
            let outcome = BrewAdapter::new(runner.clone())
                .with_kegs_fn(|_, _| {
                    Some(Kegs {
                        versions: ["1.24.0", "1.25.0", "1.25.1", "1.26.0"]
                            .map(str::to_string)
                            .to_vec(),
                        pinned: false,
                    })
                })
                .execute(&plan, sink.clone(), 7, CancellationToken::new())
                .await
                .unwrap();
            assert!(matches!(outcome, Outcome::Succeeded));
            assert_eq!(runner.calls().len(), 1, "unconfirmed 1.25.1 must survive");
            assert!(
                format!("{:?}", sink.events.lock().unwrap()).contains("OldVersionsCleanupSkipped")
            );
        }

        #[tokio::test]
        async fn the_cleanup_does_not_run_when_the_settings_no_longer_allow_it_at_its_turn() {
            // Review F4 (r6): the preview found cleanup allowed and planned
            // `brew cleanup wget` after the update. Before the click, or
            // while the update runs, the person turns it off, names wget,
            // pins it, or a brew.env becomes one Banager cannot read (or
            // takes Banager's `1` back, so Homebrew's own update cleaned up
            // already). `brew cleanup wget` checks none of the first
            // (`cleanup.rb:497-519`), so Banager asks again right before it
            // and, when the answer is no longer yes, runs nothing more and
            // says so in the log. The update stands.
            //
            // The last two only once the update has run: at the click they
            // stop the update itself
            // (`an_install_or_update_runs_nothing_when_homebrew_would_now_delete_more_than_its_preview_said`).
            use std::sync::atomic::{AtomicBool, Ordering};
            static UPDATED: AtomicBool = AtomicBool::new(false);
            /// Answers as `inner`, and marks `UPDATED` once it has run
            /// `after`.
            struct MarkAfter {
                inner: Arc<MockRunner>,
                after: Vec<String>,
            }
            #[async_trait]
            impl CommandRunner for MarkAfter {
                async fn run(
                    &self,
                    spec: CommandSpec,
                    on_line: Option<crate::runner::LineCallback>,
                    cancel: CancellationToken,
                ) -> Result<CommandOutput, crate::runner::RunnerError> {
                    let mut argv = vec![spec.program.to_string_lossy().into_owned()];
                    argv.extend(spec.args.iter().cloned());
                    let output = self.inner.run(spec, on_line, cancel).await;
                    if argv == self.after {
                        UPDATED.store(true, Ordering::SeqCst);
                    }
                    output
                }
            }
            type Files = fn(&Path) -> brew_env::EnvFile;
            type Read = fn(&Path, &str) -> Option<Kegs>;
            fn system(bytes: &'static [u8]) -> impl Fn(&Path) -> brew_env::EnvFile {
                move |path| {
                    (path == Path::new(brew_env::SYSTEM_FILE))
                        .then(|| bytes.to_vec())
                        .into()
                }
            }
            let none: Files = |_| brew_env::EnvFile::Skipped;
            let pinned: Read = |_, _| {
                Some(Kegs {
                    versions: vec!["1.24.0".to_string(), "1.25.0".to_string()],
                    pinned: true,
                })
            };
            let unread: Read = |_, _| None;
            let cases: [(&str, Files, Read); 6] = [
                (
                    "opted out",
                    |p| system(b"HOMEBREW_NO_INSTALL_CLEANUP=1\n")(p),
                    two_versions,
                ),
                (
                    "named",
                    |p| system(b"HOMEBREW_NO_CLEANUP_FORMULAE=jq,wget\n")(p),
                    two_versions,
                ),
                (
                    "taken back",
                    |p| {
                        if UPDATED.load(Ordering::SeqCst) {
                            system(b"HOMEBREW_NO_INSTALL_CLEANUP=\n")(p)
                        } else {
                            brew_env::EnvFile::Skipped
                        }
                    },
                    two_versions,
                ),
                (
                    "unknown",
                    |p| {
                        if UPDATED.load(Ordering::SeqCst) && p == Path::new(brew_env::SYSTEM_FILE) {
                            brew_env::EnvFile::Unknown
                        } else {
                            brew_env::EnvFile::Skipped
                        }
                    },
                    two_versions,
                ),
                ("pinned", none, pinned),
                ("unread", none, unread),
            ];
            let upgrade = vec!["/opt/homebrew/bin/brew", "upgrade", "--formula", "wget"];
            let inst = test_instance();
            let req = request(OpKind::Upgrade, ArtifactKind::Formula, "wget");
            let skipped = format!(
                "{:?}",
                LogNote::OldVersionsCleanupSkipped {
                    name: "wget".to_string()
                }
            );
            for (case, files, read) in cases {
                UPDATED.store(false, Ordering::SeqCst);
                let runner = Arc::new(MockRunner::new());
                runner.respond(upgrade.clone(), ok("==> Upgrading wget\n", "", 0));
                let plan = BrewAdapter::new(runner.clone())
                    .with_kegs_fn(two_versions)
                    .plan(&inst, &req)
                    .await
                    .expect("plan");
                assert!(upgrade_then_cleanup(&plan).is_some(), "{case}");
                let sink = Arc::new(VecSink::new());
                let marking = Arc::new(MarkAfter {
                    inner: runner.clone(),
                    after: upgrade.iter().map(|s| s.to_string()).collect(),
                });
                let outcome = BrewAdapter::new(marking)
                    .with_brew_env_fn(files)
                    .with_kegs_fn(read)
                    .execute(&plan, sink.clone(), 7, CancellationToken::new())
                    .await
                    .expect("execute");
                assert_eq!(outcome, Outcome::Succeeded, "{case}");
                assert_eq!(runner.calls(), vec![upgrade.clone()], "{case}");
                assert_eq!(
                    log_lines(&sink),
                    vec!["==> Upgrading wget".to_string(), skipped.clone()],
                    "{case}"
                );
            }
            // Their own variable in Banager's environment, as at the preview.
            let runner = Arc::new(MockRunner::new());
            runner.respond(upgrade.clone(), ok("", "", 0));
            let plan = BrewAdapter::new(runner.clone())
                .with_kegs_fn(two_versions)
                .plan(&inst, &req)
                .await
                .expect("plan");
            let sink = Arc::new(VecSink::new());
            let outcome = BrewAdapter::new(runner.clone())
                .with_kegs_fn(two_versions)
                .with_env_var_fn(|name| {
                    (name == "HOMEBREW_NO_INSTALL_CLEANUP").then(|| OsString::from("1"))
                })
                .execute(&plan, sink.clone(), 7, CancellationToken::new())
                .await
                .expect("execute");
            assert_eq!(outcome, Outcome::Succeeded);
            assert_eq!(runner.calls(), vec![upgrade.clone()]);
            assert_eq!(log_lines(&sink), vec![skipped]);
        }

        #[tokio::test]
        async fn an_install_or_update_runs_nothing_when_homebrew_would_now_delete_more_than_its_preview_said(
        ) {
            // Review of v1-brew's fixes (r6), the reverse of F4: a brew.env
            // edited while the confirmation is open takes Banager's
            // `HOMEBREW_NO_INSTALL_CLEANUP=1` back, or becomes one Banager
            // cannot read. The install or update would then run Homebrew's
            // own cleanup (`Cleanup.install_clean!`) -- the periodic one of
            // every formula when due -- where the preview said other
            // software and its old versions are kept. So the switches are
            // read again at the click, and when Homebrew would now delete
            // more than the preview said -- cleanup, its autoremove, or a
            // formula `HOMEBREW_NO_CLEANUP_FORMULAE` no longer leaves out --
            // nothing runs. Where it deletes what the preview said, or
            // less, or "will" where the preview said "may", it runs.
            type Files = fn(&Path) -> brew_env::EnvFile;
            fn system(bytes: &'static [u8]) -> impl Fn(&Path) -> brew_env::EnvFile {
                move |path| {
                    (path == Path::new(brew_env::SYSTEM_FILE))
                        .then(|| bytes.to_vec())
                        .into()
                }
            }
            let none: Files = |_| brew_env::EnvFile::Skipped;
            let taken_back: Files = |p| system(b"HOMEBREW_NO_INSTALL_CLEANUP=\n")(p);
            let unknown: Files = |p| {
                if p == Path::new(brew_env::SYSTEM_FILE) {
                    brew_env::EnvFile::Unknown
                } else {
                    brew_env::EnvFile::Skipped
                }
            };
            let autoremove_too: Files =
                |p| system(b"HOMEBREW_NO_INSTALL_CLEANUP=\nHOMEBREW_NO_AUTOREMOVE=0\n")(p);
            let jq_left_out: Files =
                |p| system(b"HOMEBREW_NO_INSTALL_CLEANUP=\nHOMEBREW_NO_CLEANUP_FORMULAE=jq\n")(p);
            let jq_and_wget_left_out: Files = |p| {
                system(b"HOMEBREW_NO_INSTALL_CLEANUP=\nHOMEBREW_NO_CLEANUP_FORMULAE=jq,wget\n")(p)
            };
            let upgrade = vec!["/opt/homebrew/bin/brew", "upgrade", "--formula", "wget"];
            let install = vec!["/opt/homebrew/bin/brew", "install", "--formula", "jq"];
            // What, the operation, brew.env at the preview and at the
            // click, and whether the command runs.
            let cases: [(&str, OpKind, Files, Files, bool); 10] = [
                (
                    "Banager's 1 taken back",
                    OpKind::Upgrade,
                    none,
                    taken_back,
                    false,
                ),
                ("unreadable now", OpKind::Upgrade, none, unknown, false),
                ("an install", OpKind::Install, none, taken_back, false),
                (
                    "autoremove back too",
                    OpKind::Upgrade,
                    taken_back,
                    autoremove_too,
                    false,
                ),
                (
                    "jq no longer left out",
                    OpKind::Upgrade,
                    jq_left_out,
                    taken_back,
                    false,
                ),
                (
                    "as previewed",
                    OpKind::Upgrade,
                    taken_back,
                    taken_back,
                    true,
                ),
                ("may, then will", OpKind::Upgrade, unknown, taken_back, true),
                (
                    "one more left out",
                    OpKind::Upgrade,
                    jq_left_out,
                    jq_and_wget_left_out,
                    true,
                ),
                ("an install as previewed", OpKind::Install, none, none, true),
                (
                    "less than previewed",
                    OpKind::Upgrade,
                    taken_back,
                    none,
                    true,
                ),
            ];
            for (case, kind, at_preview, at_click, runs) in cases {
                let (name, command) = match kind {
                    OpKind::Install => ("jq", &install),
                    _ => ("wget", &upgrade),
                };
                let runner = Arc::new(MockRunner::new());
                runner.respond(command.clone(), ok("", "", 0));
                let plan = BrewAdapter::new(runner.clone())
                    .with_brew_env_fn(at_preview)
                    .with_kegs_fn(two_versions)
                    .plan(
                        &test_instance(),
                        &request(kind, ArtifactKind::Formula, name),
                    )
                    .await
                    .expect("plan");
                let outcome = BrewAdapter::new(runner.clone())
                    .with_brew_env_fn(at_click)
                    .with_kegs_fn(two_versions)
                    .execute(&plan, Arc::new(VecSink::new()), 7, CancellationToken::new())
                    .await
                    .expect("execute");
                if runs {
                    assert_eq!(outcome, Outcome::Succeeded, "{case}");
                    assert_eq!(runner.calls().first().expect(case), command, "{case}");
                } else {
                    assert_eq!(
                        outcome,
                        Outcome::BanagerFailed(Fault::HomebrewSettingsChanged),
                        "{case}"
                    );
                    assert!(runner.calls().is_empty(), "{case}: {:?}", runner.calls());
                }
            }
        }

        #[tokio::test]
        async fn an_update_a_cleanup_follows_is_refused_when_brew_env_turns_auto_update_back_on() {
            // The same check as every install and upgrade, at the click:
            // a brew.env that sets HOMEBREW_NO_AUTO_UPDATE to nothing since
            // the preview stops it before anything runs.
            let runner = Arc::new(MockRunner::new());
            let inst = test_instance();
            let plan = BrewAdapter::new(runner.clone())
                .with_kegs_fn(two_versions)
                .plan(
                    &inst,
                    &request(OpKind::Upgrade, ArtifactKind::Formula, "wget"),
                )
                .await
                .expect("plan");
            assert!(upgrade_then_cleanup(&plan).is_some());
            let changed = BrewAdapter::new(runner.clone()).with_brew_env_fn(|path| {
                (path == Path::new(brew_env::SYSTEM_FILE))
                    .then(|| b"HOMEBREW_NO_AUTO_UPDATE=\n".to_vec())
                    .into()
            });
            let result = changed
                .execute(&plan, Arc::new(VecSink::new()), 7, CancellationToken::new())
                .await;
            assert!(result.is_err(), "{result:?}");
            assert!(runner.calls().is_empty(), "{:?}", runner.calls());
        }

        #[tokio::test]
        async fn an_uninstall_of_every_version_is_refused_when_the_formula_is_pinned_since_the_preview(
        ) {
            // `--force` skips Homebrew's own refusal of a pinned formula, so
            // the pin is looked at again at the click, as brew.env is: one
            // pinned in Terminal since the preview -- or a pin record that
            // cannot be looked at now -- stops it before anything runs.
            let runner = Arc::new(MockRunner::new());
            nothing_uses(&runner, "wget");
            let inst = test_instance();
            let plan = BrewAdapter::new(runner.clone())
                .with_kegs_fn(two_versions)
                .plan(
                    &inst,
                    &request(OpKind::Uninstall, ArtifactKind::Formula, "wget"),
                )
                .await
                .expect("plan");
            assert_eq!(
                command_args(&plan),
                ["uninstall", "--formula", "--force", "wget"]
            );
            let planned = runner.calls().len();
            type Read = fn(&Path, &str) -> Option<Kegs>;
            let pinned: Read = |_, _| {
                Some(Kegs {
                    versions: vec!["1.24.0".to_string(), "1.25.0".to_string()],
                    pinned: true,
                })
            };
            let unread: Read = |_, _| None;
            for read in [pinned, unread] {
                let outcome = BrewAdapter::new(runner.clone())
                    .with_kegs_fn(read)
                    .execute(&plan, Arc::new(VecSink::new()), 7, CancellationToken::new())
                    .await
                    .expect("execute");
                // Said as what it is, a change since the preview -- not as
                // an internal error (`AdapterError::Refused` became
                // `Fault::Internal`).
                assert_eq!(
                    outcome,
                    Outcome::BanagerFailed(Fault::FormulaChanged {
                        name: "wget".to_string()
                    })
                );
                assert_eq!(runner.calls().len(), planned, "{:?}", runner.calls());
            }
            // Still not pinned: it runs as the preview showed it.
            runner.respond(
                vec![
                    "/opt/homebrew/bin/brew",
                    "uninstall",
                    "--formula",
                    "--force",
                    "wget",
                ],
                ok("", "", 0),
            );
            let outcome = BrewAdapter::new(runner.clone())
                .with_kegs_fn(two_versions)
                .execute(&plan, Arc::new(VecSink::new()), 7, CancellationToken::new())
                .await
                .expect("execute");
            assert_eq!(outcome, Outcome::Succeeded);
            assert_eq!(runner.calls().len(), planned + 1);
        }

        #[tokio::test]
        async fn an_uninstall_of_every_version_runs_nothing_when_a_version_the_preview_did_not_name_is_installed(
        ) {
            // Review F3 (r6): the preview named 1.24.0 and 1.25.0. An update
            // in Terminal with its cleanup off then installs 1.26.0 while the
            // confirmation is open, or the operation waits in the queue.
            // `brew uninstall --force` deletes every version it finds, 1.26.0
            // too, which the person never saw named: so the Cellar is read
            // again right before the command, and a version the preview did
            // not name stops it -- nothing runs, and the outcome asks for a
            // new look. Fewer versions than the preview named, or the same
            // ones, delete nothing it did not name, and run.
            let uninstall = vec![
                "/opt/homebrew/bin/brew",
                "uninstall",
                "--formula",
                "--force",
                "wget",
            ];
            let runner = Arc::new(MockRunner::new());
            nothing_uses(&runner, "wget");
            runner.respond(uninstall.clone(), ok("", "", 0));
            let plan = BrewAdapter::new(runner.clone())
                .with_kegs_fn(two_versions)
                .plan(
                    &test_instance(),
                    &request(OpKind::Uninstall, ArtifactKind::Formula, "wget"),
                )
                .await
                .expect("plan");
            assert_eq!(
                command_args(&plan),
                ["uninstall", "--formula", "--force", "wget"]
            );
            let planned = runner.calls().len();
            type Read = fn(&Path, &str) -> Option<Kegs>;
            let one_more: Read = |_, _| {
                Some(Kegs {
                    versions: vec![
                        "1.24.0".to_string(),
                        "1.25.0".to_string(),
                        "1.26.0".to_string(),
                    ],
                    pinned: false,
                })
            };
            let another: Read = |_, _| {
                Some(Kegs {
                    versions: vec!["1.25.0".to_string(), "1.26.0".to_string()],
                    pinned: false,
                })
            };
            for read in [one_more, another] {
                let outcome = BrewAdapter::new(runner.clone())
                    .with_kegs_fn(read)
                    .execute(&plan, Arc::new(VecSink::new()), 7, CancellationToken::new())
                    .await
                    .expect("execute");
                assert_eq!(
                    outcome,
                    Outcome::BanagerFailed(Fault::FormulaChanged {
                        name: "wget".to_string()
                    })
                );
                assert_eq!(runner.calls().len(), planned, "{:?}", runner.calls());
            }
            let fewer: Read = |_, _| {
                Some(Kegs {
                    versions: vec!["1.25.0".to_string()],
                    pinned: false,
                })
            };
            for (read, ran) in [(fewer, 1), (two_versions as Read, 2)] {
                let outcome = BrewAdapter::new(runner.clone())
                    .with_kegs_fn(read)
                    .execute(&plan, Arc::new(VecSink::new()), 7, CancellationToken::new())
                    .await
                    .expect("execute");
                assert_eq!(outcome, Outcome::Succeeded);
                assert_eq!(runner.calls().len(), planned + ran);
                assert_eq!(runner.calls().last().expect("a call"), &uninstall);
            }
        }

        #[tokio::test]
        async fn an_uninstall_of_one_version_runs_nothing_when_another_version_is_installed_since_the_preview(
        ) {
            // Review of v1-brew's fixes (r6): the preview found one version,
            // 1.25.0, and passes no `--force` -- it says it removes "this
            // version". An update in Terminal with its cleanup off then
            // installs and links 1.26.0 while the confirmation is open. A
            // plain `brew uninstall` deletes the version opt/ points to
            // (`resolve_default_keg`, `cli/named_args.rb:567-578`): 1.26.0,
            // which the person never saw, leaving the 1.25.0 they meant. So
            // the Cellar is read again right before the command, and a
            // second version with no pin -- what would now make the preview
            // pass `--force` -- stops it: nothing runs.
            let uninstall = vec!["/opt/homebrew/bin/brew", "uninstall", "--formula", "wget"];
            let runner = Arc::new(MockRunner::new());
            nothing_uses(&runner, "wget");
            runner.respond(uninstall.clone(), ok("", "", 0));
            type Read = fn(&Path, &str) -> Option<Kegs>;
            let one: Read = |_, _| {
                Some(Kegs {
                    versions: vec!["1.25.0".to_string()],
                    pinned: false,
                })
            };
            let plan = BrewAdapter::new(runner.clone())
                .with_kegs_fn(one)
                .plan(
                    &test_instance(),
                    &request(OpKind::Uninstall, ArtifactKind::Formula, "wget"),
                )
                .await
                .expect("plan");
            assert_eq!(command_args(&plan), ["uninstall", "--formula", "wget"]);
            let planned = runner.calls().len();
            let one_more: Read = |_, _| {
                Some(Kegs {
                    versions: vec!["1.25.0".to_string(), "1.26.0".to_string()],
                    pinned: false,
                })
            };
            let outcome = BrewAdapter::new(runner.clone())
                .with_kegs_fn(one_more)
                .execute(&plan, Arc::new(VecSink::new()), 7, CancellationToken::new())
                .await
                .expect("execute");
            assert_eq!(
                outcome,
                Outcome::BanagerFailed(Fault::FormulaChanged {
                    name: "wget".to_string()
                })
            );
            assert_eq!(runner.calls().len(), planned, "{:?}", runner.calls());
            // Each of these runs as before: the same one version; another
            // one in its place, which is the whole of the formula the
            // person asked to remove; two, pinned, which Homebrew refuses
            // by itself without `--force` (`uninstall.rb:45-53`); and a
            // Cellar that cannot be read now, as the preview may not have.
            let replaced: Read = |_, _| {
                Some(Kegs {
                    versions: vec!["1.26.0".to_string()],
                    pinned: false,
                })
            };
            let pinned: Read = |_, _| {
                Some(Kegs {
                    versions: vec!["1.25.0".to_string(), "1.26.0".to_string()],
                    pinned: true,
                })
            };
            let unread: Read = |_, _| None;
            for (ran, read) in [one, replaced, pinned, unread].into_iter().enumerate() {
                let outcome = BrewAdapter::new(runner.clone())
                    .with_kegs_fn(read)
                    .execute(&plan, Arc::new(VecSink::new()), 7, CancellationToken::new())
                    .await
                    .expect("execute");
                assert_eq!(outcome, Outcome::Succeeded);
                assert_eq!(runner.calls().len(), planned + ran + 1);
                assert_eq!(runner.calls().last().expect("a call"), &uninstall);
            }
        }
    }
}
