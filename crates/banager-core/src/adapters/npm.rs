use crate::adapters::{
    ensure_instance_match, lookup_failure_reason, reconcile_from, run_plan,
    uncheckable_from_inventory, validate_package_name, Adapter, AdapterError, AdapterMeta,
    CheckOptions, CheckOutcome, LookupFailure,
};
use crate::events::{EventSink, OpId};
use crate::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstallReason, InstalledArtifact, InstanceStatus,
    ManagerInstance, OpKind, OpRequest, Outcome, Plan, PlanAction, ReadOnlyReason, Reconciled,
    ResourceLock, Scope, SearchHit, Unavailable, UninstallBlocked, UninstallScope, UpdateBlocked,
    UpdateCandidate, UpdateChannel, Warning,
};

/// npm's own package, as `npm ls -g` lists it beside the user's: the npm
/// every other package is updated and uninstalled with, and the program
/// `npm uninstall -g npm` would remove (`UninstallBlocked::SourceProgram`).
const OWN_PACKAGE: &str = "npm";

/// The package Node brings beside npm (up to Node 24), whose `corepack`
/// command a Homebrew `node@…` formula links into the prefix as it does
/// `npm` (R42-2): then it updates and goes with that formula
/// (`UpdateBlocked::UpdatesWithFormula`, `UninstallBlocked::ComesWithFormula`).
const COREPACK: &str = "corepack";

/// The packages whose command in the prefix's `bin` may be a Homebrew
/// formula's link (`real_comes_with_formula`).
const WITH_FORMULA: [&str; 2] = [OWN_PACKAGE, COREPACK];
use crate::protected::{look, Protected};
use crate::runner::{resolve_exe, CommandOutput, CommandRunner, CommandSpec, HostEnv, OutputUse};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// A search box takes free text, not a package name: `validate_package_name`
/// matches `^[A-Za-z0-9@._+/-]+$`, so it rejects any multi-word query
/// ("json parser") as an *invalid name*, which is both wrong and confusing.
/// That function exists to keep a path out of an argv; this one exists to
/// keep a query out of argv's flag namespace and to bound its length. npm
/// itself decides what matches.
fn validate_search_query(query: &str) -> Result<(), AdapterError> {
    let trimmed = query.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') || trimmed.len() > 200 {
        return Err(AdapterError::InvalidName(query.to_string()));
    }
    Ok(())
}

/// The real check for `NpmAdapter::new`'s `prefix_read_only_fn` default:
/// whether the current user can write where npm actually places global
/// packages under `prefix` (the prefix *root* that `npm prefix -g` reports,
/// e.g. `/opt/homebrew` — **not** the `node_modules` directory itself, despite
/// what an earlier version of this comment claimed). npm writes into
/// `{prefix}/lib/node_modules`. `None` where it can, and otherwise why not.
///
/// When that directory doesn't exist yet (e.g. the first global install on
/// this prefix), npm has to create it, which needs write permission on the
/// nearest *existing* ancestor, not on the missing leaf. So this walks
/// `{prefix}/lib/node_modules` → `{prefix}/lib` → `{prefix}` and tests
/// whichever of those exists first. A prefix owned by another user (e.g. a
/// system-wide npm) is `PrefixNotWritable` for this adapter — see the
/// per-adapter contract table's Notes column.
///
/// Each is looked up one step at a time and never into or through a
/// protected place (`protected::look`): one that is, or leads into, one --
/// a prefix kept in `~/Documents` -- is not looked at, and the prefix is
/// read-only here as `PrefixProtected`: whether it could be written is not
/// known, which is not the same as the account not being able to.
fn real_prefix_read_only(prefix: &Path) -> Option<ReadOnlyReason> {
    let protected = Protected::of_this_process();
    let node_modules = prefix.join("lib").join("node_modules");
    let lib_dir = prefix.join("lib");
    let not_writable = |candidate: &Path| {
        (!look::writable_folder(candidate, &protected)).then_some(ReadOnlyReason::PrefixNotWritable)
    };
    for candidate in [node_modules.as_path(), lib_dir.as_path(), prefix] {
        match look::target(candidate, &protected) {
            Ok(_) => return not_writable(candidate),
            Err(error) if look::is_protected(&error) => {
                return Some(ReadOnlyReason::PrefixProtected)
            }
            // Not there, or not to be looked up: the next one out.
            Err(_) => continue,
        }
    }
    // None of the three exist (npm prefix -g pointed at a prefix that isn't
    // on disk at all) — there is nothing nearer to test than the root itself,
    // and a missing path correctly reports "not writable".
    not_writable(prefix)
}

/// Whether the `command` (`npm` or `corepack`) in `prefix`'s `bin` is a
/// Homebrew formula's: a link that leads, every link followed, into
/// `<prefix>/Cellar/` -- what `brew link --force node@22` puts there
/// (`UpdateBlocked::UpdatesWithFormula`). The unversioned `node` formula's
/// npm is a copy in `<prefix>/lib/node_modules/npm`, outside the Cellar,
/// and is not; nor is npm's own, after `npm install -g npm`, or `npm
/// install -g corepack`. Read-only: where two links lead, one step at a
/// time and never into or through a protected place (`protected::look`);
/// `false` for anything it cannot tell.
fn real_comes_with_formula(prefix: &Path, command: &str) -> bool {
    let protected = Protected::of_this_process();
    let Ok(cellar) = look::real_path(&prefix.join("Cellar"), &protected) else {
        return false;
    };
    match look::target(&prefix.join("bin").join(command), &protected) {
        Ok((real, _)) => real.starts_with(&cellar),
        Err(_) => false,
    }
}

/// `NpmAdapter::comes_with_formula_fn` as `NpmAdapter::new` sets it: the
/// real read in every build but this crate's unit tests, where nothing is
/// read unless a test installs a reader.
#[cfg(not(test))]
const DEFAULT_COMES_WITH_FORMULA_FN: fn(&Path, &str) -> bool = real_comes_with_formula;
#[cfg(test)]
const DEFAULT_COMES_WITH_FORMULA_FN: fn(&Path, &str) -> bool = |_, _| false;

/// The sentence an uninstall says under the tool (`UninstallScope::Npm`),
/// for the npm `version` Banager detected (`npm --version`), only when that
/// is 7 or later. npm 7 and later run no script of the package's on
/// `uninstall -g` (npm 10.9.9: arborist's `reify.js:1308-1341` runs scripts
/// only for added and changed packages, and for `-g` it loads only the
/// named one, `reify.js:372-395`; npm 12.0.2 the same), so its settings and
/// data outside its folder stay; npm 6 ran the package's `preuninstall`,
/// `uninstall` and `postuninstall` scripts (npm 10.9.9's
/// `docs/content/using-npm/scripts.md:216-228`), which could do anything.
/// A version Banager could not read, or whose major number does not parse,
/// gets no sentence.
fn uninstall_scope(version: Option<&str>) -> Option<Warning> {
    let major: u64 = version?.trim().split('.').next()?.parse().ok()?;
    (major >= 7).then_some(Warning::UninstallScope {
        what: UninstallScope::Npm,
    })
}

/// The id `NpmAdapter::detect` gives an npm at `exe_path` whose `npm
/// prefix -g` did not answer: built from the executable, since the global
/// prefix every other npm id is built from is what failed. Read by
/// `resume_unanswered_npm` (session/refresh.rs) to recognise that
/// stand-in.
pub(crate) fn unanswered_instance_id(exe_path: &std::path::Path) -> String {
    crate::model::instance_id("npm", Some(&exe_path.display().to_string()))
}

pub struct NpmAdapter {
    runner: Arc<dyn CommandRunner>,
    meta: AdapterMeta,
    /// How to decide whether `inst.prefix` is writable by the current user
    /// (in practice: whether npm can write into `{prefix}/lib/node_modules`,
    /// creating it if needed — see `real_prefix_read_only`), and if not,
    /// why: `None` for writable, gating install/uninstall/upgrade
    /// otherwise. Production always gets `real_prefix_read_only`; tests
    /// inject a fixed answer via the `#[cfg(test)]`-only
    /// `with_prefix_read_only_fn`, mirroring `BrewAdapter::with_euid_fn`.
    /// `detect()` asks the same question to fill `read_only_reason`, which
    /// is why this takes `&Path` (the prefix) rather than a
    /// `&ManagerInstance` that does not exist yet.
    prefix_read_only_fn: fn(&Path) -> Option<ReadOnlyReason>,
    /// How to tell whether the `npm` or `corepack` in a prefix's `bin` is
    /// a Homebrew formula's (`real_comes_with_formula`), whose package's
    /// update is then not offered (`UpdateBlocked::UpdatesWithFormula`),
    /// nor corepack's uninstall (`UninstallBlocked::ComesWithFormula`).
    comes_with_formula_fn: fn(&Path, &str) -> bool,
    /// How the queue key of the Homebrew prefix a plan's prefix may be
    /// looks at folders (`brew::prefix_lock`): `brew::PREFIX_IDENTITY_FN`.
    prefix_identity_fn: fn(&Path) -> Option<(u64, u64)>,
}

impl NpmAdapter {
    pub const ENV: [(&'static str, &'static str); 3] = [
        ("NO_COLOR", "1"),
        ("npm_config_update_notifier", "false"),
        ("npm_config_fund", "false"),
    ];

    pub fn new(runner: Arc<dyn CommandRunner>) -> NpmAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../adapters/meta/npm.toml"))
            .expect("adapters/meta/npm.toml must parse");
        NpmAdapter {
            runner,
            meta,
            prefix_read_only_fn: real_prefix_read_only,
            comes_with_formula_fn: DEFAULT_COMES_WITH_FORMULA_FN,
            prefix_identity_fn: super::brew::PREFIX_IDENTITY_FN,
        }
    }

    /// Test support (the `test-support` feature, for the integration tests,
    /// which are built without `cfg(test)`): a plan's Homebrew queue key
    /// looks at no discovery prefix (`/opt/homebrew`, `/usr/local`,
    /// `/home/linuxbrew/.linuxbrew`) of the Mac running the test, as in this
    /// crate's unit tests.
    #[cfg(feature = "test-support")]
    pub fn looking_at_no_homebrew_prefix(mut self) -> NpmAdapter {
        self.prefix_identity_fn = |_| None;
        self
    }

    /// Test support (`test-support`): what a plan's Homebrew queue key
    /// makes of the folder at `path` (`brew::prefix_lock`), so that an
    /// integration test can hold `new` to the real reader
    /// (`brew::PREFIX_IDENTITY_FN`) on folders of its own.
    #[cfg(feature = "test-support")]
    pub fn prefix_identity(&self, path: &Path) -> Option<(u64, u64)> {
        (self.prefix_identity_fn)(path)
    }

    #[cfg(test)]
    fn with_prefix_read_only_fn(mut self, f: fn(&Path) -> Option<ReadOnlyReason>) -> NpmAdapter {
        self.prefix_read_only_fn = f;
        self
    }

    /// Test-only hook: whether the prefix's `npm` or `corepack` is a
    /// Homebrew formula's (see `comes_with_formula_fn`).
    #[cfg(test)]
    fn with_comes_with_formula_fn(mut self, f: fn(&Path, &str) -> bool) -> NpmAdapter {
        self.comes_with_formula_fn = f;
        self
    }

    /// Whether `name` is npm's own package or corepack, and its command in
    /// `inst`'s prefix a Homebrew formula's link (`comes_with_formula_fn`).
    fn comes_with_formula(&self, inst: &ManagerInstance, name: &str) -> bool {
        WITH_FORMULA.contains(&name) && (self.comes_with_formula_fn)(&inst.prefix, name)
    }

    /// `candidates`, npm's own and corepack marked `UpdatesWithFormula`
    /// where their command in `inst`'s prefix is a Homebrew formula's: the
    /// update would take that formula's link away
    /// (`UpdateBlocked::UpdatesWithFormula`). A link is read only when its
    /// package's update is among them.
    fn with_formulas_npm_marked(
        &self,
        inst: &ManagerInstance,
        mut candidates: Vec<UpdateCandidate>,
    ) -> Vec<UpdateCandidate> {
        for candidate in candidates.iter_mut() {
            if candidate.blocked.is_none() && self.comes_with_formula(inst, &candidate.key.name) {
                candidate.blocked = Some(UpdateBlocked::UpdatesWithFormula);
            }
        }
        candidates
    }

    /// npm's own lock, and that of a Homebrew at npm's global prefix
    /// (y1-keg review), which every plan takes and a refresh reads under
    /// (`Adapter::refresh_locks`). npm that came with a Node from Homebrew
    /// writes into Homebrew's prefix -- `npm install -g npm` puts
    /// its own `bin/npm` there -- and a `brew upgrade` of that Node unlinks
    /// the places it linked and links them again, stopping at any file in
    /// the way (`Keg::ConflictError`). With both locks no npm operation
    /// runs while a brew one on the same prefix does, so none can land
    /// between that unlink and that link; nor does a refresh ask this npm
    /// anything then (r37 F2), when it may not be found or not start.
    /// Where no Homebrew lives at the prefix, no other plan takes the
    /// second lock.
    fn instance_locks(&self, inst: &ManagerInstance) -> Vec<ResourceLock> {
        vec![
            ResourceLock(inst.id.clone()),
            super::brew::prefix_lock(&inst.prefix, self.prefix_identity_fn),
        ]
    }

    fn env_vec(&self) -> Vec<(String, String)> {
        Self::ENV
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn instance_id_for(&self, prefix: &Path) -> String {
        crate::model::instance_id(&self.meta.id, Some(&prefix.display().to_string()))
    }

    async fn run_npm(
        &self,
        inst: &ManagerInstance,
        args: Vec<String>,
        timeout: Duration,
    ) -> Result<CommandOutput, AdapterError> {
        let mut args = args;
        if args.iter().any(|arg| arg == "-g") {
            args.extend([
                "--prefix".into(),
                inst.prefix.to_string_lossy().into_owned(),
            ]);
        }
        let spec = CommandSpec {
            program: inst.exe_path.clone(),
            args,
            env: self.env_vec(),
            cwd: None,
            timeout,
            // Every caller of this helper hands the result to a parser.
            output_use: OutputUse::Parsed,
        };
        Ok(self
            .runner
            .run(spec, None, CancellationToken::new())
            .await?)
    }

    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let Some(exe_path) = resolve_exe("npm", env) else {
            return Vec::new();
        };
        let prefix_spec = CommandSpec {
            program: exe_path.clone(),
            args: vec!["prefix".to_string(), "-g".to_string()],
            env: self.env_vec(),
            cwd: None,
            timeout: Duration::from_secs(30),
            output_use: OutputUse::Parsed,
        };
        let prefix_output = self
            .runner
            .run(prefix_spec, None, CancellationToken::new())
            .await;
        let prefix = match &prefix_output {
            Ok(o) if o.exit_code == Some(0) => PathBuf::from(o.stdout.trim()),
            // npm is on `PATH` but would not even answer `npm prefix -g`
            // -- a malformed `~/.npmrc` makes every npm command exit
            // non-zero, for instance. Every other adapter that finds its
            // executable but cannot talk to it (brew/cargo/pipx/uv/ollama,
            // all on a failed `--version`) still reports the instance,
            // marked unavailable, so the user is told; returning
            // `Vec::new()` here instead made npm vanish with no notice and
            // no `SourceError` -- the exact vanishing act `refresh()`'s
            // carry-forward exists to prevent, one level below where it
            // looks for it.
            //
            // Every other adapter's id survives this because it never
            // depended on the command that just failed (brew's three
            // candidate paths, cargo's `CARGO_HOME`-or-`~/.cargo`) --  npm
            // has no such fallback; `npm prefix -g`'s answer *is* the only
            // source of its id, and there is no env-level default global
            // prefix to substitute. So this id is built from the resolved
            // executable path instead: stable across rounds of this same
            // failure (`resolve_exe` answers the same way each time), and
            // never collides with a real `npm:{prefix}` id, since a real
            // global prefix never ends in `/bin/npm`. It will simply not
            // match the id a later, successful round produces -- carrying
            // this round's non-existent inventory forward to that one
            // would be wrong access anyway; see `refresh.rs`'s per-instance
            // carry-forward, which is keyed on exactly this id being
            // stable while it recurs and does not promise continuity
            // across a source going from broken to working.
            //
            // Continuity across a source going from working to broken is
            // `refresh_round`'s: it gives this instance back the id the
            // same executable had last round (`resume_unanswered_npm` in
            // session/refresh.rs, which knows this id by
            // `unanswered_instance_id`), so its rows and the updates the
            // user hid stay with it.
            _ => {
                return vec![ManagerInstance {
                    id: unanswered_instance_id(&exe_path),
                    adapter_id: self.meta.id.clone(),
                    exe_path: exe_path.clone(),
                    prefix: exe_path
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| PathBuf::from("/")),
                    scope: Scope::User,
                    status: InstanceStatus {
                        unavailable: Some(Unavailable::NotResponding),
                        notes: Vec::new(),
                        // What `npm prefix -g` did: npm's launcher with no
                        // `node` on PATH never starts (finding (1) of the
                        // 2026-10-07 run).
                        no_answer: crate::runner::no_answer::of(&prefix_output),
                    },
                    version: None,
                    answered_at: None,
                    unverified_version: None,
                    read_only_reason: None,
                }];
            }
        };
        let version_spec = CommandSpec {
            program: exe_path.clone(),
            args: vec!["--version".to_string()],
            env: self.env_vec(),
            cwd: None,
            timeout: Duration::from_secs(30),
            output_use: OutputUse::Parsed,
        };
        let version_output = self
            .runner
            .run(version_spec, None, CancellationToken::new())
            .await;
        let version = match &version_output {
            Ok(o) if o.exit_code == Some(0) => Some(o.stdout.trim().to_string()),
            _ => None,
        };
        let unverified_version = self.meta.unverified_version(&version);
        // Asked once, here, about the prefix npm itself reported -- not
        // about wherever the `npm` binary happens to live. A Node
        // installed from nodejs.org's package leaves a root-owned prefix
        // this user cannot write, and the two pages need to say so
        // *before* the user clicks anything. `plan()` asks again at click
        // time: permissions can change in between, so that gate stays.
        let read_only_reason = (self.prefix_read_only_fn)(&prefix);
        vec![ManagerInstance {
            id: self.instance_id_for(&prefix),
            adapter_id: self.meta.id.clone(),
            exe_path,
            prefix,
            scope: Scope::User,
            status: InstanceStatus {
                // The state axis. `version` is `None` exactly when the
                // CLI is on PATH but `--version` would not run or could
                // not be parsed: the tool is there, it just did not
                // answer.
                unavailable: version.is_none().then_some(Unavailable::NotResponding),
                notes: Vec::new(),
                no_answer: version
                    .is_none()
                    .then(|| crate::runner::no_answer::of(&version_output))
                    .flatten(),
            },
            version,
            answered_at: None,
            unverified_version,
            read_only_reason,
        }]
    }

    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let mut installed = self.read_ls_global(inst).await?.0;
        // corepack that a Homebrew formula linked: `npm uninstall -g
        // corepack` would delete that link and the commands corepack
        // declares (pnpm, pnpx, yarn, yarnpkg), Homebrew's own pnpm's and
        // yarn's among them (R42-2).
        if let Some(corepack) = installed
            .iter_mut()
            .find(|artifact| artifact.key.name == COREPACK)
        {
            if self.comes_with_formula(inst, COREPACK) {
                corepack.uninstall_blocked = Some(UninstallBlocked::ComesWithFormula);
            }
        }
        Ok(installed)
    }

    /// `npm ls -g --depth=0 --json`, read: the packages, and npm's answer
    /// they were read from (`installed_from_elsewhere` reads it again).
    async fn read_ls_global(
        &self,
        inst: &ManagerInstance,
    ) -> Result<(Vec<InstalledArtifact>, String), AdapterError> {
        let output = self
            .run_npm(
                inst,
                vec![
                    "ls".to_string(),
                    "-g".to_string(),
                    "--depth=0".to_string(),
                    "--json".to_string(),
                ],
                Duration::from_secs(60),
            )
            .await?;
        // `npm ls -g --depth=0 --json` exits 1 for problems it found in a
        // tree it did read (`ELSPROBLEMS`: the packages, then `problems`
        // and an `error` merged in), and for an error with no errno -- with
        // no tree at all. Only the first is a list; the reason for anything
        // else is stderr's, which the runner masks, never stdout's, which
        // it does not (`CommandOutput::stdout`).
        if output.exit_code != Some(0) && output.exit_code != Some(1) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        let parsed = parse_ls_global(&output.stdout, &inst.id);
        if output.exit_code == Some(1) {
            let has_tree = serde_json::from_str::<serde_json::Value>(&output.stdout)
                .ok()
                .is_some_and(|root| root.get("dependencies").is_some_and(|d| d.is_object()));
            if parsed.is_err() || !has_tree {
                return Err(AdapterError::CommandFailed {
                    code: output.exit_code,
                    stderr: output.stderr,
                });
            }
        }
        Ok((parsed?, output.stdout))
    }

    /// `candidates` without the updates of globals npm did not install from
    /// a registry (`installed_from_elsewhere`): `npm outdated -g` looks
    /// every global up on the registry by name, whatever it came from
    /// (npm 10.9.9 `outdated.js:122`, `:179`), and `npm install -g <name>`
    /// would put the registry's package in place of an `npm link`, a
    /// folder, a fork from git or an alias (R42-3). Read from one more `npm
    /// ls -g`, only when the check found an update to offer. npm 7 and
    /// later name the source of a link or a folder only (no hidden lockfile
    /// for globals, arborist `reify.js:253`), so git, URL and alias globals
    /// keep their update there.
    async fn without_updates_from_elsewhere(
        &self,
        inst: &ManagerInstance,
        mut candidates: Vec<UpdateCandidate>,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        if !candidates.iter().any(|candidate| candidate.checkable) {
            return Ok(candidates);
        }
        let (_, listing) = self.read_ls_global(inst).await?;
        let elsewhere = installed_from_elsewhere(&listing)?;
        candidates.retain(|candidate| !elsewhere.contains(&candidate.key.name));
        Ok(candidates)
    }

    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        let output = self
            .run_npm(
                inst,
                vec![
                    "outdated".to_string(),
                    "-g".to_string(),
                    "--json".to_string(),
                ],
                Duration::from_secs(60),
            )
            .await?;
        // Exit 0 is the plain answer: whatever is on stdout is the result,
        // and stdout that will not parse is a real parse error.
        if output.exit_code == Some(0) {
            let found = parse_outdated_global(&output.stdout, &inst.id)?;
            let found = self.without_updates_from_elsewhere(inst, found).await?;
            return Ok(self.with_formulas_npm_marked(inst, found).into());
        }
        // npm exits 1 whenever it *finds* a version difference -- a result,
        // not a failure (per-adapter contract table) -- including one that
        // is no upgrade: a global installed ahead of `latest` is listed too
        // (`current !== wanted`, npm's `commands/outdated.js`). So exit 1
        // with rows npm printed is that result even when the SemVer filter
        // keeps none of them as an update (review r7 F1)...
        if output.exit_code == Some(1) {
            if let Ok((found, has_rows)) = parse_outdated_result(&output.stdout, &inst.id) {
                if has_rows {
                    let found = self.without_updates_from_elsewhere(inst, found).await?;
                    return Ok(self.with_formulas_npm_marked(inst, found).into());
                }
            }
        }
        // ...and any other non-zero exit, or exit 1 with no rows to show
        // for it, is a lookup that did not happen: "exit 1 and printed
        // nothing" is a contradiction, not good news. Returning the empty
        // list here is what used to tell a user whose registry was
        // unreachable that everything was up to date.
        let failure = LookupFailure::words(
            lookup_failure_reason("npm outdated -g", output.exit_code, &output.stderr),
            &output.stderr,
        );
        let installed = self.inventory(inst).await?;
        Ok(uncheckable_from_inventory(&installed, UpdateChannel::Native, &failure).into())
    }

    pub async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        validate_search_query(query)?;
        let output = self
            .run_npm(
                inst,
                vec![
                    "search".to_string(),
                    "--json".to_string(),
                    "--searchlimit".to_string(),
                    "20".to_string(),
                    query.to_string(),
                ],
                Duration::from_secs(30),
            )
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        parse_search(&output.stdout, &self.meta.id)
    }

    pub async fn plan(
        &self,
        inst: &ManagerInstance,
        req: &OpRequest,
    ) -> Result<Plan, AdapterError> {
        ensure_instance_match(req, inst)?;
        validate_package_name(&req.name)?;
        // The same refusal the actionability gate in `Session::issue_plan`
        // gives a prefix `detect` already found read-only, because it is the
        // same fact found later: permissions can change between detect and
        // the click. Sent as `NotActionable` rather than a `Refused` string
        // so the front end words it with the very sentence the source's own
        // rows use (`READ_ONLY_DETAIL_KEYS` in src/lib/sources.ts), in the
        // user's language, instead of this adapter's English.
        if let Some(reason) = (self.prefix_read_only_fn)(&inst.prefix) {
            return Err(AdapterError::NotActionable {
                read_only: Some(reason),
                unavailable: None,
            });
        }
        // npm itself (`UninstallBlocked::SourceProgram`): the gate's late
        // twin (`blocked_uninstall` in session/plans.rs refuses the same
        // row from the snapshot), so no preview of `npm uninstall -g npm` is
        // ever built. Its update is planned as any package's.
        if req.kind == OpKind::Uninstall && req.name == OWN_PACKAGE {
            return Err(AdapterError::UninstallBlocked {
                reason: UninstallBlocked::SourceProgram,
            });
        }
        // npm or corepack that a Homebrew formula linked into the prefix
        // updates and goes with it (`UpdateBlocked::UpdatesWithFormula`,
        // `UninstallBlocked::ComesWithFormula`): the gate's late twins, for
        // a page from before the formula was linked.
        if req.kind == OpKind::Upgrade && self.comes_with_formula(inst, &req.name) {
            return Err(AdapterError::UpdateBlocked {
                reason: UpdateBlocked::UpdatesWithFormula,
            });
        }
        if req.kind == OpKind::Uninstall
            && req.name == COREPACK
            && self.comes_with_formula(inst, COREPACK)
        {
            return Err(AdapterError::UninstallBlocked {
                reason: UninstallBlocked::ComesWithFormula,
            });
        }
        let locks = self.instance_locks(inst);
        let warnings = match req.kind {
            OpKind::Uninstall => uninstall_scope(inst.version.as_deref())
                .into_iter()
                .collect(),
            OpKind::Install | OpKind::Upgrade => Vec::new(),
            OpKind::Link => return Err(super::links_nothing(&self.meta.id)),
        };
        // An update asks for `<name>@*`: npm picks from the range `*` as
        // `npm outdated -g` did for the row (the newest version that runs
        // on this Node and is not deprecated; npm 10.9.9
        // `outdated.js:186-187`, npm 8.19.4 `outdated.js` the same), and
        // installs it over the older one asked for by name (arborist
        // `can-place-dep.js:166-176`). `<name>@latest` would take the
        // `latest` tag whatever Node it needs (R42-1), and so would the
        // bare name on npm 7 and 8, which read it as `latest`; from npm 9
        // the bare name is `*` (npm-package-arg `npa.js:51`).
        let mut args = match req.kind {
            OpKind::Install => {
                vec!["install".to_string(), "-g".to_string(), req.name.clone()]
            }
            OpKind::Upgrade => {
                vec![
                    "install".to_string(),
                    "-g".to_string(),
                    format!("{}@*", req.name),
                ]
            }
            OpKind::Uninstall => vec!["uninstall".to_string(), "-g".to_string(), req.name.clone()],
            OpKind::Link => return Err(super::links_nothing(&self.meta.id)),
        };
        args.extend([
            "--prefix".into(),
            inst.prefix.to_string_lossy().into_owned(),
        ]);
        Ok(Plan {
            request: req.clone(),
            action: PlanAction::Command {
                program: inst.exe_path.clone(),
                args,
                env: self.env_vec(),
            },
            needs_password: false,
            locks,
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings,
            affected: Vec::new(),
            basis: None,
            timeout_secs: 600,
        })
    }

    pub async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        let refused = Outcome::BanagerFailed(crate::model::Fault::ChangedSinceShown);
        let PlanAction::Command { program, args, env } = &plan.action else {
            return Ok(refused);
        };
        let Some(prefix) = args
            .windows(2)
            .find(|pair| pair[0] == "--prefix")
            .map(|pair| Path::new(&pair[1]))
            .filter(|path| path.is_absolute())
        else {
            return Ok(refused);
        };
        // Ask the same npm which global root it selects now. Do not pass
        // --prefix to this read: that would merely echo our own answer.
        // The write below still uses the *confirmed* argv, even if config
        // changes again between this read and spawning it.
        let read = self
            .runner
            .run(
                CommandSpec {
                    program: program.clone(),
                    args: vec!["prefix".into(), "-g".into()],
                    env: env.clone(),
                    cwd: None,
                    timeout: Duration::from_secs(30),
                    output_use: OutputUse::Parsed,
                },
                None,
                cancel.clone(),
            )
            .await;
        if cancel.is_cancelled() {
            return Ok(Outcome::Cancelled);
        }
        // A read that did not answer -- npm gone, its launcher finding no
        // `node`, no answer in time -- ends as npm's own failure would,
        // its stderr in the log, with nothing written; only another (or
        // an empty or relative) prefix is a change since the preview
        // (r20 R20-2).
        let output =
            match super::read_before_run(read.map_err(AdapterError::from), sink.as_ref(), op_id)? {
                super::ReadBeforeRun::Answered(output) => output,
                super::ReadBeforeRun::Ends(outcome) => return Ok(outcome),
            };
        if Path::new(output.stdout.trim()) != prefix {
            return Ok(refused);
        }
        run_plan(&self.runner, plan, sink, op_id, cancel).await
    }

    pub async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        let artifacts = self.inventory(inst).await?;
        Ok(reconcile_from(artifacts, key))
    }
}

#[async_trait]
impl Adapter for NpmAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        NpmAdapter::detect(self, env).await
    }

    fn refresh_locks(&self, inst: &ManagerInstance) -> Vec<ResourceLock> {
        self.instance_locks(inst)
    }

    /// npm's own lock, without the Homebrew prefix's: `npm outdated -g`
    /// asks the registry, which can take up to its minute
    /// (`check_updates`), and a Homebrew operation confirmed meanwhile
    /// waited for it. The listing before it is read under both (r37 F2).
    /// An operation that changes `node` or `npm` while the check runs can
    /// make it fail, which that refresh says, keeping npm's last updates.
    fn check_locks(&self, inst: &ManagerInstance) -> Vec<ResourceLock> {
        vec![ResourceLock(inst.id.clone())]
    }

    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        NpmAdapter::inventory(self, inst).await
    }

    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        NpmAdapter::check_updates(self, inst, opts).await
    }

    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        NpmAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        NpmAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        NpmAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        NpmAdapter::reconcile(self, inst, key).await
    }
}

#[derive(Debug, Deserialize)]
struct LsGlobalRoot {
    dependencies: Option<HashMap<String, LsGlobalDependency>>,
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct LsGlobalDependency {
    #[serde(default)]
    version: Option<String>,
    /// Where npm installed it from (npm 10.9.9 `ls.js:358-360`): a
    /// registry's tarball, or `file:` for a link (always, arborist
    /// `link.js:94-98`), a folder or a tarball on disk, `git+…` for git,
    /// another URL for a tarball fetched from it. Absent where npm has
    /// no record of it.
    #[serde(default)]
    resolved: Option<String>,
}

/// The globals in `npm ls -g --depth=0 --json`'s answer that npm did not
/// install from a registry (R42-3), whose registry namesake is no update.
/// One came from a registry when it was resolved to an `http(s)` URL
/// whose path holds `/<name>/-/`, every npm registry's tarball path,
/// mirrors included (a scope's slash may be spelled `%2f`); anything else
/// npm says -- `file:`, `git+…`, another URL, a registry tarball of
/// another name (an alias) -- is from elsewhere. A global with no
/// `resolved` is not known to be, and keeps its update: on npm 7 and later
/// that is every global but a link or a folder (`node.js:140-158` takes it
/// from a lockfile or `_resolved`, and a global has neither), so the
/// `git+…`, URL and alias answers come from npm 6 only.
fn installed_from_elsewhere(json: &str) -> Result<HashSet<String>, AdapterError> {
    let root: LsGlobalRoot =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    Ok(root
        .dependencies
        .unwrap_or_default()
        .into_iter()
        .filter(|(name, dep)| {
            dep.resolved.as_deref().is_some_and(|resolved| {
                let path = resolved
                    .strip_prefix("https://")
                    .or_else(|| resolved.strip_prefix("http://"))
                    .and_then(|rest| rest.find('/').map(|slash| &rest[slash..]))
                    .map(|path| path.split(['?', '#']).next().unwrap_or_default());
                let from_registry = path.is_some_and(|path| {
                    path.contains(&format!("/{name}/-/"))
                        || path
                            .to_ascii_lowercase()
                            .contains(&format!("/{}/-/", name.replacen('/', "%2f", 1)))
                });
                !from_registry
            })
        })
        .map(|(name, _)| name)
        .collect())
}

/// Parses `npm ls -g --depth=0 --json`. The real, committed fixture
/// (`adapters/fixtures/npm/12.0.2/ls-global.json`) shows the top level is a
/// `dependencies` **object** keyed by package name, not an array — a parser
/// expecting an array silently sees zero packages instead of erroring.
/// Sorted by name for deterministic output (a `HashMap`'s own iteration
/// order is not). npm's own `npm` is listed like any package, and is the
/// one Banager does not offer to uninstall (`UninstallBlocked::
/// SourceProgram`).
pub(crate) fn parse_ls_global(
    json: &str,
    instance_id: &str,
) -> Result<Vec<InstalledArtifact>, crate::adapters::AdapterError> {
    let root: LsGlobalRoot = serde_json::from_str(json)
        .map_err(|e| crate::adapters::AdapterError::Parse(e.to_string()))?;
    // Only an `error` and no tree: npm could not read its packages, which
    // is not "none installed". Named by npm's code alone (`ENOTDIR`), and
    // only when it is one: the rest is stdout as npm wrote it, with no
    // login masked out of it (`CommandOutput::stdout`).
    if root.dependencies.is_none() {
        if let Some(error) = root.error {
            let code = error
                .get("code")
                .and_then(|code| code.as_str())
                .filter(|code| {
                    (1..=32).contains(&code.len())
                        && code
                            .chars()
                            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                });
            return Err(AdapterError::Parse(match code {
                Some(code) => format!("npm answered with its error {code}, not its packages"),
                None => "npm answered with an error, not its packages".to_string(),
            }));
        }
    }
    let mut out: Vec<InstalledArtifact> = root
        .dependencies
        .unwrap_or_default()
        .into_iter()
        .map(|(name, dep)| InstalledArtifact {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Package,
                name: name.clone(),
            },
            uninstall_blocked: (name == OWN_PACKAGE).then_some(UninstallBlocked::SourceProgram),
            display_name: name,
            version: dep.version.unwrap_or_default(),
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path: None,
            auto_updates: false,
            facts: Default::default(),
        })
        .collect();
    out.sort_by(|a, b| a.key.name.cmp(&b.key.name));
    Ok(crate::adapters::sanity::artifacts(out))
}

/// One row of `npm outdated -g --json`. Either version may be missing:
/// npm lists a global whose own version it cannot read -- an `npm link`
/// whose working copy is gone, a folder left without its `package.json` --
/// and leaves `current` out of the JSON (npm 10.9.9 `outdated.js:159`,
/// `:188`, `:264-271`). Such a row is that package's alone to be unable to
/// check (R42-4).
#[derive(Debug, Deserialize)]
struct OutdatedEntry {
    #[serde(default)]
    current: Option<String>,
    #[serde(default)]
    latest: Option<String>,
}

/// Parses `npm outdated -g --json`. npm exits 1 whenever it finds anything
/// outdated — the caller must still treat that stdout as the real result,
/// not an error (see the per-adapter contract table). Empty stdout (no
/// output at all, not even `{}`) means nothing is outdated.
pub(crate) fn parse_outdated_global(
    json: &str,
    instance_id: &str,
) -> Result<Vec<UpdateCandidate>, crate::adapters::AdapterError> {
    Ok(parse_outdated_result(json, instance_id)?.0)
}

fn parse_outdated_result(
    json: &str,
    instance_id: &str,
) -> Result<(Vec<UpdateCandidate>, bool), AdapterError> {
    if json.trim().is_empty() {
        return Ok((Vec::new(), false));
    }
    let root: HashMap<String, OutdatedEntry> = serde_json::from_str(json)
        .map_err(|e| crate::adapters::AdapterError::Parse(e.to_string()))?;
    // A row is a name with a version npm gave, either of the two: npm's
    // error answer, `{"error": {...}}`, has neither.
    let is_version = |version: &Option<String>| {
        version
            .as_deref()
            .is_some_and(crate::adapters::sanity::is_name)
    };
    let has_rows = root.iter().any(|(name, entry)| {
        crate::adapters::sanity::is_name(name)
            && (is_version(&entry.current) || is_version(&entry.latest))
            && entry
                .current
                .as_deref()
                .is_none_or(crate::adapters::sanity::is_name)
            && entry
                .latest
                .as_deref()
                .is_none_or(crate::adapters::sanity::is_name)
    });
    let mut out = Vec::new();
    // Rows npm gave one version for, kept past `sanity::candidates`, whose
    // target must be a version: theirs is the unknown one they have, as on
    // the rows of a check that failed (`uncheckable_from_inventory`).
    let mut unreadable = Vec::new();
    for (name, entry) in root {
        let key = ArtifactKey {
            instance_id: instance_id.to_string(),
            kind: ArtifactKind::Package,
            name,
        };
        let (current, latest) = match (entry.current, entry.latest) {
            (Some(current), Some(latest)) => (current, latest),
            (None, None) => continue,
            (current, latest) => {
                if !crate::adapters::sanity::is_name(&key.name)
                    || latest
                        .as_deref()
                        .is_some_and(|latest| !crate::adapters::sanity::is_name(latest))
                {
                    continue;
                }
                let missing = if current.is_none() {
                    "npm did not say which version of it is installed"
                } else {
                    "npm did not say which version of it is newest"
                };
                unreadable.push(crate::adapters::uncheckable_candidate(
                    key,
                    current
                        .filter(|current| crate::adapters::sanity::is_version(current))
                        .unwrap_or_default(),
                    UpdateChannel::Native,
                    missing.to_string(),
                ));
                continue;
            }
        };
        if !crate::adapters::sanity::is_name(&latest) {
            continue;
        }
        match (
            semver::Version::parse(&latest),
            semver::Version::parse(&current),
        ) {
            (Ok(latest_version), Ok(current_version))
                if latest_version.cmp_precedence(&current_version).is_gt() =>
            {
                out.push(UpdateCandidate {
                    key,
                    current,
                    target: latest,
                    channel: UpdateChannel::Native,
                    checkable: true,
                    warnings: Vec::new(),
                    blocked: None,
                    download_bytes: None,
                });
            }
            (Ok(_), Ok(_)) => {}
            _ => out.push(crate::adapters::uncheckable_candidate(
                key,
                current,
                UpdateChannel::Native,
                "could not compare npm versions".to_string(),
            )),
        }
    }
    let mut out = crate::adapters::sanity::candidates(out);
    out.extend(unreadable);
    out.sort_by(|a, b| a.key.name.cmp(&b.key.name));
    Ok((out, has_rows))
}

#[derive(Debug, Deserialize)]
struct SearchEntry {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

/// Parses `npm search --json --searchlimit 20 {query}`.
pub(crate) fn parse_search(
    json: &str,
    adapter_id: &str,
) -> Result<Vec<SearchHit>, crate::adapters::AdapterError> {
    let entries: Vec<SearchEntry> = serde_json::from_str(json)
        .map_err(|e| crate::adapters::AdapterError::Parse(e.to_string()))?;
    Ok(crate::adapters::sanity::hits(
        entries
            .into_iter()
            .map(|e| SearchHit {
                adapter_id: adapter_id.to_string(),
                kind: ArtifactKind::Package,
                name: e.name,
                description: e.description,
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_outdated_requires_strict_semver_increase() {
        for (current, latest, count, checkable) in [
            ("2.0.0-beta.1", "1.9.0", 0, false),
            ("2.0.0", "1.9.0", 0, false),
            ("1.0.0+local", "1.0.0+registry", 0, false),
            ("2.0.0-beta.1", "2.0.0", 1, true),
            ("unknown", "2.0.0", 1, false),
        ] {
            let json = serde_json::json!({"tool": {"current": current, "latest": latest}});
            let rows = super::parse_outdated_global(&json.to_string(), "npm").unwrap();
            assert_eq!(rows.len(), count, "{current} -> {latest}");
            if let Some(row) = rows.first() {
                assert_eq!(row.checkable, checkable);
            }
        }
    }

    use super::*;

    #[tokio::test]
    async fn regression_f01_npm_filtered_rows_are_still_a_successful_check() {
        // Rows as npm prints them (`outdated-global.json`'s shape), for the
        // two ways an installed global can be ahead of the registry: a
        // prerelease past `latest`, and a newer release installed from a
        // tarball. npm lists both (`current !== wanted`) and exits 1.
        for (name, current, latest) in [
            ("demo", "2.0.0-beta.1", "1.9.0"),
            ("@scope/tool", "3.1.0", "3.0.2"),
        ] {
            let row = serde_json::json!({ name: {
                "current": current,
                "wanted": latest,
                "latest": latest,
                "dependent": "global",
                "location": format!("/opt/homebrew/lib/node_modules/{name}"),
            }});
            let runner = Arc::new(MockRunner::new());
            runner.respond(
                vec![
                    "/opt/homebrew/bin/npm",
                    "outdated",
                    "-g",
                    "--json",
                    "--prefix",
                    "/opt/homebrew",
                ],
                CommandOutput {
                    exit_code: Some(1),
                    stdout: row.to_string(),
                    stderr_cause: Default::default(),
                    stderr: String::new(),
                    timed_out: false,
                    cancelled: false,
                },
            );
            let result = NpmAdapter::new(runner.clone())
                .check_updates(&test_instance(), &CheckOptions::default())
                .await
                .expect("valid npm rows answer the check even without upgrades");
            assert!(result.candidates.is_empty());
            assert_eq!(runner.calls().len(), 1);
        }
        // Empty/error-shaped exit 1 and a genuine nonzero failure still
        // fall back to inventory, even if the latter printed valid rows.
        for (code, stdout) in [
            (1, "{}"),
            (
                1,
                r#"{"error":{"code":"ENOTFOUND","summary":"request to https://registry.npmjs.org/demo failed, reason: getaddrinfo ENOTFOUND registry.npmjs.org","detail":"This is a problem related to network connectivity."}}"#,
            ),
            (
                2,
                r#"{"demo":{"current":"1.0.0","wanted":"2.0.0","latest":"2.0.0","dependent":"global","location":"/opt/homebrew/lib/node_modules/demo"}}"#,
            ),
        ] {
            let runner = Arc::new(MockRunner::new());
            let output = |text: &str, exit_code| CommandOutput {
                exit_code: Some(exit_code),
                stdout: text.into(),
                stderr: String::new(),
                stderr_cause: Default::default(),
                timed_out: false,
                cancelled: false,
            };
            runner.respond(
                vec![
                    "/opt/homebrew/bin/npm",
                    "outdated",
                    "-g",
                    "--json",
                    "--prefix",
                    "/opt/homebrew",
                ],
                output(stdout, code),
            );
            runner.respond(
                vec![
                    "/opt/homebrew/bin/npm",
                    "ls",
                    "-g",
                    "--depth=0",
                    "--json",
                    "--prefix",
                    "/opt/homebrew",
                ],
                output(r#"{"dependencies":{"demo":{"version":"1.0.0"}}}"#, 0),
            );
            let rows = NpmAdapter::new(runner)
                .check_updates(&test_instance(), &CheckOptions::default())
                .await
                .unwrap()
                .candidates;
            assert_eq!(rows.len(), 1);
            assert!(!rows[0].checkable);
        }
    }

    #[tokio::test]
    async fn regression_f01_npm_global_commands_pin_the_confirmed_prefix() {
        let runner = Arc::new(MockRunner::new());
        let adapter = NpmAdapter::new(runner.clone()).with_prefix_read_only_fn(|_| None);
        let mut inst = test_instance();
        inst.prefix = PathBuf::from("/tmp/confirmed prefix");
        let _ = adapter.inventory(&inst).await;
        let _ = adapter.check_updates(&inst, &CheckOptions::default()).await;
        for call in runner.calls() {
            assert!(
                call.windows(2)
                    .any(|w| w == ["--prefix", "/tmp/confirmed prefix"]),
                "{call:?}"
            );
        }
        runner.respond(
            vec!["/opt/homebrew/bin/npm", "prefix", "-g"],
            f08_npm_output("/tmp/confirmed prefix", 0),
        );
        for kind in [OpKind::Install, OpKind::Upgrade, OpKind::Uninstall] {
            let plan = adapter
                .plan(
                    &inst,
                    &OpRequest {
                        kind,
                        instance_id: inst.id.clone(),
                        artifact_kind: ArtifactKind::Package,
                        name: "demo".into(),
                    },
                )
                .await
                .unwrap();
            assert!(command_args(&plan)
                .windows(2)
                .any(|w| w == ["--prefix", "/tmp/confirmed prefix"]));
            let mut argv = vec![inst.exe_path.to_str().unwrap()];
            let args = command_args(&plan);
            argv.extend(args.iter().map(String::as_str));
            runner.respond(
                argv,
                CommandOutput {
                    exit_code: Some(0),
                    stdout: String::new(),
                    stderr: String::new(),
                    stderr_cause: Default::default(),
                    timed_out: false,
                    cancelled: false,
                },
            );
            assert_eq!(
                adapter
                    .execute(&plan, Arc::new(VecSink::new()), 1, CancellationToken::new())
                    .await
                    .unwrap(),
                Outcome::Succeeded
            );
            assert!(runner
                .calls()
                .last()
                .unwrap()
                .windows(2)
                .any(|w| w == ["--prefix", "/tmp/confirmed prefix"]));
        }
    }

    // Regressions found by `adapters/robustness.rs`.

    #[test]
    fn regression_parse_ls_global_reads_a_version_with_a_control_character_as_unknown() {
        let json = r#"{"dependencies":{"a":{"version":"1.0\n"},"":{"version":"1"}}}"#;
        let artifacts = parse_ls_global(json, "npm:/opt/homebrew/bin/npm").unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].key.name, "a");
        assert_eq!(artifacts[0].version, "");
    }

    #[test]
    fn regression_parse_outdated_global_drops_an_update_to_nothing() {
        let json = r#"{"a":{"current":"1","latest":""},"b":{"current":"1\u0000","latest":"2"},
            "c":{"current":"1","latest":"2"}}"#;
        let names: Vec<String> = parse_outdated_global(json, "npm:/opt/homebrew/bin/npm")
            .unwrap()
            .into_iter()
            .map(|c| c.key.name)
            .collect();
        assert_eq!(names, vec!["c".to_string()]);
    }

    #[test]
    fn regression_parse_search_drops_a_hit_with_no_name() {
        let json = r#"[{"name":""},{"name":"jq\u001b"},{"name":"node-jq"}]"#;
        let hits = parse_search(json, "npm").unwrap();
        let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, vec!["node-jq"]);
    }
    use crate::testing::command_args;

    #[test]
    fn parse_ls_global_matches_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
            .expect("read adapters/fixtures/npm/12.0.2/ls-global.json");
        let artifacts = parse_ls_global(&json, "npm:/opt/homebrew").expect("parse");
        assert_eq!(artifacts.len(), 6);
        let names: Vec<&str> = artifacts.iter().map(|a| a.key.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "@alisaitteke/photoshop-mcp",
                "@openai/codex",
                "corepack",
                "get-shit-done-cc",
                "npm",
                "zsxq-cli",
            ]
        );
        let npm_self = artifacts
            .iter()
            .find(|a| a.key.name == "npm")
            .expect("npm entry");
        assert_eq!(npm_self.version, "12.0.2");
        assert_eq!(npm_self.key.kind, ArtifactKind::Package);
        // npm itself is not offered for uninstalling; every other package,
        // `corepack` among them, is.
        assert_eq!(
            npm_self.uninstall_blocked,
            Some(UninstallBlocked::SourceProgram)
        );
        let blocked: Vec<&str> = artifacts
            .iter()
            .filter(|a| a.uninstall_blocked.is_some())
            .map(|a| a.key.name.as_str())
            .collect();
        assert_eq!(blocked, vec!["npm"]);
    }

    #[test]
    fn parse_outdated_global_matches_the_recorded_fixture() {
        let json =
            std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/outdated-global.json")
                .expect("read adapters/fixtures/npm/12.0.2/outdated-global.json");
        let candidates = parse_outdated_global(&json, "npm:/opt/homebrew").expect("parse");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "@alisaitteke/photoshop-mcp");
        assert_eq!(candidates[0].current, "1.7.15");
        assert_eq!(candidates[0].target, "1.7.17");
        assert_eq!(candidates[0].channel, UpdateChannel::Native);
    }

    #[test]
    fn parse_outdated_global_of_empty_stdout_is_no_updates() {
        // With nothing outdated npm prints either nothing at all or `{}`,
        // depending on version; both mean "no updates". Neither is committed
        // as a fixture, since there is nothing to record — but the parser
        // must not choke on either.
        let candidates = parse_outdated_global("", "npm:/opt/homebrew").expect("parse");
        assert!(candidates.is_empty());
        assert!(parse_outdated_global("{}", "npm:/opt/homebrew")
            .expect("parse")
            .is_empty());
    }

    #[test]
    fn parse_search_matches_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/search-jq.json")
            .expect("read adapters/fixtures/npm/12.0.2/search-jq.json");
        let hits = parse_search(&json, "npm").expect("parse");
        assert_eq!(hits.len(), 20);
        assert_eq!(hits[0].name, "jq");
        assert_eq!(
            hits[0].description.as_deref(),
            Some("Server-side jQuery wrapper for node.")
        );
        assert!(hits.iter().all(|h| h.adapter_id == "npm"));
    }

    use crate::adapters::{AdapterError, CheckOptions};
    use crate::events::VecSink;
    use crate::model::{
        ArtifactKey, CancelPolicy, OpKind, OpRequest, Outcome, ResourceLock, Warning,
    };
    use crate::runner::{CommandOutput, HostEnv, MockRunner};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    fn test_instance() -> ManagerInstance {
        ManagerInstance {
            exe_path: PathBuf::from("/opt/homebrew/bin/npm"),
            prefix: PathBuf::from("/opt/homebrew"),
            version: Some("12.0.2".to_string()),
            ..crate::testing::manager_instance("npm", "npm:/opt/homebrew")
        }
    }

    fn fake_exe(dir: &std::path::Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"#!/bin/sh\n").expect("write fake npm executable");
        path
    }

    #[tokio::test]
    async fn test_detect_finds_npm_on_path_and_resolves_its_global_prefix() {
        let dir = crate::testing::unique_temp_path("npm-detect");
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let npm_path = fake_exe(&dir, "npm");
        let npm_path_str = npm_path.to_str().expect("utf8 path");

        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![npm_path_str, "prefix", "-g"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: "/opt/homebrew\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec![npm_path_str, "--version"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: "12.0.2\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        // The writability answer is pinned like every other detect test
        // below: `/opt/homebrew` is what this mock `npm prefix -g` reports,
        // not a directory the machine running this is expected to have, and
        // asking the real filesystem about it would make the answer depend
        // on which Mac this is.
        let adapter = NpmAdapter::new(runner).with_prefix_read_only_fn(|_| None);
        let env = HostEnv {
            path_dirs: vec![dir.clone()],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let instances = adapter.detect(&env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].id, "npm:/opt/homebrew");
        assert_eq!(instances[0].version, Some("12.0.2".to_string()));
        assert!(instances[0].available());
        assert!(
            instances[0].unverified_version.is_none(),
            "12.0.2 is verified in adapters/meta/npm.toml"
        );
    }

    #[tokio::test]
    async fn test_detect_flags_an_unverified_version() {
        let dir = crate::testing::unique_temp_path("npm-detect-unverified");
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let npm_path = fake_exe(&dir, "npm");
        let npm_path_str = npm_path.to_str().expect("utf8 path");

        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![npm_path_str, "prefix", "-g"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: "/opt/homebrew\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec![npm_path_str, "--version"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: "99.9.9\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner).with_prefix_read_only_fn(|_| None);
        let env = HostEnv {
            path_dirs: vec![dir.clone()],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let instances = adapter.detect(&env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].unverified_version, Some("99.9.9".to_string()));
    }

    /// A temp directory holding a fake `npm` executable, plus a `HostEnv`
    /// whose `PATH` is exactly that directory. Returned rather than
    /// inlined because every detect test needs the same three lines and
    /// the unique-name dance around them.
    fn detect_fixture(tag: &str) -> (PathBuf, PathBuf, HostEnv) {
        let dir = std::env::temp_dir().join(format!(
            "banager-npm-detect-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let npm_path = fake_exe(&dir, "npm");
        let env = HostEnv {
            path_dirs: vec![dir.clone()],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        (dir, npm_path, env)
    }

    /// `/opt/homebrew` here is what this *mock* `npm prefix -g` answers --
    /// a realistic prefix to read, not a claim about the machine running
    /// the test. Nothing in these detect tests touches a real one: the exe
    /// lives in a temp directory and the writability probe is injected.
    fn detect_runner(npm_path: &std::path::Path) -> Arc<MockRunner> {
        let npm_path_str = npm_path.to_str().expect("utf8 path");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![npm_path_str, "prefix", "-g"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: "/opt/homebrew\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec![npm_path_str, "--version"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: "12.0.2\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner
    }

    #[tokio::test]
    async fn test_detect_reports_prefix_not_writable_when_npm_cannot_write_where_it_installs() {
        // The nodejs.org installer's npm: the CLI works, `npm prefix -g`
        // answers, but the directory it would write into belongs to root.
        // The instance must still be detected -- Banager lists what is
        // there -- and must carry the reason it cannot be changed, so the
        // Updates page can say "install Node with Homebrew instead"
        // rather than pip's "use pipx or uv".
        let (dir, npm_path, env) = detect_fixture("readonly");
        let adapter = NpmAdapter::new(detect_runner(&npm_path))
            .with_prefix_read_only_fn(|_| Some(ReadOnlyReason::PrefixNotWritable));
        let instances = adapter.detect(&env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(
            instances[0].read_only_reason,
            Some(ReadOnlyReason::PrefixNotWritable)
        );
        assert!(!instances[0].writable());
    }

    #[tokio::test]
    async fn test_detect_reports_a_prefix_in_a_protected_place_as_not_looked_into() {
        // `npm config set prefix ~/Documents/npm-global`: the folder may
        // well be the user's to write, but Banager never looks into a
        // protected place, so it cannot say. The reason says that, not
        // that the account cannot change it (decision I23).
        let (dir, npm_path, env) = detect_fixture("protected");
        let adapter = NpmAdapter::new(detect_runner(&npm_path))
            .with_prefix_read_only_fn(|_| Some(ReadOnlyReason::PrefixProtected));
        let instances = adapter.detect(&env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(
            instances[0].read_only_reason,
            Some(ReadOnlyReason::PrefixProtected)
        );
        assert!(!instances[0].writable());
    }

    #[tokio::test]
    async fn test_detect_leaves_the_instance_writable_when_the_prefix_is_writable() {
        let (dir, npm_path, env) = detect_fixture("writable");
        let adapter = NpmAdapter::new(detect_runner(&npm_path)).with_prefix_read_only_fn(|_| None);
        let instances = adapter.detect(&env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].read_only_reason, None);
        assert!(instances[0].writable());
    }

    #[tokio::test]
    async fn test_detect_asks_about_the_prefix_npm_reported_not_the_exe_directory() {
        // The writability question is about `npm prefix -g`'s answer
        // (`/opt/homebrew`), not about wherever the `npm` binary happens to
        // live -- a Homebrew npm's exe sits in `{prefix}/bin`, but a
        // volta/nvm shim's does not.
        use std::sync::Mutex;
        static ASKED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
        ASKED.lock().unwrap().clear();
        fn record(path: &Path) -> Option<ReadOnlyReason> {
            ASKED.lock().unwrap().push(path.to_path_buf());
            None
        }
        let (dir, npm_path, env) = detect_fixture("prefix-arg");
        let adapter = NpmAdapter::new(detect_runner(&npm_path)).with_prefix_read_only_fn(record);
        let instances = adapter.detect(&env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(
            *ASKED.lock().unwrap(),
            vec![PathBuf::from("/opt/homebrew")],
            "detect must ask about the reported prefix exactly once"
        );
    }

    #[tokio::test]
    async fn test_detect_returns_empty_when_npm_is_not_on_path() {
        let runner = Arc::new(MockRunner::new());
        let adapter = NpmAdapter::new(runner.clone());
        let env = HostEnv {
            path_dirs: vec![PathBuf::from("/definitely/not/a/real/path")],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let instances = adapter.detect(&env).await;
        assert!(instances.is_empty());
        assert!(
            runner.calls().is_empty(),
            "no subprocess should run when npm isn't found"
        );
    }

    #[tokio::test]
    async fn test_detect_says_why_npm_did_not_answer() {
        // Finding (1) of the 2026-10-07 run: `brew upgrade node@22` left no
        // `node` on PATH, npm's launcher (`#!/usr/bin/env node`) could not
        // start, and the window said only 「npm没有响应」. What the command
        // did is the reason: it never ran, for want of `node`. And when the
        // prefix answers but `--version` does not, that command's reason.
        use crate::model::{NoAnswer, NoAnswerKind};
        let env_said = |exit_code| CommandOutput {
            stderr_cause: Default::default(),
            exit_code: Some(exit_code),
            stdout: String::new(),
            stderr: "env: node: No such file or directory\n".to_string(),
            timed_out: false,
            cancelled: false,
        };
        let (dir, npm_path, env) = detect_fixture("no-node");
        let npm = npm_path.to_str().expect("utf8 path");
        let runner = Arc::new(MockRunner::new());
        runner.respond(vec![npm, "prefix", "-g"], env_said(127));
        let instances = NpmAdapter::new(runner).detect(&env).await;
        assert_eq!(
            instances[0].status.no_answer,
            Some(NoAnswer {
                diagnostic: Some("env: node: No such file or directory".into()),
                cause: Some(crate::history::FailureCause::NotFound),
                kind: NoAnswerKind::CouldNotStart,
                missing_program: Some("node".to_string()),
                link_fixes: Vec::new(),
            })
        );

        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![npm, "prefix", "-g"],
            CommandOutput {
                stdout: "/opt/homebrew\n".to_string(),
                stderr: String::new(),
                ..env_said(0)
            },
        );
        runner.respond(
            vec![npm, "--version"],
            CommandOutput {
                timed_out: true,
                exit_code: None,
                ..env_said(0)
            },
        );
        let instances = NpmAdapter::new(runner)
            .with_prefix_read_only_fn(|_| None)
            .detect(&env)
            .await;
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::NotResponding)
        );
        assert_eq!(
            instances[0].status.no_answer.as_ref().map(|why| why.kind),
            Some(NoAnswerKind::TimedOut)
        );
    }

    #[tokio::test]
    async fn test_detect_reports_unavailable_instead_of_vanishing_when_npm_prefix_fails() {
        // A malformed ~/.npmrc makes every npm command exit non-zero: npm
        // itself is on PATH, but `npm prefix -g` -- the very command this
        // adapter's id comes from -- cannot answer. Returning `Vec::new()`
        // here used to make npm disappear with no unavailable notice and
        // no SourceError, indistinguishable from npm not being installed
        // at all. brew/cargo/pipx/uv/ollama all report an instance marked
        // unavailable on an equivalent failure (their own `--version`);
        // npm must too.
        let (dir, npm_path, env) = detect_fixture("prefix-fails");
        let npm_path_str = npm_path.to_str().expect("utf8 path");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![npm_path_str, "prefix", "-g"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "npm error config Invalid npmrc".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner.clone());
        let instances = adapter.detect(&env).await;
        // Stable across repeated failures of this same shape: calling
        // detect again while npm is still broken must not fabricate a new
        // id each time, or refresh()'s carry-forward (keyed on the
        // instance id recurring) could never accumulate anything for it.
        let instances_again = adapter.detect(&env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::NotResponding)
        );
        assert!(!instances[0].available());
        assert_eq!(
            instances[0]
                .status
                .no_answer
                .as_ref()
                .unwrap()
                .diagnostic
                .as_deref(),
            Some("npm error config Invalid npmrc")
        );
        assert_eq!(instances[0].version, None);
        assert_eq!(instances_again.len(), 1);
        assert_eq!(instances[0].id, instances_again[0].id);
        // And it must not collide with a real `npm:{prefix}` id: no real
        // global prefix ends in `/bin/npm`.
        assert_eq!(instances[0].id, format!("npm:{}", npm_path.display()));
        assert_eq!(
            runner.calls().len(),
            2,
            "must not run --version after prefix already failed to answer, across both detect() calls"
        );
    }

    /// `npm ls -g --depth=0 --json`'s run, answered with `stdout`,
    /// `stderr` and exit `code`.
    fn ls_global_answering(code: i32, stdout: &str, stderr: &str) -> Arc<MockRunner> {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "ls",
                "-g",
                "--depth=0",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(code),
                stdout: stdout.into(),
                stderr: stderr.into(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner
    }

    /// As npm 10.9.9 answered `npm ls -g --depth=0 --json` with a prefix
    /// whose `lib` it could not look into (recorded 2026-10-07, the path
    /// shortened): exit 236 (errno -20), only an `error` object on stdout,
    /// and the same said on stderr.
    const LS_ENOTDIR_STDOUT: &str = r#"{
  "error": {
    "code": "ENOTDIR",
    "summary": "ENOTDIR: not a directory, lstat '/Users/me/prefix/lib'",
    "detail": ""
  }
}"#;
    const LS_ENOTDIR_STDERR: &str = "npm error code ENOTDIR\n\
        npm error syscall lstat\n\
        npm error path /Users/me/prefix/lib\n\
        npm error errno -20\n\
        npm error ENOTDIR: not a directory, lstat '/Users/me/prefix/lib'\n";

    #[tokio::test]
    async fn test_inventory_without_a_package_tree_is_a_failed_reading_with_npms_stderr() {
        for (code, stdout) in [
            (236, LS_ENOTDIR_STDOUT),
            // npm exits 1 for an error with no errno.
            (1, LS_ENOTDIR_STDOUT),
            (1, ""),
            (1, "not json"),
            // npm's own answer for a prefix with nothing in it, but on exit 1.
            (1, r#"{"name":"lib"}"#),
            (1, r#"{"name":"lib","dependencies":[]}"#),
        ] {
            let runner = ls_global_answering(code, stdout, LS_ENOTDIR_STDERR);
            let result = NpmAdapter::new(runner).inventory(&test_instance()).await;
            assert!(
                matches!(&result, Err(AdapterError::CommandFailed { code: Some(actual), stderr })
                    if *actual == code && stderr.starts_with("npm error code ENOTDIR")),
                "exit {code}, stdout {stdout:?}: {result:?}"
            );
        }
    }

    /// stdout is the runner's as npm wrote it (`OutputUse::Parsed`): no
    /// proxy login is masked out of it, as one is out of stderr
    /// (`CommandOutput::stdout`, runner/redact.rs). npm says a proxy's
    /// login back when it cannot read it (``Invalid protocol `user:` ``).
    #[tokio::test]
    async fn test_a_failed_readings_reason_is_never_read_off_stdout() {
        let stdout = r#"{"error":{"code":"EINVALIDPROXY","summary":"Invalid protocol `ada:` in ada:hunter22@proxy.example:8080","detail":""}}"#;
        for code in [1, 236] {
            let runner = ls_global_answering(code, stdout, "");
            let result = NpmAdapter::new(runner).inventory(&test_instance()).await;
            assert!(
                matches!(&result, Err(AdapterError::CommandFailed { code: Some(actual), .. }) if *actual == code),
                "{result:?}"
            );
            let said = result.unwrap_err().to_string();
            assert!(
                !said.contains("hunter22") && !said.contains("ada"),
                "{said}"
            );
        }
    }

    #[tokio::test]
    async fn test_inventory_reads_an_empty_prefix_and_a_tree_npm_found_problems_in() {
        // The tree npm writes when `npm ls` finds problems (npm 10.9.9,
        // recorded 2026-10-07 from a project; a global tree is the same
        // shape): the packages, `problems`, and an `error` merged in.
        let with_problems = r#"{
  "name": "lib",
  "problems": ["invalid: tool@1.0.0 /opt/homebrew/lib/node_modules/tool"],
  "dependencies": {
    "npm": {"version": "10.9.9", "overridden": false},
    "tool": {
      "version": "1.0.0",
      "overridden": false,
      "invalid": "\"^2.0.0\" from the root project",
      "problems": ["invalid: tool@1.0.0 /opt/homebrew/lib/node_modules/tool"]
    }
  },
  "error": {
    "code": "ELSPROBLEMS",
    "summary": "invalid: tool@1.0.0 /opt/homebrew/lib/node_modules/tool",
    "detail": ""
  }
}"#;
        for (code, stdout, count) in [
            // npm 10.9.9's answer for a global prefix with nothing in it.
            (0, r#"{"name":"lib"}"#, 0),
            (0, "{}", 0),
            (0, r#"{"name":"lib","dependencies":{}}"#, 0),
            (1, r#"{"name":"lib","dependencies":{}}"#, 0),
            (1, with_problems, 2),
        ] {
            let runner = ls_global_answering(code, stdout, "npm error code ELSPROBLEMS\n");
            let artifacts = NpmAdapter::new(runner)
                .inventory(&test_instance())
                .await
                .unwrap_or_else(|e| panic!("exit {code}, stdout {stdout:?}: {e}"));
            assert_eq!(artifacts.len(), count, "exit {code}, stdout {stdout:?}");
        }
    }

    #[test]
    fn test_an_error_only_answer_on_exit_zero_is_no_reading_and_names_only_npms_code() {
        let result = parse_ls_global(
            r#"{"error":{"code":"EINVALIDPROXY","summary":"Invalid protocol `ada:` in ada:hunter22@proxy.example:8080"}}"#,
            "npm:prefix",
        );
        let Err(AdapterError::Parse(reason)) = result else {
            panic!("an error-only answer is not an empty list: {result:?}");
        };
        assert!(reason.contains("EINVALIDPROXY"), "{reason}");
        assert!(
            !reason.contains("hunter22") && !reason.contains("ada"),
            "{reason}"
        );
        // A code that is not one of npm's names is not said at all.
        let result = parse_ls_global(
            r#"{"error":{"code":"ada:hunter22@proxy.example","summary":""}}"#,
            "npm:prefix",
        );
        let Err(AdapterError::Parse(reason)) = result else {
            panic!("an error-only answer is not an empty list: {result:?}");
        };
        assert!(
            !reason.contains("hunter22") && !reason.contains("ada"),
            "{reason}"
        );
    }

    #[tokio::test]
    async fn test_inventory_accepts_exit_code_1() {
        let runner = Arc::new(MockRunner::new());
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
            .expect("read fixture");
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "ls",
                "-g",
                "--depth=0",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(1),
                stdout: json,
                stderr: "npm warn config global".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
        let artifacts = adapter
            .inventory(&test_instance())
            .await
            .expect("exit 1 must still be parsed");
        assert_eq!(artifacts.len(), 6);
    }

    #[tokio::test]
    async fn test_check_updates_accepts_exit_code_1() {
        // The listing the check reads before offering an update.
        let runner = ls_global_answering(
            0,
            &std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
                .expect("read fixture"),
            "",
        );
        let json =
            std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/outdated-global.json")
                .expect("read fixture");
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "outdated",
                "-g",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(1),
                stdout: json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("exit 1 means updates were found, not a failure")
            .candidates;
        assert_eq!(candidates.len(), 1);
    }

    #[tokio::test]
    async fn r42_one_row_without_current_leaves_only_that_package_uncheckable() {
        // R42-4: npm lists a global whose version it cannot read -- an
        // `npm link` whose working copy is gone -- with no `current` key
        // (npm 10.9.9 outdated.js:159, 188, 264-271). That one row made
        // the whole answer unreadable, and every npm row "couldn't check".
        let runner = ls_global_answering(0, r#"{"name": "lib"}"#, "");
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "outdated",
                "-g",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(1),
                stdout: r#"{
                  "left-behind": {"wanted": "1.0.0", "latest": "1.0.0", "dependent": "global",
                                  "location": "/opt/homebrew/lib/node_modules/left-behind"},
                  "prettier": {"current": "3.8.1", "wanted": "3.8.2", "latest": "3.8.2",
                               "dependent": "global",
                               "location": "/opt/homebrew/lib/node_modules/prettier"}
                }"#
                .to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let candidates = NpmAdapter::new(runner)
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("npm's answer, not a failed check")
            .candidates;
        let rows: Vec<(&str, &str, &str, bool)> = candidates
            .iter()
            .map(|c| {
                (
                    c.key.name.as_str(),
                    c.current.as_str(),
                    c.target.as_str(),
                    c.checkable,
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                ("left-behind", "", "", false),
                ("prettier", "3.8.1", "3.8.2", true)
            ]
        );
    }

    #[tokio::test]
    async fn r42_a_global_npm_did_not_install_from_a_registry_is_offered_no_update() {
        // R42-3: `npm outdated -g` looks every global up on the registry by
        // name, whatever it came from, and `npm install -g <name>` would put
        // the registry's package in place of an `npm link`, a folder, a
        // tarball, a fork from git, or an alias of another package. What
        // each came from is `resolved` in `npm ls -g` (npm 10.9.9 ls.js:
        // 358-360; a link's is always `file:`, arborist link.js:94-98).
        // npm 7 and later give it for a link or a folder only; the `git+`,
        // URL and alias answers below are npm 6's.
        let runner = ls_global_answering(
            0,
            r#"{"name": "lib", "dependencies": {
              "mytool": {"version": "0.0.0-development",
                         "resolved": "file:../../../../Users/you/dev/mytool"},
              "fork": {"version": "1.0.0",
                       "resolved": "git+ssh://git@github.com/them/fork.git#0123abc"},
              "urltool": {"version": "1.0.0",
                          "resolved": "https://example.com/releases/urltool-1.0.0.tgz"},
              "alias": {"version": "1.0.0",
                        "resolved": "https://registry.npmjs.org/bar/-/bar-1.0.0.tgz"},
              "prettier": {"version": "3.8.1",
                           "resolved": "https://registry.npmjs.org/prettier/-/prettier-3.8.1.tgz"},
              "@scope/pkg": {"version": "1.0.0",
                             "resolved": "https://registry.npmmirror.com/@scope/pkg/-/pkg-1.0.0.tgz"},
              "@old/escaped": {"version": "1.0.0",
                               "resolved": "https://npm.example.com/@old%2Fescaped/-/escaped-1.0.0.tgz"},
              "unsaid": {"version": "1.0.0"}
            }}"#,
            "",
        );
        let row = |name: &str, current: &str| {
            format!(
                r#""{name}": {{"current": "{current}", "wanted": "4.2.0", "latest": "4.2.0", "dependent": "global"}}"#
            )
        };
        let outdated = format!(
            "{{{}}}",
            [
                row("mytool", "0.0.0-development"),
                row("fork", "1.0.0"),
                row("urltool", "1.0.0"),
                row("alias", "1.0.0"),
                row("prettier", "3.8.1"),
                row("@scope/pkg", "1.0.0"),
                row("@old/escaped", "1.0.0"),
                row("unsaid", "1.0.0"),
            ]
            .join(",")
        );
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "outdated",
                "-g",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(1),
                stdout: outdated,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let candidates = NpmAdapter::new(runner)
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("checked")
            .candidates;
        let names: Vec<&str> = candidates.iter().map(|c| c.key.name.as_str()).collect();
        // A registry's tarball (any registry: a mirror, a scope's escaped
        // spelling), or nothing said, keeps its update.
        assert_eq!(
            names,
            vec!["@old/escaped", "@scope/pkg", "prettier", "unsaid"]
        );
    }

    #[tokio::test]
    async fn r45_a_link_npm_reads_no_version_for_is_no_row_with_nothing_else_listed() {
        // R45-2: an `npm link` whose package.json has no "version" is in
        // `npm outdated -g` with no `current` (an uncheckable row) and in
        // `npm ls -g` with `resolved: file:...` (arborist link.js:98). It
        // got no row while another global had an update, and "Couldn't
        // check" once it was the only row: the listing was read only for
        // a checkable row.
        let runner = ls_global_answering(
            0,
            r#"{"name": "lib", "dependencies": {
              "mytool": {"resolved": "file:../../../../Users/you/dev/mytool"}
            }}"#,
            "",
        );
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "outdated",
                "-g",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(1),
                stdout: r#"{"mytool": {"wanted": "1.0.0", "latest": "1.0.0", "dependent": "global",
                                       "location": "/opt/homebrew/lib/node_modules/mytool"}}"#
                    .to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let candidates = NpmAdapter::new(runner)
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("checked")
            .candidates;
        assert!(candidates.is_empty(), "{candidates:?}");
    }

    #[tokio::test]
    async fn r42_with_nothing_to_offer_the_check_lists_no_packages() {
        // The listing is read only for an update the check would offer.
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "outdated",
                "-g",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let found = NpmAdapter::new(runner.clone())
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("checked")
            .candidates;
        assert!(found.is_empty());
        assert_eq!(runner.calls().len(), 1);
    }

    #[test]
    fn r42_an_answer_with_neither_version_is_no_row() {
        // npm's error answer, `{"error": {...}}`, is no package named
        // "error": exit 1 with it alone is still a check that failed.
        let json = r#"{"error": {"code": "E404", "summary": "Not found", "detail": ""}}"#;
        let (found, has_rows) = parse_outdated_result(json, "npm:/opt/homebrew").unwrap();
        assert!(found.is_empty());
        assert!(!has_rows);
    }

    /// A prefix as the author's was before 2026-10-07: `node@22` linked by
    /// hand, so `<prefix>/bin/npm` is Homebrew's link into the keg's own
    /// npm. `own` false: npm's own copy, as after `npm install -g npm`.
    fn prefix_with_npm(tag: &str, own: bool) -> PathBuf {
        use std::os::unix::fs::symlink;
        let root = crate::testing::unique_temp_path(&format!("npm-formula-{tag}"));
        let keg = root.join("Cellar/node@22/22.23.3_1");
        std::fs::create_dir_all(keg.join("lib/node_modules/npm/bin")).unwrap();
        std::fs::write(keg.join("lib/node_modules/npm/bin/npm-cli.js"), b"").unwrap();
        std::fs::create_dir_all(keg.join("bin")).unwrap();
        symlink(
            "../lib/node_modules/npm/bin/npm-cli.js",
            keg.join("bin/npm"),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("lib/node_modules/npm/bin")).unwrap();
        std::fs::write(root.join("lib/node_modules/npm/bin/npm-cli.js"), b"").unwrap();
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let target = if own {
            "../lib/node_modules/npm/bin/npm-cli.js"
        } else {
            "../Cellar/node@22/22.23.3_1/bin/npm"
        };
        symlink(target, root.join("bin/npm")).unwrap();
        root
    }

    #[test]
    fn r42_a_corepack_a_homebrew_formula_linked_is_that_formulas() {
        // R42-2: `node@22` linked by hand puts its own `bin/corepack` in
        // the prefix as well, a link into its keg.
        use std::os::unix::fs::symlink;
        let formulas = prefix_with_npm("corepack", false);
        let keg = formulas.join("Cellar/node@22/22.23.3_1");
        std::fs::create_dir_all(keg.join("lib/node_modules/corepack/dist")).unwrap();
        std::fs::write(keg.join("lib/node_modules/corepack/dist/corepack.js"), b"").unwrap();
        symlink(
            "../lib/node_modules/corepack/dist/corepack.js",
            keg.join("bin/corepack"),
        )
        .unwrap();
        symlink(
            "../Cellar/node@22/22.23.3_1/bin/corepack",
            formulas.join("bin/corepack"),
        )
        .unwrap();
        // npm's own copy, after `npm install -g corepack`.
        let own = prefix_with_npm("corepack-own", true);
        std::fs::create_dir_all(own.join("lib/node_modules/corepack/dist")).unwrap();
        std::fs::write(own.join("lib/node_modules/corepack/dist/corepack.js"), b"").unwrap();
        symlink(
            "../lib/node_modules/corepack/dist/corepack.js",
            own.join("bin/corepack"),
        )
        .unwrap();
        let found = (
            real_comes_with_formula(&formulas, "corepack"),
            real_comes_with_formula(&own, "corepack"),
            real_comes_with_formula(&own, "npm"),
        );
        let _ = std::fs::remove_dir_all(&formulas);
        let _ = std::fs::remove_dir_all(&own);
        assert_eq!(found, (true, false, false));
    }

    #[tokio::test]
    async fn r42_corepack_that_comes_with_a_formula_is_neither_updated_nor_uninstalled() {
        // R42-2: updating or uninstalling corepack takes away the formula's
        // `bin/corepack` link and, with Homebrew's pnpm or yarn installed,
        // theirs (corepack declares pnpm, pnpx, yarn and yarnpkg).
        let runner = ls_global_answering(
            0,
            r#"{"name": "lib", "dependencies": {
              "corepack": {"version": "0.34.0"}, "npm": {"version": "10.9.9"},
              "prettier": {"version": "3.8.1"}}}"#,
            "",
        );
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "outdated",
                "-g",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(1),
                stdout: r#"{
                  "corepack": {"current": "0.34.0", "wanted": "0.34.5", "latest": "0.34.5"},
                  "npm": {"current": "10.9.9", "wanted": "10.9.9", "latest": "10.9.10"},
                  "prettier": {"current": "3.8.1", "wanted": "3.8.2", "latest": "3.8.2"}
                }"#
                .to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let inst = test_instance();
        let request = |kind: OpKind, name: &str| OpRequest {
            kind,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: name.to_string(),
        };
        // Only corepack's link leads into the Cellar here.
        let formulas = NpmAdapter::new(runner.clone())
            .with_prefix_read_only_fn(|_| None)
            .with_comes_with_formula_fn(|_, command| command == "corepack");
        let blocked: Vec<(String, Option<UpdateBlocked>)> = formulas
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("checked")
            .candidates
            .into_iter()
            .map(|candidate| (candidate.key.name, candidate.blocked))
            .collect();
        assert_eq!(
            blocked,
            vec![
                (
                    "corepack".to_string(),
                    Some(UpdateBlocked::UpdatesWithFormula)
                ),
                ("npm".to_string(), None),
                ("prettier".to_string(), None),
            ]
        );
        let uninstall_blocked: Vec<(String, Option<UninstallBlocked>)> = formulas
            .inventory(&inst)
            .await
            .expect("listed")
            .into_iter()
            .map(|artifact| (artifact.key.name, artifact.uninstall_blocked))
            .collect();
        assert_eq!(
            uninstall_blocked,
            vec![
                (
                    "corepack".to_string(),
                    Some(UninstallBlocked::ComesWithFormula)
                ),
                ("npm".to_string(), Some(UninstallBlocked::SourceProgram)),
                ("prettier".to_string(), None),
            ]
        );
        match formulas
            .plan(&inst, &request(OpKind::Upgrade, "corepack"))
            .await
        {
            Err(AdapterError::UpdateBlocked { reason }) => {
                assert_eq!(reason, UpdateBlocked::UpdatesWithFormula)
            }
            other => panic!("expected UpdateBlocked, got {other:?}"),
        }
        match formulas
            .plan(&inst, &request(OpKind::Uninstall, "corepack"))
            .await
        {
            Err(AdapterError::UninstallBlocked { reason }) => {
                assert_eq!(reason, UninstallBlocked::ComesWithFormula)
            }
            other => panic!("expected UninstallBlocked, got {other:?}"),
        }
        // npm's own corepack, after `npm install -g corepack`, is any
        // package's.
        let own = NpmAdapter::new(runner)
            .with_prefix_read_only_fn(|_| None)
            .with_comes_with_formula_fn(|_, _| false);
        for kind in [OpKind::Upgrade, OpKind::Uninstall] {
            own.plan(&inst, &request(kind, "corepack"))
                .await
                .expect("planned");
        }
        let corepack = own
            .inventory(&inst)
            .await
            .expect("listed")
            .into_iter()
            .find(|artifact| artifact.key.name == "corepack")
            .expect("corepack");
        assert_eq!(corepack.uninstall_blocked, None);
    }

    #[test]
    fn test_an_npm_a_homebrew_formula_linked_is_that_formulas() {
        // Finding (2) of the y2-npmwhy review: `npm install -g npm@latest`
        // in a prefix whose `bin/npm` is Homebrew's link into `node@22`'s
        // keg replaces that link, and the next `brew upgrade node@22`
        // cannot link the new version over it.
        let formulas = prefix_with_npm("formulas", false);
        let own = prefix_with_npm("own", true);
        let elsewhere = crate::testing::unique_temp_path("npm-formula-none");
        let found = (
            real_comes_with_formula(&formulas, "npm"),
            real_comes_with_formula(&own, "npm"),
            real_comes_with_formula(&elsewhere, "npm"),
        );
        let _ = std::fs::remove_dir_all(&formulas);
        let _ = std::fs::remove_dir_all(&own);
        assert_eq!(found, (true, false, false));
    }

    #[tokio::test]
    async fn test_npm_that_comes_with_a_homebrew_formula_is_not_offered_its_own_update() {
        let runner = ls_global_answering(0, r#"{"name": "lib"}"#, "");
        let json = r#"{
            "npm": {"current": "10.9.9", "wanted": "10.9.9", "latest": "12.2.0", "dependent": "global", "location": "/opt/homebrew/lib/node_modules/npm"},
            "prettier": {"current": "3.8.1", "wanted": "3.8.2", "latest": "3.8.2", "dependent": "global", "location": "/opt/homebrew/lib/node_modules/prettier"}
        }"#;
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "outdated",
                "-g",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(1),
                stdout: json.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        async fn blocked(adapter: &NpmAdapter) -> Vec<(String, Option<UpdateBlocked>)> {
            let mut found: Vec<(String, Option<UpdateBlocked>)> = adapter
                .check_updates(&test_instance(), &CheckOptions::default())
                .await
                .expect("checked")
                .candidates
                .into_iter()
                .map(|candidate| (candidate.key.name, candidate.blocked))
                .collect();
            found.sort_by(|a, b| a.0.cmp(&b.0));
            found
        }
        let formulas = NpmAdapter::new(runner.clone()).with_comes_with_formula_fn(|_, _| true);
        assert_eq!(
            blocked(&formulas).await,
            vec![
                ("npm".to_string(), Some(UpdateBlocked::UpdatesWithFormula)),
                ("prettier".to_string(), None),
            ]
        );
        let own = NpmAdapter::new(runner).with_comes_with_formula_fn(|_, _| false);
        assert_eq!(
            blocked(&own).await,
            vec![("npm".to_string(), None), ("prettier".to_string(), None)]
        );

        // The gate's late twin: planned from a page that is out of date.
        let inst = test_instance();
        let upgrade = |name: &str| OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: name.to_string(),
        };
        let formulas = NpmAdapter::new(Arc::new(MockRunner::new()))
            .with_prefix_read_only_fn(|_| None)
            .with_comes_with_formula_fn(|_, _| true);
        match formulas.plan(&inst, &upgrade("npm")).await {
            Err(AdapterError::UpdateBlocked { reason }) => {
                assert_eq!(reason, UpdateBlocked::UpdatesWithFormula)
            }
            other => panic!("expected UpdateBlocked, got {other:?}"),
        }
        formulas
            .plan(&inst, &upgrade("prettier"))
            .await
            .expect("every other package is updated as before");
    }

    #[tokio::test]
    async fn test_check_updates_does_not_call_a_failed_registry_lookup_up_to_date() {
        // `npm outdated -g --json` exits 1 *when it finds updates*, which is
        // why exit 1 is accepted at all. A registry that could not be
        // reached also exits non-zero -- with nothing on stdout and the
        // reason on stderr -- and reading that as an empty result told the
        // user "Everything is up to date" about packages Banager had not
        // managed to ask about. It is the one adapter whose failure mode was
        // indistinguishable from good news.
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/npm", "outdated", "-g", "--json", "--prefix", "/opt/homebrew"],
            CommandOutput { stderr_cause: Default::default(),
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "npm error code ENOTFOUND\nnpm error network request to https://registry.npmjs.org failed".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let ls = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
            .expect("read fixture");
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "ls",
                "-g",
                "--depth=0",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: ls,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("a registry that did not answer is not a source failure")
            .candidates;
        assert_eq!(
            candidates.len(),
            6,
            "one row per installed package, so the page cannot claim to know"
        );
        assert!(candidates.iter().all(|c| !c.checkable));
        let reason = candidates[0]
            .warnings
            .iter()
            .find_map(|w| match w {
                Warning::Message(m) => Some(m.clone()),
                _ => None,
            })
            .expect("the row carries why");
        assert!(
            reason.contains("ENOTFOUND"),
            "the reason the user reads has to be npm's own: {reason}"
        );
        // npm's words name the network: every row says a later check can
        // get past it, so the lists count them as not checked this time.
        assert!(candidates
            .iter()
            .all(|c| c.warnings.contains(&Warning::TransientLookupFailure)));
    }

    #[tokio::test]
    async fn test_check_updates_still_errors_when_npm_itself_could_not_be_run() {
        // The other half of the same rule: `Err` is reserved for "the tool
        // could not be run at all". Nothing is mocked, so the runner reports
        // no such command -- that must not turn into a page full of
        // "can't check" rows built from an inventory nobody could read.
        let adapter = NpmAdapter::new(Arc::new(MockRunner::new()));
        let result = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_search_matches_the_recorded_fixture_end_to_end() {
        let runner = Arc::new(MockRunner::new());
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/search-jq.json")
            .expect("read fixture");
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "search",
                "--json",
                "--searchlimit",
                "20",
                "jq",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
        let hits = adapter
            .search(&test_instance(), "jq")
            .await
            .expect("search");
        assert_eq!(hits.len(), 20);
        assert_eq!(hits[0].name, "jq");
    }

    #[tokio::test]
    async fn test_plan_install() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_read_only_fn(|_| None);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(
            command_args(&plan),
            vec!["install", "-g", "jq", "--prefix", "/opt/homebrew"]
        );
        assert!(!plan.needs_password);
        assert_eq!(plan.locks[0], ResourceLock(inst.id.clone()));
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);
    }

    /// y1-keg review: npm's global prefix is Homebrew's own when npm came
    /// with a Node from Homebrew, and `npm install -g npm@latest` there
    /// rewrites `bin/npm` -- the very place `brew upgrade node@22` unlinks
    /// and links again. Each npm operation takes the lock of a Homebrew at
    /// that prefix too, so the two never run at the same time.
    #[tokio::test]
    async fn test_every_plan_takes_the_lock_of_a_homebrew_at_its_prefix() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_read_only_fn(|_| None);
        let inst = ManagerInstance {
            prefix: PathBuf::from("/opt/homebrew"),
            ..test_instance()
        };
        for kind in [OpKind::Install, OpKind::Upgrade, OpKind::Uninstall] {
            let req = OpRequest {
                kind,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Package,
                name: "typescript".to_string(),
            };
            let plan = adapter.plan(&inst, &req).await.expect("plan");
            assert_eq!(
                plan.locks,
                vec![
                    ResourceLock(inst.id.clone()),
                    ResourceLock("brew:/opt/homebrew".to_string()),
                ],
                "{kind:?}"
            );
            // And a refresh reads this npm under the same two (r37 F2), so
            // it asks it nothing while `brew` relinks the prefix.
            assert_eq!(
                crate::adapters::Adapter::refresh_locks(&adapter, &inst),
                plan.locks,
                "{kind:?}"
            );
            // Its update check, which asks the registry, under its own
            // alone: a Homebrew operation waits for the listing only.
            assert_eq!(
                crate::adapters::Adapter::check_locks(&adapter, &inst),
                vec![ResourceLock(inst.id.clone())],
                "{kind:?}"
            );
        }
    }

    #[tokio::test]
    async fn test_f12_npm_keeps_prefix_spelling_in_argv_with_shared_lock() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_read_only_fn(|_| None);
        let inst = ManagerInstance {
            prefix: PathBuf::from("/opt/Homebrew"),
            ..test_instance()
        };
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "typescript".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.unwrap();
        assert_eq!(
            plan.locks[1],
            ResourceLock("brew:/opt/homebrew".to_string())
        );
        assert_eq!(
            command_args(&plan),
            vec!["install", "-g", "typescript", "--prefix", "/opt/Homebrew"]
        );
        assert_eq!(inst.prefix, PathBuf::from("/opt/Homebrew"));
    }

    #[tokio::test]
    async fn test_plan_uninstall() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_read_only_fn(|_| None);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(
            command_args(&plan),
            vec!["uninstall", "-g", "jq", "--prefix", "/opt/homebrew"]
        );
    }

    #[tokio::test]
    async fn test_npm_itself_is_never_planned_for_uninstalling_but_is_updated_as_any_package() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_read_only_fn(|_| None);
        let inst = test_instance();
        let request = |kind: OpKind, name: &str| OpRequest {
            kind,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: name.to_string(),
        };
        match adapter
            .plan(&inst, &request(OpKind::Uninstall, "npm"))
            .await
        {
            Err(AdapterError::UninstallBlocked { reason }) => {
                assert_eq!(reason, UninstallBlocked::SourceProgram)
            }
            other => panic!("expected UninstallBlocked(SourceProgram), got {other:?}"),
        }
        let upgrade = adapter
            .plan(&inst, &request(OpKind::Upgrade, "npm"))
            .await
            .expect("an update of npm is planned");
        assert_eq!(
            command_args(&upgrade),
            vec!["install", "-g", "npm@*", "--prefix", "/opt/homebrew"]
        );
        // Nor is a package merely named like it, or npm's other bundled one.
        for name in ["npm-check-updates", "@scope/npm", "corepack"] {
            let plan = adapter
                .plan(&inst, &request(OpKind::Uninstall, name))
                .await
                .expect("planned");
            assert_eq!(
                command_args(&plan),
                vec!["uninstall", "-g", name, "--prefix", "/opt/homebrew"]
            );
        }
        // A prefix that cannot be written is the bigger news.
        let read_only = NpmAdapter::new(Arc::new(MockRunner::new()))
            .with_prefix_read_only_fn(|_| Some(ReadOnlyReason::PrefixNotWritable));
        assert!(matches!(
            read_only
                .plan(&inst, &request(OpKind::Uninstall, "npm"))
                .await,
            Err(AdapterError::NotActionable { .. })
        ));
    }

    fn uninstall_jq(inst: &ManagerInstance) -> OpRequest {
        OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        }
    }

    #[tokio::test]
    async fn test_an_uninstall_by_npm_7_or_later_says_what_goes_and_what_stays() {
        // npm 7 and later run none of a package's scripts on `uninstall -g`,
        // so its settings and data outside its folder stay. The version is
        // the one `detect` read from `npm --version`.
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_read_only_fn(|_| None);
        for version in ["7.0.0", "10.9.9", "12.0.2", " 12.0.2\n"] {
            let inst = ManagerInstance {
                version: Some(version.to_string()),
                ..test_instance()
            };
            let plan = adapter
                .plan(&inst, &uninstall_jq(&inst))
                .await
                .expect("plan");
            assert_eq!(
                plan.warnings,
                vec![Warning::UninstallScope {
                    what: UninstallScope::Npm
                }],
                "{version:?}"
            );
        }
    }

    #[tokio::test]
    async fn test_an_uninstall_by_npm_6_or_of_an_unread_version_says_nothing_of_what_stays() {
        // npm 6 ran the package's `preuninstall`, `uninstall` and
        // `postuninstall` scripts, which could do anything; a version
        // Banager could not read might be that.
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_read_only_fn(|_| None);
        for version in [
            Some("6.14.18"),
            Some("5.6.0"),
            Some(""),
            Some("v-next"),
            None,
        ] {
            let inst = ManagerInstance {
                version: version.map(str::to_string),
                ..test_instance()
            };
            let plan = adapter
                .plan(&inst, &uninstall_jq(&inst))
                .await
                .expect("plan");
            assert!(plan.warnings.is_empty(), "{version:?}: {:?}", plan.warnings);
        }
    }

    #[tokio::test]
    async fn test_no_install_or_upgrade_plan_says_what_an_uninstall_would() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_read_only_fn(|_| None);
        let inst = test_instance();
        for kind in [OpKind::Install, OpKind::Upgrade] {
            let req = OpRequest {
                kind,
                ..uninstall_jq(&inst)
            };
            let plan = adapter.plan(&inst, &req).await.expect("plan");
            assert!(plan.warnings.is_empty(), "{kind:?}: {:?}", plan.warnings);
        }
    }

    #[tokio::test]
    async fn r42_an_update_asks_for_the_version_npm_outdated_offered_not_the_latest_tag() {
        // R42-1: `npm outdated -g` offers the newest version whose engines
        // take the running Node and that is not deprecated (`*`, npm
        // 10.9.9 outdated.js:186-187); `<name>@latest` takes the `latest`
        // tag whatever it needs, so on an older Node it installed a
        // version the row never showed, and npm's own update failed with
        // EBADENGINE every time. `<name>@*` is resolved as the check
        // resolved it. A bare name is `*` only from npm 9 (npm-package-arg
        // 10); npm 7 and 8 read it as the `latest` tag, though their
        // `outdated` picks with `*` too (npm 8.19.4 outdated.js), so the
        // range is spelled out (skeptic's problem 1).
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_read_only_fn(|_| None);
        let inst = test_instance();
        for name in ["jq", "npm", "@scope/tool"] {
            let req = OpRequest {
                kind: OpKind::Upgrade,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Package,
                name: name.to_string(),
            };
            let plan = adapter.plan(&inst, &req).await.expect("plan");
            assert_eq!(
                command_args(&plan),
                vec![
                    "install",
                    "-g",
                    &format!("{name}@*"),
                    "--prefix",
                    "/opt/homebrew"
                ]
            );
        }
    }

    #[tokio::test]
    async fn test_plan_is_refused_when_the_prefix_is_not_writable() {
        let adapter = NpmAdapter::new(Arc::new(MockRunner::new()))
            .with_prefix_read_only_fn(|_| Some(ReadOnlyReason::PrefixNotWritable));
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        let result = adapter.plan(&inst, &req).await;
        match result {
            Err(AdapterError::NotActionable {
                read_only: Some(ReadOnlyReason::PrefixNotWritable),
                unavailable: None,
            }) => {}
            other => panic!("expected NotActionable(PrefixNotWritable), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_plan_is_refused_with_the_protected_place_as_its_reason() {
        let adapter = NpmAdapter::new(Arc::new(MockRunner::new()))
            .with_prefix_read_only_fn(|_| Some(ReadOnlyReason::PrefixProtected));
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        match adapter.plan(&inst, &req).await {
            Err(AdapterError::NotActionable {
                read_only: Some(ReadOnlyReason::PrefixProtected),
                unavailable: None,
            }) => {}
            other => panic!("expected NotActionable(PrefixProtected), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_execute_streams_log_events_and_succeeds() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "install",
                "-g",
                "jq",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: "added 1 package\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec!["/opt/homebrew/bin/npm", "prefix", "-g"],
            f08_npm_output("/opt/homebrew", 0),
        );
        let adapter = NpmAdapter::new(runner).with_prefix_read_only_fn(|_| None);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome = adapter
            .execute(&plan, sink.clone(), 1, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(sink.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_and_absent() {
        let runner = Arc::new(MockRunner::new());
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
            .expect("read fixture");
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "ls",
                "-g",
                "--depth=0",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
        let inst = test_instance();
        let present = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Package,
                    name: "npm".to_string(),
                },
            )
            .await
            .expect("reconcile present");
        assert!(present.present);
        assert_eq!(present.version, Some("12.0.2".to_string()));

        let absent = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Package,
                    name: "does-not-exist".to_string(),
                },
            )
            .await
            .expect("reconcile absent");
        assert!(!absent.present);
    }

    #[tokio::test]
    async fn test_reconcile_ignores_an_artifact_of_a_different_kind() {
        // Every npm artifact is a `Package` today, so this can only fail
        // if npm's match rule stops looking at `kind` at all -- which is
        // exactly the drift `reconcile_from` exists to prevent.
        let runner = Arc::new(MockRunner::new());
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
            .expect("read fixture");
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "ls",
                "-g",
                "--depth=0",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
        let inst = test_instance();
        let wrong_kind = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Tool,
                    name: "npm".to_string(),
                },
            )
            .await
            .expect("reconcile wrong kind");
        assert!(
            !wrong_kind.present,
            "a key of another kind must not match an installed package by name alone"
        );
    }

    #[test]
    fn test_validate_search_query_rejects_a_flag_and_accepts_free_text() {
        // A search box takes free text, so a multi-word query must pass
        // where `validate_package_name` would reject it as an invalid name.
        assert!(validate_search_query("json parser").is_ok());
        assert!(validate_search_query("jq").is_ok());
        // Anything that would land in argv's flag namespace, or that is not
        // a query at all, is refused before it reaches npm.
        assert!(validate_search_query("--registry=http://evil.invalid").is_err());
        assert!(validate_search_query("   ").is_err());
        assert!(validate_search_query(&"x".repeat(201)).is_err());
    }

    fn scratch_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "banager-npm-prefix-writable-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    /// `set_mode` only strips permission bits; it never has to widen them
    /// back for cleanup here because an empty, no-write directory can still
    /// be unlinked by its (writable) parent.
    fn make_read_only(dir: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(dir).expect("stat").permissions();
        perms.set_mode(0o555);
        std::fs::set_permissions(dir, perms).expect("chmod read-only");
    }

    #[test]
    fn real_prefix_read_only_tests_node_modules_itself_not_just_the_prefix_root() {
        // `{prefix}/lib/node_modules` exists but is not writable, even though
        // the prefix root (which we just created and own) is. npm writes
        // into node_modules directly here, so this must report false — a
        // regression to "test prefix root only" would wrongly report true.
        let prefix = scratch_dir("existing-node-modules-read-only");
        let node_modules = prefix.join("lib").join("node_modules");
        std::fs::create_dir_all(&node_modules).expect("create node_modules");
        make_read_only(&node_modules);

        assert_eq!(
            real_prefix_read_only(&prefix),
            Some(ReadOnlyReason::PrefixNotWritable),
            "node_modules itself is read-only, so npm cannot write packages into it"
        );

        let _ = std::fs::remove_dir_all(&prefix);
    }

    #[test]
    fn real_prefix_read_only_walks_up_to_lib_when_node_modules_does_not_exist_yet() {
        // First global install on this prefix: node_modules hasn't been
        // created yet, but its parent (`lib`) exists and is writable, so npm
        // can create node_modules when it needs to.
        let prefix = scratch_dir("missing-node-modules-writable-lib");
        let lib_dir = prefix.join("lib");
        std::fs::create_dir_all(&lib_dir).expect("create lib");

        assert_eq!(real_prefix_read_only(&prefix), None);

        let _ = std::fs::remove_dir_all(&prefix);
    }

    #[test]
    fn real_prefix_read_only_walks_up_to_lib_and_finds_it_unwritable() {
        // Same as above, but `lib` itself cannot be written to, so npm could
        // not create node_modules inside it even though the prefix root can
        // be written to.
        let prefix = scratch_dir("missing-node-modules-read-only-lib");
        let lib_dir = prefix.join("lib");
        std::fs::create_dir_all(&lib_dir).expect("create lib");
        make_read_only(&lib_dir);

        assert_eq!(
            real_prefix_read_only(&prefix),
            Some(ReadOnlyReason::PrefixNotWritable)
        );

        let _ = std::fs::remove_dir_all(&prefix);
    }

    #[test]
    fn real_prefix_read_only_never_looks_into_a_protected_place_and_says_so() {
        // A prefix kept in `~/Documents`, by its own path or through a
        // link (`npm config set prefix ~/.npm-global`, that folder synced
        // into Documents): never looked at, so read-only here, though it
        // could be written -- and the reason is that it was not looked
        // into, not that the account cannot change it (decision I23).
        let home = std::fs::canonicalize(scratch_dir("prefix-in-documents")).unwrap();
        let kept = home.join("Documents/npm-global");
        std::fs::create_dir_all(kept.join("lib/node_modules")).expect("create prefix");
        let linked = home.join(".npm-global");
        std::os::unix::fs::symlink(&kept, &linked).expect("link");
        assert_eq!(
            real_prefix_read_only(&linked),
            None,
            "writable where nothing is protected"
        );
        let as_if = crate::protected::as_if_home(&home);
        assert_eq!(
            real_prefix_read_only(&kept),
            Some(ReadOnlyReason::PrefixProtected)
        );
        assert_eq!(
            real_prefix_read_only(&linked),
            Some(ReadOnlyReason::PrefixProtected)
        );
        drop(as_if);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn real_prefix_read_only_falls_back_to_the_prefix_root_when_lib_is_also_missing() {
        // Neither `lib` nor `lib/node_modules` exist yet; the nearest
        // existing ancestor is the prefix root itself.
        let prefix = scratch_dir("missing-lib-entirely");

        assert_eq!(real_prefix_read_only(&prefix), None);

        let _ = std::fs::remove_dir_all(&prefix);
    }
    fn f08_npm_output(stdout: &str, exit: i32) -> CommandOutput {
        CommandOutput {
            stderr_cause: Default::default(),
            exit_code: Some(exit),
            stdout: stdout.into(),
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        }
    }
    const F08_FATAL: &str =
        r#"{"error":{"code":"EJSONPARSE","summary":"Failed to parse","detail":""}}"#;
    const F08_TREE: &str = r#"{"dependencies":{"jq":{"version":"1.0.0"}}}"#;

    #[tokio::test]
    async fn f08_g03_fatal_exit_one_error_is_not_empty_inventory() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "ls",
                "-g",
                "--depth=0",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            f08_npm_output(F08_FATAL, 1),
        );
        let result = NpmAdapter::new(runner).inventory(&test_instance()).await;
        assert!(
            matches!(
                result,
                Err(AdapterError::Parse(_)) | Err(AdapterError::CommandFailed { .. })
            ),
            "fatal npm JSON is not a successful empty inventory: {result:?}"
        );
    }

    #[tokio::test]
    async fn f08_g03_usable_elsproblems_tree_retains_dependency_rows() {
        let runner = Arc::new(MockRunner::new());
        let tree = r#"{"problems":["invalid: jq@1.0.0"],"dependencies":{"jq":{"version":"1.0.0","invalid":"^2.0.0","problems":["invalid: jq@1.0.0"]}},"error":{"code":"ELSPROBLEMS","summary":"invalid: jq@1.0.0","detail":""}}"#;
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "ls",
                "-g",
                "--depth=0",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            f08_npm_output(tree, 1),
        );
        let rows = NpmAdapter::new(runner)
            .inventory(&test_instance())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key.name, "jq");
        assert_eq!(rows[0].version, "1.0.0");
    }

    #[tokio::test]
    async fn f08_g03_fatal_inventory_cannot_confirm_an_uninstall() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "ls",
                "-g",
                "--depth=0",
                "--json",
                "--prefix",
                "/opt/homebrew",
            ],
            f08_npm_output(F08_FATAL, 1),
        );
        let inst = test_instance();
        let result = NpmAdapter::new(runner)
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Package,
                    name: "jq".into(),
                },
            )
            .await;
        assert!(
            result.is_err(),
            "fatal inventory cannot assert the tool is absent: {result:?}"
        );
    }

    #[tokio::test]
    async fn f08_g03_session_keeps_inventory_after_fatal_npm_json() {
        let dir = tempfile::tempdir().unwrap();
        let exe = fake_exe(dir.path(), "npm");
        let npm = exe.to_str().unwrap();
        let prefix = dir.path().to_str().unwrap();
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![npm, "prefix", "-g"],
            f08_npm_output(dir.path().to_str().unwrap(), 0),
        );
        runner.respond(vec![npm, "--version"], f08_npm_output("12.0.2", 0));
        runner.respond(
            vec![npm, "outdated", "-g", "--json", "--prefix", prefix],
            f08_npm_output("{}", 0),
        );
        runner.respond(
            vec![npm, "ls", "-g", "--depth=0", "--json", "--prefix", prefix],
            f08_npm_output(F08_TREE, 0),
        );
        let adapter = Arc::new(NpmAdapter::new(runner.clone()).with_prefix_read_only_fn(|_| None));
        let session =
            crate::session::Session::with_adapters(Arc::new(VecSink::new()), vec![adapter], None);
        let env = HostEnv {
            path_dirs: vec![dir.path().to_owned()],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let initial = session.refresh(&env, &CheckOptions::default()).await;
        assert_eq!(
            initial.artifacts.len(),
            1,
            "precondition: previous inventory was actually read"
        );
        runner.respond(
            vec![npm, "ls", "-g", "--depth=0", "--json", "--prefix", prefix],
            f08_npm_output(F08_FATAL, 1),
        );
        let next = session.refresh(&env, &CheckOptions::default()).await;
        assert_eq!(
            next.artifacts, initial.artifacts,
            "retain previously known tools on failed inventory"
        );
        assert!(!next.errors.is_empty(), "report inventory failure");
    }

    // A fake npm with two real test-owned roots. It applies writes only to
    // its in-memory package sets, choosing the destination exactly from the
    // dispatched --prefix / npm_config_prefix, or the current config.
    struct F08PrefixNpm {
        current: std::sync::Mutex<PathBuf>,
        installed: std::sync::Mutex<std::collections::HashSet<PathBuf>>,
        writes: std::sync::Mutex<Vec<PathBuf>>,
        write_specs: std::sync::Mutex<Vec<CommandSpec>>,
        prefix_reply: std::sync::Mutex<Option<CommandOutput>>,
        switch_after_prefix: std::sync::Mutex<Option<PathBuf>>,
    }
    #[async_trait]
    impl CommandRunner for F08PrefixNpm {
        async fn run(
            &self,
            spec: CommandSpec,
            _: Option<crate::runner::LineCallback>,
            _: CancellationToken,
        ) -> Result<CommandOutput, crate::runner::RunnerError> {
            let root = spec
                .args
                .windows(2)
                .find(|pair| pair[0] == "--prefix")
                .map(|pair| PathBuf::from(&pair[1]))
                .or_else(|| {
                    spec.args
                        .iter()
                        .find_map(|arg| arg.strip_prefix("--prefix=").map(PathBuf::from))
                })
                .or_else(|| {
                    spec.env
                        .iter()
                        .find(|(key, _)| key.eq_ignore_ascii_case("npm_config_prefix"))
                        .map(|(_, value)| PathBuf::from(value))
                })
                .unwrap_or_else(|| self.current.lock().unwrap().clone());
            let command = spec
                .args
                .iter()
                .find(|arg| matches!(arg.as_str(), "prefix" | "--version" | "ls" | "uninstall"))
                .expect("a supported fake npm command");
            let output = match command.as_str() {
                "prefix" => {
                    if let Some(reply) = self.prefix_reply.lock().unwrap().clone() {
                        return Ok(reply);
                    }
                    if let Some(next) = self.switch_after_prefix.lock().unwrap().take() {
                        *self.current.lock().unwrap() = next;
                    }
                    root.to_str().unwrap().to_owned()
                }
                "--version" => "12.0.2".into(),
                "ls" => if self.installed.lock().unwrap().contains(&root) {
                    F08_TREE
                } else {
                    "{}"
                }
                .into(),
                "uninstall" => {
                    self.writes.lock().unwrap().push(root.clone());
                    self.write_specs.lock().unwrap().push(spec.clone());
                    self.installed.lock().unwrap().remove(&root);
                    String::new()
                }
                other => panic!("unexpected fake npm command: {other}: {spec:?}"),
            };
            Ok(f08_npm_output(&output, 0))
        }
    }

    async fn f08_prefix_change(changes: bool) {
        f30b_prefix_change(if changes { "changed" } else { "unchanged" }).await;
    }

    async fn f30b_prefix_change(change: &str) {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        fake_exe(a.path(), "npm");
        let runner = Arc::new(F08PrefixNpm {
            current: std::sync::Mutex::new(a.path().to_owned()),
            installed: std::sync::Mutex::new([a.path().to_owned(), b.path().to_owned()].into()),
            writes: std::sync::Mutex::new(Vec::new()),
            write_specs: std::sync::Mutex::new(Vec::new()),
            prefix_reply: std::sync::Mutex::new(None),
            switch_after_prefix: std::sync::Mutex::new(None),
        });
        let adapter = NpmAdapter::new(runner.clone()).with_prefix_read_only_fn(|_| None);
        let env = HostEnv {
            path_dirs: vec![a.path().to_owned()],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let instances = adapter.detect(&env).await;
        assert_eq!(instances.len(), 1);
        let inst = &instances[0];
        assert_eq!(inst.prefix, a.path());
        let rows = adapter.inventory(inst).await.unwrap();
        assert_eq!(rows.len(), 1);
        let plan = adapter
            .plan(
                inst,
                &OpRequest {
                    kind: OpKind::Uninstall,
                    instance_id: inst.id.clone(),
                    artifact_kind: ArtifactKind::Package,
                    name: "jq".into(),
                },
            )
            .await
            .unwrap();
        match change {
            "changed" => *runner.current.lock().unwrap() = b.path().to_owned(),
            "race" => *runner.switch_after_prefix.lock().unwrap() = Some(b.path().to_owned()),
            "failed" => *runner.prefix_reply.lock().unwrap() = Some(f08_npm_output("", 1)),
            "no node" => {
                *runner.prefix_reply.lock().unwrap() = Some(CommandOutput {
                    stderr: "env: node: No such file or directory\n".into(),
                    ..f08_npm_output("", 127)
                })
            }
            "empty" => *runner.prefix_reply.lock().unwrap() = Some(f08_npm_output("", 0)),
            "relative" => {
                *runner.prefix_reply.lock().unwrap() = Some(f08_npm_output("relative", 0))
            }
            "timeout" => {
                *runner.prefix_reply.lock().unwrap() = Some(CommandOutput {
                    exit_code: None,
                    timed_out: true,
                    ..f08_npm_output(a.path().to_str().unwrap(), 0)
                })
            }
            _ => {
                // A refresh between the preview and the confirmation reads
                // the same prefix and list again; that is no change.
                let again = adapter.detect(&env).await;
                assert_eq!(again.len(), 1);
                assert_eq!(again[0].prefix, a.path());
                assert_eq!(adapter.inventory(&again[0]).await.unwrap().len(), 1);
            }
        }
        let changes = !matches!(change, "unchanged" | "race");
        let result = adapter
            .execute(&plan, Arc::new(VecSink::new()), 1, CancellationToken::new())
            .await;
        let writes = runner.writes.lock().unwrap().clone();
        assert!(
            !writes.contains(&b.path().to_owned()),
            "must never uninstall the other prefix: {writes:?}; {result:?}"
        );
        if changes {
            assert!(
                writes.is_empty(),
                "changed prefix must refuse the saved plan: {writes:?}; {result:?}"
            );
            // A read that did not answer says what running npm would have
            // said, not that anything changed (r20 R20-2); one that
            // answered with another, empty or relative prefix did change.
            match change {
                "failed" => assert_eq!(
                    result.unwrap(),
                    Outcome::Failed {
                        exit_code: Some(1),
                        summary: String::new(),
                        cause: None,
                    }
                ),
                "no node" => assert_eq!(
                    result.unwrap(),
                    Outcome::Failed {
                        exit_code: Some(127),
                        summary: "env: node: No such file or directory".into(),
                        cause: Some(crate::history::FailureCause::NotFound),
                    }
                ),
                "timeout" => assert_eq!(
                    result.unwrap(),
                    Outcome::Failed {
                        exit_code: None,
                        summary: String::new(),
                        cause: Some(crate::history::FailureCause::TimedOut),
                    }
                ),
                _ => assert!(
                    matches!(
                        result,
                        Ok(Outcome::BanagerFailed(
                            crate::model::Fault::ChangedSinceShown
                        ))
                    ),
                    "explicit stale-preview refusal: {result:?}"
                ),
            }
        } else {
            assert_eq!(result.unwrap(), Outcome::Succeeded);
            assert_eq!(writes, [a.path().to_owned()]);
            let PlanAction::Command { program, args, env } = &plan.action else {
                panic!("command");
            };
            let specs = runner.write_specs.lock().unwrap().clone();
            assert_eq!(specs.len(), 1);
            assert_eq!(
                (&specs[0].program, &specs[0].args, &specs[0].env),
                (program, args, env)
            );
            assert!(
                !adapter.reconcile(inst, &rows[0].key).await.unwrap().present,
                "reconcile must read the same root A"
            );
            assert!(
                adapter.inventory(inst).await.unwrap().is_empty(),
                "inventory must remain bound to A"
            );
        }
        assert!(runner.installed.lock().unwrap().contains(b.path()));
    }

    #[tokio::test]
    async fn f08_g06_changed_npm_prefix_never_removes_other_installation() {
        f08_prefix_change(true).await;
    }
    #[tokio::test]
    async fn f08_g06_unchanged_npm_prefix_removes_and_reconciles_same_root() {
        f08_prefix_change(false).await;
    }
    /// A prefix read that answers with no usable prefix refuses the plan;
    /// one that fails or runs out of time ends as npm's own failure (r20
    /// R20-2). Neither writes anything.
    #[tokio::test]
    async fn f30b_npm_prefix_read_with_no_usable_answer_writes_nothing() {
        for change in ["failed", "empty", "relative", "timeout"] {
            f30b_prefix_change(change).await;
        }
    }

    /// npm's launcher with no `node` on `PATH` (a `brew upgrade node`
    /// earlier in Update all that could not link it again): the read fails
    /// as the update would have, and says so (r20 R20-2).
    #[tokio::test]
    async fn r20_npm_prefix_read_with_no_node_says_what_is_missing_without_a_write() {
        f30b_prefix_change("no node").await;
    }

    #[tokio::test]
    async fn f30b_npm_change_after_recheck_still_uses_confirmed_prefix() {
        f30b_prefix_change("race").await;
    }
}
