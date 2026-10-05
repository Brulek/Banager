use crate::adapters::{
    ensure_instance_match, get_ok, reconcile_from, run_plan, second_token, uncheckable_candidate,
    url_path_segment, validate_package_name, Adapter, AdapterError, AdapterMeta, CheckOptions,
    CheckOutcome, LookupFailure,
};
use crate::events::{EventSink, OpId};
use crate::http::HttpClient;
use crate::model::{
    ArtifactFacts, ArtifactKey, ArtifactKind, CancelPolicy, CommandInputs, InstallReason,
    InstalledArtifact, InstanceStatus, ManagerInstance, OpKind, OpRequest, Outcome, Plan,
    PlanAction, ProvidedCommand, Reconciled, ResourceLock, Scope, SearchHit, Unavailable,
    UninstallScope, UpdateCandidate, UpdateChannel, Warning,
};
use crate::runner::{resolve_exe, CommandRunner, CommandSpec, HostEnv, OutputUse};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// `.crates2.json`'s `installs` object carries the package name, version and
/// source **in the JSON key** — e.g. `"hexyl 0.17.0 (registry+https://
/// github.com/rust-lang/crates.io-index)"` — not in the value, which only
/// has `bins`/`features`/`profile`/`rustc`/`target`/`version_req` (this
/// phase's documented trap for cargo). Splits that key into
/// `(name, version, source)`, preserving the complete registry identity.
fn parse_install_key(key: &str) -> Option<(String, String, String)> {
    let mut parts = key.splitn(3, ' ');
    let name = parts.next()?.to_string();
    let version = parts.next()?.to_string();
    let source = parts.next()?;
    let source = source.strip_prefix('(')?.strip_suffix(')')?;
    Some((name, version, source.to_string()))
}

/// crates.io's index as cargo names it, passed to `cargo install
/// --index` so a `registry.default` naming another registry cannot
/// swap the crate Banager checked against crates.io for a namesake.
/// Cargo takes this URL for its own crates-io source (cargo 1.98.1
/// `SourceId::for_registry` makes the id `SourceId::crates_io` makes:
/// same kind, same canonical URL), so the user's `[source.crates-io]`
/// replacement and the sparse protocol still apply, and cargo reaches
/// the hosts it reaches with no flag -- `index.crates.io` by default.
const CRATES_IO_INDEX: &str = "https://github.com/rust-lang/crates.io-index";

/// crates.io's sparse index, passed to `cargo-binstall --index` for the
/// same reason. Not `CRATES_IO_INDEX`: binstall reads any `--index`
/// without `sparse+` as a git registry and shallow-clones the whole
/// index from github.com on every run (binstalk-registry
/// `GitRegistry::new`). This URL is binstall's own default since 1.3.0
/// (`Registry::default`, `crates_io_sparse_registry`), the index it
/// reaches with no flag and no `registry.default`, so the flag adds no
/// host. (1.1 and 1.2 defaulted to crates.io's API instead; before 1.1
/// there is no `--index`, and binstall stops on the unknown flag before
/// connecting anywhere.) Not `--registry crates-io`: binstall 1.4-1.10
/// fail on that name with "unknown registry name" unless the user has
/// configured an index for it.
const CRATES_IO_SPARSE_INDEX: &str = "sparse+https://index.crates.io/";

fn is_crates_io(source: &str) -> bool {
    matches!(
        source,
        "registry+https://github.com/rust-lang/crates.io-index"
            | "registry+sparse+https://index.crates.io/"
            | "sparse+https://index.crates.io/"
    )
}

/// SemVer precedence ignores build metadata. Invalid versions are unknown,
/// never evidence for replacing a binary.
fn newer_stable(latest: &str, current: &str) -> Result<bool, LookupFailure> {
    let parse = |text: &str| {
        semver::Version::parse(text)
            .map_err(|_| LookupFailure::from("could not compare Cargo versions".to_string()))
    };
    let latest = parse(latest)?;
    let current = parse(current)?;
    Ok(latest.pre.is_empty() && latest.cmp_precedence(&current).is_gt())
}

#[derive(Deserialize)]
struct CratesIoResponse {
    #[serde(rename = "crate")]
    krate: CrateInfo,
}

#[derive(Deserialize)]
struct CrateInfo {
    max_stable_version: String,
}

/// The newest stable version in crates.io's answer about one crate
/// (`GET /api/v1/crates/<name>`), or why there is none. A version that is
/// empty or holds a control character (`sanity::is_name`) is no version:
/// the crate's row says it could not be checked rather than offer an
/// update to it.
pub(crate) fn parse_crates_io_body(body: &str) -> Result<String, String> {
    let parsed: CratesIoResponse = serde_json::from_str(body)
        .map_err(|e| format!("could not parse crates.io response: {e}"))?;
    let version = parsed.krate.max_stable_version;
    if crate::adapters::sanity::is_name(&version) {
        Ok(version)
    } else {
        Err("crates.io named no usable version".to_string())
    }
}

#[derive(Debug, Deserialize)]
struct Crates2Root {
    #[serde(default)]
    installs: HashMap<String, serde_json::Value>,
}

fn parse_crates2_entries(json: &str) -> Result<Vec<(String, String, String)>, AdapterError> {
    let root: Crates2Root =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    let mut entries: Vec<(String, String, String)> = root
        .installs
        .keys()
        .filter_map(|k| parse_install_key(k))
        .collect();
    entries.sort();
    Ok(entries)
}

/// The one field of an `installs` *value* Banager reads: the programs the
/// crate put in `<cargo_home>/bin`.
#[derive(Debug, Deserialize)]
struct Crates2Install {
    #[serde(default)]
    bins: Vec<String>,
}

/// The profile `cargo install` builds with when it is given no
/// `--profile` or `--debug` (cargo 1.98.1 `commands/install.rs`: the
/// `install.profile` setting, else `release`). Cargo writes the profile
/// into every `.crates2.json` record, so this one is no choice.
const DEFAULT_INSTALL_PROFILE: &str = "release";

/// Build choices saved by Cargo. Replayed only for upgrades; a binary
/// installer cannot promise to preserve these source-build options.
///
/// Cargo writes `profile` and `target` into *every* record (the recorded
/// fixture `adapters/fixtures/cargo/1.98.1/crates2.json`:
/// `"profile":"release","target":"aarch64-apple-darwin"` for a plain
/// `cargo install hexyl`), so their mere presence says nothing about
/// what the user asked for. Only a value that differs from what cargo
/// would pick by itself is a choice (`args`, `foreign_target`).
#[derive(Default, Deserialize)]
#[serde(default)]
struct BuildChoices {
    features: Vec<String>,
    all_features: bool,
    no_default_features: bool,
    profile: Option<String>,
    target: Option<String>,
    /// `rustc -vV` of the compiler that built the crate, as cargo saves
    /// it; its `host:` line is what the build was for by default.
    rustc: Option<String>,
}

impl BuildChoices {
    /// The flags that repeat what the user chose when installing, empty
    /// for a crate installed with cargo's defaults. Features,
    /// `--all-features` and `--no-default-features` are always choices;
    /// a profile only when it is not `release` (`--debug` is saved as
    /// `dev`). Never `--target`: see `foreign_target`.
    fn args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if !self.features.is_empty() {
            args.extend(["--features".into(), self.features.join(",")]);
        }
        if self.all_features {
            args.push("--all-features".into());
        }
        if self.no_default_features {
            args.push("--no-default-features".into());
        }
        if let Some(profile) = self
            .profile
            .as_deref()
            .filter(|profile| *profile != DEFAULT_INSTALL_PROFILE)
        {
            args.extend(["--profile".into(), profile.to_string()]);
        }
        args
    }

    /// Whether the upgrade must be built from source by cargo rather than
    /// downloaded by cargo-binstall: a choice `args` replays, or a binary
    /// built for another target (`foreign_target`), which cargo, not
    /// binstall, rebuilds the way the user's own Cargo settings say.
    fn builds_from_source(&self) -> bool {
        !self.args().is_empty() || self.foreign_target().is_some()
    }

    /// The saved target when it differs from the `host:` of the saved
    /// `rustc -vV`: a binary built for another machine than the compiler
    /// ran on. Without `--target` or a `build.target` setting cargo builds
    /// for, and saves, its own host, so a target equal to that host is
    /// cargo's default -- also when it is not this Mac's: a record that
    /// Migration Assistant brought from an Intel Mac says
    /// `x86_64-apple-darwin` twice. A record with no host to compare with
    /// names no foreign target either.
    ///
    /// A foreign target is never replayed as `--target`. Cargo saves the
    /// target it resolved, not whether `--target` or a `build.target`
    /// setting chose it (cargo `ops/cargo_install.rs`, `InstallInfo`), and
    /// replaying it asks for a standard library this Mac's Rust may not
    /// have ("can't find crate for `std`") or builds a program this Mac
    /// cannot run (a Linux target). The upgrade is built by `cargo
    /// install` with no `--target`, which applies the user's own
    /// `build.target` if there is one and otherwise builds for this Mac.
    fn foreign_target(&self) -> Option<&str> {
        let target = self.target.as_deref()?;
        let host = self
            .rustc
            .as_deref()?
            .lines()
            .find_map(|line| line.strip_prefix("host:"))?
            .trim();
        (!host.is_empty() && target != host).then_some(target)
    }
}

#[derive(Debug, Deserialize)]
struct Crates2Bins {
    #[serde(default)]
    installs: HashMap<String, Crates2Install>,
}

/// `(crate name, the programs it installed)` for every crate in
/// `.crates2.json`, sorted by name: the `bins` array in each `installs`
/// value. `parse_crates2_entries` reads only the keys, which carry the
/// name, version and source and nothing about the binaries -- so it
/// cannot say which *file* a crate left in `bin/`: `ripgrep` installs
/// `rg`, and a sentence that named the crate would name a program that
/// is not there (phase 4 spec §6.4, §十三 #4). Reader: `parse_crates2`
/// (the artifact's `path`); from this step's Task 5 on, the rustup
/// recipe's uninstall warnings (`adapters/standalone/rustup.rs`, which
/// name what `rustup self uninstall` deletes) read it too.
///
/// Cargo copies each program to `<cargo_home>/bin/<bin>`, so a bin is a
/// plain file name; one that is not (`""`, `..`, `/etc/x`, a control
/// character) names no file there -- joined to the folder, an absolute
/// one would even name a file outside it -- and is left out (`plain_bin`).
pub(crate) fn parse_crates2_bins(json: &str) -> Result<Vec<(String, Vec<String>)>, AdapterError> {
    let root: Crates2Bins =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    let mut out: Vec<(String, Vec<String>)> = root
        .installs
        .into_iter()
        .filter_map(|(key, install)| {
            let bins = install.bins.into_iter().filter(|b| plain_bin(b)).collect();
            parse_install_key(&key).map(|(name, _, _)| (name, bins))
        })
        .collect();
    out.sort();
    Ok(out)
}

/// A file name in `<cargo_home>/bin`: one usable name
/// (`sanity::is_name`), not `.` or `..`, with no `/`.
fn plain_bin(bin: &str) -> bool {
    crate::adapters::sanity::is_name(bin) && bin != "." && bin != ".." && !bin.contains('/')
}

/// One artifact per crate. `path` is the program the crate installed
/// under `<cargo_home>/bin`: the binary named after the crate when the
/// record lists one, else the first it lists, else `None` for a crate
/// that installed no program. `InstalledArtifact.path` holds one path,
/// so the other binaries of a multi-binary crate (`cargo-binstall`'s
/// `detect-targets`) are not attributed and stay on the Unknown page
/// until it can hold several (backlog). The reader is the Unknown page's
/// rule 2 (`scan/mod.rs`, `Known::index`): a `~/.cargo/bin/hexyl` that
/// canonicalises to this path is cargo's. Every binary the record lists
/// is one of the crate's commands (`CommandInputs.provided`), at
/// `<cargo_home>/bin/<bin>`, which cargo copies the program to.
pub(crate) fn parse_crates2(
    json: &str,
    instance_id: &str,
    cargo_home: &Path,
) -> Result<Vec<InstalledArtifact>, AdapterError> {
    let entries = parse_crates2_entries(json)?;
    let bins = parse_crates2_bins(json)?;
    // Each crate's programs by name, the first record of a name winning
    // as it does in the sorted list. A lookup per crate, not a search of
    // the list per crate: that was quadratic, 16 s over a record of 60,000
    // crates in a debug build.
    let mut bins_of: HashMap<&str, &[String]> = HashMap::new();
    for (crate_name, crate_bins) in &bins {
        bins_of
            .entry(crate_name.as_str())
            .or_insert(crate_bins.as_slice());
    }
    let bin_dir = cargo_home.join("bin");
    Ok(crate::adapters::sanity::artifacts(
        entries
            .into_iter()
            .map(|(name, version, _source_kind)| {
                let crate_bins: &[String] = bins_of.get(name.as_str()).copied().unwrap_or(&[]);
                let path = crate_bins
                    .iter()
                    .find(|b| **b == name)
                    .or_else(|| crate_bins.first())
                    .map(|bin| bin_dir.join(bin));
                let provided = crate_bins
                    .iter()
                    .map(|bin| ProvidedCommand {
                        name: bin.clone(),
                        path: bin_dir.join(bin),
                        within: Vec::new(),
                    })
                    .collect();
                InstalledArtifact {
                    key: ArtifactKey {
                        instance_id: instance_id.to_string(),
                        kind: ArtifactKind::Binary,
                        name: name.clone(),
                    },
                    display_name: name,
                    version,
                    reason: InstallReason::Requested,
                    description: None,
                    homepage: None,
                    size_bytes: None,
                    installed_at: None,
                    path,
                    auto_updates: false,
                    uninstall_blocked: None,
                    facts: ArtifactFacts {
                        command_inputs: CommandInputs {
                            provided,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                }
            })
            .collect(),
    ))
}

/// rustup's switch against installing a toolchain as a side effect
/// (`RUSTUP_AUTO_INSTALL=0`; rustup 1.29.1 `should_auto_install`,
/// config.rs:435-441). On a rustup Mac `cargo` *is* the rustup binary,
/// running in proxy mode (src/cli/proxy_mode.rs:14-59): before it runs
/// the real cargo it resolves the active toolchain and, with none active
/// and this switch off, installs one -- a download and a write, which a
/// refresh must never cause, and which a confirmed install, upgrade or
/// uninstall must not begin with: its preview never named one. So every
/// cargo command Banager runs carries it, through `CargoAdapter::ENV`:
/// `detect`'s `cargo --version` and the command of every plan `plan`
/// builds, cargo-binstall's included, so that a `cargo` or `rustc`
/// cargo-binstall starts inherits it. So does the rustup recipe's own
/// `--version`, through its `version.env`
/// (`adapters/standalone/recipes.rs`, `RUSTUP`). A cargo that is not
/// rustup's ignores the variable.
pub(crate) const RUSTUP_AUTO_INSTALL_OFF: (&str, &str) = ("RUSTUP_AUTO_INSTALL", "0");

/// Where cargo's home is, by the `home` crate's rule (`path_env::tool_home`
/// over `CARGO_HOME`): the one rule for the directory. From this step's
/// Task 4 on the rustup recipe's `$CARGO_HOME/…` paths
/// (`adapters/standalone/route.rs`, through `StandaloneAdapter::detect`)
/// share it, so the two adapters can never disagree about it, and the
/// lock name `instance_id_for` builds is built from the same path
/// `detect` names cargo's instance by. `None` for a relative
/// `CARGO_HOME`, which cargo resolves against a current directory Banager
/// does not share: `detect` then lists no cargo instance rather than one
/// whose prefix is somewhere cargo never looks.
pub(crate) fn cargo_home_of(env: &HostEnv) -> Option<PathBuf> {
    crate::runner::path_env::tool_home(env.cargo_home.as_deref(), &env.home, ".cargo")
}

/// The id of the cargo instance whose home is `cargo_home`:
/// `cargo:<cargo_home>`, the persisted shape (`model::instance_id`). The
/// single producer of that string (phase 4 spec §2.4): `detect` names its
/// instance with it, and from this step's Task 5 on the rustup recipe's
/// `extra_locks` (`adapters/standalone/rustup.rs`) builds the
/// `ResourceLock` its `self update` and `self uninstall` plans hold with
/// it. `acquire_resource_lock` compares lock names byte for byte and
/// reports nothing for two that merely look alike, so there is one
/// function and not two spellings.
pub(crate) fn instance_id_for(cargo_home: &Path) -> String {
    crate::model::instance_id("cargo", Some(&cargo_home.display().to_string()))
}

/// Real detection: resolves `cargo-binstall` through `HostEnv`'s hydrated
/// `PATH` — the same list `cargo` itself would search for a
/// `cargo-<subcommand>` plugin, and the same list every other adapter
/// resolves its executable from. Returns the resolved path, not a boolean,
/// so `plan` can preview exactly the program that will run; an earlier draft
/// scanned the *process* `PATH` for a yes/no answer and then previewed a
/// sibling-of-cargo path that the scan had never checked. A plain `fn`
/// pointer (not a closure) so tests can swap in a fixed answer — a test
/// cannot control whether the machine running it has cargo-binstall.
fn default_binstall_check(env: &HostEnv) -> Option<PathBuf> {
    resolve_exe("cargo-binstall", env)
}

pub struct CargoAdapter {
    runner: Arc<dyn CommandRunner>,
    http: Arc<dyn HttpClient>,
    meta: AdapterMeta,
    binstall_check: fn(&HostEnv) -> Option<PathBuf>,
    /// The path `detect` last resolved for cargo-binstall, or `None` when it
    /// is not installed. `plan` has no `HostEnv` of its own — the `Adapter`
    /// trait gives it only an instance — and `Session` always refreshes, and
    /// therefore detects, before it will issue a plan for an instance, so
    /// reading the cached answer here is what makes the previewed
    /// `Plan::program` the exact path that will run.
    binstall: Mutex<Option<PathBuf>>,
}

impl CargoAdapter {
    /// The environment every cargo command Banager runs is given, through
    /// `env_vec`: `detect`'s `cargo --version` and the command of every
    /// plan `plan` builds, the cargo-binstall ones included. Only
    /// `RUSTUP_AUTO_INSTALL_OFF`; why is said there.
    /// `tests/what_we_run_test.rs` checks that the `## Cargo` section of
    /// `docs/what-we-run.md` says so under this constant's name and shows
    /// each entry.
    pub const ENV: [(&'static str, &'static str); 1] = [RUSTUP_AUTO_INSTALL_OFF];

    pub fn new(runner: Arc<dyn CommandRunner>, http: Arc<dyn HttpClient>) -> CargoAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../adapters/meta/cargo.toml"))
            .expect("adapters/meta/cargo.toml must parse");
        CargoAdapter {
            runner,
            http,
            meta,
            binstall_check: default_binstall_check,
            binstall: Mutex::new(None),
        }
    }

    /// Test seam: pin what `plan` will believe about cargo-binstall without
    /// running `detect` (and so without depending on the host machine).
    #[cfg(test)]
    fn with_binstall(self, path: Option<PathBuf>) -> CargoAdapter {
        *self.binstall.lock().unwrap() = path;
        self
    }

    /// `ENV` as the owned pairs a `CommandSpec` and a
    /// `PlanAction::Command` hold.
    fn env_vec(&self) -> Vec<(String, String)> {
        Self::ENV
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let Some(exe_path) = resolve_exe("cargo", env) else {
            return Vec::new();
        };
        // The `home` crate's rule; `None` is a relative CARGO_HOME, which
        // names a directory relative to cargo's own cwd, not Banager's:
        // no instance, rather than one that reads the wrong place.
        let Some(cargo_home) = cargo_home_of(env) else {
            return Vec::new();
        };
        *self.binstall.lock().unwrap() = (self.binstall_check)(env);
        let output = self
            .runner
            .run(
                CommandSpec {
                    program: exe_path.clone(),
                    args: vec!["--version".to_string()],
                    // The cargo proxy is the rustup binary: never let a
                    // version read install a toolchain (RUSTUP_AUTO_INSTALL_OFF).
                    env: self.env_vec(),
                    cwd: None,
                    timeout: Duration::from_secs(30),
                    output_use: OutputUse::Parsed,
                },
                None,
                CancellationToken::new(),
            )
            .await;
        let version = match output {
            // "cargo 1.98.1 (hash date)" — the shared second-token rule
            // (crate::adapters::second_token, Task 5).
            Ok(o) if o.exit_code == Some(0) => second_token(&o.stdout),
            _ => None,
        };
        let unverified_version = self.meta.unverified_version(&version);
        vec![ManagerInstance {
            // Through `instance_id_for`, which the rustup recipe's cargo
            // lock is built with too from this step's Task 5 on: one
            // spelling of this id.
            id: instance_id_for(&cargo_home),
            adapter_id: self.meta.id.clone(),
            exe_path,
            prefix: cargo_home,
            scope: Scope::User,
            status: InstanceStatus {
                // The state axis. `version` is `None` exactly when the
                // CLI is on PATH but `--version` would not run or could
                // not be parsed: the tool is there, it just did not
                // answer.
                unavailable: version.is_none().then_some(Unavailable::NotResponding),
                notes: Vec::new(),
            },
            version,
            answered_at: None,
            unverified_version,
            read_only_reason: None,
        }]
    }

    /// A Rust toolchain that has never run `cargo install` has no
    /// `.crates2.json` at all, which is "nothing installed", not a failure —
    /// treating it as one would make every refresh on such a machine report
    /// a per-instance error and hold the whole snapshot permanently stale.
    /// Any other IO error (an unreadable or truncated file) is still an
    /// error.
    fn read_crates2(&self, inst: &ManagerInstance) -> Result<String, AdapterError> {
        let path = inst.prefix.join(".crates2.json");
        // Bounded: a named pipe there is refused, not waited on, and so is
        // a file past `read_file::LIMIT`, or one in or through a protected
        // place -- an error, never "nothing installed".
        match crate::adapters::read_file::read_text(
            &path,
            &crate::protected::Protected::of_this_process(),
        ) {
            Ok(json) => Ok(json),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok("{\"installs\":{}}".to_string())
            }
            Err(e) => Err(AdapterError::Parse(format!(
                "reading {}: {e}",
                path.display()
            ))),
        }
    }

    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let json = self.read_crates2(inst)?;
        parse_crates2(&json, &inst.id, &inst.prefix)
    }

    async fn latest_stable_version(&self, name: &str) -> Result<String, LookupFailure> {
        // Percent-encoded: the crate name is a `.crates2.json` key, i.e.
        // off disk, and raw it could add path segments or a query string to
        // crates.io's API url.
        let url = format!(
            "https://crates.io/api/v1/crates/{}",
            url_path_segment(name)?
        );
        let resp = get_ok(
            self.http.as_ref(),
            url,
            Vec::new(),
            "crates.io request failed",
            "crates.io",
        )
        .await?;
        Ok(parse_crates_io_body(&resp.body)?)
    }

    /// Registry-sourced crates are checked one at a time against crates.io.
    /// Git and path sources are `checkable: false` with a reason
    /// unconditionally — Banager has no way to check those for updates at
    /// all, so every such crate always gets a row explaining why, not just
    /// the ones that happen to be outdated (contract: "git and path sources
    /// are checkable: false with a reason").
    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        let json = self.read_crates2(inst)?;
        let entries = parse_crates2_entries(&json)?;
        let mut out = Vec::new();
        // The same rows `parse_crates2` lists: a crate without a usable
        // name is not one, and a version that is not text is unknown.
        for (name, mut version, source_kind) in entries {
            if !crate::adapters::sanity::is_name(&name) {
                continue;
            }
            if !crate::adapters::sanity::is_version(&version) {
                version.clear();
            }
            let key = ArtifactKey {
                instance_id: inst.id.clone(),
                kind: ArtifactKind::Binary,
                name: name.clone(),
            };
            if !is_crates_io(&source_kind) {
                out.push(UpdateCandidate {
                    key,
                    current: version.clone(),
                    target: version,
                    channel: UpdateChannel::Registry,
                    checkable: false,
                    warnings: vec![Warning::NonRegistrySource],
                    blocked: None,
                    download_bytes: None,
                });
                continue;
            }
            match self
                .latest_stable_version(&name)
                .await
                .and_then(|latest| newer_stable(&latest, &version).map(|newer| (latest, newer)))
            {
                Ok((latest, true)) => out.push(UpdateCandidate {
                    key,
                    current: version,
                    target: latest,
                    channel: UpdateChannel::Registry,
                    checkable: true,
                    warnings: Vec::new(),
                    blocked: None,
                    download_bytes: None,
                }),
                Ok(_) => {}
                Err(reason) => out.push(uncheckable_candidate(
                    key,
                    version,
                    UpdateChannel::Registry,
                    reason,
                )),
            }
        }
        Ok(out.into())
    }

    pub async fn search(
        &self,
        _inst: &ManagerInstance,
        _query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        Err(AdapterError::Unsupported(
            "cargo has no search command Banager uses; browse crates.io directly".to_string(),
        ))
    }

    pub async fn plan(
        &self,
        inst: &ManagerInstance,
        req: &OpRequest,
    ) -> Result<Plan, AdapterError> {
        ensure_instance_match(req, inst)?;
        validate_package_name(&req.name)?;
        let lock = ResourceLock(inst.id.clone());
        let root = inst
            .prefix
            .to_str()
            .filter(|_| inst.prefix.is_absolute())
            .ok_or_else(|| {
                AdapterError::Refused("Cargo install root must be an absolute UTF-8 path".into())
            })?;
        let mut build_args = Vec::new();
        let mut from_source = false;
        if req.kind == OpKind::Upgrade {
            let json = self.read_crates2(inst)?;
            let entries = parse_crates2_entries(&json)?;
            let matching: Vec<_> = entries
                .iter()
                .filter(|(name, _, _)| name == &req.name)
                .collect();
            if matching.is_empty() || matching.iter().any(|(_, _, source)| !is_crates_io(source)) {
                return Err(AdapterError::Refused(
                    "only installed crates.io packages can be upgraded".into(),
                ));
            }
            // Ambiguous duplicate install records are not a safe upgrade.
            let root: Crates2Root =
                serde_json::from_str(&json).map_err(|e| AdapterError::Parse(e.to_string()))?;
            let records: Vec<_> = root
                .installs
                .into_iter()
                .filter(|(key, _)| {
                    parse_install_key(key).is_some_and(|(name, _, _)| name == req.name)
                })
                .collect();
            if records.len() != 1 {
                return Err(AdapterError::Refused(
                    "ambiguous Cargo install records".into(),
                ));
            }
            let choices: BuildChoices =
                serde_json::from_value(records.into_iter().next().unwrap().1)
                    .map_err(|e| AdapterError::Parse(e.to_string()))?;
            build_args = choices.args();
            from_source = choices.builds_from_source();
        }
        match req.kind {
            OpKind::Install | OpKind::Upgrade => {
                // The path `detect` resolved through HostEnv, not a fresh
                // guess: whatever is previewed here is exactly what runs.
                // A crate installed with build choices of its own, or
                // built for another target, is rebuilt from source; one
                // installed with cargo's defaults may be fetched as a
                // binary.
                let binstall = self
                    .binstall
                    .lock()
                    .unwrap()
                    .clone()
                    .filter(|_| !from_source);
                let mut warnings = Vec::new();
                // Do not inherit registry.default: the checked source is
                // crates.io. Each program is given crates.io's index in the
                // form that keeps it on the hosts it reaches by default.
                let (program, mut args, index) = match binstall {
                    Some(path) => (path, vec!["-y".to_string()], CRATES_IO_SPARSE_INDEX),
                    None => {
                        warnings.push(Warning::CompilesLocally);
                        (
                            inst.exe_path.clone(),
                            vec!["install".to_string()],
                            CRATES_IO_INDEX,
                        )
                    }
                };
                if matches!(req.kind, OpKind::Upgrade) {
                    args.push("--force".to_string());
                }
                args.extend(["--root".to_string(), root.to_string()]);
                args.extend(["--index".to_string(), index.to_string()]);
                args.extend(build_args);
                args.push(req.name.clone());
                Ok(Plan {
                    request: req.clone(),
                    action: PlanAction::Command {
                        program,
                        args,
                        // For either program: `cargo install` must not
                        // start with a toolchain download, and a `cargo`
                        // or `rustc` cargo-binstall starts inherits the
                        // switch (RUSTUP_AUTO_INSTALL_OFF).
                        env: self.env_vec(),
                    },
                    needs_password: false,
                    locks: vec![lock],
                    cancel_policy: CancelPolicy::KillThenReconcile,
                    warnings,
                    affected: Vec::new(),
                    timeout_secs: 1800,
                })
            }
            OpKind::Uninstall => Ok(Plan {
                request: req.clone(),
                action: PlanAction::Command {
                    program: inst.exe_path.clone(),
                    args: vec![
                        "uninstall".to_string(),
                        "--root".to_string(),
                        root.to_string(),
                        req.name.clone(),
                    ],
                    // Removing one program must not start with a toolchain
                    // download (RUSTUP_AUTO_INSTALL_OFF).
                    env: self.env_vec(),
                },
                needs_password: false,
                locks: vec![lock],
                cancel_policy: CancelPolicy::KillThenReconcile,
                // What `cargo uninstall` removes and leaves (cargo 1.98.1
                // `src/cargo/ops/cargo_uninstall.rs`: the binaries its
                // install record lists for the crate), said under the tool.
                warnings: vec![Warning::UninstallScope {
                    what: UninstallScope::Cargo,
                }],
                affected: Vec::new(),
                timeout_secs: 300,
            }),
        }
    }

    pub async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
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
impl Adapter for CargoAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        CargoAdapter::detect(self, env).await
    }

    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        CargoAdapter::inventory(self, inst).await
    }

    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        CargoAdapter::check_updates(self, inst, opts).await
    }

    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        CargoAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        CargoAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        CargoAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        CargoAdapter::reconcile(self, inst, key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The recorded `.crates2.json` (cargo 1.98.1, Apple silicon, after a
    /// plain `cargo install hexyl`): one record, with the `profile`,
    /// `target` and `rustc` cargo writes into every record.
    const RECORDED_CRATES2: &str = "../../adapters/fixtures/cargo/1.98.1/crates2.json";

    /// The recorded fixture, byte for byte.
    fn recorded_crates2() -> String {
        std::fs::read_to_string(RECORDED_CRATES2).expect("read cargo crates2.json fixture")
    }

    /// A `.crates2.json` whose one record is the recorded fixture's value
    /// -- every field cargo writes -- under the install key `key`, with
    /// `change` applied to it. The shape of a real record, never `{}`.
    fn recorded_record_as(
        key: &str,
        change: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>),
    ) -> String {
        let root: serde_json::Value = serde_json::from_str(&recorded_crates2()).unwrap();
        let mut record = root["installs"]
            .as_object()
            .and_then(|installs| installs.values().next())
            .and_then(|record| record.as_object())
            .expect("the fixture holds one record")
            .clone();
        change(&mut record);
        serde_json::json!({ "installs": { key: record } }).to_string()
    }

    /// The fixture's hexyl record with `change` applied.
    fn recorded_hexyl_with(
        change: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>),
    ) -> String {
        recorded_record_as(
            "hexyl 0.17.0 (registry+https://github.com/rust-lang/crates.io-index)",
            change,
        )
    }

    /// The fixture's `rustc -vV` with its `host:` line naming `host`, as
    /// a compiler on that machine would have written it.
    fn recorded_rustc_on(host: &str) -> serde_json::Value {
        let root: serde_json::Value = serde_json::from_str(&recorded_crates2()).unwrap();
        let rustc = root["installs"]
            .as_object()
            .and_then(|installs| installs.values().next())
            .and_then(|record| record["rustc"].as_str())
            .expect("the fixture's record names its rustc");
        assert!(rustc.contains("\nhost: aarch64-apple-darwin\n"));
        rustc
            .replace(
                "\nhost: aarch64-apple-darwin\n",
                &format!("\nhost: {host}\n"),
            )
            .into()
    }

    /// The plan an upgrade of hexyl gets from `crates2`, with or without
    /// cargo-binstall on the Mac.
    async fn upgrade_plan(tag: &str, crates2: &str, binstall: bool) -> (Plan, PathBuf) {
        let home = temp_cargo_home(tag);
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(".crates2.json"), crates2).unwrap();
        let inst = test_instance(home.clone());
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()))
                .with_binstall(binstall.then(|| PathBuf::from(BINSTALL)));
        let req = OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".into(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        std::fs::remove_dir_all(&home).unwrap();
        (plan, home)
    }

    const BINSTALL: &str = "/Users/brulek/.cargo/bin/cargo-binstall";

    #[tokio::test]
    async fn regression_a_plain_cargo_install_record_upgrades_through_cargo_binstall() {
        // opus-int finding 1: cargo writes `"profile":"release"` and the
        // host triple into every record, and reading those as saved
        // choices sent every upgrade to a local compile, never to
        // cargo-binstall, with a "compiles locally" warning. The recorded
        // record of a plain `cargo install hexyl` holds no choice.
        let (plan, home) = upgrade_plan("recorded-binstall", &recorded_crates2(), true).await;
        assert_eq!(command_program(&plan), PathBuf::from(BINSTALL));
        assert_eq!(
            command_args(&plan),
            vec![
                "-y",
                "--force",
                "--root",
                home.to_str().unwrap(),
                "--index",
                "sparse+https://index.crates.io/",
                "hexyl"
            ]
        );
        assert!(plan.warnings.is_empty());
    }

    #[tokio::test]
    async fn regression_a_plain_cargo_install_record_rebuilds_with_no_profile_or_target() {
        // Without cargo-binstall the same record compiles, as before
        // b59d3b8d: no `--profile release`, no `--target <triple>`.
        let (plan, home) = upgrade_plan("recorded-cargo", &recorded_crates2(), false).await;
        assert_eq!(
            command_program(&plan),
            PathBuf::from("/Users/brulek/.cargo/bin/cargo")
        );
        assert_eq!(
            command_args(&plan),
            vec![
                "install",
                "--force",
                "--root",
                home.to_str().unwrap(),
                "--index",
                CRATES_IO_INDEX,
                "hexyl"
            ]
        );
        assert_eq!(plan.warnings, vec![Warning::CompilesLocally]);
    }

    #[tokio::test]
    async fn regression_a_record_from_an_intel_mac_upgrades_for_this_mac() {
        // Migration Assistant carries `~/.cargo` over from an Intel Mac:
        // the record says `x86_64-apple-darwin` as target and as the
        // compiler's host -- cargo's default there, no choice. Replaying
        // `--target x86_64-apple-darwin` on Apple silicon needs a
        // standard library rustup may not have, so it never is.
        let migrated = recorded_hexyl_with(|record| {
            record.insert("target".into(), "x86_64-apple-darwin".into());
            record.insert("rustc".into(), recorded_rustc_on("x86_64-apple-darwin"));
        });
        for binstall in [true, false] {
            let (plan, _) = upgrade_plan("migrated", &migrated, binstall).await;
            let args = command_args(&plan);
            assert!(!args.iter().any(|a| a == "--target"), "{args:?}");
            assert!(!args.iter().any(|a| a.contains("x86_64")), "{args:?}");
            assert_eq!(command_program(&plan) == Path::new(BINSTALL), binstall);
        }
    }

    #[tokio::test]
    async fn regression_a_foreign_target_is_rebuilt_by_cargo_without_target() {
        // Astra's j1 review, finding 3: a record whose target is not its
        // own compiler's host -- `--target x86_64-apple-darwin` on Apple
        // silicon, a `build.target` setting, a Linux cross-build -- used
        // to replay `--target`, which fails on a Mac without that
        // standard library and builds a Linux program this Mac cannot
        // run. Never replayed: cargo rebuilds the crate with no
        // `--target`, applying the user's own `build.target` if any, and
        // cargo-binstall is not used for it.
        for foreign in ["x86_64-apple-darwin", "x86_64-unknown-linux-gnu"] {
            let crates2 = recorded_hexyl_with(|record| {
                record.insert("target".into(), foreign.into());
            });
            for binstall in [true, false] {
                let (plan, home) = upgrade_plan("foreign-target", &crates2, binstall).await;
                assert_eq!(
                    command_program(&plan),
                    PathBuf::from("/Users/brulek/.cargo/bin/cargo"),
                    "{foreign}"
                );
                assert_eq!(
                    command_args(&plan),
                    vec![
                        "install",
                        "--force",
                        "--root",
                        home.to_str().unwrap(),
                        "--index",
                        CRATES_IO_INDEX,
                        "hexyl"
                    ],
                    "{foreign}"
                );
                assert_eq!(plan.warnings, vec![Warning::CompilesLocally]);
            }
        }
    }

    #[tokio::test]
    async fn test_upgrade_preserves_build_choices_and_uses_source_build() {
        // A real record of `cargo install hexyl --features pcre2,extra
        // --all-features --no-default-features --debug` on Apple silicon:
        // every choice is replayed, and the upgrade compiles even with
        // cargo-binstall there.
        let chosen = recorded_hexyl_with(|record| {
            record.insert("features".into(), serde_json::json!(["pcre2", "extra"]));
            record.insert("all_features".into(), true.into());
            record.insert("no_default_features".into(), true.into());
            record.insert("profile".into(), "dev".into());
        });
        let (plan, home) = upgrade_plan("build-choices", &chosen, true).await;
        assert_eq!(
            command_program(&plan),
            PathBuf::from("/Users/brulek/.cargo/bin/cargo")
        );
        assert_eq!(
            command_args(&plan),
            vec![
                "install",
                "--force",
                "--root",
                home.to_str().unwrap(),
                "--index",
                CRATES_IO_INDEX,
                "--features",
                "pcre2,extra",
                "--all-features",
                "--no-default-features",
                "--profile",
                "dev",
                "hexyl"
            ]
        );
        assert_eq!(plan.warnings, vec![Warning::CompilesLocally]);
    }

    #[test]
    fn test_build_choices_count_only_what_differs_from_cargos_defaults() {
        let choices = |crates2: String| -> (Vec<String>, bool) {
            let root: serde_json::Value = serde_json::from_str(&crates2).unwrap();
            let record = root["installs"]
                .as_object()
                .unwrap()
                .values()
                .next()
                .unwrap();
            let choices = serde_json::from_value::<BuildChoices>(record.clone()).unwrap();
            (choices.args(), choices.builds_from_source())
        };
        let none: Vec<String> = Vec::new();
        // The recorded record: release profile, host target -- nothing.
        assert_eq!(choices(recorded_crates2()), (none.clone(), false));
        for (what, crates2, expected) in [
            (
                "one feature",
                recorded_hexyl_with(|r| {
                    r.insert("features".into(), serde_json::json!(["pcre2"]));
                }),
                vec!["--features", "pcre2"],
            ),
            (
                "--no-default-features alone",
                recorded_hexyl_with(|r| {
                    r.insert("no_default_features".into(), true.into());
                }),
                vec!["--no-default-features"],
            ),
            (
                "--debug, saved as the dev profile",
                recorded_hexyl_with(|r| {
                    r.insert("profile".into(), "dev".into());
                }),
                vec!["--profile", "dev"],
            ),
            (
                "a custom profile",
                recorded_hexyl_with(|r| {
                    r.insert("profile".into(), "dist".into());
                }),
                vec!["--profile", "dist"],
            ),
        ] {
            let expected: Vec<String> = expected.into_iter().map(String::from).collect();
            assert_eq!(choices(crates2), (expected, true), "{what}");
        }
        // A target other than the compiler's host: no flag, but a source
        // build all the same.
        for (what, crates2) in [
            (
                "an Intel target on Apple silicon",
                recorded_hexyl_with(|r| {
                    r.insert("target".into(), "x86_64-apple-darwin".into());
                }),
            ),
            (
                "an Apple silicon target on an Intel Mac",
                recorded_hexyl_with(|r| {
                    r.insert("rustc".into(), recorded_rustc_on("x86_64-apple-darwin"));
                }),
            ),
        ] {
            assert_eq!(choices(crates2), (none.clone(), true), "{what}");
        }
        for (what, crates2) in [
            (
                "the Intel Mac's own default",
                recorded_hexyl_with(|r| {
                    r.insert("target".into(), "x86_64-apple-darwin".into());
                    r.insert("rustc".into(), recorded_rustc_on("x86_64-apple-darwin"));
                }),
            ),
            (
                "no compiler to compare the target with",
                recorded_hexyl_with(|r| {
                    r.remove("rustc");
                    r.insert("target".into(), "x86_64-apple-darwin".into());
                }),
            ),
            (
                "a compiler that names no host",
                recorded_hexyl_with(|r| {
                    r.insert("rustc".into(), "rustc 1.98.1 (48a229cea 2026-09-01)".into());
                    r.insert("target".into(), "x86_64-apple-darwin".into());
                }),
            ),
            (
                "a null profile and target",
                recorded_hexyl_with(|r| {
                    r.insert("profile".into(), serde_json::Value::Null);
                    r.insert("target".into(), serde_json::Value::Null);
                }),
            ),
        ] {
            assert_eq!(choices(crates2), (none.clone(), false), "{what}");
        }
    }

    // Regressions found by `adapters/robustness.rs`.

    #[test]
    fn regression_semver_updates_never_downgrade_or_change_only_metadata() {
        for (current, latest, newer) in [
            ("2.0.0-rc.1", "1.9.0", false),
            ("2.0.0-rc.1", "2.0.0", true),
            ("1.0.0+local", "1.0.0+remote", false),
            ("1.9.0", "1.10.0", true),
            ("1.0.0", "1.1.0-rc.1", false),
        ] {
            assert_eq!(newer_stable(latest, current).unwrap(), newer);
        }
        assert!(newer_stable("latest", "1.0.0").is_err());
        assert!(newer_stable("1.0.0", "unknown").is_err());
    }

    #[tokio::test]
    async fn regression_private_registry_is_never_queried_or_upgraded_as_crates_io() {
        let home = temp_cargo_home("private-registry");
        std::fs::create_dir_all(&home).unwrap();
        let http = Arc::new(MockHttpClient::new());
        let adapter = CargoAdapter::new(Arc::new(MockRunner::new()), http.clone());
        let inst = test_instance(home.clone());
        for source in [
            "registry+https://company.example/index",
            "git+https://example.test/repo",
        ] {
            std::fs::write(
                home.join(".crates2.json"),
                recorded_record_as(&format!("foo 1.0.0 ({source})"), |record| {
                    record.insert("bins".into(), serde_json::json!(["foo"]));
                }),
            )
            .unwrap();
            let rows = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .unwrap()
                .candidates;
            assert_eq!(rows.len(), 1);
            assert!(!rows[0].checkable);
            let req = OpRequest {
                kind: OpKind::Upgrade,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Binary,
                name: "foo".into(),
            };
            assert!(matches!(
                adapter.plan(&inst, &req).await,
                Err(AdapterError::Refused(_))
            ));
        }
        assert!(http.calls().is_empty());
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn regression_parse_crates2_reads_sixty_thousand_crates_in_linear_time() {
        let many: Vec<String> = (0..60_000)
            .map(|i| format!("\"c{i} 1.0.{i} (registry+https://x)\":{{\"bins\":[\"c{i}\"]}}"))
            .collect();
        let json = format!("{{\"installs\":{{{}}}}}", many.join(","));
        let started = std::time::Instant::now();
        let artifacts = parse_crates2(&json, "cargo:/x", Path::new("/h/.cargo")).unwrap();
        assert_eq!(artifacts.len(), 60_000);
        assert_eq!(
            artifacts[0].path.as_deref(),
            Some(Path::new("/h/.cargo/bin/c0"))
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn regression_parse_crates2_names_no_program_outside_the_bin_folder() {
        let json = r#"{"installs":{
            "hexyl 0.17.0 (registry+https://x)":{"bins":["/exyl","..","","a\nb","hexyl"]},
            " 1.0 (registry+https://x)":{"bins":["x"]}}}"#;
        let artifacts = parse_crates2(json, "cargo:/x", Path::new("/h/.cargo")).unwrap();
        assert_eq!(artifacts.len(), 1, "the crate with no name is left out");
        assert_eq!(
            artifacts[0].path.as_deref(),
            Some(Path::new("/h/.cargo/bin/hexyl"))
        );
        let commands: Vec<&str> = artifacts[0]
            .facts
            .command_inputs
            .provided
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(commands, vec!["hexyl"]);
        let bins = parse_crates2_bins(json).unwrap();
        assert_eq!(bins[1], ("hexyl".to_string(), vec!["hexyl".to_string()]));
    }
    use crate::testing::{command_args, command_env, command_program};
    use std::path::Path;

    #[test]
    fn test_second_token_reads_cargos_recorded_version_line() {
        // adapters/fixtures/cargo/1.98.1/version.txt:
        // "cargo 1.98.1 (797e8a9bc 2026-08-05)"
        // The rule is crate::adapters::second_token (Task 5); this pins it
        // against cargo's real recorded output.
        assert_eq!(
            second_token("cargo 1.98.1 (797e8a9bc 2026-08-05)\n"),
            Some("1.98.1".to_string())
        );
    }

    #[test]
    fn test_parse_install_key_splits_name_version_and_source_kind() {
        // ".crates2.json keeps the package name, version and source in the
        // JSON key" (this phase's documented trap for cargo).
        let key = "hexyl 0.17.0 (registry+https://github.com/rust-lang/crates.io-index)";
        assert_eq!(
            parse_install_key(key),
            Some((
                "hexyl".to_string(),
                "0.17.0".to_string(),
                "registry+https://github.com/rust-lang/crates.io-index".to_string()
            ))
        );
    }

    #[test]
    fn test_parse_install_key_recognizes_git_and_path_sources() {
        assert_eq!(
            parse_install_key("my-fork 0.1.0 (git+https://github.com/example/my-fork#abc123)"),
            Some((
                "my-fork".to_string(),
                "0.1.0".to_string(),
                "git+https://github.com/example/my-fork#abc123".to_string()
            ))
        );
        assert_eq!(
            parse_install_key("local-tool 0.1.0 (path+file:///Users/brulek/dev/local-tool)"),
            Some((
                "local-tool".to_string(),
                "0.1.0".to_string(),
                "path+file:///Users/brulek/dev/local-tool".to_string()
            ))
        );
    }

    #[test]
    fn test_parse_crates2_from_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/cargo/1.98.1/crates2.json")
            .expect("read cargo crates2.json fixture");
        let artifacts = parse_crates2(
            &json,
            "cargo:/Users/someone/.cargo",
            Path::new("/Users/someone/.cargo"),
        )
        .expect("parse crates2.json");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].key.kind, ArtifactKind::Binary);
        assert_eq!(artifacts[0].key.name, "hexyl");
        assert_eq!(artifacts[0].version, "0.17.0");
        // The program the crate installed, for the Unknown page's rule 2:
        // `hexyl`'s one binary is `hexyl` (`"bins":["hexyl"]` in the
        // recording), under the Cargo home's `bin/`.
        assert_eq!(
            artifacts[0].path,
            Some(PathBuf::from("/Users/someone/.cargo/bin/hexyl"))
        );
    }

    #[test]
    fn test_parse_crates2_bins_reads_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/cargo/1.98.1/crates2.json")
            .expect("read cargo crates2.json fixture");
        assert_eq!(
            parse_crates2_bins(&json).expect("parse"),
            vec![("hexyl".to_string(), vec!["hexyl".to_string()])]
        );
    }

    #[test]
    fn test_parse_crates2_bins_names_the_binaries_not_the_crate() {
        // Edge cases the recorded fixture (one crate, one binary named
        // after it) cannot show: `ripgrep` installs `rg`; a crate can
        // install several programs; a record with no `bins` key is a
        // crate that installed none. Sorted by crate name.
        let json = r#"{"installs":{
            "ripgrep 15.1.0 (registry+https://github.com/rust-lang/crates.io-index)":{"version_req":null,"bins":["rg"],"features":[],"all_features":false,"no_default_features":false,"profile":"release","target":"aarch64-apple-darwin","rustc":"rustc 1.98.1\n"},
            "cargo-binstall 1.16.0 (registry+https://github.com/rust-lang/crates.io-index)":{"version_req":null,"bins":["cargo-binstall","detect-targets"],"features":[],"all_features":false,"no_default_features":false,"profile":"release","target":"aarch64-apple-darwin","rustc":"rustc 1.98.1\n"},
            "libonly 0.1.0 (registry+https://github.com/rust-lang/crates.io-index)":{"version_req":null,"features":[],"all_features":false,"no_default_features":false,"profile":"release","target":"aarch64-apple-darwin","rustc":"rustc 1.98.1\n"}
        }}"#;
        assert_eq!(
            parse_crates2_bins(json).expect("parse"),
            vec![
                (
                    "cargo-binstall".to_string(),
                    vec!["cargo-binstall".to_string(), "detect-targets".to_string()]
                ),
                ("libonly".to_string(), Vec::new()),
                ("ripgrep".to_string(), vec!["rg".to_string()]),
            ]
        );
        let artifacts = parse_crates2(
            json,
            "cargo:/Users/someone/.cargo",
            Path::new("/Users/someone/.cargo"),
        )
        .expect("parse");
        let path_of = |name: &str| {
            artifacts
                .iter()
                .find(|a| a.key.name == name)
                .expect(name)
                .path
                .clone()
        };
        // The binary named after the crate when there is one, else the
        // first listed, else none (ruling 7). `detect-targets`, the
        // second binary of `cargo-binstall`, carries no artifact path and
        // stays on the Unknown page until `path` can hold several.
        assert_eq!(
            path_of("ripgrep"),
            Some(PathBuf::from("/Users/someone/.cargo/bin/rg"))
        );
        assert_eq!(
            path_of("cargo-binstall"),
            Some(PathBuf::from("/Users/someone/.cargo/bin/cargo-binstall"))
        );
        assert_eq!(path_of("libonly"), None);
        // Every binary is one of the crate's commands, `detect-targets`
        // included: `commands` can hold several where `path` cannot.
        let commands_of = |name: &str| -> Vec<(String, PathBuf)> {
            artifacts
                .iter()
                .find(|a| a.key.name == name)
                .expect(name)
                .facts
                .command_inputs
                .provided
                .iter()
                .map(|p| (p.name.clone(), p.path.clone()))
                .collect()
        };
        assert_eq!(
            commands_of("cargo-binstall"),
            vec![
                (
                    "cargo-binstall".to_string(),
                    PathBuf::from("/Users/someone/.cargo/bin/cargo-binstall")
                ),
                (
                    "detect-targets".to_string(),
                    PathBuf::from("/Users/someone/.cargo/bin/detect-targets")
                ),
            ]
        );
        assert_eq!(
            commands_of("ripgrep"),
            vec![(
                "rg".to_string(),
                PathBuf::from("/Users/someone/.cargo/bin/rg")
            )]
        );
        assert!(commands_of("libonly").is_empty());
    }

    #[test]
    fn test_parse_crates2_bins_is_a_parse_error_for_anything_that_is_not_the_record() {
        assert!(matches!(
            parse_crates2_bins("not json"),
            Err(AdapterError::Parse(_))
        ));
        assert!(matches!(
            parse_crates2_bins(r#"{"installs":{"hexyl 0.17.0 (registry+x)":{"bins":"hexyl"}}}"#),
            Err(AdapterError::Parse(_))
        ));
    }

    #[test]
    fn test_cargo_home_of_is_the_home_crates_rule_over_the_host_environment() {
        // `tool_home` (runner/path_env.rs) over `HostEnv.cargo_home`:
        // unset and empty are `<home>/.cargo`, absolute is itself, relative
        // is unsupported -- the same answer rustup and cargo compute, so
        // the lock name built from it names the directory they use.
        let env = HostEnv {
            path_dirs: Vec::new(),
            home: PathBuf::from("/Users/someone"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        assert_eq!(
            cargo_home_of(&env),
            Some(PathBuf::from("/Users/someone/.cargo"))
        );
        let env = HostEnv {
            cargo_home: Some(PathBuf::from("")),
            ..env
        };
        assert_eq!(
            cargo_home_of(&env),
            Some(PathBuf::from("/Users/someone/.cargo"))
        );
        let env = HostEnv {
            cargo_home: Some(PathBuf::from("/Volumes/Data/cargo")),
            ..env
        };
        assert_eq!(
            cargo_home_of(&env),
            Some(PathBuf::from("/Volumes/Data/cargo"))
        );
        let env = HostEnv {
            cargo_home: Some(PathBuf::from("cargo")),
            ..env
        };
        assert_eq!(cargo_home_of(&env), None);
    }

    #[test]
    fn test_instance_id_for_is_the_persisted_cargo_shape() {
        // `cargo:<cargo_home>` (`model::instance_id`'s
        // `test_instance_id_reproduces_every_shape_already_persisted`).
        // The rustup recipe builds its cargo lock from this same function
        // (adapters/standalone/rustup.rs), so the id and the lock cannot
        // drift into two spellings that `acquire_resource_lock` would
        // treat as unrelated.
        assert_eq!(
            instance_id_for(Path::new("/Users/someone/.cargo")),
            "cargo:/Users/someone/.cargo"
        );
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        assert_eq!(
            adapter.meta.id, "cargo",
            "the literal instance_id_for spells"
        );
    }

    /// `MockRunner` keys and records argv only; this records the whole
    /// `CommandSpec`, so a test can see the environment a command was
    /// given: `detect`'s `cargo --version`, or the command `execute` runs
    /// for a plan. Every command it is handed exits 0 and prints cargo's
    /// recorded version line.
    struct EnvRecordingRunner {
        specs: Mutex<Vec<CommandSpec>>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for EnvRecordingRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _on_line: Option<crate::runner::LineCallback>,
            _cancel: CancellationToken,
        ) -> Result<CommandOutput, crate::runner::RunnerError> {
            self.specs.lock().unwrap().push(spec);
            Ok(CommandOutput {
                exit_code: Some(0),
                stdout: "cargo 1.98.1 (797e8a9bc 2026-08-05)\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            })
        }
    }

    #[tokio::test]
    async fn test_detect_reads_cargos_version_with_rustups_auto_install_off() {
        // On a rustup Mac `cargo` is the rustup binary in proxy mode
        // (rustup 1.29.1 src/cli/proxy_mode.rs:14-59): before it runs the
        // real cargo it resolves the active toolchain, and with none active
        // it *installs* one unless `RUSTUP_AUTO_INSTALL=0`
        // (`should_auto_install`, config.rs:435-441). A refresh is
        // read-only, so the switch goes on this read; a cargo that is not
        // rustup's ignores the variable.
        let home = temp_cargo_home("auto-install-off");
        let bin = home.join("bin");
        std::fs::create_dir_all(&bin).expect("bin");
        std::fs::write(bin.join("cargo"), b"#!/bin/sh\n").expect("cargo");
        let runner = Arc::new(EnvRecordingRunner {
            specs: Mutex::new(Vec::new()),
        });
        let adapter = CargoAdapter::new(runner.clone(), Arc::new(MockHttpClient::new()));
        let env = HostEnv {
            path_dirs: vec![bin.clone()],
            home: home.parent().unwrap().to_path_buf(),
            euid: 501,
            cargo_home: Some(home.clone()),
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let instances = adapter.detect(&env).await;
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].version, Some("1.98.1".to_string()));
        let specs = runner.specs.lock().unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].program, bin.join("cargo"));
        assert_eq!(specs[0].args, vec!["--version".to_string()]);
        assert_eq!(
            specs[0].env,
            vec![("RUSTUP_AUTO_INSTALL".to_string(), "0".to_string())]
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_detect_lists_no_cargo_for_a_relative_cargo_home() {
        // cargo itself would join a relative CARGO_HOME onto *its* current
        // directory (`home` 0.5.12); Banager's is not that, so an instance
        // whose prefix were that relative path would read `.crates2.json`
        // from the wrong place and lock a name nothing else uses. No
        // instance is the honest answer (ruling 6).
        let home = temp_cargo_home("relative");
        let bin = home.join("bin");
        std::fs::create_dir_all(&bin).expect("bin");
        std::fs::write(bin.join("cargo"), b"#!/bin/sh\n").expect("cargo");
        let runner = Arc::new(MockRunner::new());
        let adapter = CargoAdapter::new(runner.clone(), Arc::new(MockHttpClient::new()));
        let env = HostEnv {
            path_dirs: vec![bin],
            home: home.parent().unwrap().to_path_buf(),
            euid: 501,
            cargo_home: Some(PathBuf::from("cargo")),
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        assert!(adapter.detect(&env).await.is_empty());
        assert!(
            runner.calls().is_empty(),
            "nothing is run for a home Banager cannot name"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_inventory_gives_every_cargo_artifact_the_path_of_its_program() {
        let json = std::fs::read_to_string("../../adapters/fixtures/cargo/1.98.1/crates2.json")
            .expect("read cargo crates2.json fixture");
        let home = temp_cargo_home("paths");
        std::fs::create_dir_all(&home).expect("create cargo home");
        std::fs::write(home.join(".crates2.json"), &json).expect("write crates2.json");
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(home.clone());
        let artifacts = adapter.inventory(&inst).await.expect("inventory");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].path, Some(home.join("bin").join("hexyl")));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn test_default_binstall_check_resolves_cargo_binstall_through_host_env() {
        // The real resolver must read HostEnv's hydrated PATH, not the
        // process PATH: a Finder-launched app's process PATH is minimal, and
        // an answer taken from it could name a path `plan` then previews but
        // never runs. A dedicated temp directory stands in for a PATH entry,
        // so this cannot depend on whether the machine running it actually
        // has cargo-binstall installed.
        let dir = crate::testing::unique_temp_path("binstall");
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let env = HostEnv {
            path_dirs: vec![dir.clone()],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        assert_eq!(default_binstall_check(&env), None);

        let exe = dir.join("cargo-binstall");
        std::fs::write(&exe, b"#!/bin/sh\n").expect("write fake cargo-binstall");
        assert_eq!(default_binstall_check(&env), Some(exe));

        let _ = std::fs::remove_dir_all(&dir);
    }

    use crate::events::VecSink;
    use crate::http::{HttpResponse, MockHttpClient};
    use crate::runner::{CommandOutput, MockRunner};

    fn temp_cargo_home(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "banager-cargo-home-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn test_instance(prefix: PathBuf) -> ManagerInstance {
        let id = format!("cargo:{}", prefix.display());
        ManagerInstance {
            exe_path: PathBuf::from("/Users/brulek/.cargo/bin/cargo"),
            prefix,
            version: Some("1.98.1".to_string()),
            ..crate::testing::manager_instance("cargo", &id)
        }
    }

    #[tokio::test]
    async fn test_check_updates_flags_the_fixture_crate_as_outdated() {
        let json = std::fs::read_to_string("../../adapters/fixtures/cargo/1.98.1/crates2.json")
            .expect("read cargo crates2.json fixture");
        let home = temp_cargo_home("outdated");
        std::fs::create_dir_all(&home).expect("create cargo home");
        std::fs::write(home.join(".crates2.json"), &json).expect("write crates2.json");

        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "https://crates.io/api/v1/crates/hexyl",
            HttpResponse {
                status: 200,
                body: r#"{"crate":{"max_stable_version":"0.18.0"}}"#.to_string(),
            },
        );
        let adapter = CargoAdapter::new(Arc::new(MockRunner::new()), http);
        let inst = test_instance(home.clone());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "hexyl");
        assert!(candidates[0].checkable);
        assert_eq!(candidates[0].target, "0.18.0");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn regression_check_updates_offers_no_update_to_an_empty_or_control_character_crates_io_version(
    ) {
        let json = std::fs::read_to_string("../../adapters/fixtures/cargo/1.98.1/crates2.json")
            .expect("read cargo crates2.json fixture");
        for (i, body) in [
            r#"{"crate":{"max_stable_version":""}}"#,
            r#"{"crate":{"max_stable_version":"1.0\n"}}"#,
            r#"{"crate":{"max_stable_version":"\u001b[31m1.0"}}"#,
        ]
        .into_iter()
        .enumerate()
        {
            let home = temp_cargo_home(&format!("bad-version-{i}"));
            std::fs::create_dir_all(&home).expect("create cargo home");
            std::fs::write(home.join(".crates2.json"), &json).expect("write crates2.json");
            let http = Arc::new(MockHttpClient::new());
            http.respond(
                "https://crates.io/api/v1/crates/hexyl",
                HttpResponse {
                    status: 200,
                    body: body.to_string(),
                },
            );
            let adapter = CargoAdapter::new(Arc::new(MockRunner::new()), http);
            let inst = test_instance(home.clone());
            let candidates = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("check_updates")
                .candidates;
            let _ = std::fs::remove_dir_all(&home);
            assert_eq!(candidates.len(), 1, "{body}");
            assert!(!candidates[0].checkable, "{body}");
            assert_eq!(candidates[0].target, candidates[0].current, "{body}");
        }
    }

    #[tokio::test]
    async fn regression_check_updates_skips_a_crate_whose_name_is_not_one() {
        // `parse_crates2` lists no row for it, so no update either.
        let json = r#"{"installs":{"bad\u001bname 0.1.0 (registry+https://github.com/rust-lang/crates.io-index)":{"bins":[]}}}"#;
        let home = temp_cargo_home("bad-name");
        std::fs::create_dir_all(&home).expect("create cargo home");
        std::fs::write(home.join(".crates2.json"), json).expect("write crates2.json");
        let http = Arc::new(MockHttpClient::new());
        let adapter = CargoAdapter::new(Arc::new(MockRunner::new()), http.clone());
        let inst = test_instance(home.clone());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        let _ = std::fs::remove_dir_all(&home);
        assert!(candidates.is_empty());
        assert!(http.calls().is_empty());
    }

    #[tokio::test]
    async fn test_check_updates_reports_nothing_when_the_fixture_crate_is_current() {
        let json = std::fs::read_to_string("../../adapters/fixtures/cargo/1.98.1/crates2.json")
            .expect("read cargo crates2.json fixture");
        let home = temp_cargo_home("current");
        std::fs::create_dir_all(&home).expect("create cargo home");
        std::fs::write(home.join(".crates2.json"), &json).expect("write crates2.json");

        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "https://crates.io/api/v1/crates/hexyl",
            HttpResponse {
                status: 200,
                body: r#"{"crate":{"max_stable_version":"0.17.0"}}"#.to_string(),
            },
        );
        let adapter = CargoAdapter::new(Arc::new(MockRunner::new()), http);
        let inst = test_instance(home.clone());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        assert!(candidates.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_check_updates_marks_a_git_sourced_crate_as_uncheckable() {
        // Edge case the recorded fixture (a single registry-sourced crate)
        // cannot show: a crate installed from a git repository.
        let json = r#"{"installs":{"my-fork 0.1.0 (git+https://github.com/example/my-fork#abc123)":{"version_req":null,"bins":["my-fork"],"features":[],"all_features":false,"no_default_features":false,"profile":"release","target":"aarch64-apple-darwin","rustc":"rustc 1.98.1\n"}}}"#;
        let home = temp_cargo_home("git-source");
        std::fs::create_dir_all(&home).expect("create cargo home");
        std::fs::write(home.join(".crates2.json"), json).expect("write crates2.json");

        let http = Arc::new(MockHttpClient::new());
        let adapter = CargoAdapter::new(Arc::new(MockRunner::new()), http.clone());
        let inst = test_instance(home.clone());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        assert_eq!(candidates.len(), 1);
        assert!(!candidates[0].checkable);
        assert_eq!(candidates[0].warnings, vec![Warning::NonRegistrySource]);
        assert!(
            http.calls().is_empty(),
            "a git-sourced crate must never reach crates.io"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_inventory_of_a_cargo_home_with_no_crates2_json_is_empty_not_an_error() {
        // A Rust toolchain that has never run `cargo install` has no
        // .crates2.json. That is "nothing installed", not a failed refresh:
        // an error here would push a SourceError and hold the whole snapshot
        // stale on every refresh, forever, on an entirely healthy machine.
        let home = temp_cargo_home("empty");
        std::fs::create_dir_all(&home).expect("create cargo home");
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(home.clone());
        let artifacts = adapter.inventory(&inst).await.expect("inventory");
        assert!(artifacts.is_empty());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        assert!(candidates.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_inventory_never_reads_a_cargo_home_kept_in_a_protected_place() {
        // `~/.cargo` a link into `~/Documents` (dotfiles synced that way):
        // its record is not read there -- the inventory cannot be told, an
        // error, never "nothing installed". Kept anywhere else, it is read.
        let home = std::fs::canonicalize({
            let raw = temp_cargo_home("kept");
            std::fs::create_dir_all(&raw).unwrap();
            raw
        })
        .unwrap();
        for keep in ["elsewhere", "Documents"] {
            let kept = home.join(keep).join("cargo");
            std::fs::create_dir_all(&kept).unwrap();
            std::fs::write(kept.join(".crates2.json"), r#"{"installs":{}}"#).unwrap();
            let linked = home.join(format!(".cargo-{}", keep.to_lowercase()));
            std::os::unix::fs::symlink(&kept, &linked).unwrap();
            let adapter =
                CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
            let _home = crate::protected::as_if_home(&home);
            let read = adapter.inventory(&test_instance(linked)).await;
            if keep == "Documents" {
                assert!(read.is_err(), "{read:?}");
            } else {
                assert!(read.expect("read").is_empty());
            }
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_plan_refuses_when_request_instance_id_does_not_match_given_instance() {
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "not-cargo".to_string(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        };
        let result = CargoAdapter::plan(&adapter, &inst, &req).await;
        match result {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_plan_install_without_binstall_compiles_and_warns() {
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()))
                .with_binstall(None);
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        };
        let plan = CargoAdapter::plan(&adapter, &inst, &req)
            .await
            .expect("plan");
        assert_eq!(
            command_program(&plan),
            PathBuf::from("/Users/brulek/.cargo/bin/cargo")
        );
        assert_eq!(
            command_args(&plan),
            vec![
                "install",
                "--root",
                "/Users/brulek/.cargo",
                "--index",
                CRATES_IO_INDEX,
                "hexyl"
            ]
        );
        assert_eq!(plan.warnings, vec![Warning::CompilesLocally]);
    }

    #[tokio::test]
    async fn test_plan_install_with_binstall_skips_the_compile_warning() {
        // The upgrade's twin is
        // `regression_a_plain_cargo_install_record_upgrades_through_cargo_binstall`,
        // planned from the recorded record. cargo-binstall is given
        // crates.io's sparse index, its own default, never the git URL
        // cargo is given: with that one it clones the index from github.com.
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()))
                .with_binstall(Some(PathBuf::from(BINSTALL)));
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        };
        let plan = CargoAdapter::plan(&adapter, &inst, &req)
            .await
            .expect("plan");
        assert_eq!(command_program(&plan), PathBuf::from(BINSTALL));
        assert_eq!(
            command_args(&plan),
            vec![
                "-y",
                "--root",
                "/Users/brulek/.cargo",
                "--index",
                CRATES_IO_SPARSE_INDEX,
                "hexyl"
            ]
        );
        assert!(!command_args(&plan).iter().any(|a| a.contains("github.com")));
        assert!(plan.warnings.is_empty());
    }

    #[tokio::test]
    async fn test_plan_uninstall_never_uses_binstall() {
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()))
                .with_binstall(Some(PathBuf::from(
                    "/Users/brulek/.cargo/bin/cargo-binstall",
                )));
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        };
        let plan = CargoAdapter::plan(&adapter, &inst, &req)
            .await
            .expect("plan");
        assert_eq!(
            command_program(&plan),
            PathBuf::from("/Users/brulek/.cargo/bin/cargo")
        );
        assert_eq!(
            command_args(&plan),
            vec!["uninstall", "--root", "/Users/brulek/.cargo", "hexyl"]
        );
        // Said under the tool: cargo deletes the binaries its install
        // record lists for the crate, and nothing else.
        assert_eq!(
            plan.warnings,
            vec![Warning::UninstallScope {
                what: UninstallScope::Cargo
            }]
        );
    }

    #[tokio::test]
    async fn test_every_cargo_plan_carries_rustups_auto_install_off() {
        // On a rustup Mac `cargo` is the rustup binary in proxy mode
        // (rustup 1.29.1 src/cli/proxy_mode.rs:14-59): before it runs
        // cargo's own arguments it resolves the active toolchain and, when
        // that toolchain is not installed, installs it -- unless
        // `RUSTUP_AUTO_INSTALL=0` or `rustup set auto-install disable`
        // turned that off (`should_auto_install`, config.rs:435-441). A
        // confirmed install, upgrade or uninstall must not begin with a
        // toolchain download its preview never named, so every plan
        // carries the switch -- the cargo-binstall ones too, so that a
        // `cargo` or `rustc` cargo-binstall starts inherits it.
        let home = temp_cargo_home("plan-upgrade");
        std::fs::create_dir_all(&home).unwrap();
        // The recorded record, so that with cargo-binstall the upgrade is
        // binstall's (it holds no build choice) and both programs are seen.
        std::fs::write(home.join(".crates2.json"), recorded_crates2()).unwrap();
        let inst = test_instance(home.clone());
        for binstall in [None, Some(PathBuf::from(BINSTALL))] {
            let adapter =
                CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()))
                    .with_binstall(binstall.clone());
            for kind in [OpKind::Install, OpKind::Upgrade, OpKind::Uninstall] {
                let req = OpRequest {
                    kind,
                    instance_id: inst.id.clone(),
                    artifact_kind: ArtifactKind::Binary,
                    name: "hexyl".to_string(),
                };
                let plan = CargoAdapter::plan(&adapter, &inst, &req)
                    .await
                    .expect("plan");
                assert_eq!(
                    command_env(&plan),
                    [("RUSTUP_AUTO_INSTALL".to_string(), "0".to_string())],
                    "binstall={binstall:?} {kind:?}"
                );
            }
        }
        std::fs::remove_dir_all(home).unwrap();
    }

    #[tokio::test]
    async fn test_execute_runs_cargo_uninstall_with_rustups_auto_install_off() {
        // The plan's environment is what the runner is handed: `execute`
        // (through `run_plan`) runs `cargo uninstall hexyl` with the
        // switch, so rustup's proxy cannot install a toolchain first.
        let runner = Arc::new(EnvRecordingRunner {
            specs: Mutex::new(Vec::new()),
        });
        let adapter =
            CargoAdapter::new(runner.clone(), Arc::new(MockHttpClient::new())).with_binstall(None);
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        };
        let plan = CargoAdapter::plan(&adapter, &inst, &req)
            .await
            .expect("plan");
        let outcome = CargoAdapter::execute(
            &adapter,
            &plan,
            Arc::new(VecSink::new()),
            1,
            CancellationToken::new(),
        )
        .await
        .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        let specs = runner.specs.lock().unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(
            specs[0].program,
            PathBuf::from("/Users/brulek/.cargo/bin/cargo")
        );
        assert_eq!(
            specs[0].args,
            vec!["uninstall", "--root", "/Users/brulek/.cargo", "hexyl"]
        );
        assert_eq!(
            specs[0].env,
            vec![("RUSTUP_AUTO_INSTALL".to_string(), "0".to_string())]
        );
    }

    #[tokio::test]
    async fn test_execute_streams_log_events_and_succeeds() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![
                "/Users/brulek/.cargo/bin/cargo",
                "install",
                "--root",
                "/Users/brulek/.cargo",
                "--index",
                CRATES_IO_INDEX,
                "hexyl",
            ],
            CommandOutput {
                exit_code: Some(0),
                stdout: "Installing hexyl\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter =
            CargoAdapter::new(runner, Arc::new(MockHttpClient::new())).with_binstall(None);
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        };
        let plan = CargoAdapter::plan(&adapter, &inst, &req)
            .await
            .expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome =
            CargoAdapter::execute(&adapter, &plan, sink.clone(), 1, CancellationToken::new())
                .await
                .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(sink.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_and_absent() {
        let json = std::fs::read_to_string("../../adapters/fixtures/cargo/1.98.1/crates2.json")
            .expect("read cargo crates2.json fixture");
        let home = temp_cargo_home("reconcile");
        std::fs::create_dir_all(&home).expect("create cargo home");
        std::fs::write(home.join(".crates2.json"), &json).expect("write crates2.json");
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(home.clone());
        let present = CargoAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey {
                instance_id: inst.id.clone(),
                kind: ArtifactKind::Binary,
                name: "hexyl".to_string(),
            },
        )
        .await
        .expect("reconcile present");
        assert!(present.present);
        let absent = CargoAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey {
                instance_id: inst.id.clone(),
                kind: ArtifactKind::Binary,
                name: "missing".to_string(),
            },
        )
        .await
        .expect("reconcile absent");
        assert!(!absent.present);
        let _ = std::fs::remove_dir_all(&home);
    }

    fn fake_exe(dir: &std::path::Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"#!/bin/sh\n").expect("write fake executable");
        path
    }

    fn version_runner(cargo_path: &str, stdout: &str) -> Arc<MockRunner> {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![cargo_path, "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: stdout.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner
    }

    #[tokio::test]
    async fn test_detect_keys_the_instance_on_cargo_home_and_caches_the_binstall_path() {
        // `cargo_home` is the whole reason HostEnv grew a field in this task:
        // a machine with CARGO_HOME set keeps its crates somewhere other than
        // ~/.cargo, and `inventory` reads `.crates2.json` out of the instance
        // prefix. This also pins the hand-off detect -> plan: the program the
        // preview names is the path detect resolved, not a fresh guess.
        let dir = crate::testing::unique_temp_path("cargo-detect");
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let cargo_path = fake_exe(&dir, "cargo");
        let binstall_path = fake_exe(&dir, "cargo-binstall");
        let runner = version_runner(
            cargo_path.to_str().expect("utf8 path"),
            "cargo 1.98.1 (797e8a9bc 2026-08-05)\n",
        );
        let adapter = CargoAdapter::new(runner, Arc::new(MockHttpClient::new()));
        let env = HostEnv {
            path_dirs: vec![dir.clone()],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: Some(PathBuf::from("/opt/cargo")),
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let instances = CargoAdapter::detect(&adapter, &env).await;

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].id, "cargo:/opt/cargo");
        assert_eq!(instances[0].prefix, PathBuf::from("/opt/cargo"));
        assert_eq!(instances[0].exe_path, cargo_path);
        assert_eq!(instances[0].version, Some("1.98.1".to_string()));
        assert!(instances[0].available());
        assert!(
            instances[0].unverified_version.is_none(),
            "1.98.1 is verified in adapters/meta/cargo.toml"
        );

        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: instances[0].id.clone(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        };
        let plan = CargoAdapter::plan(&adapter, &instances[0], &req)
            .await
            .expect("plan");
        assert_eq!(command_program(&plan), binstall_path);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_detect_falls_back_to_home_dot_cargo_and_flags_an_unverified_version() {
        let dir = crate::testing::unique_temp_path("cargo-detect-default");
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let cargo_path = fake_exe(&dir, "cargo");
        let runner = version_runner(
            cargo_path.to_str().expect("utf8 path"),
            "cargo 99.9.9 (deadbeef 2099-01-01)\n",
        );
        let adapter = CargoAdapter::new(runner, Arc::new(MockHttpClient::new()));
        let env = HostEnv {
            path_dirs: vec![dir.clone()],
            home: PathBuf::from("/Users/brulek"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let instances = CargoAdapter::detect(&adapter, &env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].id, "cargo:/Users/brulek/.cargo");
        assert_eq!(
            instances[0].unverified_version,
            Some("99.9.9".to_string()),
            "99.9.9 is not in adapters/meta/cargo.toml's verified_versions"
        );
    }

    #[tokio::test]
    async fn test_detect_returns_no_instance_when_cargo_is_not_on_path() {
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let env = HostEnv {
            path_dirs: vec![PathBuf::from("/definitely/not/a/real/path")],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        assert!(CargoAdapter::detect(&adapter, &env).await.is_empty());
    }

    #[tokio::test]
    async fn test_search_is_unsupported() {
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let result = <CargoAdapter as Adapter>::search(&adapter, &inst, "hexyl").await;
        assert!(matches!(result, Err(AdapterError::Unsupported(_))));
    }

    #[tokio::test]
    async fn test_latest_stable_version_percent_encodes_the_crate_name_into_the_url() {
        // The crate name comes off disk, out of `.crates2.json`'s keys —
        // a file Banager does not write. Interpolated raw, a `/` in it adds
        // path segments to crates.io's API and a `?` starts a query string.
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "https://crates.io/api/v1/crates/evil%2F..%2Fsummary%3Fx=1",
            HttpResponse {
                status: 200,
                body: r#"{"crate":{"max_stable_version":"1.0.0"}}"#.to_string(),
            },
        );
        let adapter = CargoAdapter::new(Arc::new(MockRunner::new()), http.clone());

        let latest = adapter.latest_stable_version("evil/../summary?x=1").await;

        assert_eq!(latest.as_deref(), Ok("1.0.0"));
        assert_eq!(
            http.calls(),
            vec!["https://crates.io/api/v1/crates/evil%2F..%2Fsummary%3Fx=1".to_string()]
        );
    }

    #[tokio::test]
    async fn test_latest_stable_version_holds_to_the_shared_lookup_failure_table() {
        crate::adapters::lookup_cases::hold_to_the_table(
            "https://crates.io/api/v1/crates/hexyl",
            "crates.io request failed",
            "crates.io",
            |http| async move {
                CargoAdapter::new(Arc::new(MockRunner::new()), http)
                    .latest_stable_version("hexyl")
                    .await
            },
        )
        .await;
    }
}
