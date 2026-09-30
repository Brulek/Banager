//! A refresh over the native Claude Code install while the install changes
//! under it (phase 4 step B, Astra finding B-2): the real
//! `StandaloneAdapter` over a synthetic layout in a throwaway home, driven
//! through `Session::refresh` the way the window drives it. `refresh`
//! detects, then reads the inventory and checks for updates, and the disk
//! can change between any two of those reads. The two pages must still
//! agree with each other and with the instance row: when the launcher or
//! its program files go away between detect and the inventory, the refresh
//! says it did not finish for this source and keeps the previous round's
//! rows (the next refresh lists what is there); when Claude Code updates
//! itself between the inventory and the check, both pages show the version
//! the inventory read, never one version each.
//!
//! No recorded fixture: nothing here runs a command but the scripted
//! `--version`, and every layout is built by the test (spec §9.3). The
//! `--version` runner is wrapped so a test can change the disk right after
//! a given read answers -- the only place the window's own sequence can be
//! interrupted without a seam in production code.

use async_trait::async_trait;
use banager_core::adapters::standalone::recipes::CLAUDE;
use banager_core::adapters::standalone::StandaloneAdapter;
use banager_core::adapters::{Adapter, CheckOptions};
use banager_core::events::VecSink;
use banager_core::http::{HttpResponse, MockHttpClient};
use banager_core::model::{InstalledArtifact, InstanceNote, UpdateCandidate};
use banager_core::runner::{
    CommandOutput, CommandRunner, CommandSpec, HostEnv, LineCallback, MockRunner, RunnerError,
};
use banager_core::session::{Session, Snapshot};
use banager_core::trash::MockTrasher;
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const INSTANCE: &str = "standalone-claude";
const LATEST_URL: &str = "https://downloads.claude.ai/claude-code-releases/latest";

/// A fresh home directory for one test, removed when the test ends.
/// Canonical, so the paths a test builds compare equal to what the
/// adapter resolves (macOS's `/var/folders` is `/private/var/…`).
struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Home {
        let raw = std::env::temp_dir().join(format!(
            "canager-refresh-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&raw).expect("create temp home");
        Home(std::fs::canonicalize(&raw).expect("canonical temp home"))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// `HostEnv` for this home, as the user who owns it (the one running
    /// the test), with nothing on `PATH`: every refresh then gives the row
    /// the same `NotOnPath` notice, so the rows of two refreshes compare
    /// equal when nothing else about them differs.
    fn env(&self) -> HostEnv {
        HostEnv {
            path_dirs: Vec::new(),
            home: self.0.clone(),
            euid: std::fs::metadata(&self.0).expect("stat home").uid(),
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        }
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The native Claude Code layout the installer writes: an executable
/// `~/.local/share/claude/versions/<version>` that is never run
/// (`--version` is scripted) and `~/.local/bin/claude`, one absolute link
/// to it.
struct Layout {
    launcher: PathBuf,
    root: PathBuf,
}

fn claude_layout(home: &Path, version: &str) -> Layout {
    let root = home.join(".local/share/claude");
    let real = program(&root, version);
    let launcher = home.join(".local/bin/claude");
    std::fs::create_dir_all(launcher.parent().unwrap()).expect("bin dir");
    std::os::unix::fs::symlink(&real, &launcher).expect("the launcher");
    Layout { launcher, root }
}

/// One executable `versions/<version>` under `root`.
fn program(root: &Path, version: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let real = root.join("versions").join(version);
    std::fs::create_dir_all(real.parent().unwrap()).expect("versions dir");
    std::fs::write(&real, b"#!/bin/sh\n").expect("the program");
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o755)).expect("executable");
    real
}

fn exited_0(stdout: &str) -> CommandOutput {
    CommandOutput {
        exit_code: Some(0),
        stdout: stdout.to_string(),
        stderr: String::new(),
        timed_out: false,
        cancelled: false,
    }
}

/// `MockRunner`, with one thing added: a closure to run right after the
/// n-th `--version` read has been answered (n counted from 1 across the
/// runner's life), which is how a test changes the disk between two reads
/// of one refresh. Each hook runs once.
struct AfterVersionReads {
    inner: Arc<MockRunner>,
    reads: AtomicUsize,
    hooks: Mutex<HashMap<usize, Box<dyn FnOnce() + Send>>>,
}

impl AfterVersionReads {
    fn new(inner: Arc<MockRunner>) -> AfterVersionReads {
        AfterVersionReads {
            inner,
            reads: AtomicUsize::new(0),
            hooks: Mutex::new(HashMap::new()),
        }
    }

    fn after_read(&self, n: usize, hook: impl FnOnce() + Send + 'static) {
        self.hooks.lock().unwrap().insert(n, Box::new(hook));
    }

    /// How many `--version` reads have been answered so far.
    fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl CommandRunner for AfterVersionReads {
    async fn run(
        &self,
        spec: CommandSpec,
        on_line: Option<LineCallback>,
        cancel: CancellationToken,
    ) -> Result<CommandOutput, RunnerError> {
        let is_version_read = spec.args == ["--version"];
        let output = self.inner.run(spec, on_line, cancel).await;
        if is_version_read {
            let n = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
            let hook = self.hooks.lock().unwrap().remove(&n);
            if let Some(hook) = hook {
                hook();
            }
        }
        output
    }
}

/// A session over one standalone Claude Code adapter whose `--version`
/// answers `version` through `hooked`, and whose release channel points at
/// `published`.
fn session_over(
    layout: &Layout,
    version: &str,
    published: &str,
) -> (Arc<Session>, Arc<MockRunner>, Arc<AfterVersionReads>) {
    let runner = Arc::new(MockRunner::new());
    runner.respond(
        vec![layout.launcher.to_str().unwrap(), "--version"],
        exited_0(&format!("{version} (Claude Code)\n")),
    );
    let hooked = Arc::new(AfterVersionReads::new(runner.clone()));
    let http = Arc::new(MockHttpClient::new());
    http.respond(
        LATEST_URL,
        HttpResponse {
            status: 200,
            body: format!("{published}\n"),
        },
    );
    let adapter =
        StandaloneAdapter::new(&CLAUDE, hooked.clone(), http, Arc::new(MockTrasher::new()))
            .with_trash_gap(Duration::ZERO);
    let session = Session::with_adapters(
        Arc::new(VecSink::new()),
        vec![Arc::new(adapter) as Arc<dyn Adapter>],
        None,
    );
    (session, runner, hooked)
}

fn rows_of(snapshot: &Snapshot) -> Vec<&InstalledArtifact> {
    snapshot
        .artifacts
        .iter()
        .filter(|a| a.key.instance_id == INSTANCE)
        .collect()
}

fn candidates_of(snapshot: &Snapshot) -> Vec<&UpdateCandidate> {
    snapshot
        .updates
        .iter()
        .filter(|u| u.key.instance_id == INSTANCE)
        .collect()
}

/// Two refreshes: one with the install whole, then one during which
/// `vanish` runs right after detect's `--version` read -- after detect
/// has looked at the launcher and read its version, before the inventory
/// looks. Detect then lists the install as it read it (its `PATH` look,
/// which comes after the read, finds nothing on the empty `PATH` either
/// way), and the inventory and the check find what `vanish` left. Returns
/// both snapshots, the session and the home for the refresh after.
async fn vanishing_refresh(
    tag: &str,
    vanish: impl FnOnce(&Layout) + Send + 'static,
) -> (Snapshot, Snapshot, Arc<Session>, Home) {
    let home = Home::new(tag);
    let layout = claude_layout(home.path(), "2.1.281");
    // A published version ahead of the installed one: a candidate the
    // second refresh has to keep rather than replace with "up to date".
    let (session, _runner, hooked) = session_over(&layout, "2.1.281", "2.1.290");

    let first = session.refresh(&home.env(), &CheckOptions::default()).await;
    assert!(!first.stale, "{:?}", first.errors);
    assert_eq!(rows_of(&first).len(), 1);
    assert_eq!(rows_of(&first)[0].version, "2.1.281");
    assert_eq!(candidates_of(&first).len(), 1);
    assert_eq!(candidates_of(&first)[0].target, "2.1.290");
    // The first refresh read `--version` twice (detect, inventory); the
    // second refresh's detect is the third read.
    assert_eq!(hooked.reads(), 2);
    hooked.after_read(3, move || vanish(&layout));

    let second = session.refresh(&home.env(), &CheckOptions::default()).await;
    (first, second, session, home)
}

/// What the second refresh of `vanishing_refresh` must say, whatever
/// vanished: detect's row as it was, a refresh that did not finish for
/// this source, and the previous round's rows on both pages -- not an
/// empty-version row beside "Everything is up to date".
fn assert_stale_with_the_previous_rows(first: &Snapshot, second: &Snapshot) {
    let listed = |snapshot: &Snapshot| {
        snapshot
            .instances
            .iter()
            .find(|i| i.id == INSTANCE)
            .cloned()
            .expect("detect listed the install")
    };
    let inst = listed(second);
    assert_eq!(
        inst,
        listed(first),
        "detect's row, exactly as the first refresh listed it"
    );
    assert_eq!(inst.version.as_deref(), Some("2.1.281"));
    assert_eq!(inst.status.unavailable, None);
    assert!(
        !inst.status.notes.contains(&InstanceNote::LauncherOnly),
        "{:?}",
        inst.status.notes
    );
    assert!(second.stale, "the refresh did not finish for this source");
    assert!(
        second.errors.iter().any(|e| e.instance_id == INSTANCE),
        "{:?}",
        second.errors
    );
    assert_eq!(rows_of(second), rows_of(first));
    assert_eq!(candidates_of(second), candidates_of(first));
}

#[tokio::test]
async fn test_program_files_gone_between_detect_and_inventory_keep_the_previous_rows_and_mark_the_refresh_stale(
) {
    let (first, second, session, home) = vanishing_refresh("files-vanish", |layout| {
        std::fs::remove_dir_all(layout.root.join("versions")).expect("remove the program files");
    })
    .await;
    assert_stale_with_the_previous_rows(&first, &second);

    // The refresh after lists what is there: the launcher alone, with its
    // notice, no version and no update to check.
    let third = session.refresh(&home.env(), &CheckOptions::default()).await;
    assert!(!third.stale, "{:?}", third.errors);
    let inst = third
        .instances
        .iter()
        .find(|i| i.id == INSTANCE)
        .expect("the dangling launcher is still a row");
    assert_eq!(inst.version, None);
    assert_eq!(inst.status.notes, vec![InstanceNote::LauncherOnly]);
    assert_eq!(rows_of(&third).len(), 1);
    assert_eq!(rows_of(&third)[0].version, "");
    assert_eq!(rows_of(&third)[0].path, None);
    assert!(candidates_of(&third).is_empty());
}

#[tokio::test]
async fn test_a_launcher_gone_between_detect_and_inventory_keeps_the_previous_rows_and_marks_the_refresh_stale(
) {
    let (first, second, session, home) = vanishing_refresh("launcher-vanishes", |layout| {
        std::fs::remove_file(&layout.launcher).expect("remove the launcher");
    })
    .await;
    assert_stale_with_the_previous_rows(&first, &second);

    // The refresh after has no Claude Code at all.
    let third = session.refresh(&home.env(), &CheckOptions::default()).await;
    assert!(!third.stale, "{:?}", third.errors);
    assert!(third.instances.iter().all(|i| i.id != INSTANCE));
    assert!(rows_of(&third).is_empty());
    assert!(candidates_of(&third).is_empty());
}

#[tokio::test]
async fn test_a_self_update_between_inventory_and_the_check_shows_one_version_on_both_pages() {
    let home = Home::new("self-update");
    let layout = claude_layout(home.path(), "2.1.281");
    let (session, runner, hooked) = session_over(&layout, "2.1.281", "2.1.290");
    // Right after the inventory's read (the second of this refresh), Claude
    // Code's own updater installs the published version and re-points the
    // launcher, and the launcher answers the new version from then on.
    let launcher = layout.launcher.clone();
    let root = layout.root.clone();
    let scripted = runner.clone();
    hooked.after_read(2, move || {
        let real = program(&root, "2.1.290");
        std::fs::remove_file(&launcher).expect("unlink the launcher");
        std::os::unix::fs::symlink(&real, &launcher).expect("re-point the launcher");
        scripted.respond(
            vec![launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.290 (Claude Code)\n"),
        );
    });

    let snapshot = session.refresh(&home.env(), &CheckOptions::default()).await;

    assert!(!snapshot.stale, "{:?}", snapshot.errors);
    let rows = rows_of(&snapshot);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].version, "2.1.281",
        "the Installed page: the inventory's reading"
    );
    let candidates = candidates_of(&snapshot);
    assert_eq!(
        candidates.len(),
        1,
        "the Updates page lists the version the Installed page's row is behind, not \"up to date\" beside 2.1.281"
    );
    assert_eq!(candidates[0].current, rows[0].version);
    assert_eq!(candidates[0].target, "2.1.290");
    assert!(candidates[0].checkable);
    // The check compared the inventory's reading; it read nothing itself.
    assert_eq!(
        hooked.reads(),
        2,
        "detect and inventory; the check reads nothing"
    );

    // The next refresh reads the updated install on both pages.
    let next = session.refresh(&home.env(), &CheckOptions::default()).await;
    assert!(!next.stale, "{:?}", next.errors);
    assert_eq!(rows_of(&next)[0].version, "2.1.290");
    assert!(candidates_of(&next).is_empty());
}
