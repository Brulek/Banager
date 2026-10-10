//! How the refresh-side work fares with about 5,000 installed things, as
//! on a Mac whose owner has several thousand Homebrew formulae and casks
//! (the window's side of the same question is the preview's `?state=huge`,
//! docs/ui-preview.md "Large list"). Benchmarks, not checks: each test is
//! `#[ignore]`d, prints how long its step took, and fails only past a
//! bound far above what it takes, so a slow machine does not fail it but
//! work that grows with the square of the list does.
//!
//! Run them with
//! `cargo test -p banager-core --release --test scale_test -- --ignored --nocapture`.
//!
//! Every directory tree is built under the temp directory and removed at
//! the end; nothing else is read or written.

use async_trait::async_trait;
use banager_core::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
use banager_core::commands::{bin_folders, judge, read_folders, CommandBudget};
use banager_core::events::{EventSink, OpId, VecSink};
use banager_core::families;
use banager_core::model::{
    ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, ManagerInstance, OpRequest,
    Outcome, Plan, Reconciled, SearchHit, UpdateCandidate, UpdateChannel,
};
use banager_core::runner::HostEnv;
use banager_core::session::Session;
use banager_core::size::{SizeBudget, SizeMeter};
use banager_core::testing::manager_instance;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// How many of each the Mac has: about 5,000 in all, times
/// `BANAGER_SCALE` (default 1) -- run with 2 to see whether a step's time
/// doubles or grows four times over.
const FORMULAE: usize = 3_600;
const CASKS: usize = 400;
const NPM: usize = 450;
const PIPX: usize = 150;
const UV: usize = 150;
const CARGO: usize = 250;

fn scale() -> usize {
    std::env::var("BANAGER_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| n >= 1)
        .unwrap_or(1)
}

/// A fresh folder for one test, removed when it ends; canonical, as
/// `realpath` answers (`/var` is a link to `/private/var` on a Mac).
struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Home {
        let raw = std::env::temp_dir().join(format!(
            "banager-scale-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&raw).expect("create temp home");
        Home(fs::canonicalize(&raw).expect("canonical temp home"))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn at(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }

    fn exe(&self, rel: &str) -> PathBuf {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).expect("create parent");
        fs::write(&path, b"#!/bin/sh\n").expect("write file");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    fn link(&self, rel: &str, target: &Path) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).expect("create parent");
        symlink(target, &path).expect("symlink");
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn instance(adapter_id: &str, id: &str, prefix: &Path) -> ManagerInstance {
    ManagerInstance {
        prefix: prefix.to_path_buf(),
        exe_path: prefix.join("bin").join(adapter_id),
        ..manager_instance(adapter_id, id)
    }
}

fn artifact(instance_id: &str, kind: ArtifactKind, name: &str) -> InstalledArtifact {
    banager_core::testing::installed_artifact(instance_id, kind, name)
}

/// The Mac: a Homebrew in `<home>/brew` whose every formula has a keg in
/// its Cellar with a command linked into `bin`, and whose every cask has
/// a folder in its Caskroom; an npm in `<home>/npm` with a command per
/// package; pipx's, uv's and Cargo's tools with a folder each. Some names
/// are real AI coding tools', so `families::assign` finds some.
struct Mac {
    home: Home,
    instances: Vec<ManagerInstance>,
    artifacts: Vec<InstalledArtifact>,
    path: Vec<PathBuf>,
}

fn mac(tag: &str, on_disk: bool) -> Mac {
    let home = Home::new(tag);
    let brew = instance("brew", "brew:/scale/brew", &home.at("brew"));
    let npm = instance("npm", "npm:/scale/npm", &home.at("npm"));
    let pipx = instance("pipx", "pipx", &home.at(".local/pipx"));
    let uv = instance("uv", "uv", &home.at(".local/share/uv"));
    let cargo = instance("cargo", "cargo:/scale/.cargo", &home.at(".cargo"));
    let mut artifacts = Vec::new();
    for i in 0..FORMULAE * scale() {
        let name = if i == 0 {
            "gemini-cli".to_string()
        } else {
            format!("formula{i:04}")
        };
        if on_disk {
            let keg = format!("brew/Cellar/{name}/1.0");
            let real = home.exe(&format!("{keg}/bin/{name}"));
            fs::write(home.at(&format!("{keg}/README")), b"readme").unwrap();
            home.link(&format!("brew/bin/{name}"), &real);
        }
        let mut row = artifact(&brew.id, ArtifactKind::Formula, &name);
        if i % 6 == 0 {
            row.reason = InstallReason::Dependency;
        }
        artifacts.push(row);
    }
    for i in 0..CASKS * scale() {
        let name = if i == 0 {
            "claude-code".to_string()
        } else {
            format!("cask{i:04}")
        };
        if on_disk {
            fs::create_dir_all(home.at(&format!("brew/Caskroom/{name}/1.0"))).unwrap();
        }
        let mut row = artifact(&brew.id, ArtifactKind::Cask, &name);
        row.path = Some(home.at(&format!("Applications/{name}.app")));
        if on_disk {
            home.exe(&format!("Applications/{name}.app/Contents/MacOS/{name}"));
        }
        artifacts.push(row);
    }
    for i in 0..NPM * scale() {
        let name = if i == 0 {
            "@openai/codex".to_string()
        } else {
            format!("package{i:04}")
        };
        let command = name.rsplit('/').next().unwrap().to_string();
        if on_disk {
            let real = home.exe(&format!("npm/lib/node_modules/{name}/bin/cli.js"));
            home.link(&format!("npm/bin/{command}"), &real);
        }
        artifacts.push(artifact(&npm.id, ArtifactKind::Package, &name));
    }
    for (inst, count, folder) in [
        (&pipx, PIPX * scale(), ".local/pipx/venvs"),
        (&uv, UV * scale(), ".local/share/uv/tools"),
    ] {
        for i in 0..count {
            let name = if i == 0 && inst.adapter_id == "pipx" {
                "aider-chat".to_string()
            } else {
                format!("{}tool{i:04}", inst.adapter_id)
            };
            if on_disk {
                home.exe(&format!("{folder}/{name}/bin/{name}"));
            }
            let mut row = artifact(&inst.id, ArtifactKind::Tool, &name);
            row.path = Some(home.at(&format!("{folder}/{name}")));
            artifacts.push(row);
        }
    }
    for i in 0..CARGO * scale() {
        let name = format!("crate{i:04}");
        if on_disk {
            home.exe(&format!(".cargo/bin/{name}"));
        }
        let mut row = artifact(&cargo.id, ArtifactKind::Binary, &name);
        row.path = Some(home.at(&format!(".cargo/bin/{name}")));
        artifacts.push(row);
    }
    let path = vec![
        home.at("npm/bin"),
        home.at("brew/bin"),
        home.at(".cargo/bin"),
    ];
    Mac {
        home,
        instances: vec![brew, npm, pipx, uv, cargo],
        artifacts,
        path,
    }
}

/// `f`'s time, its fastest of `runs`: the least noise.
fn fastest<T>(runs: usize, mut f: impl FnMut() -> T) -> (Duration, T) {
    let mut best = Duration::MAX;
    let mut last = None;
    for _ in 0..runs {
        let started = Instant::now();
        let out = f();
        best = best.min(started.elapsed());
        last = Some(out);
    }
    (best, last.unwrap())
}

#[test]
#[ignore = "benchmark: run with --ignored --nocapture"]
fn scale_families_assign_5000() {
    let Mac {
        instances,
        mut artifacts,
        ..
    } = mac("families", false);
    assert!(artifacts.len() >= 5_000);
    let (took, ()) = fastest(5, || families::assign(&instances, &mut artifacts));
    let found = artifacts
        .iter()
        .filter(|a| a.facts.family.is_some())
        .count();
    println!(
        "families::assign, {} artifacts: {:.2} ms ({found} in a family)",
        artifacts.len(),
        took.as_secs_f64() * 1e3
    );
    assert!(found >= 3);
    assert!(took < Duration::from_millis(500), "took {took:?}");
}

#[test]
#[ignore = "benchmark: run with --ignored --nocapture"]
fn scale_commands_read_and_judge_5000() {
    let mac = mac("commands", true);
    let started = Instant::now();
    let (read, folders) = fastest(3, || {
        read_folders(
            &mac.path,
            &bin_folders(&mac.instances),
            mac.home.path(),
            CommandBudget::default(),
        )
    });
    assert!(folders.complete(), "the folders were not all read");
    let (judged, verdicts) = fastest(3, || {
        judge(
            &folders,
            &mac.instances,
            &mac.artifacts,
            mac.home.path(),
            true,
            CommandBudget::default(),
        )
    });
    let verdicts = verdicts.expect("judged within the budget");
    let with_commands = verdicts.iter().filter(|v| !v.is_empty()).count();
    println!(
        "commands, {} artifacts ({with_commands} with commands): read_folders {:.1} ms, judge {:.1} ms (all runs {:.0} ms)",
        mac.artifacts.len(),
        read.as_secs_f64() * 1e3,
        judged.as_secs_f64() * 1e3,
        started.elapsed().as_secs_f64() * 1e3
    );
    // Every formula and package with its link, and pipx's and Cargo's tools.
    assert!(with_commands >= (FORMULAE + NPM) * scale());
    assert!(judged < Duration::from_secs(2), "judge took {judged:?}");
}

#[test]
#[ignore = "benchmark: run with --ignored --nocapture"]
fn scale_sizes_round_5000() {
    let mac = mac("sizes", true);
    let rounds = Arc::new(std::sync::Mutex::new(Vec::<Instant>::new()));
    let seen = Arc::clone(&rounds);
    let meter = SizeMeter::new(
        SizeBudget {
            max_entries: 1_000_000,
            max_duration: Duration::from_secs(60),
        },
        move |_| seen.lock().unwrap().push(Instant::now()),
    );
    let mut times = Vec::new();
    for round in 1..=2u64 {
        let started = Instant::now();
        meter.measure(round, &mac.instances, &mac.artifacts, mac.home.path());
        let first_shown = loop {
            if let Some(&first) = rounds.lock().unwrap().first() {
                break first;
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        while !meter.sizes().done || meter.sizes().round != round {
            assert!(started.elapsed() < Duration::from_secs(60), "never done");
            std::thread::sleep(Duration::from_millis(2));
        }
        let sizes = meter.sizes();
        let measured = sizes
            .artifacts
            .iter()
            .filter(|s| s.measured.is_some())
            .count();
        times.push((
            first_shown.duration_since(started),
            started.elapsed(),
            measured,
        ));
        rounds.lock().unwrap().clear();
    }
    for (round, (planned, done, measured)) in times.iter().enumerate() {
        println!(
            "sizes round {}, {} artifacts: planned and shown {:.1} ms, done {:.1} ms ({measured} measured)",
            round + 1,
            mac.artifacts.len(),
            planned.as_secs_f64() * 1e3,
            done.as_secs_f64() * 1e3
        );
    }
    assert!(times[0].2 >= (FORMULAE + NPM) * scale());
    // The second round finds every folder in the cache: no walk at all.
    assert!(times[1].1 < Duration::from_secs(5), "{:?}", times[1]);
}

// ------------------------------------------------------- through a Session

/// One source as the test describes it: the same rows every round, and
/// an update for one in seven.
struct Fixed {
    meta: AdapterMeta,
    instance: ManagerInstance,
    artifacts: Vec<InstalledArtifact>,
    /// Every update check after the first fails, as when the network goes:
    /// the round then carries last round's updates forward, less those its
    /// own list disproves.
    fail_later_checks: bool,
    checks: AtomicUsize,
}

#[async_trait]
impl Adapter for Fixed {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
        vec![self.instance.clone()]
    }

    async fn inventory(
        &self,
        _inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        Ok(self.artifacts.clone())
    }

    async fn check_updates(
        &self,
        _inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        if self.checks.fetch_add(1, Ordering::SeqCst) > 0 && self.fail_later_checks {
            return Err(AdapterError::Unsupported("the network went".to_string()));
        }
        Ok(CheckOutcome {
            candidates: self
                .artifacts
                .iter()
                .step_by(7)
                .map(|a| UpdateCandidate {
                    key: a.key.clone(),
                    current: a.version.clone(),
                    target: "2.0".to_string(),
                    channel: UpdateChannel::Native,
                    checkable: true,
                    warnings: vec![],
                    blocked: None,
                    download_bytes: None,
                })
                .collect(),
            notes: vec![],
        })
    }

    async fn search(
        &self,
        _inst: &ManagerInstance,
        _query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        Ok(Vec::new())
    }

    async fn plan(&self, _inst: &ManagerInstance, _req: &OpRequest) -> Result<Plan, AdapterError> {
        Err(AdapterError::Unsupported("test source".to_string()))
    }

    async fn execute(
        &self,
        _plan: &Plan,
        _sink: Arc<dyn EventSink>,
        _op_id: OpId,
        _cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        Ok(Outcome::Succeeded)
    }

    async fn reconcile(
        &self,
        _inst: &ManagerInstance,
        _key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        Err(AdapterError::Unsupported("test source".to_string()))
    }
}

/// A session over `mac`'s sources, and the `HostEnv` its refreshes see.
fn session_over(mac: &Mac, fail_later_checks: bool) -> (Arc<Session>, HostEnv) {
    let adapters: Vec<Arc<dyn Adapter>> = mac
        .instances
        .iter()
        .map(|inst| {
            Arc::new(Fixed {
                meta: AdapterMeta {
                    id: inst.adapter_id.clone(),
                    name: inst.adapter_id.clone(),
                    kind: "test".to_string(),
                    platforms: vec!["macos".to_string()],
                    homepage: "https://example.invalid".to_string(),
                    schema_version: 1,
                    verified_versions: vec![],
                },
                instance: inst.clone(),
                artifacts: mac
                    .artifacts
                    .iter()
                    .filter(|a| a.key.instance_id == inst.id)
                    .cloned()
                    .collect(),
                fail_later_checks,
                checks: AtomicUsize::new(0),
            }) as Arc<dyn Adapter>
        })
        .collect();
    let session = Session::with_adapters(Arc::new(VecSink::new()), adapters, None);
    session.note_login_path(true);
    let env = HostEnv {
        path_dirs: mac.path.clone(),
        home: mac.home.path().to_path_buf(),
        euid: 501,
        cargo_home: None,
        rustup_home: None,
        zdotdir: None,
        ollama_host: None,
    };
    (session, env)
}

fn millis(times: &[Duration]) -> String {
    times
        .iter()
        .map(|t| format!("{:.1} ms", t.as_secs_f64() * 1e3))
        .collect::<Vec<_>>()
        .join(", ")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "benchmark: run with --ignored --nocapture"]
async fn scale_refresh_5000() {
    let mac = mac("refresh", true);
    let (session, env) = session_over(&mac, false);
    let mut times = Vec::new();
    for _ in 0..3 {
        let started = Instant::now();
        let snapshot = session.refresh(&env, &CheckOptions::default()).await;
        times.push(started.elapsed());
        assert_eq!(snapshot.artifacts.len(), mac.artifacts.len());
        assert!(snapshot.updates.len() >= mac.artifacts.len() / 7);
    }
    println!(
        "Session::refresh, {} artifacts: {}",
        mac.artifacts.len(),
        millis(&times)
    );
    assert!(
        times.iter().min().unwrap() < &Duration::from_secs(5),
        "{times:?}"
    );
}

/// The rounds after the first, whose update checks fail: each keeps last
/// round's updates that its own list does not disprove, looked up by key.
/// No file on disk, so that this, not which copy of a command runs, is
/// what the rounds spend their time on.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "benchmark: run with --ignored --nocapture"]
async fn scale_refresh_carrying_updates_5000() {
    let mac = mac("carry", false);
    let (session, env) = session_over(&mac, true);
    let first = session.refresh(&env, &CheckOptions::default()).await;
    let mut times = Vec::new();
    for _ in 0..3 {
        let started = Instant::now();
        let snapshot = session.refresh(&env, &CheckOptions::default()).await;
        times.push(started.elapsed());
        assert_eq!(snapshot.updates.len(), first.updates.len());
        assert_eq!(snapshot.errors.len(), mac.instances.len());
    }
    println!(
        "Session::refresh carrying {} updates over {} artifacts: {}",
        first.updates.len(),
        mac.artifacts.len(),
        millis(&times)
    );
    assert!(
        times.iter().min().unwrap() < &Duration::from_secs(5),
        "{times:?}"
    );
}
