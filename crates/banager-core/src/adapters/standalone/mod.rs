//! Tools installed by their own installer rather than by a package
//! manager -- Claude Code's native install in this step; Antigravity CLI,
//! Grok Build and rustup in the steps after it (phase 4 spec, §一 D1-D7).
//!
//! One type, `StandaloneAdapter`, driven by one `&'static Recipe` per tool
//! and registered once per tool under the adapter id `standalone-<tool>`,
//! so everything keyed by adapter id today -- the front end's labels,
//! `verified_versions`, the fixture directory, the concurrent detect
//! fan-out -- works for each tool without a special case, and the logic
//! exists once. The instance *is* the native install: `exe_path` is the
//! launcher the installer wrote (`~/.local/bin/claude`, a symlink),
//! `prefix` the tool's own root, and the one artifact under it is the
//! tool itself (`ArtifactKind::Binary`).
//!
//! `recipe` holds the shape of a tool, `recipes` the tools, `route`
//! answers "is this launcher this route's, and which copy runs when its
//! name is typed", `latest` parses and compares versions.

pub mod latest;
pub mod recipe;
pub mod recipes;
pub mod removal;
pub mod route;
pub mod rustup;

use self::recipe::{CommandUninstall, Latest, Recipe, Uninstall};
use self::route::Probe;
use crate::adapters::{
    ensure_instance_match, lookup_failure_reason, reconcile_from, run_plan, uncheckable_candidate,
    validate_package_name, Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome,
};
use crate::events::{EventSink, OpId};
use crate::http::{HttpClient, HttpRequest};
use crate::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, Fault, InstallReason, InstalledArtifact, InstanceNote,
    InstanceStatus, ManagerInstance, OpKind, OpRequest, Outcome, Plan, PlanAction, Reconciled,
    ResourceLock, Scope, SearchHit, Unavailable, UninstallBlocked, UpdateBlocked, UpdateCandidate,
    UpdateChannel,
};
use crate::runner::{CommandRunner, CommandSpec, HostEnv, OutputUse};
use crate::trash::Trasher;
use async_trait::async_trait;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// How long a version read may take before the runner stops it (the same
/// 30 s every adapter gives `--version`); `detect` then reports the
/// instance as not responding.
const VERSION_TIMEOUT: Duration = Duration::from_secs(30);

/// What `detect` learned that the `Adapter` methods without a `HostEnv`
/// need later -- the same seat `CargoAdapter.binstall` is (detect writes,
/// later calls read; `Session` always detects before it asks anything
/// else of an instance). `home`: `check_updates` finds
/// `~/.claude/settings.json` with it, the removal expands its paths
/// against it, and the rustup recipe's uninstall warnings read the shell
/// startup files under it. `euid`: the removal's check 3 compares each
/// path's owner with it (`removal::plan_removal`). `cargo_home`:
/// `CARGO_HOME` by the `home` crate's rule (`cargo::cargo_home_of`, the
/// rule `CargoAdapter::detect` names its instance by; `None` for a
/// relative value, unsupported), which `seated_detected_for` and
/// `execute` expand a `$CARGO_HOME` route under, and which the rustup
/// recipe's `extra_locks` spells the cargo lock from and its gate and
/// warnings read `.crates2.json` and `bin/` under. `rustup_home`:
/// `RUSTUP_HOME` by the same rule (`path_env::tool_home`), which the
/// rustup recipe's gate compares with `~/.rustup` and its warnings list
/// `toolchains/` under. `zdotdir`: `ZDOTDIR` raw, which the rustup
/// recipe's startup-file model visits `.zshenv`/`.zprofile` under, as
/// rustup's own cleanup does. The rustup recipe's readers of the last
/// three are `rustup::{extra_locks, uninstall_blocked,
/// uninstall_preview}`, which the `RUSTUP` recipe (`recipes.rs`) names
/// and `plan` and `inventory` call through it. The seat is bound to an
/// instance by `seated_detected_for`. `Clone`, so `plan` and `execute`
/// take a copy out of the mutex before they await anything, and the
/// removal owns one for the blocking pool (`removal::Job`).
#[derive(Clone, Debug)]
pub struct Detected {
    pub home: PathBuf,
    pub euid: u32,
    pub cargo_home: Option<PathBuf>,
    pub rustup_home: Option<PathBuf>,
    pub zdotdir: Option<PathBuf>,
}

/// What `inventory` read at the launcher, kept for the `check_updates`
/// that follows it -- so the Installed page's row and the Updates page's
/// candidate come from one reading of the disk, and Claude Code updating
/// itself between the two cannot put one version on each page (B's Astra
/// finding B-2). `Session::refresh` calls `inventory` and then
/// `check_updates` for each instance, in that order, under the instance's
/// lock (`refresh_round` in session/refresh.rs), and nothing else calls
/// `check_updates`. `check_updates` takes the reading out, so one
/// inventory feeds one check, and a check with nothing to take is
/// refused. `reconcile` reads the disk too but not through `inventory`,
/// and leaves this alone.
#[derive(Clone, Debug)]
enum Reading {
    /// Not the install detect listed any more (`inventory` says which
    /// way): `inventory` refused with this reason, and `check_updates`
    /// refuses with the same, so `refresh` keeps the previous round's rows
    /// on both pages and marks them stale.
    Changed(String),
    /// The dangling launcher detect listed, still: no version to compare.
    LauncherOnly,
    /// This route's launcher, and what `--version` said (`None`: it did
    /// not answer).
    Present { version: Option<String> },
}

/// One look at the launcher, from the disk now -- never detect's answer
/// cached: `refresh` reads under the instance lock and `run_operation`'s
/// reconcile after an operation must see what is there now (spec §3.6).
/// The probe, and, for this route's launcher, its answer to `--version`.
/// Read by `inventory` and `reconcile`, through `look`.
struct Look {
    probe: Probe,
    /// `None` when the launcher is not this route's (there is no program
    /// to ask) or did not answer.
    version: Option<String>,
}

/// One tool installed by its own installer, as the `Adapter` contract
/// sees it. Built once per `Recipe` by `all()`; the instance it detects
/// *is* the native install (spec D2).
pub struct StandaloneAdapter {
    recipe: &'static Recipe,
    meta: AdapterMeta,
    runner: Arc<dyn CommandRunner>,
    /// The request an HTTP `Latest` source makes -- a channel pointer, a
    /// release file, a manifest -- in `check_updates` (`published`).
    http: Arc<dyn HttpClient>,
    /// The system's "move to Trash", for a path-list uninstall
    /// (`removal::execute_removal`, from `execute`): `RealTrasher` in
    /// production (`Session::new`), `MockTrasher` in tests -- injected
    /// like the runner and the client.
    trasher: Arc<dyn Trasher>,
    /// The gap a path-list uninstall keeps after each move to the Trash
    /// (`removal::PUT_BACK_SETTLE`); zero in tests (`with_trash_gap`).
    /// Read by `execute`.
    trash_gap: Duration,
    /// When this process last moved an item to the Trash, and the turn at
    /// it (`removal::LastMove`): one for every adapter `all()` builds, so
    /// the gap after each move holds across two tools' uninstalls running
    /// at the same time; one of its own for an adapter `new` builds alone.
    /// Read by `execute`.
    last_move: Arc<removal::LastMove>,
    detected: Mutex<Option<Detected>>,
    /// The CPU architecture this Banager runs as (`std::env::consts::ARCH`;
    /// `with_arch` in tests): a `Latest::HttpJsonField` manifest is fetched
    /// only on the architectures it was verified for
    /// (`latest::manifest_arch_allowed`). Read by `published`.
    arch: &'static str,
    /// Written by `inventory`, taken by `check_updates` (`Reading`).
    inventoried: Mutex<Option<Reading>>,
}

/// What the world knows about this tool's newest version, per the recipe's
/// `Latest` (`StandaloneAdapter::published`).
enum Published {
    /// A version to compare with the installed one (a channel pointer, a
    /// release file, a manifest).
    Version(String),
    /// The tool's own verdict (grok's `update --check --json`): shown as
    /// answered, never compared (spec §4.3).
    ToolSays(latest::UpdateCheck),
}

impl StandaloneAdapter {
    /// Panics on a meta file that does not parse or whose `id` is not
    /// `standalone-<recipe.id>`: both are compile-time data
    /// (`recipes::tests` holds every recipe to them), never a state of a
    /// user's Mac.
    pub fn new(
        recipe: &'static Recipe,
        runner: Arc<dyn CommandRunner>,
        http: Arc<dyn HttpClient>,
        trasher: Arc<dyn Trasher>,
    ) -> StandaloneAdapter {
        let meta = AdapterMeta::from_toml(recipe.meta_toml).unwrap_or_else(|e| {
            panic!(
                "adapters/meta/standalone-{}.toml must parse: {e}",
                recipe.id
            )
        });
        assert_eq!(
            meta.id,
            format!("standalone-{}", recipe.id),
            "the meta file's id must be the recipe's adapter id"
        );
        StandaloneAdapter {
            recipe,
            meta,
            runner,
            http,
            trasher,
            trash_gap: removal::PUT_BACK_SETTLE,
            last_move: Arc::default(),
            detected: Mutex::new(None),
            arch: std::env::consts::ARCH,
            inventoried: Mutex::new(None),
        }
    }

    /// Test seam, like `BrewAdapter::with_background_change`: the gap a
    /// path-list uninstall keeps after each move -- zero in tests, so no
    /// test waits seconds per item. Public so `tests/` can use it too.
    pub fn with_trash_gap(mut self, gap: Duration) -> StandaloneAdapter {
        self.trash_gap = gap;
        self
    }

    /// Test seam: the architecture `published` believes it runs on, so
    /// both the Apple-silicon and the Intel branch of a manifest lookup
    /// are tested on whatever machine runs the tests.
    pub fn with_arch(mut self, arch: &'static str) -> StandaloneAdapter {
        self.arch = arch;
        self
    }

    /// Spec §3.3, steps 1-6: the launcher at the installer's fixed path
    /// (never `resolve_exe`, spec D3), the fingerprint, the version read,
    /// the PATH note. One instance or none; never two.
    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        // The Cargo home by the `home` crate's rule (`None`: a relative
        // CARGO_HOME, which no path of Banager's can stand for). A recipe
        // under `$CARGO_HOME` then has no launcher to look for; a `~/`
        // recipe is unaffected and seats `None`.
        let cargo_home = crate::adapters::cargo::cargo_home_of(env);
        let (Some(launcher), Some(root)) = (
            route::expand_route(&env.home, cargo_home.as_deref(), self.recipe.route.launcher),
            route::expand_route(&env.home, cargo_home.as_deref(), self.recipe.route.root),
        ) else {
            return Vec::new();
        };
        let (version, unavailable, notes) =
            match route::probe(self.recipe.route.kind, &launcher, &root) {
                Probe::Absent => return Vec::new(),
                // No program to ask: no version read; not unavailable,
                // because nothing about the source has stopped answering --
                // the state is on the row's note, and from step C the
                // uninstall that finishes it must be allowed on this
                // instance (spec Q17).
                Probe::LauncherOnly => (None, None, vec![InstanceNote::LauncherOnly]),
                Probe::Present { real } => {
                    let version = self.read_version(&launcher).await;
                    // The state axis, exactly as uv's rule: the launcher is
                    // there and is ours, it just did not answer.
                    let unavailable = version.is_none().then_some(Unavailable::NotResponding);
                    let notes = route::shadow_note(self.recipe.id, env, &real)
                        .into_iter()
                        .collect();
                    (version, unavailable, notes)
                }
            };
        *self.detected.lock().unwrap() = Some(Detected {
            home: env.home.clone(),
            euid: env.euid,
            cargo_home,
            rustup_home: crate::runner::path_env::tool_home(
                env.rustup_home.as_deref(),
                &env.home,
                ".rustup",
            ),
            zdotdir: env.zdotdir.clone(),
        });
        let unverified_version = self.meta.unverified_version(&version);
        vec![ManagerInstance {
            // Bare adapter id: one native install per tool is the real
            // cardinality (spec §2.1), and this id is persisted in
            // `Settings.ignored_updates` and `Settings.skipped_versions`.
            id: crate::model::instance_id(&self.meta.id, None),
            adapter_id: self.meta.id.clone(),
            // The launcher itself: the program every plan runs, the path
            // the notices take the command name from, the raw path the
            // Unknown page's rule 0 matches.
            exe_path: launcher,
            // The tool's own root: what the launcher must resolve into,
            // and an owned root for the Unknown page's rule 3
            // (`scan::owned_roots`).
            prefix: root,
            scope: Scope::User,
            status: InstanceStatus { unavailable, notes },
            version,
            unverified_version,
            read_only_reason: None,
        }]
    }

    /// `<launcher> --version` with the recipe's environment (the updater
    /// switched off, spec §3.4), parsed per the recipe; `None` when it did
    /// not exit 0, timed out, could not be spawned, or printed no version.
    async fn read_version(&self, launcher: &Path) -> Option<String> {
        let cmd = &self.recipe.version;
        let output = self
            .runner
            .run(
                CommandSpec {
                    program: launcher.to_path_buf(),
                    args: cmd.args.iter().map(|a| a.to_string()).collect(),
                    env: cmd
                        .env
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect(),
                    cwd: None,
                    timeout: VERSION_TIMEOUT,
                    output_use: OutputUse::Parsed,
                },
                None,
                CancellationToken::new(),
            )
            .await;
        match output {
            Ok(o) if o.exit_code == Some(0) && !o.timed_out && !o.cancelled => {
                latest::parse_version(&o.stdout, cmd.parse)
            }
            _ => None,
        }
    }

    /// The one artifact's key: the tool id as the name (what `OpRequest`
    /// hands back and `reconcile_from` matches on), `Binary` as the kind
    /// (a standalone tool is a binary; `ArtifactKey` includes the instance
    /// id, so cargo's `Binary` artifacts never collide).
    fn artifact_key(&self, inst: &ManagerInstance) -> ArtifactKey {
        ArtifactKey {
            instance_id: inst.id.clone(),
            kind: ArtifactKind::Binary,
            name: self.recipe.id.to_string(),
        }
    }

    /// What `detect` seated, bound to `inst` -- for the plan arms and the
    /// inventory gate that need it: rustup's cargo lock, its
    /// standard-layout gate and its uninstall warnings. The seat is one
    /// slot that the latest `detect` overwrites, and `plan` is handed an
    /// instance: a plan for an instance detected under home A after a
    /// detect under home B would run A's launcher with B's locks and
    /// warnings. So the launcher and root the seat expands to must be the
    /// instance's own `exe_path` and `prefix`; anything else is
    /// `Refused`, and so is a plan asked before any detect, which
    /// `Session` never does (spec §3.2). A copy, so no mutex guard is
    /// held across anything `plan` awaits. Read by both of `plan`'s arms
    /// and, for a `Command` uninstall, by `inventory`'s gate (`rows`).
    fn seated_detected_for(&self, inst: &ManagerInstance) -> Result<Detected, AdapterError> {
        let seat = self.detected.lock().unwrap().clone().ok_or_else(|| {
            AdapterError::Refused(format!(
                "{} has not been detected yet, so nothing can be planned for it",
                self.meta.name
            ))
        })?;
        let route = &self.recipe.route;
        let launcher = route::expand_route(&seat.home, seat.cargo_home.as_deref(), route.launcher);
        let root = route::expand_route(&seat.home, seat.cargo_home.as_deref(), route.root);
        if launcher.as_deref() != Some(inst.exe_path.as_path())
            || root.as_deref() != Some(inst.prefix.as_path())
        {
            return Err(AdapterError::Refused(format!(
                "{} was last detected under a different home than this instance's; refresh and try again",
                self.meta.name
            )));
        }
        Ok(seat)
    }

    /// Every lock a plan for this instance holds: its own, then the
    /// recipe's `extra_locks` (rustup: the cargo instance's). Every plan
    /// arm uses it, so none can forget the second lock.
    fn locks(&self, inst: &ManagerInstance, detected: &Detected) -> Vec<ResourceLock> {
        let mut locks = vec![ResourceLock(inst.id.clone())];
        locks.extend((self.recipe.extra_locks)(detected));
        locks
    }

    /// `Uninstall::Command` (spec §6.4): the tool's own uninstall argv
    /// against the launcher, refused with the recipe's reason when its
    /// preview's gate says this layout is not offered, otherwise with the
    /// warnings that same preview built from the seat and the disk. The
    /// recipe's `preview` is asked once and answers both: asking `blocked`
    /// and then a warnings function was two readings of the disk, and a
    /// layout that changed between them -- passing the first, refused by
    /// the second -- gave a plan with no warnings at all (step E's
    /// whole-step review). Nothing is run here (the preview must not run
    /// rustup: plan ruling 4). Through `run_plan` like every command,
    /// after `execute` has asked the gate (`blocked`) once more;
    /// `affected` stays empty because a non-empty list disables Confirm
    /// and nothing here breaks another package.
    fn command_uninstall_plan(
        &self,
        inst: &ManagerInstance,
        req: &OpRequest,
        detected: &Detected,
        cmd: &CommandUninstall,
    ) -> Result<Plan, AdapterError> {
        let warnings =
            (cmd.preview)(detected).map_err(|refusal| AdapterError::UninstallBlocked {
                reason: refusal.reason,
            })?;
        Ok(Plan {
            request: req.clone(),
            action: PlanAction::Command {
                program: inst.exe_path.clone(),
                args: cmd.args.iter().map(|a| a.to_string()).collect(),
                env: Vec::new(),
            },
            needs_password: false,
            locks: self.locks(inst, detected),
            cancel_policy: cmd.cancel,
            warnings,
            affected: Vec::new(),
            timeout_secs: cmd.timeout_secs,
        })
    }

    /// `Look`: the probe at the instance's own `exe_path` and `prefix`,
    /// which detect expanded, then `--version` when the launcher is this
    /// route's.
    async fn look(&self, inst: &ManagerInstance) -> Look {
        let probe = route::probe(self.recipe.route.kind, &inst.exe_path, &inst.prefix);
        let version = match probe {
            Probe::Present { .. } => self.read_version(&inst.exe_path).await,
            Probe::Absent | Probe::LauncherOnly => None,
        };
        Look { probe, version }
    }

    /// The tool itself, read from the disk again (`look`), not detect's
    /// answer cached. What is there has to be the install detect listed:
    /// the launcher whole, or the launcher alone (the instance then carries
    /// `InstanceNote::LauncherOnly`, which only `detect` writes). Anything
    /// else -- the launcher gone; the program files gone behind a launcher
    /// detect saw whole; the program files back behind one detect saw
    /// dangling -- is refused, because no artifact list could say it: the
    /// instance row is detect's, and would stay healthy beside no row, or
    /// beside an empty-version row, with the Updates page saying
    /// everything is up to date (B's Astra finding B-2). `refresh` turns
    /// the refusal into "this refresh did not finish for this source": the
    /// previous round's rows are kept and marked stale, and the next
    /// refresh detects what is there now. Whatever the answer, the reading
    /// is left for `check_updates` (`Reading`).
    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let look = self.look(inst).await;
        let listed_launcher_only = inst.status.notes.contains(&InstanceNote::LauncherOnly);
        let reading = match (&look.probe, listed_launcher_only) {
            (Probe::Absent, _) => Reading::Changed(format!(
                "{}'s launcher {} is gone since it was detected",
                self.meta.name,
                inst.exe_path.display()
            )),
            (Probe::LauncherOnly, false) => Reading::Changed(format!(
                "{}'s program files are gone since it was detected; only the launcher {} is left",
                self.meta.name,
                inst.exe_path.display()
            )),
            (Probe::Present { .. }, true) => Reading::Changed(format!(
                "{}'s program files are back since it was detected with the launcher {} alone",
                self.meta.name,
                inst.exe_path.display()
            )),
            (Probe::LauncherOnly, true) => Reading::LauncherOnly,
            (Probe::Present { .. }, false) => Reading::Present {
                version: look.version.clone(),
            },
        };
        *self.inventoried.lock().unwrap() = Some(reading.clone());
        match reading {
            Reading::Changed(reason) => Err(AdapterError::Refused(reason)),
            Reading::LauncherOnly | Reading::Present { .. } => Ok(self.rows(inst, look)),
        }
    }

    /// The artifact list for what `look` found: nothing for `Absent`; for
    /// the launcher alone, the row with no version and no path that an
    /// Uninstall finishes (spec §3.6); for this route's launcher, the tool
    /// with the version it answered (empty when it did not) and the real
    /// binary. Read by `inventory` and `reconcile`.
    fn rows(&self, inst: &ManagerInstance, look: Look) -> Vec<InstalledArtifact> {
        let (version, path) = match look.probe {
            Probe::Absent => return Vec::new(),
            Probe::LauncherOnly => (String::new(), None),
            Probe::Present { real } => (look.version.unwrap_or_default(), Some(real)),
        };
        vec![InstalledArtifact {
            key: self.artifact_key(inst),
            // From the meta TOML, not a second copy in the recipe.
            display_name: self.meta.name.clone(),
            version,
            // The user ran the installer themselves; `Dependency` would fold
            // the row behind "N components".
            reason: InstallReason::Requested,
            // A sentence has to be localised, and this field is a bare
            // string that does not know the UI language: the summary is
            // left to the front end, keyed by adapter id
            // (`STANDALONE_SUMMARY_KEYS`, which arrives with Task 10 of the
            // phase 4 step B plan).
            description: None,
            homepage: Some(self.meta.homepage.clone()),
            size_bytes: None,
            installed_at: None,
            // The real binary: the Unknown page's rule 2.
            path,
            // For the Updates page's `selfUpdatingHint` sentence, which
            // arrives with Task 10 of the phase 4 step B plan.
            auto_updates: self.recipe.self_updates,
            // No uninstall method at all (spec §6.1 "Neither"; none in
            // the first batch, the second batch's Ollama.app):
            // `NoSafeMethod` -- the gate refuses, the page hides the button
            // and says why. A path list: offered. A command: the recipe's
            // gate decides from the seat -- rustup offers its own uninstall
            // only for the standard layout (plan ruling 18) -- and a seat
            // that is missing or describes another home reads as blocked,
            // never as offered.
            uninstall_blocked: match &self.recipe.uninstall {
                None => Some(UninstallBlocked::NoSafeMethod),
                Some(Uninstall::Paths { .. }) => None,
                Some(Uninstall::Command(cmd)) => self
                    .seated_detected_for(inst)
                    .map_or(Some(UninstallBlocked::NoSafeMethod), |seat| {
                        (cmd.blocked)(&seat).map(|refusal| refusal.reason)
                    }),
            },
            facts: Default::default(),
        }]
    }

    /// Discovery is phase 5; a standalone tool has nothing to search anyway.
    pub async fn search(
        &self,
        _inst: &ManagerInstance,
        _query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        Err(AdapterError::Unsupported(format!(
            "{} is one tool with nothing to search; installing tools is phase 5",
            self.meta.name
        )))
    }

    /// Spec §4.1/§4.3/D4/D5: the installed version `inventory` read just
    /// before this (`Reading`) against the published one; a candidate only
    /// when the published one is greater, comparing dotted integers --
    /// Claude Code's `stable` pointer sits behind its `latest`, so
    /// "different" would be a downgrade badge. Never detect's
    /// `inst.version`, which predates the inventory, and never a read of
    /// its own, which could postdate it: Claude Code updating itself
    /// between the two would then put one version on the Installed page
    /// and another on the Updates page (B's Astra finding B-2); a
    /// self-update that lands between the inventory and this is listed by
    /// the next refresh, on both pages. A launcher-only row has no
    /// installed version to compare: no request and no row. An inventory
    /// that refused because the install is not what detect listed is
    /// refused here with the same reason, so `refresh` keeps the previous
    /// candidates beside the previous rows. Anything else that stops the
    /// comparison (the version read having failed, no network, a non-200, a
    /// body that is not a version, an incomparable pair) is one "could not
    /// check" row, never an `Err`: a failed lookup is not knowing, and an
    /// `Err` would hold the whole source stale.
    ///
    /// `include_self_updating` is not read: that is Homebrew's `--greedy`
    /// for casks whose live version `brew outdated` cannot see. This badge
    /// compares the launcher's live version and is true whatever the
    /// switch says; that the tool usually updates itself is for the row to
    /// say (`selfUpdatingHint`, which arrives with Task 10 of the phase 4
    /// step B plan), not hidden behind a setting.
    ///
    /// A recipe with no `upgrade` (agy) gets its candidate with
    /// `UpdateBlocked::SelfUpdatesOnly`: no button, a badge, and a sentence
    /// saying to open the tool. A `Latest::Command` recipe (grok) is asked
    /// itself and believed: `updateAvailable` decides, and `latestVersion`
    /// is shown as printed (`Published::ToolSays`).
    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        let reading = self.inventoried.lock().unwrap().take();
        let version = match reading {
            // Unreachable through `Session`, which inventories before it
            // checks (`refresh_round`), so no sentence of its own.
            None => {
                return Err(AdapterError::Refused(format!(
                    "{} has not been inventoried since its last update check",
                    self.meta.name
                )))
            }
            Some(Reading::Changed(reason)) => return Err(AdapterError::Refused(reason)),
            Some(Reading::LauncherOnly) => return Ok(CheckOutcome::default()),
            Some(Reading::Present { version }) => version,
        };
        let key = self.artifact_key(inst);
        let Some(current) = version else {
            return Ok(vec![uncheckable_candidate(
                key,
                inst.version.clone().unwrap_or_default(),
                UpdateChannel::Registry,
                "cannot read the installed version now".to_string(),
            )]
            .into());
        };
        let decided: Result<Option<(String, UpdateChannel)>, String> =
            match self.published(&inst.exe_path).await {
                Err(reason) => Err(reason),
                // The tool's own verdict, as answered (spec §4.3).
                Ok(Published::ToolSays(check)) => {
                    Ok(check.available.then_some((check.latest, UpdateChannel::Native)))
                }
                Ok(Published::Version(remote)) => match latest::compare_dotted(&current, &remote) {
                    Some(Ordering::Less) => Ok(Some((remote, UpdateChannel::Registry))),
                    Some(Ordering::Equal | Ordering::Greater) => Ok(None),
                    None => Err(format!(
                        "cannot compare the installed version {current:?} with the published {remote:?}"
                    )),
                },
            };
        Ok(match decided {
            Err(reason) => vec![uncheckable_candidate(
                key,
                current,
                UpdateChannel::Registry,
                reason,
            )],
            Ok(None) => Vec::new(),
            Ok(Some((target, channel))) => vec![UpdateCandidate {
                key,
                current,
                target,
                channel,
                checkable: true,
                warnings: Vec::new(),
                // A tool with no update command Banager may run: the newer
                // version is real and has no button (spec §4.4, D5 item 4).
                blocked: self
                    .recipe
                    .upgrade
                    .is_none()
                    .then_some(UpdateBlocked::SelfUpdatesOnly),
            }],
        }
        .into())
    }

    /// What the world knows about this tool's newest version, per the
    /// recipe's `Latest`: a version to compare with the installed one
    /// (`Published::Version`), or the tool's own verdict
    /// (`Published::ToolSays`, grok). `Err` is the one-line reason of an
    /// uncheckable row.
    async fn published(&self, launcher: &Path) -> Result<Published, String> {
        match self.recipe.latest {
            Latest::ClaudeChannel { base } => {
                // `home` from detect's seat; before any detect (which
                // `Session` never does) the default channel is as good an
                // answer as any.
                let home = self
                    .detected
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|d| d.home.clone());
                let channel = match home {
                    Some(home) => latest::claude_channel(&home),
                    None => latest::CHANNEL_LATEST,
                };
                let url = format!("{base}/{channel}");
                let resp = self
                    .http
                    .send(HttpRequest {
                        method: "GET",
                        url,
                        headers: Vec::new(),
                        timeout: Duration::from_secs(30),
                    })
                    .await
                    .map_err(|e| format!("downloads.claude.ai request failed: {e}"))?;
                if resp.status != 200 {
                    return Err(format!(
                        "downloads.claude.ai returned status {}",
                        resp.status
                    ));
                }
                latest::parse_channel_body(&resp.body).map(Published::Version)
            }
            Latest::HttpTomlVersion { url } => {
                let resp = self
                    .http
                    .send(HttpRequest {
                        method: "GET",
                        url: url.to_string(),
                        headers: Vec::new(),
                        timeout: Duration::from_secs(30),
                    })
                    .await
                    .map_err(|e| format!("request to {url} failed: {e}"))?;
                if resp.status != 200 {
                    return Err(format!("{url} returned status {}", resp.status));
                }
                latest::parse_release_stable_toml(&resp.body).map(Published::Version)
            }
            Latest::HttpJsonField { url, field } => {
                // Only where the manifest URL was verified (Apple silicon);
                // elsewhere the row says so and nothing is sent.
                latest::manifest_arch_allowed(self.arch)?;
                let resp = self
                    .http
                    .send(HttpRequest {
                        method: "GET",
                        url: url.to_string(),
                        headers: Vec::new(),
                        timeout: Duration::from_secs(30),
                    })
                    .await
                    .map_err(|e| format!("request to {url} failed: {e}"))?;
                if resp.status != 200 {
                    return Err(format!("{url} returned status {}", resp.status));
                }
                latest::parse_json_field(&resp.body, field).map(Published::Version)
            }
            Latest::Command {
                args,
                timeout_secs,
                latest_field,
                available_field,
                error_field,
            } => {
                // The tool's own read-only check, against the launcher, with
                // no environment of Banager's (spec §3.4's variables are for
                // the version read). A failure names it as the user would
                // type it: `recipe.id` is the command's name.
                let shown = format!("{} {}", self.recipe.id, args.join(" "));
                let output = self
                    .runner
                    .run(
                        CommandSpec {
                            program: launcher.to_path_buf(),
                            args: args.iter().map(|a| a.to_string()).collect(),
                            env: Vec::new(),
                            cwd: None,
                            timeout: Duration::from_secs(timeout_secs),
                            output_use: OutputUse::Parsed,
                        },
                        None,
                        CancellationToken::new(),
                    )
                    .await
                    .map_err(|e| format!("could not run `{shown}`: {e}"))?;
                if output.timed_out || output.cancelled {
                    return Err(format!("`{shown}` did not finish within {timeout_secs} s"));
                }
                if output.exit_code != Some(0) {
                    // Worded as every other lookup that runs a command: the
                    // tool's own first line of stderr, or, when it said
                    // nothing, its exit code (or that it did not finish).
                    return Err(lookup_failure_reason(
                        &shown,
                        output.exit_code,
                        &output.stderr,
                    ));
                }
                latest::parse_update_check(
                    &output.stdout,
                    latest_field,
                    available_field,
                    error_field,
                )
                .map(Published::ToolSays)
            }
        }
    }

    /// Spec §五: the tool's own documented update command, run against the
    /// launcher through `run_plan` unchanged, or, for a recipe with none
    /// (agy), `UpdateBlocked::SelfUpdatesOnly` -- the gate
    /// (`blocked_upgrade`) refuses that first. `Install` is `Unsupported`
    /// (the installer is the tool's own and Banager never runs it;
    /// installing tools is phase 5). `Uninstall` is the recipe's path list
    /// as a `TrashPaths` plan under the removal's checks (spec §6.2-§6.3),
    /// with what the preview saw at each path riding along on this side
    /// only (`previewed`, skipped by serde; Ruling 10); or the tool's own
    /// uninstall command as a `Command` plan (`command_uninstall_plan`,
    /// spec §6.4), refused with the recipe's reason when its gate says
    /// this layout is not offered; or `NoSafeMethod` for a recipe with
    /// neither -- the gate (`blocked_uninstall`) refuses that first; this
    /// is its late twin for a stale snapshot. The one artifact is
    /// `Binary`/`<recipe.id>`, so any other name or kind is a request this
    /// adapter cannot mean.
    pub async fn plan(
        &self,
        inst: &ManagerInstance,
        req: &OpRequest,
    ) -> Result<Plan, AdapterError> {
        ensure_instance_match(req, inst)?;
        validate_package_name(&req.name)?;
        if req.name != self.recipe.id || req.artifact_kind != ArtifactKind::Binary {
            return Err(AdapterError::InvalidName(req.name.clone()));
        }
        match req.kind {
            OpKind::Install => Err(AdapterError::Unsupported(format!(
                "{} is installed by its own installer, which Banager never runs",
                self.meta.name
            ))),
            OpKind::Uninstall => {
                let Some(uninstall) = &self.recipe.uninstall else {
                    return Err(AdapterError::UninstallBlocked {
                        reason: UninstallBlocked::NoSafeMethod,
                    });
                };
                match *uninstall {
                    Uninstall::Paths { remove, keep } => {
                        let detected = self.seated_detected_for(inst)?;
                        let locks = self.locks(inst, &detected);
                        let removal = removal::plan_removal(&removal::Job {
                            recipe: self.recipe,
                            detected,
                            remove,
                            keep,
                            globs: self.recipe.backup_globs,
                        })?;
                        Ok(Plan {
                            request: req.clone(),
                            // No command: `execute` moves these itself. What
                            // the preview saw at each stays with the plan on
                            // this side (`previewed`, skipped on the wire).
                            action: PlanAction::TrashPaths {
                                paths: removal.paths,
                                previewed: removal.identities,
                            },
                            // Everything lives under $HOME (spec §6.2).
                            needs_password: false,
                            locks,
                            // Between items the token is watched; there is
                            // no process to stop (spec §6.2).
                            cancel_policy: CancelPolicy::KillThenReconcile,
                            warnings: removal.warnings,
                            // "Would break": nothing depends on a tool's
                            // own files this way, and a non-empty list
                            // disables the confirm button.
                            affected: Vec::new(),
                            timeout_secs: removal::TIMEOUT_SECS,
                        })
                    }
                    Uninstall::Command(ref cmd) => {
                        let detected = self.seated_detected_for(inst)?;
                        self.command_uninstall_plan(inst, req, &detected, cmd)
                    }
                }
            }
            OpKind::Upgrade => {
                let detected = self.seated_detected_for(inst)?;
                // A tool that installs its updates itself and offers nothing
                // Banager may run (agy): the gate refuses this first
                // (`blocked_upgrade`, from the candidate's `blocked`); this is
                // its late twin for a stale snapshot (spec §五).
                let Some(upgrade) = &self.recipe.upgrade else {
                    return Err(AdapterError::UpdateBlocked {
                        reason: UpdateBlocked::SelfUpdatesOnly,
                    });
                };
                Ok(Plan {
                    request: req.clone(),
                    action: PlanAction::Command {
                        // The launcher, exactly as previewed: never a
                        // program the recipe could name (spec 附录 B).
                        program: inst.exe_path.clone(),
                        args: upgrade.args.iter().map(|a| a.to_string()).collect(),
                        // Not the version read's environment: the tool's
                        // updater must not be told to stop updating
                        // (spec §3.4), and rustup's self update may
                        // install nothing anyway.
                        env: Vec::new(),
                    },
                    // Everything lives under $HOME (spec §五).
                    needs_password: false,
                    // The instance's own lock, plus rustup's cargo lock
                    // (spec §2.4): a self update replaces the binary the
                    // cargo instance's `cargo` proxy runs.
                    locks: self.locks(inst, &detected),
                    cancel_policy: upgrade.cancel,
                    warnings: Vec::new(),
                    affected: Vec::new(),
                    timeout_secs: upgrade.timeout_secs,
                })
            }
        }
    }

    /// What `detect` wrote, or a plain `Refused` when nothing has been
    /// detected -- unreachable through `Session`, which detects before it
    /// plans (spec §3.2), so no sentence of its own. For `execute`, which
    /// is handed a plan and no instance to bind the seat to; `plan` binds
    /// through `seated_detected_for`.
    fn detected_or_refuse(&self) -> Result<Detected, AdapterError> {
        self.detected.lock().unwrap().clone().ok_or_else(|| {
            AdapterError::Refused(format!(
                "{} has not been detected in this session",
                self.meta.name
            ))
        })
    }

    /// A `Command` plan -- the upgrade, `<launcher> update`, or a
    /// `Command` uninstall, rustup's `<launcher> self uninstall -y` -- runs
    /// through `run_plan` like every source's, after one more look at the
    /// launcher immediately before the spawn and, for the uninstall, one
    /// more ask of the recipe's gate (both below). A `TrashPaths` plan
    /// is carried out here, item by item (`removal::execute_removal`),
    /// against the list re-read from the recipe and the disk and compared
    /// with what the preview saw (`previewed`) -- the plan's paths are
    /// what the user confirmed, not the source of truth.
    pub async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        match &plan.action {
            // The launcher is a link that the user, another installer or
            // the tool's own updater can re-point between the preview and
            // the click: at Homebrew's or npm's copy, at some other file,
            // or at nothing (B's Astra finding B-1). The plan names that
            // path, so spawning it would run whatever is behind it now --
            // another copy's updater. So the launcher is looked at again
            // here, right before the spawn, with the same `probe` detect
            // used: it must still be this route's (`Present`: one link
            // straight into the root, resolving there, not a package
            // manager's). Anything else -- gone, dangling, a plain file,
            // resolving outside the root, or a launcher `probe` cannot look
            // at -- is `Fault::PathChanged` naming the launcher, nothing
            // started: what a path-list uninstall reports for a path that
            // changed. A link re-pointed at a newer version *inside* the
            // root (the updater's own work) is still this route's, and
            // runs. What remains is the instant between this look and the
            // spawn, the same edge the removal documents (`take_turn`).
            PlanAction::Command { program, .. } => {
                let detected = self.detected_or_refuse()?;
                let route = &self.recipe.route;
                // The seat's own launcher and root. `None` only for a
                // `$CARGO_HOME` route whose seat has no usable Cargo home,
                // which `detect` never seats (it lists nothing first); so
                // a plan that reaches this cannot be that seat's, and is
                // refused as one naming another program would be.
                let (Some(launcher), Some(root)) = (
                    route::expand_route(
                        &detected.home,
                        detected.cargo_home.as_deref(),
                        route.launcher,
                    ),
                    route::expand_route(&detected.home, detected.cargo_home.as_deref(), route.root),
                ) else {
                    return Err(AdapterError::Refused(format!(
                        "{}: the plan runs {}, but the last detect found no Cargo home to place the launcher under",
                        self.meta.name,
                        program.display()
                    )));
                };
                // `plan` names the launcher and nothing else (spec 附录 B):
                // a plan naming any other program was not built by it, and
                // is a bug's, refused before anything is looked at or run.
                if *program != launcher {
                    return Err(AdapterError::Refused(format!(
                        "{}: the plan runs {} rather than the launcher {}",
                        self.meta.name,
                        program.display(),
                        launcher.display()
                    )));
                }
                if !matches!(
                    route::probe(self.recipe.route.kind, &launcher, &root),
                    Probe::Present { .. }
                ) {
                    return Ok(Outcome::BanagerFailed(Fault::PathChanged {
                        path: crate::scan::display_path(&launcher, &detected.home)
                            .display()
                            .to_string(),
                    }));
                }
                // A `Command` uninstall deletes folders the preview named
                // by path, and rustup's `uninstall()` deletes wherever
                // `RUSTUP_HOME` and `CARGO_HOME` resolve *when it runs*
                // (self_update.rs:955-966, :1029), permanently. So the
                // recipe's gate, which passed at the preview, is asked
                // again here, of the same seat and the disk as it is now,
                // for the same reason the launcher is: a `~/.rustup` or
                // `~/.cargo` that became a link to somewhere else after
                // the preview -- the launcher's probe alone still passes
                // for the second, a regular file at the end of the link --
                // is `Fault::PathChanged` naming that folder, nothing
                // started. The upgrade is not gated (plan ruling 18: `self
                // update` touches only `$CARGO_HOME/bin`). A `Command` plan
                // for an uninstall no `Command` recipe backs, or for an
                // install, was not built by `plan` and is a bug's, refused.
                match plan.request.kind {
                    OpKind::Upgrade => {}
                    OpKind::Uninstall => {
                        let Some(Uninstall::Command(cmd)) = &self.recipe.uninstall else {
                            return Err(AdapterError::Refused(format!(
                                "{} has no uninstall command to run",
                                self.meta.name
                            )));
                        };
                        if let Some(refusal) = (cmd.blocked)(&detected) {
                            return Ok(Outcome::BanagerFailed(Fault::PathChanged {
                                path: crate::scan::display_path(&refusal.path, &detected.home)
                                    .display()
                                    .to_string(),
                            }));
                        }
                    }
                    OpKind::Install => {
                        return Err(AdapterError::Refused(format!(
                            "{} is never installed by a plan of Banager's",
                            self.meta.name
                        )));
                    }
                }
                run_plan(&self.runner, plan, sink, op_id, cancel).await
            }
            PlanAction::TrashPaths { paths, previewed } => {
                // A recipe with no uninstall, or with a `Command` one, has
                // no list to move: `plan` built no `TrashPaths` plan for
                // it, so this one is a bug's, refused before anything is
                // touched.
                let Some(Uninstall::Paths { remove, keep }) = self.recipe.uninstall else {
                    return Err(AdapterError::Refused(format!(
                        "{} has no path list to carry out",
                        self.meta.name
                    )));
                };
                removal::execute_removal(
                    &removal::Job {
                        recipe: self.recipe,
                        detected: self.detected_or_refuse()?,
                        remove,
                        keep,
                        globs: self.recipe.backup_globs,
                    },
                    removal::Confirmed { paths, previewed },
                    &self.trasher,
                    removal::Pacing {
                        settle: self.trash_gap,
                        budget: Duration::from_secs(plan.timeout_secs),
                        last_move: Arc::clone(&self.last_move),
                    },
                    sink,
                    op_id,
                    cancel,
                )
                .await
            }
        }
    }

    /// The reading before and after an upgrade (and after an install,
    /// which this adapter never plans), from the disk now (`look`, then
    /// `rows`) -- not through `inventory`, which refuses an install that
    /// is no longer what detect listed: a launcher gone after an upgrade
    /// must read as absent here (`GoneAfterUpgrade`), not as a refusal
    /// (`Unconfirmed`). An owned launcher without a readable version --
    /// one that did not answer, or the dangling link alone -- is no
    /// evidence that an upgrade succeeded, so it is refused (`Parse`) and
    /// `run_operation` reports `Unconfirmed` (B's Astra finding 1). After
    /// an uninstall `run_operation` reads `reconcile_after_uninstall`
    /// instead, which counts that launcher as still there.
    pub async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        let look = self.look(inst).await;
        let reconciled = reconcile_from(self.rows(inst, look), key);
        if reconciled.present && reconciled.version.as_deref().is_none_or(str::is_empty) {
            return Err(AdapterError::Parse(
                "cannot verify the standalone launcher's installed version".to_string(),
            ));
        }
        Ok(reconciled)
    }

    /// The reading after an uninstall (`Adapter::reconcile_after_uninstall`):
    /// presence alone, from the disk now, through `route::probe_strict`. A
    /// launcher-only launcher -- the dangling link a stopped path-list
    /// uninstall leaves -- is present, so `run_operation` reports the stop
    /// truthfully (`Cancelled` after the user's Cancel,
    /// `StillInstalledAfterUninstall` after a run that claimed success) and
    /// the next refresh shows the row a second Uninstall finishes; a
    /// launcher that is gone is absent (spec §3.6); and a launcher Banager
    /// cannot look at -- a permission error, a loop -- is neither: an
    /// error, which `run_operation` reports as `Unconfirmed`, never as a
    /// finished uninstall (Ruling 27). No version is read: presence is the
    /// whole question, and a stopped uninstall's launcher has none.
    ///
    /// For a recipe with a path list, a launcher that is gone is not the
    /// whole answer: a copy of the tool still running can put its program
    /// folder or its cache back after the launcher's move, and without the
    /// launcher no row would show them. So every other path on the list,
    /// and every backup file its patterns match, is looked at too
    /// (`removal::left_behind`, with `plan`'s paths as the ones the run
    /// moved, bound to this instance's seat): one that is there is the
    /// tool still there, and one Banager cannot look at is an error, as
    /// for the launcher. What the preview's own rule keeps as not the
    /// tool's, and this run never moved, is not counted (`left_behind`
    /// says why). A launcher still there answers alone: the tool is there,
    /// whatever else is. (`removal::execute_removal` takes the same look
    /// itself once the pause after its last move is over, and names what
    /// it finds; this reading, a moment later, sees what came back since.)
    pub async fn reconcile_after_uninstall(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
        plan: &Plan,
    ) -> Result<Reconciled, AdapterError> {
        let launcher_there =
            match route::probe_strict(self.recipe.route.kind, &inst.exe_path, &inst.prefix) {
                Ok(Probe::Absent) => false,
                Ok(Probe::Present { .. } | Probe::LauncherOnly) => true,
                Err(error) => {
                    return Err(AdapterError::Parse(format!(
                        "cannot tell whether {} is still there: {error}",
                        inst.exe_path.display()
                    )))
                }
            };
        let left_behind = match self.recipe.uninstall {
            Some(Uninstall::Paths { remove, keep }) if !launcher_there => {
                let moved: &[PathBuf] = match &plan.action {
                    PlanAction::TrashPaths { paths, .. } => paths,
                    PlanAction::Command { .. } => &[],
                };
                let job = removal::Job {
                    recipe: self.recipe,
                    detected: self.seated_detected_for(inst)?,
                    remove,
                    keep,
                    globs: self.recipe.backup_globs,
                };
                removal::left_behind(&job, moved).map_err(|path| {
                    AdapterError::Parse(format!(
                        "cannot tell whether {} is still there",
                        path.display()
                    ))
                })?
            }
            _ => Vec::new(),
        };
        // The one artifact, matched as `reconcile_from` matches -- kind and
        // name, never the instance id (adapters/mod.rs says why).
        let this_tool = key.kind == ArtifactKind::Binary && key.name == self.recipe.id;
        Ok(Reconciled {
            present: (launcher_there || !left_behind.is_empty()) && this_tool,
            version: None,
        })
    }
}

/// One adapter per recipe in `recipes::RECIPES`, over the shared runner,
/// http client and trasher and one shared `removal::LastMove`
/// (`one_per_recipe`), for `Session::new`'s registration list.
pub fn all(
    runner: Arc<dyn CommandRunner>,
    http: Arc<dyn HttpClient>,
    trasher: Arc<dyn Trasher>,
) -> Vec<Arc<dyn Adapter>> {
    one_per_recipe(runner, http, trasher)
        .into_iter()
        .map(|adapter| Arc::new(adapter) as Arc<dyn Adapter>)
        .collect()
}

/// `all`'s adapters: one per recipe, over the one runner, client and
/// trasher, and one `removal::LastMove` among them all. `Session::new`
/// calls `all` once, so that clock is this process's: the gap after each
/// move to the Trash holds across every path-list uninstall it runs at the
/// same time (up to three operations run at once, and each path-list
/// uninstall locks only its own instance), not only within one.
fn one_per_recipe(
    runner: Arc<dyn CommandRunner>,
    http: Arc<dyn HttpClient>,
    trasher: Arc<dyn Trasher>,
) -> Vec<StandaloneAdapter> {
    let last_move = Arc::new(removal::LastMove::default());
    recipes::RECIPES
        .iter()
        .map(|&recipe| StandaloneAdapter {
            last_move: Arc::clone(&last_move),
            ..StandaloneAdapter::new(recipe, runner.clone(), http.clone(), trasher.clone())
        })
        .collect()
}

#[async_trait]
impl Adapter for StandaloneAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        StandaloneAdapter::detect(self, env).await
    }

    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        StandaloneAdapter::inventory(self, inst).await
    }

    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        StandaloneAdapter::check_updates(self, inst, opts).await
    }

    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        StandaloneAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        StandaloneAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        StandaloneAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        StandaloneAdapter::reconcile(self, inst, key).await
    }

    async fn reconcile_after_uninstall(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
        plan: &Plan,
    ) -> Result<Reconciled, AdapterError> {
        StandaloneAdapter::reconcile_after_uninstall(self, inst, key, plan).await
    }
}

/// Synthetic installs in a throwaway home, for this module's tests: real
/// links and real files on a real file system, since `route::probe`
/// answers from `lstat`, `readlink` and `realpath` and nothing else. Never
/// a recorded fixture (spec §9.3: fingerprint tests build their own
/// layouts and must not write under `adapters/fixtures/`).
#[cfg(test)]
pub(super) mod testing {
    use crate::runner::HostEnv;
    use std::path::{Path, PathBuf};

    /// A fresh, canonical directory under the system temp dir, removed on
    /// drop. Canonical, so paths built from it compare equal to what
    /// `canonicalize` answers (macOS's `/var/folders` is `/private/var/…`).
    pub struct TempHome(PathBuf);

    impl TempHome {
        pub fn new(tag: &str) -> TempHome {
            let raw = std::env::temp_dir().join(format!(
                "banager-standalone-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&raw).expect("create temp home");
            TempHome(std::fs::canonicalize(&raw).expect("canonical temp home"))
        }

        pub fn path(&self) -> &Path {
            &self.0
        }

        /// Creates `rel` (and its parents) under the home.
        pub fn dir(&self, rel: &str) -> PathBuf {
            let path = self.0.join(rel);
            std::fs::create_dir_all(&path).expect("create dir");
            path
        }

        /// Writes a small regular file at `rel` (parents created).
        pub fn file(&self, rel: &str) -> PathBuf {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("create parent");
            std::fs::write(&path, b"#!/bin/sh\n").expect("write file");
            path
        }

        /// An executable synthetic target; it is never actually spawned.
        pub fn executable(&self, rel: &str) -> PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let path = self.file(rel);
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("executable target");
            path
        }

        /// A symbolic link at `rel` whose text is `target` exactly --
        /// absolute or relative, existing or not (parents created).
        pub fn link(&self, rel: &str, target: &Path) -> PathBuf {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("create parent");
            std::os::unix::fs::symlink(target, &path).expect("symlink");
            path
        }

        /// A `HostEnv` whose home is this directory and whose `PATH` is
        /// `path_dirs`.
        pub fn env(&self, path_dirs: Vec<PathBuf>) -> HostEnv {
            HostEnv {
                path_dirs,
                home: self.0.clone(),
                euid: 501,
                cargo_home: None,
                rustup_home: None,
                zdotdir: None,
                ollama_host: None,
            }
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The native Claude Code layout, as `ls -la` shows it on a Mac with
    /// the installer's defaults: `~/.local/bin/claude` linking (absolute
    /// text) to `~/.local/share/claude/versions/<version>`.
    pub struct ClaudeLayout {
        pub launcher: PathBuf,
        pub root: PathBuf,
        pub real: PathBuf,
    }

    pub fn claude_layout(home: &TempHome, version: &str) -> ClaudeLayout {
        let real = home.executable(&format!(".local/share/claude/versions/{version}"));
        let launcher = home.link(".local/bin/claude", &real);
        ClaudeLayout {
            launcher,
            root: home.path().join(".local/share/claude"),
            real,
        }
    }

    /// Antigravity's layout as its installer writes it (agy.md §2, §3a): a
    /// regular executable at `~/.local/bin/agy` -- the whole program -- and
    /// the tool's root beside the Gemini CLI's other folders.
    pub struct AgyLayout {
        pub launcher: PathBuf,
        pub root: PathBuf,
    }

    pub fn agy_layout(home: &TempHome) -> AgyLayout {
        let launcher = home.executable(".local/bin/agy");
        let root = home.dir(".gemini/antigravity-cli");
        home.file(".gemini/antigravity-cli/updater/update_status.json");
        home.file(".gemini/antigravity-cli/conversations/c1.jsonl");
        AgyLayout { launcher, root }
    }

    /// Grok Build's layout as its installer writes it (grok.md §1, §2): the
    /// download `~/.grok/downloads/grok-<version>-macos-aarch64`, two links
    /// to it in `~/.grok/bin` -- `grok`, whose text is *relative*
    /// (`../downloads/…`, spec §3.5, VERIFIED), and `agent`, made the same
    /// way here -- the vendored `bundled/` and `completions/`, and the files
    /// `~/.grok` keeps that an uninstall leaves alone.
    pub struct GrokLayout {
        pub launcher: PathBuf,
        pub agent: PathBuf,
        pub root: PathBuf,
        pub real: PathBuf,
    }

    pub fn grok_layout(home: &TempHome, version: &str) -> GrokLayout {
        let real = home.executable(&format!(".grok/downloads/grok-{version}-macos-aarch64"));
        let target = PathBuf::from(format!("../downloads/grok-{version}-macos-aarch64"));
        let launcher = home.link(".grok/bin/grok", &target);
        let agent = home.link(".grok/bin/agent", &target);
        home.file(".grok/bundled/agents/default.md");
        home.file(".grok/completions/zsh/_grok");
        home.file(".grok/config.toml");
        home.file(".grok/auth.json");
        home.file(".grok/sessions/s1.jsonl");
        home.file(".grok/memory/notes.md");
        GrokLayout {
            launcher,
            agent,
            root: home.path().join(".grok"),
            real,
        }
    }

    /// rustup's native layout: `<cargo_home>/bin/rustup`, an executable
    /// regular file, and the thirteen proxies rustup installs beside it as
    /// relative links to it (`TOOLS` + `DUP_TOOLS` in rustup's
    /// `src/lib.rs`; `ls -la ~/.cargo/bin` on this Mac, unknown-scan.md
    /// §2).
    pub struct RustupLayout {
        pub cargo_home: PathBuf,
        pub launcher: PathBuf,
    }

    pub fn rustup_layout(cargo_home: &Path) -> RustupLayout {
        use std::os::unix::fs::PermissionsExt;
        let bin = cargo_home.join("bin");
        std::fs::create_dir_all(&bin).expect("create cargo bin");
        let launcher = bin.join("rustup");
        std::fs::write(&launcher, b"#!/bin/sh\n").expect("write rustup");
        // Executable, as the installer leaves it and as
        // `TempHome::executable` makes claude's target: B's `shadow_note`
        // accepts only a `PATH` hit with an execute bit, so a 0644
        // launcher would give every detect over `<cargo_home>/bin` a
        // `NotOnPath` note.
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755))
            .expect("executable rustup");
        for proxy in super::rustup::RUSTUP_PROXIES {
            std::os::unix::fs::symlink("rustup", bin.join(proxy)).expect("proxy link");
        }
        RustupLayout {
            cargo_home: cargo_home.to_path_buf(),
            launcher,
        }
    }

    /// A `Detected` seat as `detect` writes it for `home` with this Cargo
    /// home, the default rustup home and no `ZDOTDIR`, for the recipe
    /// functions that take one.
    pub fn detected(home: &Path, cargo_home: &Path) -> super::Detected {
        super::Detected {
            home: home.to_path_buf(),
            euid: 501,
            cargo_home: Some(cargo_home.to_path_buf()),
            rustup_home: Some(home.join(".rustup")),
            zdotdir: None,
        }
    }

    /// Takes every permission off the folder `path` until dropped, so an
    /// `lstat` of anything inside it fails with a permission error -- the
    /// "could not tell" a probe must never read as "gone" -- and gives
    /// `0o755` back on drop, so `TempHome` can remove the tree. `None` when
    /// the tests run as root, whom permissions do not stop: the caller
    /// then skips its check and says so.
    pub struct Unreadable(PathBuf);

    impl Unreadable {
        pub fn new(path: &Path) -> Option<Unreadable> {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            if std::fs::metadata(path).expect("the folder exists").uid() == 0 {
                eprintln!("running as root: permissions stop nothing, check skipped");
                return None;
            }
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000))
                .expect("take the folder's permissions away");
            Some(Unreadable(path.to_path_buf()))
        }
    }

    impl Drop for Unreadable {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }

    /// A `MockTrasher` that also notes when each move began, for the tests
    /// of the gap after each move -- two uninstalls' moves among them when
    /// both runs are handed this one trasher, as `all()` hands its adapters
    /// one.
    #[derive(Default)]
    pub struct TimedTrasher {
        pub inner: crate::trash::MockTrasher,
        began: std::sync::Mutex<Vec<std::time::Instant>>,
    }

    impl TimedTrasher {
        /// When each move began, earliest first.
        pub fn began(&self) -> Vec<std::time::Instant> {
            let mut began = self.began.lock().unwrap().clone();
            began.sort();
            began
        }

        /// The time between each move's beginning and the next one's.
        pub fn gaps(&self) -> Vec<std::time::Duration> {
            self.began()
                .windows(2)
                .map(|pair| pair[1].duration_since(pair[0]))
                .collect()
        }
    }

    impl crate::trash::Trasher for TimedTrasher {
        fn trash(
            &self,
            path: &Path,
            kind: crate::model::ItemKind,
        ) -> Result<PathBuf, crate::trash::TrashError> {
            self.began.lock().unwrap().push(std::time::Instant::now());
            self.inner.trash(path, kind)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::recipe::{
        no_extra_locks, GateRefusal, Route, RouteKind, UpgradeCmd, VersionCmd, VersionParse,
    };
    use super::recipes::{AGY, CLAUDE, GROK, RUSTUP};
    use super::testing::{
        agy_layout, claude_layout, grok_layout, rustup_layout, TempHome, TimedTrasher,
    };
    use super::*;
    use crate::adapters::cargo::CargoAdapter;
    use crate::adapters::{Adapter, CheckOptions};
    use crate::events::{LogNote, OperationEvent, VecSink};
    use crate::http::{HttpResponse, MockHttpClient};
    use crate::model::{
        ArtifactKind, CancelPolicy, Fault, InstallReason, InstanceNote, ItemKind, KeptWhat, OpKind,
        OpRequest, Outcome, PlanAction, RemovedWhat, ResourceLock, Unavailable, UninstallBlocked,
        UninstallUnsafeReason, UpdateBlocked, UpdateChannel, Warning,
    };
    use crate::runner::{CommandOutput, MockRunner, RunnerError};
    use crate::testing::{command_args, command_env, command_program};
    use crate::trash::MockTrasher;
    // For `RecordingRunner` below. Named here as well as through
    // `super::*` (the parent imports it for `impl Adapter`), so this impl
    // does not lean on the imports above; an explicit import beside a glob
    // is not a warning.
    use async_trait::async_trait;
    use std::sync::Mutex as StdMutex;

    fn exited_0(stdout: &str) -> CommandOutput {
        CommandOutput {
            exit_code: Some(0),
            stdout: stdout.to_string(),
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        }
    }

    fn adapter(runner: Arc<dyn CommandRunner>) -> StandaloneAdapter {
        adapter_with(runner, Arc::new(MockTrasher::new()))
    }

    /// An adapter over a trasher the test holds on to (to read its calls
    /// and its bin), with no pause after each item (ruling 12).
    fn adapter_with(
        runner: Arc<dyn CommandRunner>,
        trasher: Arc<MockTrasher>,
    ) -> StandaloneAdapter {
        StandaloneAdapter::new(&CLAUDE, runner, Arc::new(MockHttpClient::new()), trasher)
            .with_trash_gap(Duration::ZERO)
    }

    /// `TempHome::env` with the euid of the user who made the temp home
    /// (the files in it are theirs), for a `detect` whose `Detected.euid`
    /// the removal's check 3 will compare against. (`TempHome::env` says
    /// 501; nothing before this step read it.)
    fn env_as_owner(home: &TempHome) -> HostEnv {
        use std::os::unix::fs::MetadataExt;
        HostEnv {
            euid: std::fs::metadata(home.path()).expect("home metadata").uid(),
            ..home.env(vec![])
        }
    }

    /// The claude layout plus the cache and settings the dialog lists,
    /// detected (so `Detected` is filled) with `--version` answered.
    async fn full_install(
        tag: &str,
        trasher: Arc<MockTrasher>,
    ) -> (
        TempHome,
        super::testing::ClaudeLayout,
        StandaloneAdapter,
        ManagerInstance,
    ) {
        let home = TempHome::new(tag);
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        home.file(".claude/projects/p/session.jsonl");
        home.file(".claude.json");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = adapter_with(runner, trasher);
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        (home, layout, adapter, inst)
    }

    fn uninstall() -> OpRequest {
        request(OpKind::Uninstall, ArtifactKind::Binary, "claude")
    }

    fn notes_of(sink: &VecSink) -> Vec<LogNote> {
        sink.snapshot()
            .into_iter()
            .filter_map(|event| match event {
                OperationEvent::Note { note, .. } => Some(note),
                _ => None,
            })
            .collect()
    }

    /// `MockRunner` keys and records argv only. This one records every
    /// `CommandSpec` it is handed, so a test can prove what environment,
    /// timeout and output use the version read carried -- the point of
    /// spec §3.4 is one environment variable.
    struct RecordingRunner {
        specs: StdMutex<Vec<CommandSpec>>,
        output: CommandOutput,
    }

    #[async_trait]
    impl CommandRunner for RecordingRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _on_line: Option<crate::runner::LineCallback>,
            _cancel: CancellationToken,
        ) -> Result<CommandOutput, RunnerError> {
            self.specs.lock().unwrap().push(spec);
            Ok(self.output.clone())
        }
    }

    #[test]
    fn test_new_takes_its_meta_from_the_recipe_and_names_the_standalone_id() {
        let adapter = adapter(Arc::new(MockRunner::new()));
        assert_eq!(adapter.meta.id, "standalone-claude");
        assert_eq!(adapter.meta.name, "Claude Code");
        assert!(
            adapter.detected.lock().unwrap().is_none(),
            "nothing detected yet"
        );
    }

    #[tokio::test]
    async fn test_detect_lists_the_native_install_as_one_instance() {
        let home = TempHome::new("detect-present");
        let meta = AdapterMeta::from_toml(CLAUDE.meta_toml).expect("meta");
        let version = meta.verified_versions.first().expect("a verified version");
        let layout = claude_layout(&home, version);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0(&format!("{version} (Claude Code)\n")),
        );
        let adapter = adapter(runner);

        let instances = adapter
            .detect(&home.env(vec![home.path().join(".local/bin")]))
            .await;

        assert_eq!(instances.len(), 1);
        let inst = &instances[0];
        assert_eq!(inst.id, "standalone-claude");
        assert_eq!(inst.adapter_id, "standalone-claude");
        // The launcher itself, not the binary it resolves to: the program
        // every plan runs and the path `sourceNoticesFor` takes the
        // command name from (spec §2.2).
        assert_eq!(inst.exe_path, layout.launcher);
        assert_eq!(inst.prefix, layout.root);
        assert_eq!(inst.scope, Scope::User);
        assert_eq!(inst.version, Some(version.clone()));
        assert_eq!(
            inst.unverified_version, None,
            "the metadata version is verified"
        );
        assert_eq!(inst.read_only_reason, None);
        assert_eq!(inst.status.unavailable, None);
        assert!(
            inst.status.notes.is_empty(),
            "PATH finds this very copy: no note"
        );
        assert_eq!(
            adapter
                .detected
                .lock()
                .unwrap()
                .as_ref()
                .map(|d| d.home.clone()),
            Some(home.path().to_path_buf()),
            "detect seats home for check_updates"
        );
    }

    #[tokio::test]
    async fn test_detect_reads_the_version_with_the_autoupdater_off_and_a_thirty_second_timeout() {
        // Spec §3.4: Claude Code checks for updates on startup (doc text)
        // and a refresh must not set off its update, so the documented
        // switch for that background check goes on this read (and
        // inventory's) whether or not a bare `--version` would reach the
        // updater -- never on the upgrade plan.
        let home = TempHome::new("detect-env");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(RecordingRunner {
            specs: StdMutex::new(Vec::new()),
            output: exited_0("2.1.281 (Claude Code)\n"),
        });
        let adapter = adapter(runner.clone());

        adapter.detect(&home.env(vec![])).await;

        let specs = runner.specs.lock().unwrap();
        assert_eq!(specs.len(), 1);
        let spec = &specs[0];
        assert_eq!(spec.program, layout.launcher);
        assert_eq!(spec.args, vec!["--version".to_string()]);
        assert_eq!(
            spec.env,
            vec![("DISABLE_AUTOUPDATER".to_string(), "1".to_string())]
        );
        assert_eq!(spec.timeout, Duration::from_secs(30));
        assert_eq!(spec.output_use, OutputUse::Parsed);
        assert_eq!(spec.cwd, None);
    }

    #[tokio::test]
    async fn test_detect_flags_a_version_outside_the_verified_list() {
        let meta = AdapterMeta::from_toml(CLAUDE.meta_toml).expect("meta");
        let version = (0_u64..)
            .map(|major| format!("{major}.0.0"))
            .find(|version| !meta.verified_versions.contains(version))
            .expect("a version outside the finite recorded list");
        let home = TempHome::new("detect-unverified");
        let layout = claude_layout(&home, &version);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0(&format!("{version} (Claude Code)\n")),
        );
        let instances = adapter(runner).detect(&home.env(vec![])).await;
        assert_eq!(instances[0].version, Some(version.clone()));
        assert_eq!(instances[0].unverified_version, Some(version));
    }

    #[tokio::test]
    async fn test_detect_marks_a_failed_version_read_as_not_responding() {
        // The launcher is there and is this route's, but `--version` did
        // not answer: the state axis (`NotResponding`), like uv's rule.
        let home = TempHome::new("detect-not-responding");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            CommandOutput {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "dyld: Library not loaded\n".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let instances = adapter(runner).detect(&home.env(vec![])).await;
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].version, None);
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::NotResponding)
        );
        assert_eq!(instances[0].exe_path, layout.launcher);
    }

    #[tokio::test]
    async fn test_detect_marks_a_timed_out_version_read_as_not_responding() {
        // A `claude --version` that never returns is stopped by the runner
        // at 30 s and reported as not answering; nothing hangs and nothing
        // crashes. A guard, not a known bug: the one hang claude.md §6
        // records is `claude update`'s (before 2.1.214, on a directory
        // named like a shell rc file), and none is recorded for `--version`.
        let home = TempHome::new("detect-timed-out");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            CommandOutput {
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: true,
                cancelled: false,
            },
        );
        let instances = adapter(runner).detect(&home.env(vec![])).await;
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::NotResponding)
        );
    }

    #[tokio::test]
    async fn test_detect_marks_a_runner_error_as_not_responding() {
        // No canned answer is the mock's spawn failure; a real one is the
        // same shape (`RunnerError::Spawn`).
        let home = TempHome::new("detect-spawn-failed");
        claude_layout(&home, "2.1.281");
        let instances = adapter(Arc::new(MockRunner::new()))
            .detect(&home.env(vec![]))
            .await;
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::NotResponding)
        );
    }

    #[tokio::test]
    async fn test_detect_carries_the_path_note_when_another_copy_wins() {
        let home = TempHome::new("detect-shadowed");
        let layout = claude_layout(&home, "2.1.281");
        let cask = home.executable("opt/homebrew/Caskroom/claude-code/2.1.267/claude");
        let brew_bin = home.dir("opt/homebrew/bin");
        home.link("opt/homebrew/bin/claude", &cask);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let env = home.env(vec![brew_bin, home.path().join(".local/bin")]);
        let instances = adapter(runner).detect(&env).await;
        assert_eq!(
            instances[0].status.notes,
            vec![InstanceNote::ShadowedByHomebrew]
        );
        // And with nothing on PATH at all: NotOnPath.
        let instances = adapter(Arc::new(MockRunner::new()))
            .detect(&home.env(vec![]))
            .await;
        assert_eq!(instances[0].status.notes, vec![InstanceNote::NotOnPath]);
        // And with Homebrew's directory on PATH but not the launcher's:
        // NotOnPath too. Homebrew's copy runs when the name is typed, but
        // this copy is not behind it on PATH; it is not on PATH at all.
        let instances = adapter(Arc::new(MockRunner::new()))
            .detect(&home.env(vec![home.path().join("opt/homebrew/bin")]))
            .await;
        assert_eq!(instances[0].status.notes, vec![InstanceNote::NotOnPath]);
    }

    #[tokio::test]
    async fn test_detect_keeps_a_dangling_launcher_as_a_launcher_only_row() {
        // The half-uninstalled state: no version read at all (there is no
        // program to run); not unavailable (the source has not stopped
        // answering, and step C's uninstall must be allowed on this
        // instance); the note says what is left.
        let home = TempHome::new("detect-launcher-only");
        let root = home.path().join(".local/share/claude");
        let launcher = home.link(".local/bin/claude", &root.join("versions/2.1.281"));
        let runner = Arc::new(MockRunner::new());
        let adapter = adapter(runner.clone());

        let instances = adapter
            .detect(&home.env(vec![home.path().join(".local/bin")]))
            .await;

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].exe_path, launcher);
        assert_eq!(instances[0].version, None);
        assert_eq!(instances[0].status.unavailable, None);
        assert_eq!(instances[0].status.notes, vec![InstanceNote::LauncherOnly]);
        assert!(runner.calls().is_empty(), "nothing to run");
    }

    #[tokio::test]
    async fn test_detect_finds_nothing_without_a_launcher_or_with_a_package_managers_copy() {
        let home = TempHome::new("detect-absent");
        let runner = Arc::new(MockRunner::new());
        assert!(adapter(runner.clone())
            .detect(&home.env(vec![]))
            .await
            .is_empty());
        let cask = home.executable("opt/homebrew/Caskroom/claude-code/2.1.267/claude");
        home.link(".local/bin/claude", &cask);
        assert!(adapter(runner.clone())
            .detect(&home.env(vec![]))
            .await
            .is_empty());
        assert!(
            runner.calls().is_empty(),
            "no `--version` for a row this adapter does not own"
        );
    }

    fn instance_for(
        layout: &super::testing::ClaudeLayout,
        version: Option<&str>,
    ) -> ManagerInstance {
        ManagerInstance {
            exe_path: layout.launcher.clone(),
            prefix: layout.root.clone(),
            version: version.map(str::to_string),
            ..crate::testing::manager_instance("standalone-claude", "standalone-claude")
        }
    }

    #[tokio::test]
    async fn test_inventory_is_the_tool_itself_and_offers_the_path_list_uninstall() {
        let home = TempHome::new("inventory-present");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = adapter(runner);
        let inst = instance_for(&layout, Some("2.1.281"));

        let artifacts = adapter.inventory(&inst).await.expect("inventory");

        assert_eq!(artifacts.len(), 1);
        let a = &artifacts[0];
        assert_eq!(
            a.key,
            ArtifactKey {
                instance_id: "standalone-claude".to_string(),
                kind: ArtifactKind::Binary,
                name: "claude".to_string(),
            }
        );
        assert_eq!(a.display_name, "Claude Code");
        assert_eq!(a.version, "2.1.281");
        assert_eq!(a.reason, InstallReason::Requested);
        // Left to the front end to localise (`STANDALONE_SUMMARY_KEYS`,
        // which arrives with Task 10 of the phase 4 step B plan), not a
        // bare English string here.
        assert_eq!(a.description, None);
        assert_eq!(
            a.homepage.as_deref(),
            Some("https://code.claude.com/docs/en/setup")
        );
        // The real binary, for the Unknown page's rule 2.
        assert_eq!(a.path, Some(layout.real.clone()));
        assert!(a.auto_updates, "claude updates itself in the background");
        // The recipe has a path list, so nothing blocks the uninstall: the
        // gate lets it through to `plan`, and the page shows the button.
        assert_eq!(a.uninstall_blocked, None);
        assert_eq!(a.size_bytes, None);
        assert_eq!(a.installed_at, None);
    }

    #[tokio::test]
    async fn test_inventory_refuses_an_install_that_is_not_what_detect_listed() {
        // `refresh` calls inventory under the instance lock, after detect,
        // and reads the disk as it is now, never detect's answer (spec
        // §3.6). What it finds has to fit the instance detect listed, or
        // the snapshot would say two things at once -- a healthy row with
        // no artifact, or with an empty-version one, beside "Everything is
        // up to date" (B's Astra finding B-2). So a launcher that is gone,
        // program files gone behind a launcher detect saw whole, or
        // program files back behind a launcher detect saw dangling are each
        // a refusal, which `refresh` turns into "this refresh did not
        // finish for this source" with the previous rows kept; the next
        // refresh lists what is there.
        let home = TempHome::new("inventory-changed");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let whole = instance_for(&layout, Some("2.1.281"));
        let dangling = ManagerInstance {
            status: InstanceStatus {
                unavailable: None,
                notes: vec![InstanceNote::LauncherOnly],
            },
            ..instance_for(&layout, None)
        };

        // The program files are back behind a launcher detect saw dangling.
        let Err(AdapterError::Refused(reason)) = adapter.inventory(&dangling).await else {
            panic!("program files back behind a launcher detect saw dangling is a refusal");
        };
        assert!(reason.contains("program files"), "{reason}");

        // The program files are gone behind a launcher detect saw whole.
        std::fs::remove_dir_all(layout.root.join("versions")).expect("remove the program files");
        let Err(AdapterError::Refused(reason)) = adapter.inventory(&whole).await else {
            panic!("program files gone behind a launcher detect saw whole is a refusal");
        };
        assert!(reason.contains("program files"), "{reason}");

        // The launcher is gone, whatever detect listed.
        std::fs::remove_file(&layout.launcher).expect("remove the launcher");
        for inst in [&whole, &dangling] {
            let Err(AdapterError::Refused(reason)) = adapter.inventory(inst).await else {
                panic!("a launcher that is gone is a refusal");
            };
            assert!(reason.contains("gone"), "{reason}");
        }
    }

    #[tokio::test]
    async fn test_inventory_of_a_launcher_only_install_has_no_version_and_no_path() {
        // The row detect listed as the launcher alone (its
        // `InstanceNote::LauncherOnly` is what tells inventory so).
        let home = TempHome::new("inventory-launcher-only");
        let root = home.path().join(".local/share/claude");
        home.link(".local/bin/claude", &root.join("versions/2.1.281"));
        let runner = Arc::new(MockRunner::new());
        let adapter = adapter(runner.clone());
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        assert_eq!(inst.status.notes, vec![InstanceNote::LauncherOnly]);
        let artifacts = adapter.inventory(&inst).await.expect("inventory");
        assert_eq!(
            artifacts.len(),
            1,
            "still a row: the link is still there, and the state must be visible"
        );
        assert_eq!(artifacts[0].version, "");
        assert_eq!(artifacts[0].path, None);
        // The uninstall that finishes this state is allowed on this row
        // (spec Q17).
        assert_eq!(artifacts[0].uninstall_blocked, None);
        assert!(runner.calls().is_empty());
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_with_the_live_version_and_absent_when_gone() {
        let home = TempHome::new("reconcile");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = adapter(runner);
        let inst = instance_for(&layout, Some("2.1.281"));
        let key = ArtifactKey {
            instance_id: inst.id.clone(),
            kind: ArtifactKind::Binary,
            name: "claude".to_string(),
        };

        let present = adapter.reconcile(&inst, &key).await.expect("reconcile");
        assert!(present.present);
        assert_eq!(present.version, Some("2.1.281".to_string()));

        // Another kind or name of the same instance is not this artifact.
        let other = ArtifactKey {
            kind: ArtifactKind::Tool,
            ..key.clone()
        };
        assert!(
            !adapter
                .reconcile(&inst, &other)
                .await
                .expect("reconcile")
                .present
        );

        // Gone is absent here -- `GoneAfterUpgrade` -- where `inventory`
        // would refuse (`test_inventory_refuses_an_install_that_is_not_what_detect_listed`).
        std::fs::remove_file(&layout.launcher).expect("remove launcher");
        let absent = adapter.reconcile(&inst, &key).await.expect("reconcile");
        assert!(!absent.present);
        assert_eq!(absent.version, None);
    }

    #[tokio::test]
    async fn test_reconcile_rejects_an_unreadable_version_and_a_dangling_launcher() {
        let home = TempHome::new("reconcile-broken");
        let layout = claude_layout(&home, "2.1.281");
        let inst = instance_for(&layout, Some("2.1.281"));
        let adapter = adapter(Arc::new(MockRunner::new()));
        let key = adapter.artifact_key(&inst);
        assert!(matches!(
            adapter.reconcile(&inst, &key).await,
            Err(AdapterError::Parse(_))
        ));
        // The dangling link alone is present with nothing to verify: the
        // same refusal, read from the disk and not through `inventory`
        // (which refuses this instance outright, detected whole).
        std::fs::remove_file(&layout.real).unwrap();
        assert!(matches!(
            adapter.reconcile(&inst, &key).await,
            Err(AdapterError::Parse(_))
        ));
    }

    #[tokio::test]
    async fn test_search_is_unsupported() {
        let home = TempHome::new("search");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let result = adapter
            .search(&instance_for(&layout, Some("2.1.281")), "claude")
            .await;
        assert!(matches!(result, Err(AdapterError::Unsupported(_))));
    }

    const LATEST_URL: &str = "https://downloads.claude.ai/claude-code-releases/latest";
    const STABLE_URL: &str = "https://downloads.claude.ai/claude-code-releases/stable";

    fn answer(body: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            body: body.to_string(),
        }
    }

    /// An adapter that has detected the layout in `home` (so `Detected`
    /// holds that home) and read its inventory (so `Reading` holds the
    /// launcher's version), over `http`: what `refresh` has done before it
    /// checks for updates.
    async fn refreshed_adapter(
        home: &TempHome,
        layout: &super::testing::ClaudeLayout,
        http: Arc<MockHttpClient>,
    ) -> (StandaloneAdapter, ManagerInstance) {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = StandaloneAdapter::new(&CLAUDE, runner, http, Arc::new(MockTrasher::new()));
        let inst = adapter
            .detect(&home.env(vec![home.path().join(".local/bin")]))
            .await
            .remove(0);
        adapter.inventory(&inst).await.expect("inventory");
        (adapter, inst)
    }

    #[tokio::test]
    async fn test_check_updates_compares_the_version_inventory_read_not_detects_nor_a_read_of_its_own(
    ) {
        // Detect read 2.1.281, the inventory 2.1.290, and by the time of
        // the check the launcher answers 2.1.299 (Claude Code updated
        // itself again): the candidate is built from the inventory's
        // 2.1.290, so the Updates page names the version the Installed
        // page's row shows, and the check runs nothing.
        let home = TempHome::new("check-inventoried");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        let argv = vec![layout.launcher.to_str().unwrap(), "--version"];
        runner.respond(argv.clone(), exited_0("2.1.281 (Claude Code)\n"));
        let http = Arc::new(MockHttpClient::new());
        http.respond(LATEST_URL, answer("2.1.295"));
        let adapter =
            StandaloneAdapter::new(&CLAUDE, runner.clone(), http, Arc::new(MockTrasher::new()));
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        assert_eq!(inst.version.as_deref(), Some("2.1.281"));
        runner.respond(argv.clone(), exited_0("2.1.290 (Claude Code)\n"));
        assert_eq!(
            adapter.inventory(&inst).await.unwrap()[0].version,
            "2.1.290"
        );
        runner.respond(argv, exited_0("2.1.299 (Claude Code)\n"));
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .unwrap();
        assert_eq!(out.candidates.len(), 1);
        assert_eq!(out.candidates[0].current, "2.1.290");
        assert_eq!(out.candidates[0].target, "2.1.295");
        assert_eq!(
            runner.calls().len(),
            2,
            "detect and inventory; the check reads nothing"
        );
    }

    #[tokio::test]
    async fn test_prereleases_stay_available_and_incomparable_pairs_are_uncheckable() {
        for (local, remote) in [
            ("2.1.281-beta", "2.1.290"),
            ("2.1.281", "2.1.290-beta"),
            ("2.1.281+build.7", "2.1.290+build.8"),
        ] {
            let home = TempHome::new("check-prerelease");
            let layout = claude_layout(&home, local);
            let runner = Arc::new(MockRunner::new());
            runner.respond(
                vec![layout.launcher.to_str().unwrap(), "--version"],
                exited_0(&format!("{local} (Claude Code)\n")),
            );
            let http = Arc::new(MockHttpClient::new());
            http.respond(LATEST_URL, answer(remote));
            let adapter =
                StandaloneAdapter::new(&CLAUDE, runner, http, Arc::new(MockTrasher::new()));
            let inst = adapter.detect(&home.env(vec![])).await.remove(0);
            assert_eq!(inst.status.unavailable, None);
            assert_eq!(inst.version.as_deref(), Some(local));
            assert_eq!(adapter.inventory(&inst).await.unwrap()[0].version, local);
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .unwrap();
            assert_eq!(out.candidates.len(), 1);
            let candidate = &out.candidates[0];
            assert!(!candidate.checkable);
            assert_eq!(candidate.current, local);
            assert_eq!(
                candidate.target, local,
                "existing uncheckable candidate convention"
            );
            assert!(
                matches!(&candidate.warnings[..], [Warning::Message(message)]
                if message.contains("cannot compare") && message.contains(local) && message.contains(remote))
            );
        }
    }

    #[tokio::test]
    async fn test_check_updates_does_not_use_a_stale_version_after_a_failed_read() {
        let home = TempHome::new("check-failed-live-read");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        let argv = vec![layout.launcher.to_str().unwrap(), "--version"];
        runner.respond(argv.clone(), exited_0("2.1.281 (Claude Code)\n"));
        let http = Arc::new(MockHttpClient::new());
        http.respond(LATEST_URL, answer("2.1.290"));
        let adapter = StandaloneAdapter::new(
            &CLAUDE,
            runner.clone(),
            http.clone(),
            Arc::new(MockTrasher::new()),
        );
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        runner.respond(argv, exited_0(""));
        // The inventory's read did not answer: its row has no version, and
        // the check, comparing that same reading, cannot check -- detect's
        // 2.1.281 is only what its row displays.
        assert_eq!(adapter.inventory(&inst).await.unwrap()[0].version, "");
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .unwrap();
        assert_eq!(out.candidates.len(), 1);
        assert!(!out.candidates[0].checkable);
        assert_eq!(out.candidates[0].current, "2.1.281");
        assert!(http.calls().is_empty());
    }

    #[tokio::test]
    async fn test_check_updates_lists_a_newer_published_version_as_one_candidate() {
        let home = TempHome::new("check-newer");
        let layout = claude_layout(&home, "2.1.281");
        let http = Arc::new(MockHttpClient::new());
        http.respond(LATEST_URL, answer("2.1.290\n"));
        let (adapter, inst) = refreshed_adapter(&home, &layout, http.clone()).await;

        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");

        assert!(out.notes.is_empty());
        assert_eq!(
            out.candidates,
            vec![UpdateCandidate {
                key: ArtifactKey {
                    instance_id: "standalone-claude".to_string(),
                    kind: ArtifactKind::Binary,
                    name: "claude".to_string(),
                },
                current: "2.1.281".to_string(),
                target: "2.1.290".to_string(),
                channel: UpdateChannel::Registry,
                checkable: true,
                warnings: Vec::new(),
                blocked: None,
            }]
        );
        assert_eq!(http.calls(), vec![LATEST_URL.to_string()]);
        let request = &http.requests()[0];
        assert_eq!(request.method, "GET");
        assert!(
            request.headers.is_empty(),
            "nothing of Banager's own but the client's UA"
        );
        assert_eq!(request.timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_check_updates_lists_nothing_when_the_pointer_is_equal_or_behind() {
        // The `stable` pointer was 2.1.273 while 2.1.281 was installed
        // (claude.md §4, 2026-09-24): "different" would badge a downgrade.
        // Only remote > local is an update (spec §4.3, D4).
        for body in ["2.1.281", "2.1.273\n", "2.0.999"] {
            let home = TempHome::new("check-not-newer");
            let layout = claude_layout(&home, "2.1.281");
            let http = Arc::new(MockHttpClient::new());
            http.respond(LATEST_URL, answer(body));
            let (adapter, inst) = refreshed_adapter(&home, &layout, http).await;
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("check_updates");
            assert!(out.candidates.is_empty(), "{body:?}");
        }
    }

    #[tokio::test]
    async fn test_check_updates_asks_the_stable_pointer_when_settings_say_so() {
        // Q7: `~/.claude/settings.json` `autoUpdatesChannel: "stable"`
        // picks the other pointer; missing or malformed is `latest`.
        let home = TempHome::new("check-stable");
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude");
        std::fs::write(
            home.path().join(".claude/settings.json"),
            r#"{"autoUpdatesChannel":"stable"}"#,
        )
        .expect("write settings");
        let http = Arc::new(MockHttpClient::new());
        http.respond(STABLE_URL, answer("2.1.273"));
        let (adapter, inst) = refreshed_adapter(&home, &layout, http.clone()).await;
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert!(out.candidates.is_empty());
        assert_eq!(http.calls(), vec![STABLE_URL.to_string()]);

        std::fs::write(home.path().join(".claude/settings.json"), "{ not json").expect("write");
        let http = Arc::new(MockHttpClient::new());
        http.respond(LATEST_URL, answer("2.1.281"));
        let (adapter, inst) = refreshed_adapter(&home, &layout, http.clone()).await;
        adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert_eq!(http.calls(), vec![LATEST_URL.to_string()]);
    }

    #[tokio::test]
    async fn test_check_updates_marks_a_failed_request_uncheckable_never_an_error() {
        // A network failure is "Banager could not find out", not a failed
        // source (which would hold the snapshot stale): one row at the
        // installed version, `checkable: false`, with the reason.
        let home = TempHome::new("check-network");
        let layout = claude_layout(&home, "2.1.281");
        let http = Arc::new(MockHttpClient::new());
        http.fail(LATEST_URL, "connection refused");
        let (adapter, inst) = refreshed_adapter(&home, &layout, http).await;
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("a failed lookup is not a source failure");
        assert_eq!(out.candidates.len(), 1);
        let c = &out.candidates[0];
        assert!(!c.checkable);
        assert_eq!(c.current, "2.1.281");
        assert_eq!(c.target, "2.1.281");
        assert_eq!(c.channel, UpdateChannel::Registry);
        assert!(
            matches!(&c.warnings[..], [Warning::Message(m)] if m.contains("connection refused"))
        );
    }

    #[tokio::test]
    async fn test_check_updates_marks_a_non_200_uncheckable() {
        let home = TempHome::new("check-status");
        let layout = claude_layout(&home, "2.1.281");
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            LATEST_URL,
            HttpResponse {
                status: 503,
                body: "<html>busy</html>".to_string(),
            },
        );
        let (adapter, inst) = refreshed_adapter(&home, &layout, http).await;
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert_eq!(out.candidates.len(), 1);
        assert!(!out.candidates[0].checkable);
        assert!(
            matches!(&out.candidates[0].warnings[..], [Warning::Message(m)] if m.contains("503"))
        );
    }

    #[tokio::test]
    async fn test_check_updates_marks_a_non_version_answer_uncheckable() {
        // An HTML page with status 200 (a captive portal), multiple tokens:
        // no candidate is built from it, and the reason quotes only a
        // little of the body.
        for body in [
            "<html><body>Sign in to the network</body></html>",
            "2.1.290 extra",
        ] {
            let home = TempHome::new("check-garbage");
            let layout = claude_layout(&home, "2.1.281");
            let http = Arc::new(MockHttpClient::new());
            http.respond(LATEST_URL, answer(body));
            let (adapter, inst) = refreshed_adapter(&home, &layout, http).await;
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("check_updates");
            assert_eq!(out.candidates.len(), 1, "{body:?}");
            assert!(!out.candidates[0].checkable);
            assert!(
                matches!(&out.candidates[0].warnings[..], [Warning::Message(m)] if m.contains("did not answer with a version") && m.len() < 120),
                "{:?}",
                out.candidates[0].warnings
            );
        }
    }

    #[tokio::test]
    async fn test_check_updates_ignores_the_include_self_updating_switch() {
        // D5: the switch is Homebrew's `--greedy`, for casks whose live
        // version `brew outdated` cannot see. This badge is read from the
        // launcher's live version and is true whatever the switch says.
        for include_self_updating in [false, true] {
            let home = TempHome::new("check-greedy");
            let layout = claude_layout(&home, "2.1.281");
            let http = Arc::new(MockHttpClient::new());
            http.respond(LATEST_URL, answer("2.1.290"));
            let (adapter, inst) = refreshed_adapter(&home, &layout, http).await;
            let out = adapter
                .check_updates(
                    &inst,
                    &CheckOptions {
                        include_self_updating,
                        ..CheckOptions::default()
                    },
                )
                .await
                .expect("check_updates");
            assert_eq!(
                out.candidates.len(),
                1,
                "include_self_updating={include_self_updating}"
            );
        }
    }

    #[tokio::test]
    async fn test_check_updates_asks_nothing_for_a_launcher_only_row() {
        // No installed version to compare, so no request and no row: the
        // notice already says what is left.
        let home = TempHome::new("check-launcher-only");
        let root = home.path().join(".local/share/claude");
        let launcher = home.link(".local/bin/claude", &root.join("versions/2.1.281"));
        let http = Arc::new(MockHttpClient::new());
        let adapter = StandaloneAdapter::new(
            &CLAUDE,
            Arc::new(MockRunner::new()),
            http.clone(),
            Arc::new(MockTrasher::new()),
        );
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        assert_eq!(inst.exe_path, launcher);
        adapter.inventory(&inst).await.expect("inventory");
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert!(out.candidates.is_empty());
        assert!(http.calls().is_empty());
    }

    #[tokio::test]
    async fn test_check_updates_refuses_as_inventory_did_and_needs_an_inventory_first() {
        // The reading `inventory` leaves for `check_updates` (`Reading`):
        // a check with no inventory before it has nothing to compare and
        // is refused (unreachable through `Session`); after an inventory
        // that refused because the launcher is gone, the check refuses
        // with the same reason and asks the endpoint nothing, so `refresh`
        // keeps the previous candidates beside the previous rows; and one
        // inventory feeds one check.
        let home = TempHome::new("check-changed");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let http = Arc::new(MockHttpClient::new());
        http.respond(LATEST_URL, answer("2.1.290"));
        let adapter =
            StandaloneAdapter::new(&CLAUDE, runner, http.clone(), Arc::new(MockTrasher::new()));
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        let Err(AdapterError::Refused(reason)) =
            adapter.check_updates(&inst, &CheckOptions::default()).await
        else {
            panic!("a check with no inventory before it is refused");
        };
        assert!(reason.contains("inventoried"), "{reason}");

        std::fs::remove_file(&layout.launcher).expect("remove the launcher");
        let Err(AdapterError::Refused(refused)) = adapter.inventory(&inst).await else {
            panic!("a launcher that is gone is a refusal");
        };
        let Err(AdapterError::Refused(reason)) =
            adapter.check_updates(&inst, &CheckOptions::default()).await
        else {
            panic!("the check refuses as the inventory did");
        };
        assert_eq!(reason, refused);
        assert!(http.calls().is_empty());
        // That reading has been taken: another check needs another inventory.
        assert!(matches!(
            adapter.check_updates(&inst, &CheckOptions::default()).await,
            Err(AdapterError::Refused(_))
        ));
    }

    fn request(kind: OpKind, artifact_kind: ArtifactKind, name: &str) -> OpRequest {
        OpRequest {
            kind,
            instance_id: "standalone-claude".to_string(),
            artifact_kind,
            name: name.to_string(),
        }
    }

    #[tokio::test]
    async fn test_plan_upgrade_is_the_tools_own_update_command_without_the_version_env() {
        // Spec §五: `<launcher> update`, 1800 s, KillThenReconcile, the
        // instance's own lock and no other (claude's `extra_locks` is
        // empty), no password. Version reads add `DISABLE_AUTOUPDATER=1`;
        // upgrade adds no environment override (spec §3.4): the updater
        // must be allowed to update. RealRunner inherits ambient
        // variables, including this one. Detected first: the Upgrade arm
        // reads the seat.
        let home = TempHome::new("plan-upgrade");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = adapter(runner);
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);

        let plan = adapter
            .plan(
                &inst,
                &request(OpKind::Upgrade, ArtifactKind::Binary, "claude"),
            )
            .await
            .expect("plan");

        assert_eq!(command_program(&plan), layout.launcher);
        assert_eq!(command_args(&plan), vec!["update".to_string()]);
        assert!(
            command_env(&plan).is_empty(),
            "upgrade adds no environment override"
        );
        assert!(!plan.needs_password);
        assert_eq!(
            plan.locks,
            vec![ResourceLock("standalone-claude".to_string())]
        );
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);
        assert!(plan.warnings.is_empty());
        assert!(plan.affected.is_empty());
        assert_eq!(plan.timeout_secs, 1800);
        assert_eq!(plan.request.name, "claude");
    }

    #[tokio::test]
    async fn test_plan_upgrade_is_refused_before_any_detect() {
        // Unreachable through `Session`, which detects before it plans;
        // the adapter's own answer for a plan asked of it cold (spec §3.2).
        let home = TempHome::new("plan-cold");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        assert!(matches!(
            adapter
                .plan(
                    &instance_for(&layout, Some("2.1.281")),
                    &request(OpKind::Upgrade, ArtifactKind::Binary, "claude")
                )
                .await,
            Err(AdapterError::Refused(_))
        ));
    }

    #[tokio::test]
    async fn test_plan_refuses_an_instance_the_seat_no_longer_describes() {
        // The seat is one slot the latest detect overwrites. Detect home
        // A, then home B, then plan for A's instance: the program would be
        // A's launcher and the locks and warnings B's (ruling 9). Refused,
        // until a detect of A seats A again.
        let home_a = TempHome::new("seat-a");
        let layout_a = claude_layout(&home_a, "2.1.281");
        let home_b = TempHome::new("seat-b");
        let layout_b = claude_layout(&home_b, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        for layout in [&layout_a, &layout_b] {
            runner.respond(
                vec![layout.launcher.to_str().unwrap(), "--version"],
                exited_0("2.1.281 (Claude Code)\n"),
            );
        }
        let adapter = adapter(runner);
        let inst_a = adapter.detect(&home_a.env(vec![])).await.remove(0);
        let inst_b = adapter.detect(&home_b.env(vec![])).await.remove(0);
        assert_ne!(inst_a.exe_path, inst_b.exe_path);

        let req = request(OpKind::Upgrade, ArtifactKind::Binary, "claude");
        assert!(
            matches!(
                adapter.plan(&inst_a, &req).await,
                Err(AdapterError::Refused(_))
            ),
            "A's instance against B's seat"
        );
        assert_eq!(
            adapter
                .plan(&inst_b, &req)
                .await
                .expect("B's instance against B's seat")
                .action,
            PlanAction::Command {
                program: layout_b.launcher.clone(),
                args: vec!["update".to_string()],
                env: Vec::new(),
            }
        );
        adapter.detect(&home_a.env(vec![])).await;
        assert_eq!(
            adapter
                .plan(&inst_a, &req)
                .await
                .expect("A's instance against A's seat")
                .action,
            PlanAction::Command {
                program: layout_a.launcher.clone(),
                args: vec!["update".to_string()],
                env: Vec::new(),
            }
        );
    }

    #[tokio::test]
    async fn test_detect_seats_the_two_homes_and_zdotdir_for_the_plans_that_need_them() {
        // `Detected.cargo_home` follows `CARGO_HOME` (through
        // `cargo::cargo_home_of`), `rustup_home` follows `RUSTUP_HOME`
        // (through `path_env::tool_home`), `zdotdir` is carried raw: the
        // rustup recipe's lock, gate and warnings read them in `plan` and
        // `inventory`, which have no HostEnv of their own.
        let home = TempHome::new("detect-seat");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = adapter(runner);
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        let seat = adapter.seated_detected_for(&inst).expect("seated");
        assert_eq!(seat.cargo_home, Some(home.path().join(".cargo")));
        assert_eq!(seat.rustup_home, Some(home.path().join(".rustup")));
        assert_eq!(seat.zdotdir, None);

        let custom_cargo = home.path().join("elsewhere/cargo");
        let custom_rustup = home.path().join("elsewhere/rustup");
        let inst = adapter
            .detect(&HostEnv {
                cargo_home: Some(custom_cargo.clone()),
                rustup_home: Some(custom_rustup.clone()),
                zdotdir: Some(home.path().to_path_buf()),
                ..home.env(vec![])
            })
            .await
            .remove(0);
        let seat = adapter.seated_detected_for(&inst).expect("seated");
        assert_eq!(seat.cargo_home, Some(custom_cargo));
        assert_eq!(seat.rustup_home, Some(custom_rustup));
        assert_eq!(seat.zdotdir, Some(home.path().to_path_buf()));

        // Relative values: unsupported, seated as `None`; an empty one is
        // the default (the `home` crate's rule, Task 1).
        let inst = adapter
            .detect(&HostEnv {
                cargo_home: Some(PathBuf::from("cargo")),
                rustup_home: Some(PathBuf::from("")),
                ..home.env(vec![])
            })
            .await
            .remove(0);
        let seat = adapter.seated_detected_for(&inst).expect("seated");
        assert_eq!(seat.cargo_home, None);
        assert_eq!(seat.rustup_home, Some(home.path().join(".rustup")));
    }

    #[tokio::test]
    async fn test_plan_refuses_a_request_for_another_instance() {
        let home = TempHome::new("plan-other-instance");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let req = OpRequest {
            instance_id: "standalone-grok".to_string(),
            ..request(OpKind::Upgrade, ArtifactKind::Binary, "claude")
        };
        assert!(matches!(
            adapter
                .plan(&instance_for(&layout, Some("2.1.281")), &req)
                .await,
            Err(AdapterError::Refused(_))
        ));
    }

    #[tokio::test]
    async fn test_plan_refuses_a_name_or_kind_that_is_not_this_tool() {
        // The one artifact is `Binary`/`claude`; anything else is a request
        // this adapter cannot mean, refused before an argv exists.
        let home = TempHome::new("plan-wrong-name");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let inst = instance_for(&layout, Some("2.1.281"));
        for (kind, name) in [
            (ArtifactKind::Binary, "grok"),
            (ArtifactKind::Binary, "claude-code"),
            (ArtifactKind::Tool, "claude"),
        ] {
            assert!(
                matches!(
                    adapter
                        .plan(&inst, &request(OpKind::Upgrade, kind, name))
                        .await,
                    Err(AdapterError::InvalidName(_))
                ),
                "{kind:?} {name}"
            );
        }
        // A name that fails `validate_package_name` is refused as such
        // before the tool-name comparison.
        assert!(matches!(
            adapter
                .plan(
                    &inst,
                    &request(OpKind::Upgrade, ArtifactKind::Binary, "-rf")
                )
                .await,
            Err(AdapterError::InvalidName(_))
        ));
    }

    /// Claude Code's recipe with no uninstall method: what the second
    /// batch's Ollama.app will be (spec §6.1 "Neither"). Every other field
    /// is `CLAUDE`'s.
    static NO_UNINSTALL: Recipe = Recipe {
        id: "claude",
        meta_toml: include_str!("../../../../../adapters/meta/standalone-claude.toml"),
        route: Route {
            kind: RouteKind::SymlinkIntoRoot,
            launcher: "~/.local/bin/claude",
            root: "~/.local/share/claude",
        },
        version: VersionCmd {
            args: &["--version"],
            env: &[("DISABLE_AUTOUPDATER", "1")],
            parse: VersionParse::FirstToken,
        },
        latest: Latest::ClaudeChannel {
            base: "https://downloads.claude.ai/claude-code-releases",
        },
        self_updates: true,
        upgrade: Some(UpgradeCmd {
            args: &["update"],
            timeout_secs: 1800,
            cancel: CancelPolicy::KillThenReconcile,
        }),
        uninstall: None,
        extra_locks: no_extra_locks,
        backup_globs: &[],
        other_commands: &[],
    };

    #[tokio::test]
    async fn test_plan_refuses_install_as_unsupported() {
        // The gate has no install-specific rule: an install against an
        // installed, answering tool reaches this `plan`, and `Unsupported`
        // is the answer it gets (B's corrected comment, kept).
        let home = TempHome::new("plan-install");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let inst = instance_for(&layout, Some("2.1.281"));
        assert!(matches!(
            adapter
                .plan(
                    &inst,
                    &request(OpKind::Install, ArtifactKind::Binary, "claude")
                )
                .await,
            Err(AdapterError::Unsupported(_))
        ));
    }

    #[tokio::test]
    async fn test_a_recipe_without_an_uninstall_method_says_so_and_refuses_to_plan_one() {
        // Spec §6.1 "Neither": the artifact carries `NoSafeMethod` (the
        // gate and the page read it), and `plan(Uninstall)` refuses with
        // the same reason for a stale snapshot. One of the variant's two
        // production paths; the other is a `Command` uninstall's gate
        // (`rustup::uninstall_blocked`, through `rows`).
        let home = TempHome::new("plan-no-uninstall");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = StandaloneAdapter::new(
            &NO_UNINSTALL,
            runner,
            Arc::new(MockHttpClient::new()),
            Arc::new(MockTrasher::new()),
        );
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        let artifacts = adapter.inventory(&inst).await.expect("inventory");
        assert_eq!(
            artifacts[0].uninstall_blocked,
            Some(UninstallBlocked::NoSafeMethod)
        );
        match adapter.plan(&inst, &uninstall()).await {
            Err(AdapterError::UninstallBlocked { reason }) => {
                assert_eq!(reason, UninstallBlocked::NoSafeMethod)
            }
            other => panic!("expected UninstallBlocked(NoSafeMethod), got {other:?}"),
        }
    }

    /// The claude layout, detected -- so `Detected` holds the home
    /// `execute` finds the launcher under when it looks at it again before
    /// an upgrade -- over a runner the test keeps (to read its calls) that
    /// answers `--version` and `update` at the launcher.
    struct UpgradeSetup {
        home: TempHome,
        layout: super::testing::ClaudeLayout,
        runner: Arc<MockRunner>,
        adapter: StandaloneAdapter,
        inst: ManagerInstance,
    }

    async fn detected_for_upgrade(tag: &str) -> UpgradeSetup {
        let home = TempHome::new(tag);
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "update"],
            exited_0("Successfully updated from 2.1.281 to version 2.1.290\n"),
        );
        let adapter = adapter(runner.clone());
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        UpgradeSetup {
            home,
            layout,
            runner,
            adapter,
            inst,
        }
    }

    async fn upgrade_plan(adapter: &StandaloneAdapter, inst: &ManagerInstance) -> Plan {
        adapter
            .plan(
                inst,
                &request(OpKind::Upgrade, ArtifactKind::Binary, "claude"),
            )
            .await
            .expect("plan")
    }

    /// The `update` calls the runner saw: what `execute` spawned, apart
    /// from detect's version read.
    fn update_calls(runner: &MockRunner, layout: &super::testing::ClaudeLayout) -> usize {
        let update = vec![
            layout.launcher.to_str().unwrap().to_string(),
            "update".to_string(),
        ];
        runner
            .calls()
            .iter()
            .filter(|call| **call == update)
            .count()
    }

    #[tokio::test]
    async fn test_execute_runs_the_plan_and_streams_its_output() {
        // `_home` is bound, not left to `..`: the temp home lives as long as
        // it does, and the launcher `execute` looks at again is in it.
        let UpgradeSetup {
            home: _home,
            layout,
            runner,
            adapter,
            inst,
        } = detected_for_upgrade("execute").await;
        let plan = upgrade_plan(&adapter, &inst).await;
        let sink = Arc::new(VecSink::new());
        let outcome = adapter
            .execute(&plan, sink.clone(), 1, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(update_calls(&runner, &layout), 1);
        assert_eq!(sink.snapshot().len(), 1, "one log line, streamed");
    }

    #[tokio::test]
    async fn test_execute_refuses_an_upgrade_when_the_launcher_is_no_longer_the_native_installs() {
        // B's Astra finding B-1: between the preview and the click,
        // `~/.local/bin/claude` can be re-pointed at Homebrew's or npm's
        // copy (the user reinstalling another way), at some other file,
        // replaced by a plain file, left dangling, or removed. The plan
        // names that path, and spawning it would run whatever is behind it
        // now -- another copy's updater. `execute` looks at the launcher
        // again immediately before spawning, as detect does: not this
        // route's, and nothing is started; the fault names the launcher,
        // as a path-list uninstall's names a path that changed.
        /// What happens to the launcher after the preview, in one case.
        type Retarget = fn(&TempHome, &super::testing::ClaudeLayout);
        let cases: [(&str, Retarget); 6] = [
            ("re-pointed at Homebrew's copy", |home, layout| {
                let cask = home.executable("opt/homebrew/Caskroom/claude-code/2.1.267/claude");
                std::fs::remove_file(&layout.launcher).unwrap();
                std::os::unix::fs::symlink(cask, &layout.launcher).unwrap();
            }),
            ("re-pointed at npm's copy", |home, layout| {
                let cli = home
                    .executable("opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js");
                std::fs::remove_file(&layout.launcher).unwrap();
                std::os::unix::fs::symlink(cli, &layout.launcher).unwrap();
            }),
            ("re-pointed outside the root", |home, layout| {
                let elsewhere = home.executable("elsewhere/claude");
                std::fs::remove_file(&layout.launcher).unwrap();
                std::os::unix::fs::symlink(elsewhere, &layout.launcher).unwrap();
            }),
            ("replaced by a plain file", |home, layout| {
                std::fs::remove_file(&layout.launcher).unwrap();
                home.executable(".local/bin/claude");
            }),
            ("dangling: the program files gone", |_home, layout| {
                std::fs::remove_dir_all(&layout.root).unwrap();
            }),
            ("removed", |_home, layout| {
                std::fs::remove_file(&layout.launcher).unwrap();
            }),
        ];
        for (case, retarget) in cases {
            let UpgradeSetup {
                home,
                layout,
                runner,
                adapter,
                inst,
            } = detected_for_upgrade("execute-upgrade-retargeted").await;
            let plan = upgrade_plan(&adapter, &inst).await;
            retarget(&home, &layout);
            let sink = Arc::new(VecSink::new());

            let outcome = adapter
                .execute(&plan, sink.clone(), 1, CancellationToken::new())
                .await
                .expect("execute");

            assert_eq!(
                outcome,
                Outcome::BanagerFailed(Fault::PathChanged {
                    path: "~/.local/bin/claude".to_string()
                }),
                "{case}"
            );
            assert_eq!(update_calls(&runner, &layout), 0, "{case}: nothing spawned");
            assert!(sink.snapshot().is_empty(), "{case}: no log line");
        }
    }

    #[tokio::test]
    async fn test_execute_runs_an_upgrade_after_the_launcher_moved_to_a_newer_version_inside_the_root(
    ) {
        // Claude Code updating itself between the preview and the click
        // re-points the launcher at a new file inside its own root. That is
        // still the native install, and `<launcher> update` is still the
        // command the user confirmed: it runs. (A path-list uninstall
        // refuses the same change, because there the link itself is what
        // moves: `test_execute_refuses_a_launcher_the_updater_re_pointed_after_the_preview`.)
        let UpgradeSetup {
            home,
            layout,
            runner,
            adapter,
            inst,
        } = detected_for_upgrade("execute-upgrade-self-updated").await;
        let plan = upgrade_plan(&adapter, &inst).await;
        let newer = home.executable(".local/share/claude/versions/2.1.282");
        std::fs::remove_file(&layout.launcher).unwrap();
        std::os::unix::fs::symlink(newer, &layout.launcher).unwrap();
        let sink = Arc::new(VecSink::new());

        let outcome = adapter
            .execute(&plan, sink.clone(), 1, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(update_calls(&runner, &layout), 1);
        assert_eq!(sink.snapshot().len(), 1, "one log line, streamed");
    }

    #[tokio::test]
    async fn test_execute_refuses_an_upgrade_plan_that_names_another_program_as_banagers_own_bug() {
        // `plan` names the launcher and nothing else (spec 附录 B). A plan
        // whose program is any other file was not built by `plan`, and so
        // is one handed to an adapter with no detect before it (no home to
        // find the launcher under -- unreachable through `Session`, which
        // detects first; `plan` itself refuses such an adapter,
        // `test_plan_upgrade_is_refused_before_any_detect`): both are
        // refused as Banager's own bug (`Refused`, which `run_operation`
        // reports as `Fault::Internal`), nothing spawned -- never run on
        // the plan's word.
        let UpgradeSetup {
            home,
            runner,
            adapter: detected,
            inst,
            ..
        } = detected_for_upgrade("execute-upgrade-other-program").await;
        let other = home.executable("elsewhere/claude");
        runner.respond(
            vec![other.to_str().unwrap(), "update"],
            exited_0("Successfully updated from 2.1.281 to version 2.1.290\n"),
        );
        let plan = Plan {
            action: PlanAction::Command {
                program: other,
                args: vec!["update".to_string()],
                env: Vec::new(),
            },
            ..upgrade_plan(&detected, &inst).await
        };
        assert!(matches!(
            detected
                .execute(&plan, Arc::new(VecSink::new()), 1, CancellationToken::new())
                .await,
            Err(AdapterError::Refused(_))
        ));
        assert_eq!(runner.calls().len(), 1, "only detect's version read");

        let fresh = Arc::new(MockRunner::new());
        let undetected = adapter(fresh.clone());
        let plan = upgrade_plan(&detected, &inst).await;
        assert!(matches!(
            undetected
                .execute(&plan, Arc::new(VecSink::new()), 1, CancellationToken::new())
                .await,
            Err(AdapterError::Refused(_))
        ));
        assert!(fresh.calls().is_empty(), "nothing spawned");
    }

    #[tokio::test]
    async fn test_all_builds_one_adapter_per_recipe_under_its_standalone_id() {
        let adapters = all(
            Arc::new(MockRunner::new()),
            Arc::new(MockHttpClient::new()),
            Arc::new(MockTrasher::new()),
        );
        let ids: Vec<String> = adapters.iter().map(|a| a.meta().id.clone()).collect();
        // `RECIPES`' reading order, one adapter each.
        assert_eq!(
            ids,
            vec![
                "standalone-claude".to_string(),
                "standalone-agy".to_string(),
                "standalone-grok".to_string(),
                "standalone-rustup".to_string()
            ]
        );
        assert_eq!(adapters.len(), super::recipes::RECIPES.len());
    }

    #[tokio::test]
    async fn test_the_adapter_trait_delegates_to_the_inherent_methods() {
        // Through `dyn Adapter`, as Session sees it.
        let home = TempHome::new("trait");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter: Arc<dyn Adapter> = Arc::new(StandaloneAdapter::new(
            &CLAUDE,
            runner,
            Arc::new(MockHttpClient::new()),
            Arc::new(MockTrasher::new()),
        ));
        let instances = adapter.detect(&home.env(vec![])).await;
        assert_eq!(instances.len(), 1);
        let artifacts = adapter.inventory(&instances[0]).await.expect("inventory");
        assert_eq!(artifacts[0].display_name, "Claude Code");
        assert!(matches!(
            adapter.search(&instances[0], "x").await,
            Err(AdapterError::Unsupported(_))
        ));
    }

    /// The recorded fixture directory for the version the meta file
    /// verifies: `adapters/fixtures/standalone-claude/<verified>/`.
    fn fixture(name: &str) -> String {
        let adapter = adapter(Arc::new(MockRunner::new()));
        let version = adapter
            .meta
            .verified_versions
            .first()
            .expect("meta lists the recorded version")
            .clone();
        let path = format!("../../adapters/fixtures/standalone-claude/{version}/{name}");
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
    }

    #[test]
    fn test_the_recorded_version_line_parses_to_the_verified_version() {
        let verified = adapter(Arc::new(MockRunner::new())).meta.verified_versions[0].clone();
        assert_eq!(
            latest::parse_version(&fixture("version.txt"), CLAUDE.version.parse),
            Some(verified)
        );
    }

    /// `layout.txt` is evidence, not parser input, and it is the one
    /// recorded file in its directory whose text was edited after
    /// recording: its README says the home directory's absolute path
    /// became `~` and the owner column's account name became `user`, so
    /// that the directory names no account and no machine. This checks
    /// that no absolute home directory and no `.local` host name survives
    /// in that file or in the README beside it, and that the edited
    /// launcher line still names the verified version.
    #[test]
    fn test_the_recorded_layout_and_its_readme_name_no_home_directory_or_host() {
        let verified = adapter(Arc::new(MockRunner::new())).meta.verified_versions[0].clone();
        let layout = fixture("layout.txt");
        let readme = fixture("README.md");
        for (name, text) in [("layout.txt", &layout), ("README.md", &readme)] {
            for prefix in ["/Users/", "/home/"] {
                assert!(
                    !text.contains(prefix),
                    "{name} names an absolute home directory under {prefix}"
                );
            }
            // `X.local` is a Mac's Bonjour host name when `X` ends in a
            // letter, digit or hyphen; `~/.local/...` is preceded by `/`
            // and is not one.
            let names_a_host = text.match_indices(".local").any(|(at, _)| {
                text[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '-')
            });
            assert!(!names_a_host, "{name} names a .local host");
        }
        assert!(
            layout.contains(&format!(
                "~/.local/bin/claude -> ~/.local/share/claude/versions/{verified}"
            )),
            "layout.txt's launcher line names the verified version under ~"
        );
        assert!(
            layout.contains("~/.local/share/claude/versions:"),
            "layout.txt's versions listing is headed by the directory under ~"
        );
    }

    #[test]
    fn test_the_recorded_channel_pointers_are_bare_versions() {
        let latest_pointer = latest::parse_channel_body(&fixture("latest.txt")).expect("latest");
        let stable_pointer = latest::parse_channel_body(&fixture("stable.txt")).expect("stable");
        // The recording's reason for existing: the stable pointer is not
        // ahead of the latest one.
        assert_ne!(
            latest::compare_dotted(&stable_pointer, &latest_pointer),
            Some(Ordering::Greater),
            "stable {stable_pointer} is not ahead of latest {latest_pointer}"
        );
    }

    #[tokio::test]
    async fn test_check_updates_over_the_recorded_pointers_lists_only_a_real_update() {
        // Fed each recorded pointer as the endpoint's body: the stable
        // pointer, behind or equal to the installed version, yields no
        // candidate; the latest pointer yields one exactly when it is
        // greater than the installed version -- both derived from the
        // recording, so a re-recording on a later day stays honest.
        let installed = adapter(Arc::new(MockRunner::new())).meta.verified_versions[0].clone();
        for (name, url) in [("stable.txt", STABLE_URL), ("latest.txt", LATEST_URL)] {
            let body = fixture(name);
            let pointer = latest::parse_channel_body(&body).expect(name);
            let home = TempHome::new("check-recorded");
            let layout = claude_layout(&home, &installed);
            if name == "stable.txt" {
                home.dir(".claude");
                std::fs::write(
                    home.path().join(".claude/settings.json"),
                    r#"{"autoUpdatesChannel":"stable"}"#,
                )
                .expect("write settings");
            }
            let http = Arc::new(MockHttpClient::new());
            http.respond(url, answer(&body));
            let runner = Arc::new(MockRunner::new());
            runner.respond(
                vec![layout.launcher.to_str().unwrap(), "--version"],
                exited_0(&format!("{installed} (Claude Code)\n")),
            );
            let adapter =
                StandaloneAdapter::new(&CLAUDE, runner, http, Arc::new(MockTrasher::new()));
            let inst = adapter.detect(&home.env(vec![])).await.remove(0);
            adapter.inventory(&inst).await.expect("inventory");
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("check_updates");
            match latest::compare_dotted(&installed, &pointer) {
                Some(Ordering::Less) => {
                    assert_eq!(out.candidates.len(), 1, "{name}");
                    assert_eq!(out.candidates[0].target, pointer);
                    assert!(out.candidates[0].checkable);
                }
                Some(Ordering::Equal | Ordering::Greater) => assert!(out.candidates.is_empty()),
                None => {
                    assert_eq!(out.candidates.len(), 1, "{name}");
                    assert!(!out.candidates[0].checkable);
                    assert_eq!(out.candidates[0].current, installed);
                    assert_eq!(out.candidates[0].target, installed);
                }
            }
        }
    }

    #[tokio::test]
    async fn test_plan_uninstall_is_a_trash_paths_plan_carrying_the_dialogs_list() {
        // Spec §6.2, §6.6: no command; the paths in execution order, the
        // launcher last, with what the preview saw at each (Ruling 10) --
        // which never reaches the window: the action's JSON is the paths
        // alone; the warnings the dialog lists, in that order; the
        // instance's lock; no password; the removal's own time budget.
        let trasher = Arc::new(MockTrasher::new());
        let (home, layout, adapter, inst) = full_install("plan-uninstall", trasher).await;

        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");

        let PlanAction::TrashPaths { paths, previewed } = &plan.action else {
            panic!("a path list, not a command: {:?}", plan.action);
        };
        let listed = vec![
            home.path().join(".local/share/claude"),
            home.path().join(".claude/downloads"),
            layout.launcher.clone(),
        ];
        assert_eq!(paths, &listed);
        assert_eq!(
            previewed.iter().map(|seen| seen.kind).collect::<Vec<_>>(),
            vec![ItemKind::Dir, ItemKind::Dir, ItemKind::Symlink]
        );
        assert_eq!(
            serde_json::to_value(&plan.action).unwrap(),
            serde_json::json!({ "TrashPaths": { "paths": listed } })
        );
        assert_eq!(
            plan.warnings,
            vec![
                Warning::WillTrash {
                    path: "~/.local/share/claude".to_string(),
                    what: RemovedWhat::Program
                },
                Warning::WillTrash {
                    path: "~/.claude/downloads".to_string(),
                    what: RemovedWhat::Cache
                },
                Warning::WillTrash {
                    path: "~/.local/bin/claude".to_string(),
                    what: RemovedWhat::Launcher
                },
                Warning::WillKeep {
                    path: "~/.claude".to_string(),
                    what: KeptWhat::SettingsAndHistory
                },
                Warning::WillKeep {
                    path: "~/.claude.json".to_string(),
                    what: KeptWhat::Settings
                },
            ]
        );
        assert!(!plan.needs_password);
        assert_eq!(
            plan.locks,
            vec![ResourceLock("standalone-claude".to_string())]
        );
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);
        assert!(
            plan.affected.is_empty(),
            "a non-empty list would disable Uninstall"
        );
        assert_eq!(plan.timeout_secs, removal::TIMEOUT_SECS);
        assert_eq!(plan.request, uninstall());
    }

    #[tokio::test]
    async fn test_plan_uninstall_refuses_before_anything_was_detected() {
        // Unreachable through `Session` (it detects before it plans); the
        // adapter's own answer, a plain `Refused`, for a caller that skips
        // that (spec §3.2).
        let home = TempHome::new("plan-undetected");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let inst = instance_for(&layout, Some("2.1.281"));
        assert!(matches!(
            adapter.plan(&inst, &uninstall()).await,
            Err(AdapterError::Refused(_))
        ));
    }

    #[tokio::test]
    async fn test_plan_uninstall_refuses_a_path_that_fails_a_check_with_the_reason() {
        // One of the checks failing reaches the dialog as `UninstallUnsafe`
        // (Task 5's kind), with the path abbreviated.
        let trasher = Arc::new(MockTrasher::new());
        let (home, layout, adapter, inst) = full_install("plan-unsafe", trasher).await;
        let elsewhere = home.executable("elsewhere/claude");
        std::fs::remove_file(&layout.launcher).unwrap();
        std::os::unix::fs::symlink(elsewhere, &layout.launcher).unwrap();

        match adapter.plan(&inst, &uninstall()).await {
            Err(AdapterError::UninstallUnsafe { path, reason }) => {
                assert_eq!(path, "~/.local/bin/claude");
                assert_eq!(reason, UninstallUnsafeReason::NotWhatInstructionsExpect);
            }
            other => panic!("expected UninstallUnsafe, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_plan_uninstall_on_a_launcher_only_row_lists_the_program_dir_as_already_gone() {
        let home = TempHome::new("plan-launcher-only");
        let layout = claude_layout(&home, "2.1.281");
        std::fs::remove_dir_all(&layout.root).unwrap();
        let adapter = adapter(Arc::new(MockRunner::new()));
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        assert_eq!(inst.status.notes, vec![InstanceNote::LauncherOnly]);

        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");

        let PlanAction::TrashPaths { paths, previewed } = &plan.action else {
            panic!("a path list, not a command: {:?}", plan.action);
        };
        assert_eq!(paths, &vec![layout.launcher.clone()]);
        assert_eq!(previewed.len(), 1, "what the preview saw at the launcher");
        assert_eq!(
            plan.warnings,
            vec![
                Warning::AlreadyGone {
                    path: "~/.local/share/claude".to_string()
                },
                Warning::WillTrash {
                    path: "~/.local/bin/claude".to_string(),
                    what: RemovedWhat::Launcher
                },
            ]
        );
    }

    #[tokio::test]
    async fn test_execute_moves_every_listed_path_in_order_and_logs_each() {
        // Review Focus 1: two paths named `claude`, both moved, both in the
        // Trash.
        let trasher = Arc::new(MockTrasher::new());
        let (home, layout, adapter, inst) =
            full_install("execute-uninstall", trasher.clone()).await;
        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");
        let sink = Arc::new(VecSink::new());

        let outcome = adapter
            .execute(&plan, sink.clone(), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(outcome, Outcome::Succeeded);
        let PlanAction::TrashPaths { paths, .. } = &plan.action else {
            panic!("a path list");
        };
        assert_eq!(trasher.calls(), *paths);
        assert!(trasher.bin().join("claude/versions/2.1.281").is_file());
        assert!(trasher.bin().join("downloads").is_dir());
        let link = trasher.bin().join("claude 2");
        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(std::fs::symlink_metadata(&layout.launcher).is_err());
        assert!(home.path().join(".claude.json").is_file(), "settings stay");
        assert!(home
            .path()
            .join(".claude/projects/p/session.jsonl")
            .is_file());
        assert_eq!(
            notes_of(&sink),
            vec![
                LogNote::MovedToTrash {
                    path: "~/.local/share/claude".to_string(),
                    trashed_to: trasher.bin().join("claude").display().to_string(),
                },
                LogNote::MovedToTrash {
                    path: "~/.claude/downloads".to_string(),
                    trashed_to: trasher.bin().join("downloads").display().to_string(),
                },
                LogNote::MovedToTrash {
                    path: "~/.local/bin/claude".to_string(),
                    trashed_to: link.display().to_string(),
                },
            ]
        );
        // Afterwards the tool is gone for `detect`; and the inventory of
        // the row that detect listed refuses, the launcher being gone since
        // then (the refresh after an operation detects again first).
        assert!(adapter.detect(&env_as_owner(&home)).await.is_empty());
        assert!(matches!(
            adapter.inventory(&inst).await,
            Err(AdapterError::Refused(_))
        ));
    }

    #[tokio::test]
    async fn test_execute_refuses_when_a_path_changed_since_the_preview() {
        // Review Focus 2: the launcher re-pointed between the preview and
        // the click. Nothing moved; the fault names the path.
        let trasher = Arc::new(MockTrasher::new());
        let (home, layout, adapter, inst) = full_install("execute-changed", trasher.clone()).await;
        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");
        let elsewhere = home.executable("elsewhere/claude");
        std::fs::remove_file(&layout.launcher).unwrap();
        std::os::unix::fs::symlink(elsewhere, &layout.launcher).unwrap();

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(
            outcome,
            Outcome::BanagerFailed(Fault::PathChanged {
                path: "~/.local/bin/claude".to_string()
            })
        );
        assert!(trasher.calls().is_empty());
        assert!(layout.root.join("versions/2.1.281").is_file());
    }

    #[tokio::test]
    async fn test_execute_refuses_a_launcher_the_updater_re_pointed_after_the_preview() {
        // Ruling 10 through the adapter: what the preview saw rides in the
        // plan it issued (`PlanAction::TrashPaths.previewed`). Claude Code
        // updating itself between the preview and the click re-points the
        // launcher at a new version inside the root -- every check still
        // passes and the path is the same string -- but it is not the link
        // the user was shown: nothing moves, and the user previews again.
        let trasher = Arc::new(MockTrasher::new());
        let (home, layout, adapter, inst) =
            full_install("execute-self-updated", trasher.clone()).await;
        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");
        let newer = home.executable(".local/share/claude/versions/2.1.282");
        std::fs::remove_file(&layout.launcher).unwrap();
        std::os::unix::fs::symlink(newer, &layout.launcher).unwrap();

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(
            outcome,
            Outcome::BanagerFailed(Fault::PathChanged {
                path: "~/.local/bin/claude".to_string()
            })
        );
        assert!(trasher.calls().is_empty());
        assert!(layout.root.join("versions/2.1.281").is_file());
    }

    #[tokio::test]
    async fn test_execute_stops_at_a_refused_item_and_leaves_the_launcher() {
        // Review Focus 4, first half: macOS refused the cache directory.
        let trasher = Arc::new(MockTrasher::new());
        trasher.refuse_call(1, "Operation not permitted");
        let (_home, layout, adapter, inst) = full_install("execute-refused", trasher.clone()).await;
        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");
        let sink = Arc::new(VecSink::new());

        let outcome = adapter
            .execute(&plan, sink.clone(), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(
            outcome,
            Outcome::Failed {
                exit_code: None,
                summary: "Operation not permitted".to_string()
            }
        );
        assert_eq!(trasher.calls().len(), 2);
        assert!(std::fs::symlink_metadata(&layout.launcher)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(matches!(
            notes_of(&sink)[..],
            [LogNote::MovedToTrash { .. }, LogNote::TrashFailed { .. }]
        ));
    }

    #[tokio::test]
    async fn test_execute_stops_between_items_when_cancelled() {
        // Review Focus 4, second half: Cancel after the first item.
        let token = CancellationToken::new();
        let trasher = Arc::new(MockTrasher::new());
        trasher.cancel_after_call(0, token.clone());
        let (_home, layout, adapter, inst) = full_install("execute-cancel", trasher.clone()).await;
        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, token)
            .await
            .expect("execute");

        assert_eq!(outcome, Outcome::Unconfirmed);
        assert_eq!(trasher.calls().len(), 1);
        assert!(std::fs::symlink_metadata(&layout.launcher)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[tokio::test]
    async fn test_execute_runs_out_its_budget_between_items_not_a_process() {
        // `Plan.timeout_secs` is the removal's own clock: a spent budget
        // stops the run before the next item, as `Unconfirmed`, and the
        // log line that says so carries that same number of seconds.
        let trasher = Arc::new(MockTrasher::new());
        let (_home, _layout, adapter, inst) = full_install("execute-budget", trasher.clone()).await;
        let plan = Plan {
            timeout_secs: 0,
            ..adapter.plan(&inst, &uninstall()).await.expect("plan")
        };
        let sink = Arc::new(VecSink::new());

        let outcome = adapter
            .execute(&plan, sink.clone(), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(outcome, Outcome::Unconfirmed);
        assert!(trasher.calls().is_empty());
        assert_eq!(
            notes_of(&sink),
            vec![LogNote::OutOfTime {
                path: "~/.local/share/claude".to_string(),
                seconds: plan.timeout_secs,
            }]
        );
    }

    #[tokio::test]
    async fn test_reconcile_after_uninstall_tells_there_gone_and_cannot_tell_apart() {
        // B's deviation 15, the hand-off, and Ruling 27: after an uninstall
        // only presence is asked. The launcher-only state a stopped run
        // leaves is present -- so `run_operation` reports the stop
        // truthfully (`Cancelled` after a Cancel,
        // `StillInstalledAfterUninstall` after an exit that claimed
        // success) -- while `reconcile` keeps B's strict rule for upgrades;
        // a launcher that is gone, with every other path the plan listed,
        // is absent; and one Banager cannot look at (its folder
        // unreadable) is neither: an error, which `run_operation` reports
        // as `Unconfirmed`, never as a finished uninstall.
        let trasher = Arc::new(MockTrasher::new());
        let (home, layout, adapter, inst) = full_install("reconcile-after", trasher).await;
        let key = adapter.artifact_key(&inst);
        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");
        std::fs::remove_dir_all(&layout.root).unwrap();

        let still_there = adapter
            .reconcile_after_uninstall(&inst, &key, &plan)
            .await
            .expect("a reading");
        assert!(still_there.present);
        assert!(matches!(
            adapter.reconcile(&inst, &key).await,
            Err(AdapterError::Parse(_))
        ));

        let bin = layout.launcher.parent().unwrap().to_path_buf();
        if let Some(_locked) = super::testing::Unreadable::new(&bin) {
            assert!(matches!(
                adapter.reconcile_after_uninstall(&inst, &key, &plan).await,
                Err(AdapterError::Parse(_))
            ));
        }

        std::fs::remove_file(&layout.launcher).unwrap();
        std::fs::remove_dir(home.path().join(".claude/downloads")).unwrap();
        let gone = adapter
            .reconcile_after_uninstall(&inst, &key, &plan)
            .await
            .expect("a reading");
        assert!(!gone.present);
    }

    #[tokio::test]
    async fn test_reconcile_after_uninstall_counts_a_listed_path_that_is_there_as_still_there() {
        // The launcher is last so the row shows what a stopped run left; a
        // copy of Claude Code still running can put its program folder or
        // its cache back after the launcher's move, and the launcher alone
        // would then read as gone. So after a path-list uninstall every
        // other path on the list is looked for too (`removal::left_behind`,
        // with the plan's paths as the ones this run moved): one that is
        // there is the tool still there, and one Banager cannot look at is
        // an error, never "gone".
        let trasher = Arc::new(MockTrasher::new());
        let (home, _layout, adapter, inst) =
            full_install("reconcile-left-behind", trasher.clone()).await;
        let key = adapter.artifact_key(&inst);
        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");
        let PlanAction::TrashPaths { paths, previewed } = &plan.action else {
            panic!("a path list");
        };
        for (path, seen) in paths.iter().zip(previewed) {
            trasher.trash(path, seen.kind).expect("the mock moves it");
        }
        let reading = adapter.reconcile_after_uninstall(&inst, &key, &plan).await;
        assert!(!reading.expect("a reading").present, "everything moved");

        home.file(".local/share/claude/versions/2.1.282");
        let reading = adapter.reconcile_after_uninstall(&inst, &key, &plan).await;
        assert!(reading.expect("a reading").present, "the program folder");
        std::fs::remove_dir_all(home.path().join(".local/share/claude")).unwrap();

        let cache = home.dir(".claude/downloads");
        let reading = adapter.reconcile_after_uninstall(&inst, &key, &plan).await;
        assert!(reading.expect("a reading").present, "the cache folder");
        std::fs::remove_dir(&cache).unwrap();

        if let Some(_locked) = super::testing::Unreadable::new(&home.path().join(".claude")) {
            let reading = adapter.reconcile_after_uninstall(&inst, &key, &plan).await;
            assert!(
                matches!(reading, Err(AdapterError::Parse(_))),
                "{reading:?}"
            );
        }
        let reading = adapter.reconcile_after_uninstall(&inst, &key, &plan).await;
        assert!(
            !reading.expect("a reading").present,
            "everything gone again"
        );
    }

    #[tokio::test]
    async fn test_detect_lists_nothing_for_a_launcher_that_reaches_the_root_through_another_link() {
        // Ruling 27, at the adapter: B listed this layout (its `realpath`
        // lands in the root); step C does not, because an uninstall that
        // stopped after moving the root would leave the middle link
        // dangling and the launcher reading as not installed. No instance,
        // so no Uninstall to offer; the Unknown page lists the two links.
        let home = TempHome::new("detect-hop-outside");
        let real = home.executable(".local/share/claude/versions/2.1.281");
        let current = home.link(".local/bin/claude-current", &real);
        home.link(".local/bin/claude", &current);
        let adapter = adapter(Arc::new(MockRunner::new()));

        assert!(adapter.detect(&env_as_owner(&home)).await.is_empty());
    }

    const RELEASE_URL: &str = "https://static.rust-lang.org/rustup/release-stable.toml";
    const RUSTUP_VERSION_LINE: &str = "rustup 1.29.1 (d95a37b6a 2026-08-13)\n";

    /// `StandaloneAdapter::new` with C's fourth argument: nothing of
    /// rustup's goes through the Trash (its uninstall is a command), so a
    /// fresh `MockTrasher` stands in and is never called.
    fn rustup_adapter(
        runner: Arc<dyn CommandRunner>,
        http: Arc<MockHttpClient>,
    ) -> StandaloneAdapter {
        StandaloneAdapter::new(&RUSTUP, runner, http, Arc::new(MockTrasher::new()))
    }

    /// A rustup install under `cargo_home`, whose runner answers
    /// `--version`, detected under `env`.
    async fn detected_rustup(
        env: &HostEnv,
        cargo_home: &Path,
        http: Arc<MockHttpClient>,
    ) -> (StandaloneAdapter, ManagerInstance, Arc<MockRunner>) {
        let layout = rustup_layout(cargo_home);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0(RUSTUP_VERSION_LINE),
        );
        let adapter = rustup_adapter(runner.clone(), http);
        let inst = adapter.detect(env).await.remove(0);
        (adapter, inst, runner)
    }

    /// The preview's Homebrew line depends on the Mac running the tests
    /// (`rustup::HOMEBREW_PREFIXES` are real paths): filtered out where a
    /// test asserts the whole list. `rustup::tests` proves the line
    /// itself over a temp prefix.
    fn without_homebrew_line(warnings: Vec<Warning>) -> Vec<Warning> {
        warnings
            .into_iter()
            .filter(|w| !matches!(w, Warning::HomebrewRustupLosesToolchains))
            .collect()
    }

    /// B's `request` is fixed to `standalone-claude`; rustup's requests
    /// name its own instance.
    fn request_for(instance_id: &str, kind: OpKind, name: &str) -> OpRequest {
        OpRequest {
            kind,
            instance_id: instance_id.to_string(),
            artifact_kind: ArtifactKind::Binary,
            name: name.to_string(),
        }
    }

    #[tokio::test]
    async fn test_detect_lists_rustup_under_the_cargo_home_with_the_second_token_version() {
        let home = TempHome::new("rustup-detect");
        let cargo_home = home.path().join(".cargo");
        let (adapter, inst, _runner) = detected_rustup(
            &home.env(vec![cargo_home.join("bin")]),
            &cargo_home,
            Arc::new(MockHttpClient::new()),
        )
        .await;
        assert_eq!(inst.id, "standalone-rustup");
        assert_eq!(inst.adapter_id, "standalone-rustup");
        // The launcher is the file itself; the root is the Cargo home.
        assert_eq!(inst.exe_path, cargo_home.join("bin/rustup"));
        assert_eq!(inst.prefix, cargo_home);
        assert_eq!(inst.version, Some("1.29.1".to_string()));
        assert_eq!(
            inst.unverified_version, None,
            "1.29.1 is the verified version"
        );
        assert_eq!(inst.status.unavailable, None);
        assert!(inst.status.notes.is_empty(), "PATH finds this very file");
        assert_eq!(
            adapter
                .seated_detected_for(&inst)
                .expect("seated")
                .cargo_home,
            Some(cargo_home.clone())
        );

        let artifacts = adapter.inventory(&inst).await.expect("inventory");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].display_name, "rustup");
        assert_eq!(artifacts[0].version, "1.29.1");
        assert_eq!(artifacts[0].path, Some(cargo_home.join("bin/rustup")));
        assert!(!artifacts[0].auto_updates);
        // The standard layout (`~/.cargo`, no `~/.rustup` yet): the
        // official uninstall is offered (Q5) -- `Uninstall::Command`'s
        // `blocked` answered `None` through the seat.
        assert_eq!(artifacts[0].uninstall_blocked, None);
    }

    #[tokio::test]
    async fn test_detect_reads_rustups_version_with_auto_install_off_and_a_thirty_second_timeout() {
        // Ruling 20, end to end: the `CommandSpec` the version read hands
        // the runner carries `RUSTUP_AUTO_INSTALL=0` and nothing else,
        // the 30 s every adapter gives `--version`, and no cwd.
        let home = TempHome::new("rustup-detect-env");
        let layout = rustup_layout(&home.path().join(".cargo"));
        let runner = Arc::new(RecordingRunner {
            specs: StdMutex::new(Vec::new()),
            output: exited_0(RUSTUP_VERSION_LINE),
        });
        let adapter = rustup_adapter(runner.clone(), Arc::new(MockHttpClient::new()));
        adapter.detect(&home.env(vec![])).await;
        let specs = runner.specs.lock().unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].program, layout.launcher);
        assert_eq!(specs[0].args, vec!["--version".to_string()]);
        assert_eq!(
            specs[0].env,
            vec![("RUSTUP_AUTO_INSTALL".to_string(), "0".to_string())]
        );
        assert_eq!(specs[0].timeout, Duration::from_secs(30));
        assert_eq!(specs[0].output_use, OutputUse::Parsed);
        assert_eq!(specs[0].cwd, None);
    }

    #[tokio::test]
    async fn test_detect_reads_the_version_of_a_rustup_with_no_active_toolchain_without_running_anything_else(
    ) {
        // With `RUSTUP_AUTO_INSTALL=0` and no toolchain active, 1.29.1's
        // `display_version` (rustup_mode.rs:1819-1837) takes the
        // `active_toolchain()` path, prints its version line on stdout as
        // ever, says `info: no rustc is currently active` on stderr and
        // exits 0 -- quoted from the source, not recorded: recording it
        // would need a Mac with no toolchain, and installing or removing
        // one is out of bounds. Mocked, so nothing real runs: the version
        // is read, the row is not "not responding", and no second command
        // was spawned.
        let home = TempHome::new("rustup-detect-no-toolchain");
        let layout = rustup_layout(&home.path().join(".cargo"));
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: RUSTUP_VERSION_LINE.to_string(),
                stderr: "info: This is the version for the rustup toolchain manager, not the rustc compiler.\ninfo: no `rustc` is currently active\n".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = rustup_adapter(runner.clone(), Arc::new(MockHttpClient::new()));
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        assert_eq!(inst.version, Some("1.29.1".to_string()));
        assert_eq!(inst.status.unavailable, None);
        assert_eq!(runner.calls().len(), 1, "one command, the version read");
    }

    #[tokio::test]
    async fn test_detect_follows_cargo_home_for_rustup() {
        // `CARGO_HOME=/elsewhere/cargo`: the launcher is looked for there,
        // never under ~/.cargo.
        let home = TempHome::new("rustup-detect-custom");
        let custom = home.path().join("elsewhere/cargo");
        let env = HostEnv {
            cargo_home: Some(custom.clone()),
            ..home.env(vec![custom.join("bin")])
        };
        let (_adapter, inst, _runner) =
            detected_rustup(&env, &custom, Arc::new(MockHttpClient::new())).await;
        assert_eq!(inst.exe_path, custom.join("bin/rustup"));
        assert_eq!(inst.prefix, custom);
        // With rustup under ~/.cargo but CARGO_HOME pointing elsewhere: no
        // instance -- that rustup is not where rustup itself would look.
        let home = TempHome::new("rustup-detect-mismatch");
        rustup_layout(&home.path().join(".cargo"));
        let env = HostEnv {
            cargo_home: Some(home.path().join("elsewhere/cargo")),
            ..home.env(vec![])
        };
        assert!(
            rustup_adapter(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()))
                .detect(&env)
                .await
                .is_empty()
        );
        // An empty CARGO_HOME is the default (the `home` crate's rule);
        // a relative one is unsupported and finds nothing, running
        // nothing.
        let home = TempHome::new("rustup-detect-empty-and-relative");
        let layout = rustup_layout(&home.path().join(".cargo"));
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0(RUSTUP_VERSION_LINE),
        );
        let adapter = rustup_adapter(runner.clone(), Arc::new(MockHttpClient::new()));
        let env = HostEnv {
            cargo_home: Some(PathBuf::from("")),
            ..home.env(vec![])
        };
        assert_eq!(adapter.detect(&env).await.len(), 1);
        let env = HostEnv {
            cargo_home: Some(PathBuf::from(".cargo")),
            ..home.env(vec![])
        };
        let before = runner.calls().len();
        assert!(adapter.detect(&env).await.is_empty());
        assert_eq!(
            runner.calls().len(),
            before,
            "nothing run for a home Banager cannot name"
        );
    }

    #[tokio::test]
    async fn test_check_updates_reads_the_release_file_and_lists_only_a_newer_rustup() {
        for (body, expected) in [
            ("schema-version = '1'\nversion = '1.30.0'\n", 1),
            ("schema-version = '1'\nversion = '1.29.1'\n", 0),
            ("schema-version = '1'\nversion = '1.28.2'\n", 0),
        ] {
            let home = TempHome::new("rustup-check");
            let cargo_home = home.path().join(".cargo");
            let http = Arc::new(MockHttpClient::new());
            http.respond(RELEASE_URL, answer(body));
            let (adapter, inst, _runner) =
                detected_rustup(&home.env(vec![]), &cargo_home, http.clone()).await;
            // `check_updates` compares the version `inventory` read, and
            // refuses without one (B's follow-up fix, `Reading`): the
            // order `refresh_round` keeps.
            adapter.inventory(&inst).await.expect("inventory");
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("check_updates");
            assert_eq!(out.candidates.len(), expected, "{body:?}");
            assert_eq!(http.calls(), vec![RELEASE_URL.to_string()]);
            if expected == 1 {
                let c = &out.candidates[0];
                assert_eq!(c.key.name, "rustup");
                assert_eq!(c.current, "1.29.1");
                assert_eq!(c.target, "1.30.0");
                assert_eq!(c.channel, UpdateChannel::Registry);
                assert!(c.checkable);
                assert!(c.warnings.is_empty());
                assert_eq!(c.blocked, None);
                let request = &http.requests()[0];
                assert_eq!(request.method, "GET");
                assert!(request.headers.is_empty());
                assert_eq!(request.timeout, Duration::from_secs(30));
            }
        }
    }

    #[tokio::test]
    async fn test_check_updates_marks_a_bad_release_file_uncheckable() {
        // A failed request, a non-200, HTML with status 200, a file with
        // no version: one could-not-check row each, never an `Err`.
        let cases: Vec<(Option<HttpResponse>, &str)> = vec![
            (None, "request to"),
            (
                Some(HttpResponse {
                    status: 503,
                    body: String::new(),
                }),
                "returned status 503",
            ),
            (Some(answer("<html>Sign in</html>")), "release file"),
            (Some(answer("schema-version = '1'\n")), "release file"),
        ];
        for (response, reason) in cases {
            let home = TempHome::new("rustup-check-bad");
            let cargo_home = home.path().join(".cargo");
            let http = Arc::new(MockHttpClient::new());
            match response {
                Some(r) => http.respond(RELEASE_URL, r),
                None => http.fail(RELEASE_URL, "connection refused"),
            }
            let (adapter, inst, _runner) =
                detected_rustup(&home.env(vec![]), &cargo_home, http).await;
            adapter.inventory(&inst).await.expect("inventory");
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("a failed lookup is not a source failure");
            assert_eq!(out.candidates.len(), 1, "{reason}");
            let c = &out.candidates[0];
            assert!(!c.checkable);
            assert_eq!(c.current, "1.29.1");
            assert_eq!(c.target, "1.29.1");
            assert!(
                matches!(&c.warnings[..], [Warning::Message(m)] if m.contains(reason) && m.len() < 200),
                "{reason}: {:?}",
                c.warnings
            );
        }
    }

    #[tokio::test]
    async fn test_plan_upgrade_for_rustup_is_self_update_no_cancel_with_the_cargo_lock() {
        let home = TempHome::new("rustup-plan-upgrade");
        let cargo_home = home.path().join(".cargo");
        let (adapter, inst, _runner) = detected_rustup(
            &home.env(vec![]),
            &cargo_home,
            Arc::new(MockHttpClient::new()),
        )
        .await;
        let plan = adapter
            .plan(
                &inst,
                &request_for("standalone-rustup", OpKind::Upgrade, "rustup"),
            )
            .await
            .expect("plan");
        assert_eq!(
            plan.action,
            PlanAction::Command {
                program: cargo_home.join("bin/rustup"),
                args: vec!["self".to_string(), "update".to_string()],
                env: Vec::new(),
            }
        );
        assert!(!plan.needs_password);
        assert_eq!(
            plan.locks,
            vec![
                ResourceLock("standalone-rustup".to_string()),
                ResourceLock(crate::adapters::cargo::instance_id_for(&cargo_home)),
            ]
        );
        assert_eq!(plan.cancel_policy, CancelPolicy::NoCancel);
        assert!(plan.warnings.is_empty());
        assert!(plan.affected.is_empty());
        assert_eq!(plan.timeout_secs, 600);
    }

    #[tokio::test]
    async fn test_plan_uninstall_for_rustup_runs_nothing_and_lists_the_warnings() {
        // This Mac's layout (spec §6.6): one toolchain, hexyl installed
        // with cargo, rustup's line in ~/.zshenv and ~/.profile, the same
        // line by hand in ~/.zshrc. The preview reads the disk and runs
        // no command (ruling 4).
        let home = TempHome::new("rustup-plan-uninstall");
        let cargo_home = home.path().join(".cargo");
        let (adapter, inst, runner) = detected_rustup(
            &home.env(vec![]),
            &cargo_home,
            Arc::new(MockHttpClient::new()),
        )
        .await;
        std::fs::write(cargo_home.join("bin/hexyl"), b"x").expect("write hexyl");
        std::fs::copy(
            "../../adapters/fixtures/cargo/1.98.1/crates2.json",
            cargo_home.join(".crates2.json"),
        )
        .expect("copy the recorded record");
        home.dir(".rustup/toolchains/stable-aarch64-apple-darwin");
        for rc in [".zshenv", ".profile", ".zshrc"] {
            std::fs::write(home.path().join(rc), ". \"$HOME/.cargo/env\"\n").expect("write rc");
        }
        let calls_before = runner.calls().len();

        let plan = adapter
            .plan(
                &inst,
                &request_for("standalone-rustup", OpKind::Uninstall, "rustup"),
            )
            .await
            .expect("plan");

        assert_eq!(
            plan.action,
            PlanAction::Command {
                program: cargo_home.join("bin/rustup"),
                args: vec![
                    "self".to_string(),
                    "uninstall".to_string(),
                    "-y".to_string()
                ],
                env: Vec::new(),
            }
        );
        assert_eq!(plan.cancel_policy, CancelPolicy::NoCancel);
        assert_eq!(plan.timeout_secs, 600);
        assert!(!plan.needs_password);
        assert!(
            plan.affected.is_empty(),
            "a non-empty list disables Confirm; hexyl does not break"
        );
        assert_eq!(
            plan.locks,
            vec![
                ResourceLock("standalone-rustup".to_string()),
                ResourceLock(crate::adapters::cargo::instance_id_for(&cargo_home)),
            ]
        );
        assert_eq!(
            without_homebrew_line(plan.warnings),
            vec![
                Warning::RemovesToolchains {
                    path: "~/.rustup".to_string(),
                    names: vec!["stable-aarch64-apple-darwin".to_string()]
                },
                Warning::DeletesCargoHome {
                    path: "~/.cargo".to_string()
                },
                Warning::RemovesCargoInstalled {
                    names: vec!["hexyl".to_string()]
                },
                Warning::EditsShellConfig,
                Warning::LeavesShellConfigLine {
                    path: "~/.zshrc".to_string(),
                    certain: true
                },
            ]
        );
        assert_eq!(
            runner.calls().len(),
            calls_before,
            "the preview ran no command"
        );
    }

    #[tokio::test]
    async fn test_plan_uninstall_for_rustup_survives_an_empty_cargo_home_and_no_toolchains() {
        // No `.crates2.json`, no `~/.rustup`, no startup files: the plan
        // still builds, with the toolchain sentence unnamed and nothing
        // invented.
        let home = TempHome::new("rustup-plan-uninstall-bare");
        let cargo_home = home.path().join(".cargo");
        let (adapter, inst, _runner) = detected_rustup(
            &home.env(vec![]),
            &cargo_home,
            Arc::new(MockHttpClient::new()),
        )
        .await;
        let plan = adapter
            .plan(
                &inst,
                &request_for("standalone-rustup", OpKind::Uninstall, "rustup"),
            )
            .await
            .expect("plan");
        assert_eq!(
            without_homebrew_line(plan.warnings),
            vec![
                Warning::RemovesToolchains {
                    path: "~/.rustup".to_string(),
                    names: Vec::new()
                },
                Warning::DeletesCargoHome {
                    path: "~/.cargo".to_string()
                },
                Warning::EditsShellConfig,
            ]
        );
    }

    /// `RUSTUP` with a gate that changes the layout the moment it passes:
    /// the disk changing between one reading and the next, made
    /// deterministic. Every other field is `RUSTUP`'s.
    static RUSTUP_GATE_THEN_LINK: Recipe = Recipe {
        id: "rustup",
        meta_toml: include_str!("../../../../../adapters/meta/standalone-rustup.toml"),
        route: Route {
            kind: RouteKind::FlatFile,
            launcher: "$CARGO_HOME/bin/rustup",
            root: "$CARGO_HOME",
        },
        version: VersionCmd {
            args: &["--version"],
            env: &[crate::adapters::cargo::RUSTUP_AUTO_INSTALL_OFF],
            parse: VersionParse::SecondToken,
        },
        latest: Latest::HttpTomlVersion { url: RELEASE_URL },
        self_updates: false,
        upgrade: Some(UpgradeCmd {
            args: &["self", "update"],
            timeout_secs: 600,
            cancel: CancelPolicy::NoCancel,
        }),
        uninstall: Some(Uninstall::Command(CommandUninstall {
            args: &["self", "uninstall", "-y"],
            timeout_secs: 600,
            cancel: CancelPolicy::NoCancel,
            blocked: gate_then_link,
            preview: rustup::uninstall_preview,
        })),
        extra_locks: rustup::extra_locks,
        backup_globs: &[],
        other_commands: &rustup::RUSTUP_PROXIES,
    };

    /// rustup's own gate and, when it passes, the layout change: the
    /// seat's `~/.rustup` replaced by a link to `<home>/Volumes/Data/rustup`,
    /// which the gate refuses on its next reading. Only ever handed a
    /// `TempHome`'s seat.
    fn gate_then_link(d: &Detected) -> Option<GateRefusal> {
        let refusal = rustup::uninstall_blocked(d);
        if refusal.is_none() {
            let rustup_home = d.home.join(".rustup");
            let elsewhere = d.home.join("Volumes/Data/rustup");
            std::fs::create_dir_all(&elsewhere).expect("create the other folder");
            std::fs::remove_dir_all(&rustup_home).expect("remove the real rustup home");
            std::os::unix::fs::symlink(&elsewhere, &rustup_home).expect("link the rustup home");
        }
        refusal
    }

    #[tokio::test]
    async fn test_plan_uninstall_for_rustup_answers_its_gate_and_its_warnings_from_one_reading() {
        // Step E's whole-step review: `plan` asked the recipe's gate and
        // then its warnings, two readings of the disk, and a layout that
        // changed between them -- passing the first, refused by the
        // second -- came out as a plan for `rustup self uninstall -y`
        // with no warnings at all. `RUSTUP_GATE_THEN_LINK` makes that
        // change happen the moment its gate passes. The plan asks one
        // function (`CommandUninstall.preview`) once: the layout it
        // describes is the one it let through, so it names both folders;
        // and the gate this recipe hooks is not what `plan` reads, so the
        // link is not made by it.
        let home = TempHome::new("rustup-plan-one-reading");
        let cargo_home = home.path().join(".cargo");
        let layout = rustup_layout(&cargo_home);
        home.dir(".rustup/toolchains/stable-aarch64-apple-darwin");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0(RUSTUP_VERSION_LINE),
        );
        let adapter = StandaloneAdapter::new(
            &RUSTUP_GATE_THEN_LINK,
            runner,
            Arc::new(MockHttpClient::new()),
            Arc::new(MockTrasher::new()),
        );
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        let request = request_for("standalone-rustup", OpKind::Uninstall, "rustup");
        let is_real_dir = || {
            std::fs::symlink_metadata(home.path().join(".rustup"))
                .expect("lstat the rustup home")
                .file_type()
                .is_dir()
        };

        let plan = adapter
            .plan(&inst, &request)
            .await
            .expect("the standard layout is offered");
        assert!(
            plan.warnings.contains(&Warning::RemovesToolchains {
                path: "~/.rustup".to_string(),
                names: vec!["stable-aarch64-apple-darwin".to_string()],
            }) && plan.warnings.contains(&Warning::DeletesCargoHome {
                path: "~/.cargo".to_string(),
            }),
            "a plan the gate let through names both folders; got {:?}",
            plan.warnings
        );
        assert!(
            is_real_dir(),
            "`plan` reads the preview alone, never this recipe's `blocked`"
        );

        // The hooked gate does fire where `blocked` is read, `inventory`
        // (and `execute`), and once the layout has changed the plan's one
        // reading refuses: a changed layout is a refusal, never a plan
        // with nothing to say.
        let artifacts = adapter.inventory(&inst).await.expect("inventory");
        assert_eq!(
            artifacts[0].uninstall_blocked, None,
            "the gate passed before it changed the layout"
        );
        assert!(!is_real_dir(), "the hooked gate made the link");
        assert!(matches!(
            adapter.plan(&inst, &request).await,
            Err(AdapterError::UninstallBlocked {
                reason: UninstallBlocked::NoSafeMethod
            })
        ));
    }

    #[tokio::test]
    async fn test_inventory_and_plan_refuse_the_uninstall_for_a_non_standard_layout() {
        // Ruling 18, through the adapter: a custom CARGO_HOME, a custom
        // RUSTUP_HOME, and a linked root each make the artifact carry
        // `NoSafeMethod` (so the gate in session/plans.rs refuses and the
        // page hides the button) and make `plan(Uninstall)` refuse with
        // the same reason; the upgrade is not gated, and its cargo lock
        // names the custom home.
        let home = TempHome::new("rustup-gate-custom-cargo");
        let custom = home.path().join("elsewhere/cargo");
        let env = HostEnv {
            cargo_home: Some(custom.clone()),
            ..home.env(vec![])
        };
        let (adapter, inst, _runner) =
            detected_rustup(&env, &custom, Arc::new(MockHttpClient::new())).await;
        let artifacts = adapter.inventory(&inst).await.expect("inventory");
        assert_eq!(
            artifacts[0].uninstall_blocked,
            Some(UninstallBlocked::NoSafeMethod)
        );
        assert!(matches!(
            adapter
                .plan(
                    &inst,
                    &request_for("standalone-rustup", OpKind::Uninstall, "rustup")
                )
                .await,
            Err(AdapterError::UninstallBlocked {
                reason: UninstallBlocked::NoSafeMethod
            })
        ));
        let upgrade = adapter
            .plan(
                &inst,
                &request_for("standalone-rustup", OpKind::Upgrade, "rustup"),
            )
            .await
            .expect("the upgrade is not gated");
        assert!(upgrade
            .locks
            .contains(&ResourceLock(crate::adapters::cargo::instance_id_for(
                &custom
            ))));

        let home = TempHome::new("rustup-gate-custom-rustup");
        let cargo_home = home.path().join(".cargo");
        let env = HostEnv {
            rustup_home: Some(home.path().join("elsewhere/rustup")),
            ..home.env(vec![])
        };
        let (adapter, inst, _runner) =
            detected_rustup(&env, &cargo_home, Arc::new(MockHttpClient::new())).await;
        assert_eq!(
            adapter.inventory(&inst).await.expect("inventory")[0].uninstall_blocked,
            Some(UninstallBlocked::NoSafeMethod)
        );

        let home = TempHome::new("rustup-gate-linked-rustup");
        let cargo_home = home.path().join(".cargo");
        let elsewhere = home.dir("Volumes/Data/rustup");
        home.link(".rustup", &elsewhere);
        let (adapter, inst, _runner) = detected_rustup(
            &home.env(vec![]),
            &cargo_home,
            Arc::new(MockHttpClient::new()),
        )
        .await;
        assert_eq!(
            adapter.inventory(&inst).await.expect("inventory")[0].uninstall_blocked,
            Some(UninstallBlocked::NoSafeMethod)
        );
        assert!(matches!(
            adapter
                .plan(
                    &inst,
                    &request_for("standalone-rustup", OpKind::Uninstall, "rustup")
                )
                .await,
            Err(AdapterError::UninstallBlocked { .. })
        ));
    }

    #[tokio::test]
    async fn test_rustup_locks_name_the_cargo_instance_detect_produces_with_and_without_cargo_home()
    {
        // Spec §2.4, §十三 #42: `acquire_resource_lock` compares names byte
        // for byte and reports nothing for two that merely look alike, so
        // the lock rustup's plans hold must equal the id `CargoAdapter::
        // detect` gives its instance on the same host -- with CARGO_HOME
        // unset and set. Both go through `cargo::instance_id_for` and
        // `cargo::cargo_home_of`; this proves it end to end. With it set,
        // only the upgrade has a plan (the uninstall is gated).
        for custom in [false, true] {
            let home = TempHome::new("rustup-cross-lock");
            let cargo_home = if custom {
                home.path().join("elsewhere/cargo")
            } else {
                home.path().join(".cargo")
            };
            let env = HostEnv {
                cargo_home: custom.then(|| cargo_home.clone()),
                ..home.env(vec![cargo_home.join("bin")])
            };
            let (rustup, rustup_inst, runner) =
                detected_rustup(&env, &cargo_home, Arc::new(MockHttpClient::new())).await;
            // The cargo instance, from the real cargo adapter over the same
            // env: its `cargo` is the proxy link `rustup_layout` wrote.
            runner.respond(
                vec![cargo_home.join("bin/cargo").to_str().unwrap(), "--version"],
                exited_0("cargo 1.98.1 (797e8a9bc 2026-08-05)\n"),
            );
            let cargo = CargoAdapter::new(runner.clone(), Arc::new(MockHttpClient::new()));
            let cargo_inst = cargo.detect(&env).await.remove(0);
            assert_eq!(cargo_inst.prefix, cargo_home, "custom={custom}");

            let kinds: &[OpKind] = if custom {
                &[OpKind::Upgrade]
            } else {
                &[OpKind::Upgrade, OpKind::Uninstall]
            };
            for kind in kinds {
                let plan = rustup
                    .plan(
                        &rustup_inst,
                        &request_for("standalone-rustup", *kind, "rustup"),
                    )
                    .await
                    .expect("plan");
                assert!(
                    plan.locks.contains(&ResourceLock(cargo_inst.id.clone())),
                    "custom={custom} {kind:?}: {:?} lacks {}",
                    plan.locks,
                    cargo_inst.id
                );
                assert_eq!(plan.locks.len(), 2, "custom={custom} {kind:?}");
            }
        }
    }

    #[tokio::test]
    async fn test_execute_runs_rustups_uninstall_through_run_plan() {
        let home = TempHome::new("rustup-execute");
        let cargo_home = home.path().join(".cargo");
        let (adapter, inst, runner) = detected_rustup(
            &home.env(vec![]),
            &cargo_home,
            Arc::new(MockHttpClient::new()),
        )
        .await;
        runner.respond(
            vec![
                cargo_home.join("bin/rustup").to_str().unwrap(),
                "self",
                "uninstall",
                "-y",
            ],
            exited_0("info: removing toolchains\ninfo: rustup is uninstalled\n"),
        );
        let plan = adapter
            .plan(
                &inst,
                &request_for("standalone-rustup", OpKind::Uninstall, "rustup"),
            )
            .await
            .expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome = adapter
            .execute(&plan, sink.clone(), 1, CancellationToken::new())
            .await
            .expect("execute");
        // `execute` reports the command's own exit; whether rustup is
        // gone is `run_operation`'s reading afterwards (Task 8).
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(sink.snapshot().len(), 2, "two log lines, streamed");
    }

    #[tokio::test]
    async fn test_execute_refuses_rustups_uninstall_when_a_root_became_a_link_after_the_preview() {
        // The gate (ruling 18) passed at the preview: `~/.cargo` and
        // `~/.rustup` were real directories at the standard place. Between
        // the preview and the click either can become a link to somewhere
        // else -- by hand, or by another tool -- and rustup's `uninstall()`
        // deletes wherever `RUSTUP_HOME` and `CARGO_HOME` resolve when it
        // runs (self_update.rs:955-966, :1029), permanently: a place the
        // confirmed preview never named. So the gate is asked again right
        // before the spawn, as the launcher is, and a layout that is no
        // longer standard is `PathChanged` naming the folder; nothing runs.
        // Both folders, one at a time: for `~/.cargo` the launcher's probe
        // alone would still say `Present` (a regular file at the end of the
        // link), so that case is the gate's own catch.
        for linked in [".rustup", ".cargo"] {
            let home = TempHome::new("rustup-execute-linked-root");
            let cargo_home = home.path().join(".cargo");
            let (adapter, inst, runner) = detected_rustup(
                &home.env(vec![]),
                &cargo_home,
                Arc::new(MockHttpClient::new()),
            )
            .await;
            home.dir(".rustup/toolchains/stable-aarch64-apple-darwin");
            runner.respond(
                vec![
                    cargo_home.join("bin/rustup").to_str().unwrap(),
                    "self",
                    "uninstall",
                    "-y",
                ],
                exited_0("info: rustup is uninstalled\n"),
            );
            let plan = adapter
                .plan(
                    &inst,
                    &request_for("standalone-rustup", OpKind::Uninstall, "rustup"),
                )
                .await
                .expect("the layout is standard at the preview");
            let calls_before = runner.calls().len();

            // The folder moves out and a link of its name points at it:
            // the same path, the same contents, not the folder the
            // preview showed.
            let elsewhere = home.path().join("Volumes/Data").join(&linked[1..]);
            std::fs::create_dir_all(elsewhere.parent().unwrap()).expect("create the volume");
            std::fs::rename(home.path().join(linked), &elsewhere).expect("move the folder");
            home.link(linked, &elsewhere);

            let outcome = adapter
                .execute(
                    &plan,
                    Arc::new(VecSink::new()),
                    11,
                    CancellationToken::new(),
                )
                .await
                .expect("execute");

            assert_eq!(
                outcome,
                Outcome::BanagerFailed(Fault::PathChanged {
                    path: format!("~/{linked}")
                }),
                "{linked}"
            );
            assert_eq!(
                runner.calls().len(),
                calls_before,
                "{linked}: nothing was run"
            );
            let kept = match linked {
                ".rustup" => "toolchains/stable-aarch64-apple-darwin",
                _ => "bin/rustup",
            };
            assert!(elsewhere.join(kept).exists(), "{linked}: left as it is");
        }
    }

    #[tokio::test]
    async fn test_execute_refuses_rustups_uninstall_when_bin_toolchains_or_update_hashes_became_a_link_after_the_preview(
    ) {
        // Step E's whole-step review: rustup 1.29.1's `uninstall()` reaches
        // `bin/<name>`, `toolchains/<name>` and `update-hashes/<name>`
        // through their parent and follows a link at the parent's name
        // (`rustup::standard_roots`' doc has the source lines), so a link
        // at one of those three would have it delete inside wherever that
        // link leads. The gate refuses any link at the top of either root.
        // Here the preview saw real folders, and then one of the three
        // became a link to the same contents elsewhere: `execute` asks the
        // gate again right before the spawn and stops with `PathChanged`
        // naming the link, rustup is never run, and what the link leads to
        // is left as it was. The launcher's own look still passes in all
        // three -- for `bin` the flat-file probe follows the linked folder
        // to a regular `rustup` -- so the refusal is the gate's alone. A
        // preview asked for after the change refuses as well.
        for container in [".cargo/bin", ".rustup/toolchains", ".rustup/update-hashes"] {
            let home = TempHome::new("rustup-execute-linked-container");
            let cargo_home = home.path().join(".cargo");
            let (adapter, inst, runner) = detected_rustup(
                &home.env(vec![]),
                &cargo_home,
                Arc::new(MockHttpClient::new()),
            )
            .await;
            home.file(".rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc");
            home.file(".rustup/update-hashes/stable-aarch64-apple-darwin");
            let launcher = cargo_home.join("bin/rustup");
            runner.respond(
                vec![launcher.to_str().unwrap(), "self", "uninstall", "-y"],
                exited_0("info: rustup is uninstalled\n"),
            );
            let request = request_for("standalone-rustup", OpKind::Uninstall, "rustup");
            let plan = adapter
                .plan(&inst, &request)
                .await
                .expect("the layout is standard at the preview");
            let calls_before = runner.calls().len();

            // The folder moves out and a link of its name points at it:
            // the same path, the same contents, not the folder the
            // preview showed.
            let name = Path::new(container).file_name().expect("a folder name");
            let elsewhere = home.path().join("Volumes/Data").join(name);
            std::fs::create_dir_all(elsewhere.parent().unwrap()).expect("create the volume");
            std::fs::rename(home.path().join(container), &elsewhere).expect("move the folder");
            home.link(container, &elsewhere);
            assert!(
                matches!(
                    route::probe(RouteKind::FlatFile, &launcher, &cargo_home),
                    Probe::Present { .. }
                ),
                "{container}: the launcher's look alone would let the command run"
            );

            let outcome = adapter
                .execute(
                    &plan,
                    Arc::new(VecSink::new()),
                    12,
                    CancellationToken::new(),
                )
                .await
                .expect("execute");

            assert_eq!(
                outcome,
                Outcome::BanagerFailed(Fault::PathChanged {
                    path: format!("~/{container}")
                }),
                "{container}"
            );
            assert_eq!(
                runner.calls().len(),
                calls_before,
                "{container}: nothing was run"
            );
            let kept = match container {
                ".cargo/bin" => "rustup",
                ".rustup/toolchains" => "stable-aarch64-apple-darwin/bin/rustc",
                _ => "stable-aarch64-apple-darwin",
            };
            assert!(elsewhere.join(kept).exists(), "{container}: left as it is");
            assert!(
                matches!(
                    adapter.plan(&inst, &request).await,
                    Err(AdapterError::UninstallBlocked {
                        reason: UninstallBlocked::NoSafeMethod
                    })
                ),
                "{container}: a new preview refuses"
            );
        }
    }

    /// The recorded fixture directory for the version the rustup meta
    /// file verifies: `adapters/fixtures/standalone-rustup/<verified>/`.
    fn rustup_fixture(name: &str) -> String {
        let adapter = rustup_adapter(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let version = adapter
            .meta
            .verified_versions
            .first()
            .expect("meta lists the recorded version")
            .clone();
        let path = format!("../../adapters/fixtures/standalone-rustup/{version}/{name}");
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
    }

    #[test]
    fn test_the_recorded_rustup_version_line_parses_and_its_stderr_holds_no_version() {
        let verified = rustup_adapter(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()))
            .meta
            .verified_versions[0]
            .clone();
        assert_eq!(
            latest::parse_version(&rustup_fixture("version.txt"), RUSTUP.version.parse),
            Some(verified.clone())
        );
        // The two `info:` lines. `parse_version` keeps whatever token it
        // finds (only an absent one is `None`), so fed these by mistake
        // it would answer a word -- never the installed version, and not
        // a dotted version at all -- which is why the version read takes
        // stdout only.
        let stderr = rustup_fixture("version-stderr.txt");
        assert!(stderr.starts_with("info:"), "{stderr:?}");
        let misread = latest::parse_version(&stderr, RUSTUP.version.parse);
        assert_ne!(misread, Some(verified));
        assert!(
            misread
                .as_deref()
                .is_none_or(|token| !latest::is_dotted_version(token)),
            "{misread:?}"
        );
    }

    /// `layout.txt` is corroboration, not parser input, and the one
    /// recorded file in its directory whose text was edited after
    /// recording: its README says the owner column's account name became
    /// `user`. `ls -la` of a directory prints names relative to it and
    /// the proxies' link text is relative (`cargo -> rustup`), so no
    /// absolute home directory was there to substitute; this checks that
    /// none is in that file or in the README beside it, that neither
    /// names a `.local` host, and that the layout still shows what the
    /// route relies on: `rustup` a regular file, and every one of
    /// `RUSTUP_PROXIES` a link to it.
    #[test]
    fn test_the_recorded_rustup_layout_and_its_readme_name_no_home_directory_or_host() {
        let layout = rustup_fixture("layout.txt");
        let readme = rustup_fixture("README.md");
        for (name, text) in [("layout.txt", &layout), ("README.md", &readme)] {
            for prefix in ["/Users/", "/home/"] {
                assert!(
                    !text.contains(prefix),
                    "{name} names an absolute home directory under {prefix}"
                );
            }
            // `X.local` is a Mac's Bonjour host name when `X` ends in a
            // letter, digit or hyphen; `~/.local/...` is preceded by `/`
            // and is not one.
            let names_a_host = text.match_indices(".local").any(|(at, _)| {
                text[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '-')
            });
            assert!(!names_a_host, "{name} names a .local host");
        }
        let links_to_rustup: Vec<&str> = layout
            .lines()
            .filter(|line| line.starts_with('l') && line.ends_with(" -> rustup"))
            .collect();
        assert_eq!(
            links_to_rustup.len(),
            rustup::RUSTUP_PROXIES.len(),
            "the proxies, and nothing else, link to rustup: {links_to_rustup:?}"
        );
        for proxy in rustup::RUSTUP_PROXIES {
            assert!(
                links_to_rustup
                    .iter()
                    .any(|line| line.ends_with(&format!(" {proxy} -> rustup"))),
                "{proxy} is a relative link to rustup"
            );
        }
        assert!(
            layout
                .lines()
                .any(|line| line.starts_with("-rwx") && line.ends_with(" rustup")),
            "rustup is an executable regular file"
        );
    }

    #[test]
    fn test_the_recorded_toolchain_names_list_as_the_preview_lists_them() {
        // The recording is `ls -1 ~/.rustup/toolchains`; the preview
        // reads the same directory (`rustup::toolchain_names`). A temp
        // `toolchains/` with the recorded names lists them back sorted.
        let recorded: Vec<String> = rustup_fixture("toolchains.txt")
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
        assert!(!recorded.is_empty());
        assert!(
            recorded.iter().all(|n| n.contains("apple-darwin")),
            "recorded on a Mac: {recorded:?}"
        );
        let home = TempHome::new("rustup-toolchains-recorded");
        for name in &recorded {
            home.dir(&format!(".rustup/toolchains/{name}"));
        }
        let mut expected = recorded.clone();
        expected.sort();
        assert_eq!(
            rustup::toolchain_names(&home.path().join(".rustup")),
            expected
        );
    }

    #[tokio::test]
    async fn test_check_updates_over_the_recorded_release_file_lists_only_a_real_update() {
        // The runner answers `--version` with the recorded line and the
        // endpoint with the recorded release file: a candidate exactly
        // when the published version is greater than the installed one,
        // both derived from the recording, so a re-recording on a later
        // day stays honest.
        let verified = rustup_adapter(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()))
            .meta
            .verified_versions[0]
            .clone();
        let version_line = rustup_fixture("version.txt");
        let installed =
            latest::parse_version(&version_line, RUSTUP.version.parse).expect("version line");
        assert_eq!(installed, verified, "the meta names the recorded version");
        let body = rustup_fixture("release-stable.toml");
        let published = latest::parse_release_stable_toml(&body).expect("release file");
        let home = TempHome::new("rustup-check-recorded");
        let cargo_home = home.path().join(".cargo");
        let layout = rustup_layout(&cargo_home);
        let http = Arc::new(MockHttpClient::new());
        http.respond(RELEASE_URL, answer(&body));
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0(&version_line),
        );
        let adapter = rustup_adapter(runner, http.clone());
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        // `check_updates` compares the version `inventory` read, and
        // refuses without one: the order `refresh_round` keeps.
        adapter.inventory(&inst).await.expect("inventory");
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert_eq!(http.calls(), vec![RELEASE_URL.to_string()]);
        match latest::compare_dotted(&installed, &published) {
            Some(Ordering::Less) => {
                assert_eq!(out.candidates.len(), 1, "{installed} < {published}");
                assert_eq!(out.candidates[0].target, published);
                assert!(out.candidates[0].checkable);
            }
            Some(Ordering::Equal | Ordering::Greater) => {
                assert!(out.candidates.is_empty(), "{installed} >= {published}");
            }
            None => panic!("both are dotted versions: {installed} vs {published}"),
        }
    }

    // ---- Antigravity CLI ----

    const AGY_MANIFEST_URL: &str =
        "https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/darwin_arm64.json";

    fn agy_adapter(runner: Arc<dyn CommandRunner>, http: Arc<MockHttpClient>) -> StandaloneAdapter {
        StandaloneAdapter::new(&AGY, runner, http, Arc::new(MockTrasher::new()))
            .with_trash_gap(Duration::ZERO)
            .with_arch("aarch64")
    }

    fn agy_request(kind: OpKind) -> OpRequest {
        request_for("standalone-agy", kind, "agy")
    }

    /// A detected agy over `home`, `--version` answering `version`.
    async fn detected_agy(
        home: &TempHome,
        version: &str,
        http: Arc<MockHttpClient>,
    ) -> (
        StandaloneAdapter,
        ManagerInstance,
        super::testing::AgyLayout,
    ) {
        let layout = agy_layout(home);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0(&format!("{version}\n")),
        );
        let adapter = agy_adapter(runner, http);
        let inst = adapter.detect(&env_as_owner(home)).await.remove(0);
        (adapter, inst, layout)
    }

    #[tokio::test]
    async fn test_detect_lists_agy_as_a_flat_file_read_with_its_auto_update_off() {
        // Spec §3.3 (FlatFile: a regular file, its real path itself), §3.4
        // (the documented switch on every version read), §2.2 (exe_path is
        // the file, prefix the root).
        let home = TempHome::new("agy-detect");
        let layout = agy_layout(&home);
        let runner = Arc::new(RecordingRunner {
            specs: StdMutex::new(Vec::new()),
            output: exited_0("1.2.10\n"),
        });
        let adapter = agy_adapter(runner.clone(), Arc::new(MockHttpClient::new()));

        let instances = adapter.detect(&env_as_owner(&home)).await;

        assert_eq!(instances.len(), 1);
        let inst = &instances[0];
        assert_eq!(inst.id, "standalone-agy");
        assert_eq!(inst.adapter_id, "standalone-agy");
        assert_eq!(inst.exe_path, layout.launcher);
        assert_eq!(inst.prefix, layout.root);
        assert_eq!(inst.version.as_deref(), Some("1.2.10"));
        assert_eq!(inst.status.unavailable, None);
        let specs = runner.specs.lock().unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].program, layout.launcher);
        assert_eq!(specs[0].args, vec!["--version".to_string()]);
        assert_eq!(
            specs[0].env,
            vec![(
                "AGY_CLI_DISABLE_AUTO_UPDATE".to_string(),
                "true".to_string()
            )]
        );
        assert_eq!(specs[0].timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_detect_lists_nothing_for_an_agy_that_is_a_link() {
        // The Homebrew cask's `agy` is a link into its Caskroom (agy.md §3b):
        // Homebrew's row, never this one; and a flat-file route has no
        // launcher-only state (E's ruling 8), so a dangling link is nothing.
        let home = TempHome::new("agy-link");
        let cask = home.executable("opt/homebrew/Caskroom/antigravity-cli/1.2.9/antigravity");
        home.link(".local/bin/agy", &cask);
        home.dir(".gemini/antigravity-cli");
        let adapter = agy_adapter(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        assert!(adapter.detect(&env_as_owner(&home)).await.is_empty());
        std::fs::remove_file(home.path().join(".local/bin/agy")).unwrap();
        home.link(
            ".local/bin/agy",
            &home.path().join(".gemini/antigravity-cli/bin/agy"),
        );
        assert!(adapter.detect(&env_as_owner(&home)).await.is_empty());
    }

    #[tokio::test]
    async fn test_check_updates_for_agy_lists_a_newer_manifest_version_with_no_button() {
        // Spec §4.4 D5 item 4: a real candidate (the manifest is newer),
        // `SelfUpdatesOnly` (no `upgrade`), `Registry` channel, one GET with
        // no header of Banager's.
        let home = TempHome::new("agy-check-newer");
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            AGY_MANIFEST_URL,
            answer(r#"{"version":"1.2.11","url":"https://storage.googleapis.com/x.tar.gz","sha512":"00"}"#),
        );
        let (adapter, inst, _) = detected_agy(&home, "1.2.10", http.clone()).await;
        // `check_updates` compares the version `inventory` read, and
        // refuses without one: the order `refresh_round` keeps.
        adapter.inventory(&inst).await.expect("inventory");

        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");

        assert_eq!(
            out.candidates,
            vec![UpdateCandidate {
                key: ArtifactKey {
                    instance_id: "standalone-agy".to_string(),
                    kind: ArtifactKind::Binary,
                    name: "agy".to_string(),
                },
                current: "1.2.10".to_string(),
                target: "1.2.11".to_string(),
                channel: UpdateChannel::Registry,
                checkable: true,
                warnings: Vec::new(),
                blocked: Some(UpdateBlocked::SelfUpdatesOnly),
            }]
        );
        assert_eq!(http.calls(), vec![AGY_MANIFEST_URL.to_string()]);
        let request = &http.requests()[0];
        assert_eq!(request.method, "GET");
        assert!(request.headers.is_empty());
        assert_eq!(request.timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_check_updates_for_agy_lists_nothing_when_the_manifest_is_not_newer() {
        for body in [r#"{"version":"1.2.10"}"#, r#"{"version":"1.2.9"}"#] {
            let home = TempHome::new("agy-check-current");
            let http = Arc::new(MockHttpClient::new());
            http.respond(AGY_MANIFEST_URL, answer(body));
            let (adapter, inst, _) = detected_agy(&home, "1.2.10", http).await;
            adapter.inventory(&inst).await.expect("inventory");
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("check_updates");
            assert!(out.candidates.is_empty(), "{body}");
        }
    }

    #[tokio::test]
    async fn test_check_updates_for_agy_is_uncheckable_on_an_intel_mac_without_a_request() {
        // Spec §3.1/§3.5: the darwin_amd64 manifest is unverified, so an
        // Intel Mac (or Rosetta) gets "could not check" with the reason and
        // nothing leaves the machine.
        let home = TempHome::new("agy-check-intel");
        let http = Arc::new(MockHttpClient::new());
        http.respond(AGY_MANIFEST_URL, answer(r#"{"version":"1.2.11"}"#));
        let layout = agy_layout(&home);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("1.2.10\n"),
        );
        let adapter = agy_adapter(runner, http.clone()).with_arch("x86_64");
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        adapter.inventory(&inst).await.expect("inventory");

        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");

        assert_eq!(out.candidates.len(), 1);
        let c = &out.candidates[0];
        assert!(!c.checkable);
        assert_eq!(c.current, "1.2.10");
        assert_eq!(c.target, "1.2.10");
        assert!(
            matches!(&c.warnings[..], [Warning::Message(m)] if m.contains("Intel") && m.contains("x86_64")),
            "{:?}",
            c.warnings
        );
        assert!(
            http.calls().is_empty(),
            "no request on an unverified architecture"
        );
    }

    #[tokio::test]
    async fn test_check_updates_for_agy_marks_a_bad_manifest_uncheckable() {
        for (body, needle) in [
            (
                "<html>Sign in to the network</html>",
                "manifest is not JSON",
            ),
            (r#"{"url":"x"}"#, "no `version` string"),
            (r#"{"version":"latest"}"#, "not a version"),
        ] {
            let home = TempHome::new("agy-check-bad");
            let http = Arc::new(MockHttpClient::new());
            http.respond(AGY_MANIFEST_URL, answer(body));
            let (adapter, inst, _) = detected_agy(&home, "1.2.10", http).await;
            adapter.inventory(&inst).await.expect("inventory");
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("a failed lookup is not a source failure");
            assert_eq!(out.candidates.len(), 1, "{body}");
            assert!(!out.candidates[0].checkable);
            assert!(
                matches!(&out.candidates[0].warnings[..], [Warning::Message(m)] if m.contains(needle)),
                "{body}: {:?}",
                out.candidates[0].warnings
            );
        }
        let home = TempHome::new("agy-check-503");
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            AGY_MANIFEST_URL,
            HttpResponse {
                status: 503,
                body: String::new(),
            },
        );
        let (adapter, inst, _) = detected_agy(&home, "1.2.10", http).await;
        adapter.inventory(&inst).await.expect("inventory");
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .unwrap();
        assert!(!out.candidates[0].checkable);
        assert!(
            matches!(&out.candidates[0].warnings[..], [Warning::Message(m)] if m.contains("503"))
        );
    }

    #[tokio::test]
    async fn test_plan_upgrade_for_agy_is_refused_as_self_updating() {
        // The gate refuses it first from the candidate's `blocked`; the
        // adapter refuses it again for a stale snapshot (spec §五).
        let home = TempHome::new("agy-plan-upgrade");
        let (adapter, inst, _) =
            detected_agy(&home, "1.2.10", Arc::new(MockHttpClient::new())).await;
        assert!(matches!(
            adapter.plan(&inst, &agy_request(OpKind::Upgrade)).await,
            Err(AdapterError::UpdateBlocked {
                reason: UpdateBlocked::SelfUpdatesOnly
            })
        ));
    }

    #[tokio::test]
    async fn test_plan_uninstall_for_agy_lists_the_backup_and_the_program_and_keeps_its_state() {
        // Spec §6.3's agy row as ruled: any `agy.<time>.old` first, the
        // program (the launcher) last; the root, the staging folder and the
        // two shell files kept and said when present -- and not said when
        // absent (C's ruling 6).
        let home = TempHome::new("agy-plan-uninstall-full");
        home.file(".local/bin/agy.1727000000.old");
        home.dir(".cache/antigravity/staging");
        home.file(".zshrc");
        home.file(".zprofile");
        let (adapter, inst, layout) =
            detected_agy(&home, "1.2.10", Arc::new(MockHttpClient::new())).await;

        let plan = adapter
            .plan(&inst, &agy_request(OpKind::Uninstall))
            .await
            .expect("a plan");

        let PlanAction::TrashPaths { paths, previewed } = &plan.action else {
            panic!("a path list: {:?}", plan.action);
        };
        assert_eq!(
            paths,
            &vec![
                home.path().join(".local/bin/agy.1727000000.old"),
                layout.launcher.clone(),
            ]
        );
        assert_eq!(previewed.len(), 2);
        assert_eq!(
            plan.warnings,
            vec![
                Warning::WillTrash {
                    path: "~/.local/bin/agy.1727000000.old".to_string(),
                    what: RemovedWhat::Backups,
                },
                Warning::WillTrash {
                    path: "~/.local/bin/agy".to_string(),
                    what: RemovedWhat::Launcher,
                },
                Warning::WillKeep {
                    path: "~/.gemini/antigravity-cli".to_string(),
                    what: KeptWhat::ToolState,
                },
                Warning::WillKeep {
                    path: "~/.cache/antigravity".to_string(),
                    what: KeptWhat::InstallerCache,
                },
                Warning::WillKeep {
                    path: "~/.zshrc".to_string(),
                    what: KeptWhat::ShellConfigLines,
                },
                Warning::WillKeep {
                    path: "~/.zprofile".to_string(),
                    what: KeptWhat::ShellConfigLines,
                },
            ]
        );
        assert_eq!(plan.locks, vec![ResourceLock("standalone-agy".to_string())]);
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);

        // The minimum: no backup, no staging folder, no zprofile.
        let home = TempHome::new("agy-plan-uninstall-min");
        home.file(".zshrc");
        let (adapter, inst, layout) =
            detected_agy(&home, "1.2.10", Arc::new(MockHttpClient::new())).await;
        let plan = adapter
            .plan(&inst, &agy_request(OpKind::Uninstall))
            .await
            .expect("a plan");
        let PlanAction::TrashPaths { paths, .. } = &plan.action else {
            panic!("a path list");
        };
        assert_eq!(paths, &vec![layout.launcher.clone()]);
        assert_eq!(
            plan.warnings,
            vec![
                Warning::WillTrash {
                    path: "~/.local/bin/agy".to_string(),
                    what: RemovedWhat::Launcher,
                },
                Warning::WillKeep {
                    path: "~/.gemini/antigravity-cli".to_string(),
                    what: KeptWhat::ToolState,
                },
                Warning::WillKeep {
                    path: "~/.zshrc".to_string(),
                    what: KeptWhat::ShellConfigLines,
                },
            ]
        );
    }

    #[tokio::test]
    async fn test_execute_for_agy_moves_both_items_leaves_its_state_and_reads_as_gone() {
        let home = TempHome::new("agy-execute");
        home.file(".local/bin/agy.1727000000.old");
        home.dir(".cache/antigravity/staging");
        let trasher = Arc::new(MockTrasher::new());
        let layout = agy_layout(&home);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("1.2.10\n"),
        );
        let adapter = StandaloneAdapter::new(
            &AGY,
            runner,
            Arc::new(MockHttpClient::new()),
            trasher.clone(),
        )
        .with_trash_gap(Duration::ZERO)
        .with_arch("aarch64");
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        let plan = adapter
            .plan(&inst, &agy_request(OpKind::Uninstall))
            .await
            .expect("a plan");

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(outcome, Outcome::Succeeded);
        let PlanAction::TrashPaths { paths, .. } = &plan.action else {
            panic!("a path list");
        };
        assert_eq!(&trasher.calls(), paths);
        assert_eq!(trasher.kinds(), vec![ItemKind::File, ItemKind::File]);
        assert!(
            layout.root.join("conversations/c1.jsonl").is_file(),
            "the root stays"
        );
        assert!(
            home.path().join(".cache/antigravity/staging").is_dir(),
            "the staging folder stays"
        );
        let key = adapter.artifact_key(&inst);
        assert!(
            !adapter
                .reconcile_after_uninstall(&inst, &key, &plan)
                .await
                .expect("a reading")
                .present
        );
        assert!(adapter.detect(&env_as_owner(&home)).await.is_empty());
    }

    #[tokio::test]
    async fn test_execute_for_agy_refuses_a_launcher_its_updater_replaced_after_the_preview() {
        // Review Focus 1: agy's updater replaces the file at the launcher's
        // path -- this Mac's changed twice in two days (the phase 4 step D
        // plan's ruling 18). A replacement written as a new file (agy.md §4
        // infers an atomic replace, UNVERIFIED) has a new inode, so the
        // preview's identity no longer matches: nothing moves.
        let home = TempHome::new("agy-execute-replaced");
        let trasher = Arc::new(MockTrasher::new());
        let layout = agy_layout(&home);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("1.2.10\n"),
        );
        let adapter = StandaloneAdapter::new(
            &AGY,
            runner,
            Arc::new(MockHttpClient::new()),
            trasher.clone(),
        )
        .with_trash_gap(Duration::ZERO)
        .with_arch("aarch64");
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        let plan = adapter
            .plan(&inst, &agy_request(OpKind::Uninstall))
            .await
            .expect("a plan");
        // Made while the old one still exists, then renamed over it, so it
        // cannot get the old inode back (C's tests do the same).
        let replacement = home.executable(".local/bin/agy.new");
        std::fs::rename(&replacement, &layout.launcher).unwrap();

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(
            outcome,
            Outcome::BanagerFailed(Fault::PathChanged {
                path: "~/.local/bin/agy".to_string()
            })
        );
        assert!(trasher.calls().is_empty());
    }

    // ---- Grok Build ----

    fn grok_adapter(runner: Arc<dyn CommandRunner>) -> StandaloneAdapter {
        StandaloneAdapter::new(
            &GROK,
            runner,
            Arc::new(MockHttpClient::new()),
            Arc::new(MockTrasher::new()),
        )
        .with_trash_gap(Duration::ZERO)
    }

    fn grok_request(kind: OpKind) -> OpRequest {
        request_for("standalone-grok", kind, "grok")
    }

    const GROK_VERSION_LINE: &str = "grok 1.0.41 (4220f3b224a6)\n";
    const GROK_CHECK_CURRENT: &str = r#"{"currentVersion":"1.0.41","latestVersion":"1.0.41","updateAvailable":false,"installer":"internal","channel":"stable","autoUpdate":true,"error":null}"#;
    const GROK_CHECK_NEWER: &str = r#"{"currentVersion":"1.0.41","latestVersion":"1.0.42","updateAvailable":true,"installer":"internal","channel":"stable","autoUpdate":true,"error":null}"#;

    /// A runner answering grok's two read-only commands.
    fn grok_runner(layout: &super::testing::GrokLayout, check: CommandOutput) -> Arc<MockRunner> {
        let runner = Arc::new(MockRunner::new());
        let launcher = layout.launcher.to_str().unwrap();
        runner.respond(vec![launcher, "--version"], exited_0(GROK_VERSION_LINE));
        runner.respond(vec![launcher, "update", "--check", "--json"], check);
        runner
    }

    /// A detected grok over `home` (`trasher` its Trash), its check command
    /// answering `check`.
    async fn detected_grok(
        home: &TempHome,
        check: CommandOutput,
        trasher: Arc<MockTrasher>,
    ) -> (
        StandaloneAdapter,
        ManagerInstance,
        super::testing::GrokLayout,
    ) {
        let layout = grok_layout(home, "1.0.41");
        let runner = grok_runner(&layout, check);
        let adapter =
            StandaloneAdapter::new(&GROK, runner, Arc::new(MockHttpClient::new()), trasher)
                .with_trash_gap(Duration::ZERO);
        let inst = adapter.detect(&env_as_owner(home)).await.remove(0);
        (adapter, inst, layout)
    }

    #[tokio::test]
    async fn test_detect_lists_grok_through_its_relative_launcher_link_reading_the_second_token() {
        // Spec §3.5 (VERIFIED): `~/.grok/bin/grok ->
        // ../downloads/grok-1.0.41-macos-aarch64`, relative; grok.md §1:
        // `grok --version` -> `grok 1.0.41 (4220f3b224a6)`. No environment
        // on the read (none is documented).
        let home = TempHome::new("grok-detect");
        let layout = grok_layout(&home, "1.0.41");
        let runner = Arc::new(RecordingRunner {
            specs: StdMutex::new(Vec::new()),
            output: exited_0(GROK_VERSION_LINE),
        });
        let adapter = grok_adapter(runner.clone());

        let instances = adapter
            .detect(&home.env(vec![home.path().join(".grok/bin")]))
            .await;

        // One instance, although `bin/agent` resolves to the same download:
        // the route looks at the one fixed launcher path.
        assert_eq!(instances.len(), 1);
        assert_eq!(std::fs::canonicalize(&layout.agent).unwrap(), layout.real);
        let inst = &instances[0];
        assert_eq!(inst.id, "standalone-grok");
        assert_eq!(inst.exe_path, layout.launcher);
        assert_eq!(inst.prefix, layout.root);
        assert_eq!(inst.version.as_deref(), Some("1.0.41"));
        assert!(inst.status.notes.is_empty(), "PATH finds this very copy");
        let specs = runner.specs.lock().unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].args, vec!["--version".to_string()]);
        assert!(specs[0].env.is_empty());
    }

    #[tokio::test]
    async fn test_check_updates_for_grok_asks_its_own_read_only_check_and_trusts_its_answer() {
        // Spec §4.3: grok's `updateAvailable` decides; §4.1: the channel is
        // Native (the tool's own answer); the phase 4 step D plan's ruling
        // 10: the target is `latestVersion` as printed. No button is
        // withheld (grok has `grok update`).
        let home = TempHome::new("grok-check-newer");
        let (adapter, inst, layout) = detected_grok(
            &home,
            exited_0(GROK_CHECK_NEWER),
            Arc::new(MockTrasher::new()),
        )
        .await;
        // `check_updates` takes the version `inventory` read (the
        // candidate's `current`), and refuses without one: the order
        // `refresh_round` keeps.
        adapter.inventory(&inst).await.expect("inventory");

        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");

        assert_eq!(
            out.candidates,
            vec![UpdateCandidate {
                key: ArtifactKey {
                    instance_id: "standalone-grok".to_string(),
                    kind: ArtifactKind::Binary,
                    name: "grok".to_string(),
                },
                current: "1.0.41".to_string(),
                target: "1.0.42".to_string(),
                channel: UpdateChannel::Native,
                checkable: true,
                warnings: Vec::new(),
                blocked: None,
            }]
        );
        // The check runs against the launcher, with the recipe's argv and
        // its own timeout, and no environment of Banager's.
        let runner = Arc::new(RecordingRunner {
            specs: StdMutex::new(Vec::new()),
            output: exited_0(GROK_VERSION_LINE),
        });
        let adapter = grok_adapter(runner.clone());
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        adapter.inventory(&inst).await.expect("inventory");
        let _ = adapter.check_updates(&inst, &CheckOptions::default()).await;
        let specs = runner.specs.lock().unwrap();
        let check = specs
            .iter()
            .find(|spec| spec.args.first().map(String::as_str) == Some("update"))
            .expect("the check command ran");
        assert_eq!(check.program, layout.launcher);
        assert_eq!(
            check.args,
            vec![
                "update".to_string(),
                "--check".to_string(),
                "--json".to_string()
            ]
        );
        assert!(check.env.is_empty());
        assert_eq!(check.timeout, Duration::from_secs(60));
        assert_eq!(check.output_use, OutputUse::Parsed);
    }

    #[tokio::test]
    async fn test_check_updates_for_grok_lists_nothing_when_it_says_no_update_and_is_uncheckable_when_it_fails(
    ) {
        // Review Focus 4: the tool's word is final when it answers; when it
        // fails, prints something else, or times out, the row is "could not
        // check" with a short reason -- never an `Err` for the source.
        let home = TempHome::new("grok-check-current");
        let (adapter, inst, _) = detected_grok(
            &home,
            exited_0(GROK_CHECK_CURRENT),
            Arc::new(MockTrasher::new()),
        )
        .await;
        adapter.inventory(&inst).await.expect("inventory");
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .unwrap();
        assert!(out.candidates.is_empty());

        let failing = [
            // A check that fails is worded as every other lookup that runs
            // a command (`lookup_failure_reason`, the generic sentence
            // README.md's list of English explanations names): the command
            // as typed and grok's own first line of stderr...
            (
                CommandOutput {
                    exit_code: Some(1),
                    stdout: String::new(),
                    stderr: "error: could not reach x.ai\n".to_string(),
                    timed_out: false,
                    cancelled: false,
                },
                "grok update --check --json: error: could not reach x.ai",
            ),
            // ...or, when grok said nothing, its exit code as a number...
            (
                CommandOutput {
                    exit_code: Some(1),
                    stdout: String::new(),
                    stderr: String::new(),
                    timed_out: false,
                    cancelled: false,
                },
                "`grok update --check --json` exited with code 1",
            ),
            // ...or, with no exit code (a signal ended it) and nothing
            // said, that it did not finish.
            (
                CommandOutput {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    timed_out: false,
                    cancelled: false,
                },
                "`grok update --check --json` did not finish",
            ),
            (
                exited_0("<html><body>Sign in to the network</body></html>\n"),
                "did not print JSON",
            ),
            (
                exited_0(r#"{"currentVersion":"1.0.41","latest":"1.0.42","updateAvailable":true}"#),
                "no `latestVersion` string",
            ),
            // Exit 0, `updateAvailable: false`, and grok's own `error` set:
            // the plausible offline shape. Not "up to date" -- "could not
            // check", with grok's words (the phase 4 step D plan's ruling
            // 10).
            (
                exited_0(
                    r#"{"currentVersion":"1.0.41","latestVersion":"1.0.41","updateAvailable":false,"installer":"internal","channel":"stable","autoUpdate":true,"error":"failed to reach the update server"}"#,
                ),
                "reported: failed to reach the update server",
            ),
            (
                CommandOutput {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    timed_out: true,
                    cancelled: false,
                },
                "`grok update --check --json` did not finish within 60 s",
            ),
        ];
        for (check, needle) in failing {
            let home = TempHome::new("grok-check-failing");
            let (adapter, inst, _) =
                detected_grok(&home, check, Arc::new(MockTrasher::new())).await;
            adapter.inventory(&inst).await.expect("inventory");
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("a failed lookup is not a source failure");
            assert_eq!(out.candidates.len(), 1, "{needle}");
            let c = &out.candidates[0];
            assert!(!c.checkable);
            assert_eq!(c.current, "1.0.41");
            assert_eq!(c.target, "1.0.41");
            assert_eq!(c.channel, UpdateChannel::Registry);
            // Never Rust's own spelling of an exit code (`Some(1)`).
            assert!(
                matches!(&c.warnings[..], [Warning::Message(m)] if m.contains(needle) && !m.contains("Some(")),
                "{needle}: {:?}",
                c.warnings
            );
        }
    }

    #[tokio::test]
    async fn test_plan_upgrade_for_grok_is_its_own_update_command() {
        // Spec §五's grok row: `<grok> update`, 1800 s, KillThenReconcile,
        // its own lock and no other, no environment, no password.
        let home = TempHome::new("grok-plan-upgrade");
        let (adapter, inst, layout) = detected_grok(
            &home,
            exited_0(GROK_CHECK_CURRENT),
            Arc::new(MockTrasher::new()),
        )
        .await;
        let plan = adapter
            .plan(&inst, &grok_request(OpKind::Upgrade))
            .await
            .expect("a plan");
        assert_eq!(command_program(&plan), layout.launcher);
        assert_eq!(command_args(&plan), &["update".to_string()]);
        assert!(command_env(&plan).is_empty());
        assert!(!plan.needs_password);
        assert_eq!(
            plan.locks,
            vec![ResourceLock("standalone-grok".to_string())]
        );
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);
        assert_eq!(plan.timeout_secs, 1800);
        assert!(plan.warnings.is_empty());
    }

    #[tokio::test]
    async fn test_plan_uninstall_for_grok_moves_its_folders_and_its_launcher_link_last_and_keeps_its_home(
    ) {
        // Spec §6.3's grok row as the phase 4 step D plan rules it (rulings
        // 3 and 4): with everything present but no fallback links, three
        // folders and the fish file, then `bin/agent` and `bin/grok` last --
        // the folder `~/.grok/bin` itself is not listed; `~/.grok` and
        // `~/.zshrc` kept and said. A `/usr/local/bin/grok` on the machine
        // running this test, if there is one, is not a link into this temp
        // home, so it gets no sentence (its ruling 6): the expected list does
        // not depend on the host.
        let home = TempHome::new("grok-plan-uninstall-full");
        home.file(".zshrc");
        home.file(".config/fish/completions/grok.fish");
        home.file(".grok/bin/my-own-script");
        let (adapter, inst, layout) = detected_grok(
            &home,
            exited_0(GROK_CHECK_CURRENT),
            Arc::new(MockTrasher::new()),
        )
        .await;

        let plan = adapter
            .plan(&inst, &grok_request(OpKind::Uninstall))
            .await
            .expect("a plan");

        let PlanAction::TrashPaths { paths, previewed } = &plan.action else {
            panic!("a path list: {:?}", plan.action);
        };
        assert_eq!(
            paths,
            &vec![
                home.path().join(".grok/downloads"),
                home.path().join(".grok/bundled"),
                home.path().join(".grok/completions"),
                home.path().join(".config/fish/completions/grok.fish"),
                layout.agent.clone(),
                layout.launcher.clone(),
            ]
        );
        assert_eq!(
            previewed.iter().map(|i| i.kind).collect::<Vec<_>>(),
            vec![
                ItemKind::Dir,
                ItemKind::Dir,
                ItemKind::Dir,
                ItemKind::File,
                ItemKind::Symlink,
                ItemKind::Symlink
            ]
        );
        assert_eq!(
            plan.warnings,
            vec![
                Warning::WillTrash {
                    path: "~/.grok/downloads".to_string(),
                    what: RemovedWhat::Program
                },
                Warning::WillTrash {
                    path: "~/.grok/bundled".to_string(),
                    what: RemovedWhat::Program
                },
                Warning::WillTrash {
                    path: "~/.grok/completions".to_string(),
                    what: RemovedWhat::Program
                },
                Warning::WillTrash {
                    path: "~/.config/fish/completions/grok.fish".to_string(),
                    what: RemovedWhat::Program
                },
                Warning::WillTrash {
                    path: "~/.grok/bin/agent".to_string(),
                    what: RemovedWhat::Launcher
                },
                Warning::WillTrash {
                    path: "~/.grok/bin/grok".to_string(),
                    what: RemovedWhat::Launcher
                },
                Warning::WillKeep {
                    path: "~/.grok".to_string(),
                    what: KeptWhat::SettingsAndHistory
                },
                Warning::WillKeep {
                    path: "~/.zshrc".to_string(),
                    what: KeptWhat::ShellConfigLines
                },
            ]
        );
        assert_eq!(plan.timeout_secs, removal::TIMEOUT_SECS);
        // The user's own script in the PATH folder is neither listed nor
        // moved (the step D plan's ruling 4).
        assert!(!paths.contains(&home.path().join(".grok/bin/my-own-script")));
        assert!(!paths.contains(&home.path().join(".grok/bin")));

        // The minimum: downloads and the two links only, nothing kept to
        // mention but `~/.grok` itself.
        let home = TempHome::new("grok-plan-uninstall-min");
        let layout_min = grok_layout(&home, "1.0.41");
        std::fs::remove_dir_all(layout_min.root.join("bundled")).unwrap();
        std::fs::remove_dir_all(layout_min.root.join("completions")).unwrap();
        let runner = grok_runner(&layout_min, exited_0(GROK_CHECK_CURRENT));
        let adapter = grok_adapter(runner);
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        let plan = adapter
            .plan(&inst, &grok_request(OpKind::Uninstall))
            .await
            .expect("a plan");
        let PlanAction::TrashPaths { paths, .. } = &plan.action else {
            panic!("a path list");
        };
        assert_eq!(
            paths,
            &vec![
                home.path().join(".grok/downloads"),
                layout_min.agent.clone(),
                layout_min.launcher.clone()
            ]
        );
        assert_eq!(plan.warnings.len(), 4, "{:?}", plan.warnings);
    }

    #[tokio::test]
    async fn test_plan_uninstall_for_grok_keeps_a_foreign_agent_link_and_moves_its_own_fallback_links_first(
    ) {
        // Review Focus 2 (spec §十三 #27): `~/.local/bin/agent` belongs to
        // another CLI -- kept and said, the uninstall goes on. grok's own
        // `~/.local/bin/grok` (a link into the root) is moved, and moved
        // first, while every folder its text could pass through still
        // exists (the phase 4 step D plan's ruling 3).
        let home = TempHome::new("grok-plan-foreign-agent");
        let other = home.executable("other-cli/agent");
        home.link(".local/bin/agent", &other);
        let trasher = Arc::new(MockTrasher::new());
        let (adapter, inst, layout) =
            detected_grok(&home, exited_0(GROK_CHECK_CURRENT), trasher.clone()).await;
        let fallback = home.link(".local/bin/grok", &layout.launcher);

        let plan = adapter
            .plan(&inst, &grok_request(OpKind::Uninstall))
            .await
            .expect("a plan, not a refusal");

        let PlanAction::TrashPaths { paths, .. } = &plan.action else {
            panic!("a path list");
        };
        assert_eq!(paths[0], fallback);
        assert_eq!(paths.last(), Some(&layout.launcher));
        assert!(!paths.contains(&home.path().join(".local/bin/agent")));
        assert_eq!(
            plan.warnings[0],
            Warning::WillTrash {
                path: "~/.local/bin/grok".to_string(),
                what: RemovedWhat::Launcher
            }
        );
        let kept_position = plan
            .warnings
            .iter()
            .position(|w| {
                *w == Warning::WillKeep {
                    path: "~/.local/bin/agent".to_string(),
                    what: KeptWhat::NotOurs,
                }
            })
            .expect("the foreign link is said to stay");
        assert!(
            plan.warnings[..kept_position]
                .iter()
                .all(|w| matches!(w, Warning::WillTrash { .. })),
            "after the moves, before the recipe's kept paths: {:?}",
            plan.warnings
        );

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(&trasher.calls(), paths);
        assert!(
            std::fs::symlink_metadata(home.path().join(".local/bin/agent"))
                .unwrap()
                .file_type()
                .is_symlink(),
            "the foreign link is untouched"
        );
        assert!(layout.root.join("config.toml").is_file());
        assert!(layout.root.join("auth.json").is_file());
        assert!(layout.root.join("sessions/s1.jsonl").is_file());
        let key = adapter.artifact_key(&inst);
        assert!(
            !adapter
                .reconcile_after_uninstall(&inst, &key, &plan)
                .await
                .unwrap()
                .present
        );
    }

    #[tokio::test]
    async fn test_plan_uninstall_for_grok_keeps_links_into_its_kept_folders_that_do_not_lead_to_its_program(
    ) {
        // `~/.grok` is also the folder this uninstall keeps -- its plugins
        // and skills among it -- so a link pointing into it is not thereby
        // grok's. The user's own `~/.local/bin/agent` to a plugin's program,
        // `~/.local/bin/grok` to a skill's, and grok's `~/.grok/bin/agent`
        // replaced by a link to that plugin: none leads to grok's program,
        // so each is kept and said (`NotOurs`), the uninstall goes on, and
        // afterwards each still runs what it ran -- the plugin and the skill
        // stay with the rest of `~/.grok`.
        let home = TempHome::new("grok-plan-links-into-kept");
        let trasher = Arc::new(MockTrasher::new());
        let (adapter, inst, layout) =
            detected_grok(&home, exited_0(GROK_CHECK_CURRENT), trasher.clone()).await;
        let plugin = home.executable(".grok/plugins/p/bin/agent");
        let skill = home.executable(".grok/skills/s/bin/grok");
        let local_grok = home.link(".local/bin/grok", &skill);
        let local_agent = home.link(".local/bin/agent", &plugin);
        std::fs::remove_file(&layout.agent).unwrap();
        home.link(".grok/bin/agent", Path::new("../plugins/p/bin/agent"));

        let plan = adapter
            .plan(&inst, &grok_request(OpKind::Uninstall))
            .await
            .expect("a plan, not a refusal");

        let PlanAction::TrashPaths { paths, .. } = &plan.action else {
            panic!("a path list: {:?}", plan.action);
        };
        assert_eq!(
            paths,
            &vec![
                home.path().join(".grok/downloads"),
                home.path().join(".grok/bundled"),
                home.path().join(".grok/completions"),
                layout.launcher.clone(),
            ]
        );
        let not_ours = |path: &str| Warning::WillKeep {
            path: path.to_string(),
            what: KeptWhat::NotOurs,
        };
        assert_eq!(
            plan.warnings,
            vec![
                Warning::WillTrash {
                    path: "~/.grok/downloads".to_string(),
                    what: RemovedWhat::Program
                },
                Warning::WillTrash {
                    path: "~/.grok/bundled".to_string(),
                    what: RemovedWhat::Program
                },
                Warning::WillTrash {
                    path: "~/.grok/completions".to_string(),
                    what: RemovedWhat::Program
                },
                Warning::WillTrash {
                    path: "~/.grok/bin/grok".to_string(),
                    what: RemovedWhat::Launcher
                },
                not_ours("~/.local/bin/grok"),
                not_ours("~/.local/bin/agent"),
                not_ours("~/.grok/bin/agent"),
                Warning::WillKeep {
                    path: "~/.grok".to_string(),
                    what: KeptWhat::SettingsAndHistory
                },
            ]
        );

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(&trasher.calls(), paths);
        for (link, target) in [
            (&local_grok, &skill),
            (&local_agent, &plugin),
            (&layout.agent, &plugin),
        ] {
            assert_eq!(
                std::fs::canonicalize(link).expect("the link still resolves"),
                *target,
                "{link:?}"
            );
        }
    }

    #[tokio::test]
    async fn test_a_stopped_grok_uninstall_leaves_a_launcher_only_row_that_a_second_uninstall_finishes(
    ) {
        // Review Focus 6: macOS refuses `bundled` (the second item). The
        // download folder is in the Trash, `~/.grok/bin/grok` and
        // `~/.grok/bin/agent` dangle into `~/.grok` -- the launcher-only
        // state (spec §3.3 step 2, relative link text) -- the row stays, and
        // a second uninstall lists `downloads` as already gone and finishes
        // with the two links, `grok` last.
        let home = TempHome::new("grok-stopped");
        let trasher = Arc::new(MockTrasher::new());
        trasher.refuse_call(1, "Operation not permitted");
        let (adapter, inst, layout) =
            detected_grok(&home, exited_0(GROK_CHECK_CURRENT), trasher.clone()).await;
        let plan = adapter
            .plan(&inst, &grok_request(OpKind::Uninstall))
            .await
            .expect("a plan");

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(
            outcome,
            Outcome::Failed {
                exit_code: None,
                summary: "Operation not permitted".to_string()
            }
        );
        assert_eq!(trasher.calls().len(), 2);
        assert!(std::fs::symlink_metadata(&layout.launcher)
            .unwrap()
            .file_type()
            .is_symlink());
        // Both links now dangle: there, but resolving to nothing.
        assert!(std::fs::symlink_metadata(&layout.agent)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(!layout.agent.exists() && !layout.launcher.exists());
        assert_eq!(
            route::probe(RouteKind::SymlinkIntoRoot, &layout.launcher, &layout.root),
            Probe::LauncherOnly
        );
        let key = adapter.artifact_key(&inst);
        assert!(
            adapter
                .reconcile_after_uninstall(&inst, &key, &plan)
                .await
                .unwrap()
                .present
        );

        // The next refresh: a launcher-only row, no version read.
        let rows = adapter.detect(&env_as_owner(&home)).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status.notes, vec![InstanceNote::LauncherOnly]);
        assert_eq!(rows[0].version, None);

        // The second uninstall, with a Trash that accepts everything.
        let second = Arc::new(MockTrasher::new());
        let runner = grok_runner(&layout, exited_0(GROK_CHECK_CURRENT));
        let adapter = StandaloneAdapter::new(
            &GROK,
            runner,
            Arc::new(MockHttpClient::new()),
            second.clone(),
        )
        .with_trash_gap(Duration::ZERO);
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        let plan = adapter
            .plan(&inst, &grok_request(OpKind::Uninstall))
            .await
            .expect("a plan");
        let PlanAction::TrashPaths { paths, .. } = &plan.action else {
            panic!("a path list");
        };
        assert_eq!(
            paths,
            &vec![
                home.path().join(".grok/bundled"),
                home.path().join(".grok/completions"),
                layout.agent.clone(),
                layout.launcher.clone(),
            ]
        );
        assert_eq!(
            plan.warnings[0],
            Warning::AlreadyGone {
                path: "~/.grok/downloads".to_string()
            }
        );
        let outcome = adapter
            .execute(
                &plan,
                Arc::new(VecSink::new()),
                10,
                CancellationToken::new(),
            )
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(
            route::probe(RouteKind::SymlinkIntoRoot, &layout.launcher, &layout.root),
            Probe::Absent
        );
        assert!(
            std::fs::symlink_metadata(&layout.agent).is_err(),
            "agent went too"
        );
        assert!(
            layout.root.join("bin").is_dir(),
            "the emptied folder stays (step D plan ruling 4)"
        );
        assert!(adapter.detect(&env_as_owner(&home)).await.is_empty());
    }

    // ---- Two tools' uninstalls at the same time ----

    #[tokio::test]
    async fn test_grok_and_agy_uninstalls_running_at_once_keep_the_gap_between_all_their_moves() {
        // The whole-step review's case: Uninstall confirmed on Grok Build,
        // then on Antigravity CLI while the first still runs -- the
        // operation manager runs up to three operations at once, and each
        // path-list uninstall locks only its own instance. The two adapters
        // are the ones `all()` builds (`one_per_recipe`), over one trasher
        // and one `removal::LastMove`, so each of the six moves -- grok's
        // five, agy's one -- begins at least the gap after the one before
        // it, whichever tool's that was: the Trash spike found Put Back
        // recorded for every item only when one process's moves were 2 s or
        // more apart (`removal::PUT_BACK_SETTLE`). With a gap kept within
        // each run alone, the two runs' first moves began within a
        // millisecond of each other.
        let home = TempHome::new("grok-and-agy-at-once");
        let grok = grok_layout(&home, "1.0.41");
        let agy = agy_layout(&home);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![grok.launcher.to_str().unwrap(), "--version"],
            exited_0(GROK_VERSION_LINE),
        );
        runner.respond(
            vec![agy.launcher.to_str().unwrap(), "--version"],
            exited_0("1.2.10\n"),
        );
        let timed = Arc::new(TimedTrasher::default());
        let gap = Duration::from_millis(100);
        let mut adapters: std::collections::HashMap<&str, StandaloneAdapter> =
            one_per_recipe(runner, Arc::new(MockHttpClient::new()), timed.clone())
                .into_iter()
                .map(|adapter| (adapter.recipe.id, adapter.with_trash_gap(gap)))
                .collect();
        let grok_adapter = adapters.remove("grok").expect("grok is registered");
        let agy_adapter = adapters.remove("agy").expect("agy is registered");
        let grok_inst = grok_adapter.detect(&env_as_owner(&home)).await.remove(0);
        let agy_inst = agy_adapter.detect(&env_as_owner(&home)).await.remove(0);
        let grok_plan = grok_adapter
            .plan(&grok_inst, &grok_request(OpKind::Uninstall))
            .await
            .expect("grok's plan");
        let agy_plan = agy_adapter
            .plan(&agy_inst, &agy_request(OpKind::Uninstall))
            .await
            .expect("agy's plan");

        let (grok_outcome, agy_outcome) = tokio::join!(
            grok_adapter.execute(
                &grok_plan,
                Arc::new(VecSink::new()),
                1,
                CancellationToken::new()
            ),
            agy_adapter.execute(
                &agy_plan,
                Arc::new(VecSink::new()),
                2,
                CancellationToken::new()
            ),
        );

        assert_eq!(grok_outcome.expect("grok's run"), Outcome::Succeeded);
        assert_eq!(agy_outcome.expect("agy's run"), Outcome::Succeeded);
        let moved = timed.inner.calls();
        assert_eq!(moved.len(), 6, "grok's five and agy's one: {moved:?}");
        assert!(moved.contains(&agy.launcher) && moved.contains(&grok.launcher));
        let gaps = timed.gaps();
        assert!(
            gaps.iter().all(|between| *between >= gap),
            "the time between one move's beginning and the next one's: {gaps:?}"
        );
    }

    // ---- The recordings of Antigravity CLI and Grok Build ----

    /// A file of a recipe's recorded fixture directory: the one version its
    /// meta names (`verified_versions[0]`), under
    /// `adapters/fixtures/standalone-<id>/`. B's `fixture` is claude's and
    /// E's `rustup_fixture` rustup's.
    fn recorded(recipe: &'static Recipe, name: &str) -> String {
        let meta = AdapterMeta::from_toml(recipe.meta_toml).expect("meta");
        let version = meta
            .verified_versions
            .first()
            .expect("meta lists the recorded version");
        let path = format!(
            "../../adapters/fixtures/standalone-{}/{version}/{name}",
            recipe.id
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
    }

    #[test]
    fn test_agys_recorded_version_line_is_one_bare_version_matching_the_meta() {
        let meta = AdapterMeta::from_toml(AGY.meta_toml).expect("meta");
        let line = recorded(&AGY, "version.txt");
        let version = latest::parse_version(&line, AGY.version.parse).expect("a version");
        assert_eq!(Some(&version), meta.verified_versions.first());
        assert!(latest::is_dotted_version(&version), "{version:?}");
        assert_eq!(
            line.trim(),
            version,
            "agy prints the bare version and nothing else (agy.md §4)"
        );
    }

    #[test]
    fn test_agys_recorded_manifest_names_a_version_the_check_can_compare() {
        // The manifest may name a newer version than the recorded launcher
        // (one published between the two reads, before agy's updater
        // installed it): what is pinned is that the check can read and
        // compare it, not which way the comparison goes.
        let manifest = recorded(&AGY, "manifest-darwin_arm64.json");
        let Latest::HttpJsonField { field, .. } = AGY.latest else {
            panic!("agy reads a manifest");
        };
        let remote = latest::parse_json_field(&manifest, field).expect("a version");
        let local =
            latest::parse_version(&recorded(&AGY, "version.txt"), AGY.version.parse).unwrap();
        assert!(
            latest::compare_dotted(&local, &remote).is_some(),
            "{local} vs {remote}"
        );
    }

    #[test]
    fn test_groks_recorded_version_line_yields_the_second_token_matching_the_meta() {
        let meta = AdapterMeta::from_toml(GROK.meta_toml).expect("meta");
        let line = recorded(&GROK, "version.txt");
        assert!(line.starts_with("grok "), "{line:?}");
        let version = latest::parse_version(&line, GROK.version.parse).expect("a version");
        assert_eq!(Some(&version), meta.verified_versions.first());
        assert!(latest::is_dotted_version(&version), "{version:?}");
    }

    #[test]
    fn test_groks_recorded_update_check_parses_and_names_the_installed_version_when_nothing_is_newer(
    ) {
        let body = recorded(&GROK, "update-check.json");
        let Latest::Command {
            latest_field,
            available_field,
            error_field,
            ..
        } = GROK.latest
        else {
            panic!("grok asks itself");
        };
        let check = latest::parse_update_check(&body, latest_field, available_field, error_field)
            .expect("grok's JSON: both fields present, `error` null");
        // `latest` is shown, never compared, and the recipe takes it with
        // any suffix (a prerelease day is a truthful recording too), so it
        // is not held to a dotted shape here. When grok said nothing was
        // available, its latest is the installed version it also printed.
        if !check.available {
            let local =
                latest::parse_version(&recorded(&GROK, "version.txt"), GROK.version.parse).unwrap();
            assert_eq!(check.latest, local);
        }
    }

    /// Both recordings' `layout.txt` are `ls -lan` with the home folder's
    /// absolute path replaced by `~`, and their READMEs say so (step D
    /// plan ruling 17): no absolute home directory and no `.local` host
    /// name survives in either file of either directory, and each layout
    /// still shows what its route relies on -- agy's launcher a regular
    /// executable file, grok's two `bin/` entries relative links to the
    /// verified version's download.
    #[test]
    fn test_the_recorded_agy_and_grok_layouts_show_their_routes_and_name_no_home_or_host() {
        for recipe in [&AGY, &GROK] {
            for name in ["layout.txt", "README.md"] {
                let text = recorded(recipe, name);
                for prefix in ["/Users/", "/home/"] {
                    assert!(
                        !text.contains(prefix),
                        "{}'s {name} names an absolute home directory under {prefix}",
                        recipe.id
                    );
                }
                // `X.local` is a Mac's Bonjour host name when `X` ends in a
                // letter, digit or hyphen; `~/.local/...` is preceded by `/`
                // and is not one.
                let names_a_host = text.match_indices(".local").any(|(at, _)| {
                    text[..at]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '-')
                });
                assert!(!names_a_host, "{}'s {name} names a .local host", recipe.id);
            }
        }
        let agy = recorded(&AGY, "layout.txt");
        assert!(
            agy.lines()
                .any(|line| line.starts_with("-rwx") && line.ends_with(" ~/.local/bin/agy")),
            "agy's launcher is an executable regular file: {agy}"
        );
        let verified = AdapterMeta::from_toml(GROK.meta_toml)
            .expect("meta")
            .verified_versions[0]
            .clone();
        let download = format!("grok-{verified}-macos-aarch64");
        let grok = recorded(&GROK, "layout.txt");
        for name in ["grok", "agent"] {
            assert!(
                grok.lines().any(|line| line.starts_with('l')
                    && line.ends_with(&format!(" {name} -> ../downloads/{download}"))),
                "bin/{name} is a relative link to {download}: {grok}"
            );
        }
        assert!(
            grok.lines()
                .any(|line| line.starts_with("-rwx") && line.ends_with(&format!(" {download}"))),
            "{download} is an executable regular file in downloads/: {grok}"
        );
    }

    #[tokio::test]
    async fn test_check_updates_for_agy_over_the_recorded_manifest_lists_only_a_real_update() {
        // The runner answers `--version` with the recorded line and the
        // manifest's URL answers the recorded manifest: a candidate, with no
        // button, exactly when the manifest's version is greater than the
        // installed one -- both derived from the recording, so a
        // re-recording on a later day stays honest.
        let line = recorded(&AGY, "version.txt");
        let installed = latest::parse_version(&line, AGY.version.parse).expect("version line");
        let body = recorded(&AGY, "manifest-darwin_arm64.json");
        let published = latest::parse_json_field(&body, "version").expect("manifest");
        let home = TempHome::new("agy-check-recorded");
        let layout = agy_layout(&home);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0(&line),
        );
        let http = Arc::new(MockHttpClient::new());
        http.respond(AGY_MANIFEST_URL, answer(&body));
        let adapter = agy_adapter(runner, http.clone());
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        // `check_updates` compares the version `inventory` read, and
        // refuses without one: the order `refresh_round` keeps.
        adapter.inventory(&inst).await.expect("inventory");
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert_eq!(http.calls(), vec![AGY_MANIFEST_URL.to_string()]);
        match latest::compare_dotted(&installed, &published) {
            Some(Ordering::Less) => {
                assert_eq!(out.candidates.len(), 1, "{installed} < {published}");
                assert_eq!(out.candidates[0].target, published);
                assert!(out.candidates[0].checkable);
                assert_eq!(
                    out.candidates[0].blocked,
                    Some(UpdateBlocked::SelfUpdatesOnly)
                );
            }
            Some(Ordering::Equal | Ordering::Greater) => {
                assert!(out.candidates.is_empty(), "{installed} >= {published}");
            }
            None => panic!("both are dotted versions: {installed} vs {published}"),
        }
    }

    #[tokio::test]
    async fn test_check_updates_for_grok_over_its_recorded_check_lists_what_grok_said() {
        // The runner answers `--version` and `update --check --json` with
        // the recorded bytes: a candidate exactly when grok said an update
        // is available, targeting the version grok named -- believed,
        // never compared (spec §4.3).
        let line = recorded(&GROK, "version.txt");
        let installed = latest::parse_version(&line, GROK.version.parse).expect("version line");
        let body = recorded(&GROK, "update-check.json");
        let Latest::Command {
            latest_field,
            available_field,
            error_field,
            ..
        } = GROK.latest
        else {
            panic!("grok asks itself");
        };
        let said = latest::parse_update_check(&body, latest_field, available_field, error_field)
            .expect("grok's JSON");
        let home = TempHome::new("grok-check-recorded");
        let layout = grok_layout(&home, &installed);
        let runner = Arc::new(MockRunner::new());
        let launcher = layout.launcher.to_str().unwrap();
        runner.respond(vec![launcher, "--version"], exited_0(&line));
        runner.respond(
            vec![launcher, "update", "--check", "--json"],
            exited_0(&body),
        );
        let adapter = grok_adapter(runner);
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        // `check_updates` takes the version `inventory` read (the
        // candidate's `current`), and refuses without one.
        adapter.inventory(&inst).await.expect("inventory");
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        if said.available {
            assert_eq!(out.candidates.len(), 1, "{said:?}");
            assert_eq!(out.candidates[0].current, installed);
            assert_eq!(out.candidates[0].target, said.latest);
            assert_eq!(out.candidates[0].channel, UpdateChannel::Native);
            assert_eq!(out.candidates[0].blocked, None, "grok has `grok update`");
        } else {
            assert!(out.candidates.is_empty(), "{said:?}");
        }
    }
}
