//! opencode installed by its own script, listed only (`recipes::OPENCODE`):
//! the program is one regular file at `~/.opencode/bin/opencode`, no
//! version is read (the script leaves none on the disk, and opencode is
//! never run to ask), nothing is ever run or asked, there is no update and
//! no uninstall, and the Other Programs page stops calling it a stranger.
//! Every layout is synthetic, built in a temp home the way the install
//! script writes it (read as text, `adapters/fixtures/standalone-opencode/
//! …/README.md`); the runner and the HTTP client are mocks that record
//! every call, and every test checks there was none.

use banager_core::adapters::standalone::recipes::OPENCODE;
use banager_core::adapters::standalone::StandaloneAdapter;
use banager_core::adapters::{Adapter, AdapterError, CheckOptions};
use banager_core::events::VecSink;
use banager_core::http::MockHttpClient;
use banager_core::model::{ArtifactKind, OpKind, OpRequest, UninstallBlocked, UpdateBlocked};
use banager_core::runner::{HostEnv, MockRunner};
use banager_core::session::Session;
use banager_core::trash::MockTrasher;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A fresh, canonical home for one test, removed when it ends.
struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Home {
        let raw = std::env::temp_dir().join(format!(
            "banager-opencode-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&raw).expect("create temp home");
        Home(std::fs::canonicalize(&raw).expect("canonical temp home"))
    }

    fn at(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }

    /// An executable regular file at `rel`; it is never run.
    fn executable(&self, rel: &str) -> PathBuf {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create parent");
        std::fs::write(&path, b"#!/bin/sh\necho 1.18.31\n").expect("write file");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    fn file(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create parent");
        std::fs::write(path, text).expect("write file");
    }

    fn env(&self, path_dirs: Vec<PathBuf>) -> HostEnv {
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

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The script's layout: one executable moved to `~/.opencode/bin/opencode`.
/// With `plugins`, what opencode itself writes beside it on first use (on
/// the author's Mac): a `package.json` naming a plugin package's version,
/// its lock file and `node_modules` -- not the program's version, and never
/// read.
fn install(home: &Home, plugins: bool) -> PathBuf {
    let launcher = home.executable(".opencode/bin/opencode");
    if plugins {
        home.file(
            ".opencode/package.json",
            r#"{ "dependencies": { "@opencode-ai/plugin": "1.18.31" } }"#,
        );
        home.file(".opencode/package-lock.json", "{}");
        home.file(
            ".opencode/node_modules/@opencode-ai/plugin/package.json",
            r#"{ "name": "@opencode-ai/plugin", "version": "1.18.31" }"#,
        );
    }
    launcher
}

struct Mocks {
    runner: Arc<MockRunner>,
    http: Arc<MockHttpClient>,
}

impl Mocks {
    fn new() -> Mocks {
        Mocks {
            runner: Arc::new(MockRunner::new()),
            http: Arc::new(MockHttpClient::new()),
        }
    }

    fn adapter(&self) -> StandaloneAdapter {
        StandaloneAdapter::new(
            &OPENCODE,
            self.runner.clone(),
            self.http.clone(),
            Arc::new(MockTrasher::new()),
        )
    }

    /// Nothing was run and nothing was asked.
    fn assert_untouched(&self) {
        assert!(self.runner.calls().is_empty(), "{:?}", self.runner.calls());
        assert!(self.http.calls().is_empty(), "{:?}", self.http.calls());
    }
}

fn request(kind: OpKind) -> OpRequest {
    OpRequest {
        kind,
        instance_id: "standalone-opencode".to_string(),
        artifact_kind: ArtifactKind::Binary,
        name: "opencode".to_string(),
    }
}

#[tokio::test]
async fn test_opencode_is_listed_with_its_version_unknown_and_nothing_runs() {
    let home = Home::new("listed");
    let launcher = install(&home, true);
    let mocks = Mocks::new();
    let adapter = mocks.adapter();

    let instances = adapter.detect(&home.env(Vec::new())).await;
    assert_eq!(instances.len(), 1);
    let inst = &instances[0];
    assert_eq!(inst.id, "standalone-opencode");
    assert_eq!(inst.exe_path, launcher);
    assert_eq!(inst.prefix, home.at(".opencode"));
    // Not the plugin's 1.18.31 from package.json: no version is read.
    assert_eq!(inst.version, None);
    // Nothing was asked, so nothing failed to answer.
    assert_eq!(inst.status.unavailable, None);
    assert_eq!(inst.unverified_version, None);

    let rows = adapter.inventory(inst).await.expect("inventory");
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.key.name, "opencode");
    assert_eq!(row.key.kind, ArtifactKind::Binary);
    assert_eq!(row.display_name, "opencode");
    assert_eq!(row.version, "");
    assert_eq!(row.path.as_deref(), Some(launcher.as_path()));
    // opencode downloads its updates itself by default.
    assert!(row.auto_updates);
    assert_eq!(row.uninstall_blocked, Some(UninstallBlocked::NoSafeMethod));

    // No update check: no candidate and no "could not check" row.
    let outcome = adapter
        .check_updates(inst, &CheckOptions::default())
        .await
        .expect("check");
    assert!(outcome.candidates.is_empty(), "{outcome:?}");
    assert!(outcome.notes.is_empty());

    mocks.assert_untouched();
}

#[tokio::test]
async fn test_the_scripts_layout_alone_is_listed_too() {
    // Right after the script, before opencode has run: only `bin/opencode`.
    let home = Home::new("bare");
    install(&home, false);
    let mocks = Mocks::new();
    let adapter = mocks.adapter();
    let inst = adapter.detect(&home.env(Vec::new())).await.remove(0);
    let row = adapter.inventory(&inst).await.expect("inventory").remove(0);
    assert_eq!(row.version, "");
    mocks.assert_untouched();
}

#[tokio::test]
async fn test_no_update_and_no_uninstall_is_planned() {
    let home = Home::new("plans");
    install(&home, true);
    let mocks = Mocks::new();
    let adapter = mocks.adapter();
    let inst = adapter.detect(&home.env(Vec::new())).await.remove(0);
    assert!(matches!(
        adapter.plan(&inst, &request(OpKind::Upgrade)).await,
        Err(AdapterError::UpdateBlocked {
            reason: UpdateBlocked::SelfUpdatesOnly
        })
    ));
    assert!(matches!(
        adapter.plan(&inst, &request(OpKind::Uninstall)).await,
        Err(AdapterError::UninstallBlocked {
            reason: UninstallBlocked::NoSafeMethod
        })
    ));
    assert!(matches!(
        adapter.plan(&inst, &request(OpKind::Install)).await,
        Err(AdapterError::Unsupported(_))
    ));
    mocks.assert_untouched();
}

#[tokio::test]
async fn test_other_opencodes_at_the_launcher_path_are_not_this_row() {
    let mocks = Mocks::new();
    let adapter = mocks.adapter();

    // A link at the script's path leads somewhere else: not the script's
    // copy, which is a regular file.
    let home = Home::new("link");
    home.executable("elsewhere/opencode");
    std::fs::create_dir_all(home.at(".opencode/bin")).unwrap();
    std::os::unix::fs::symlink(
        home.at("elsewhere/opencode"),
        home.at(".opencode/bin/opencode"),
    )
    .unwrap();
    assert!(adapter.detect(&home.env(Vec::new())).await.is_empty());

    // npm's `opencode-ai` or Homebrew's `opencode` elsewhere, and only
    // opencode's own folder of plugins: no launcher, no row.
    let home = Home::new("no-launcher");
    home.executable(".npm-global/bin/opencode");
    home.file(".opencode/package.json", "{}");
    assert!(adapter.detect(&home.env(Vec::new())).await.is_empty());

    // A folder at the launcher's path is not a program.
    let home = Home::new("folder");
    std::fs::create_dir_all(home.at(".opencode/bin/opencode")).unwrap();
    assert!(adapter.detect(&home.env(Vec::new())).await.is_empty());

    mocks.assert_untouched();
}

#[tokio::test]
async fn test_the_other_programs_page_stops_listing_opencode() {
    let home = Home::new("unknown");
    install(&home, true);
    let mocks = Mocks::new();
    let session = Session::with_adapters(
        Arc::new(VecSink::new()),
        vec![Arc::new(mocks.adapter()) as Arc<dyn Adapter>],
        None,
    );
    let env = home.env(vec![home.at(".opencode/bin")]);
    let listed = |scan: &banager_core::scan::UnknownScan, name: &str| {
        scan.entries
            .iter()
            .any(|e| e.path == Path::new("~/.opencode/bin").join(name))
    };

    // Before any refresh nobody claims it.
    // `/usr/local/bin`, the one folder the scan reads outside the home
    // folder, stood in for by the test's own: never this Mac's.
    session.set_unknown_scan_system_bin(&env.home.join("usr/local/bin"));
    let before = session.scan_unknown(&env);
    assert!(listed(&before, "opencode"), "{before:?}");

    let snapshot = session.refresh(&env, &CheckOptions::default()).await;
    let row = snapshot
        .artifacts
        .iter()
        .find(|a| a.key.instance_id == "standalone-opencode")
        .expect("opencode is listed");
    assert_eq!(row.version, "");
    assert_eq!(row.facts.family.as_deref(), Some("opencode"));
    assert!(snapshot
        .updates
        .iter()
        .all(|u| u.key.instance_id != "standalone-opencode"));

    let after = session.scan_unknown(&env);
    assert!(
        !listed(&after, "opencode"),
        "the launcher is opencode's: {after:?}"
    );
    mocks.assert_untouched();
}
