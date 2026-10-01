//! Codex installed by its own script, listed only (`recipes::CODEX`): the
//! version comes from the `current` link's release folder, nothing is ever
//! run or asked, there is no update and no uninstall, and the Other
//! Programs page stops calling its launcher a stranger. Every layout is
//! synthetic, built in a temp home the way the install script writes it
//! (read as text, `adapters/fixtures/standalone-codex/…/README.md`); the
//! runner and the HTTP client are mocks that record every call, and every
//! test checks there was none.

use banager_core::adapters::standalone::recipes::CODEX;
use banager_core::adapters::standalone::StandaloneAdapter;
use banager_core::adapters::{Adapter, AdapterError, CheckOptions};
use banager_core::events::VecSink;
use banager_core::http::MockHttpClient;
use banager_core::model::{
    ArtifactKind, InstanceNote, OpKind, OpRequest, UninstallBlocked, UpdateBlocked,
};
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
            "banager-codex-{tag}-{}-{}",
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
        std::fs::write(&path, b"#!/bin/sh\n").expect("write file");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    /// A symbolic link at `rel` whose text is `target` exactly.
    fn link(&self, rel: &str, target: &Path) -> PathBuf {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create parent");
        std::os::unix::fs::symlink(target, &path).expect("symlink");
        path
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

const RELEASE: &str = "0.159.3-aarch64-apple-darwin";

/// The script's layout for the default `CODEX_HOME`: the release folder
/// with its program and helper, `current` (absolute text) at it, and the
/// two links in `~/.local/bin` through `current`. `follows_latest`: the
/// script also wrote `auto-update-version` naming the release.
fn install(home: &Home, release: &str, follows_latest: bool) -> PathBuf {
    let root = home.at(".codex/packages/standalone");
    home.executable(&format!(
        ".codex/packages/standalone/releases/{release}/bin/codex"
    ));
    home.executable(&format!(
        ".codex/packages/standalone/releases/{release}/bin/codex-code-mode-host"
    ));
    home.link(
        ".codex/packages/standalone/current",
        &root.join("releases").join(release),
    );
    if follows_latest {
        std::fs::write(root.join("auto-update-version"), release).unwrap();
    }
    // The user's own settings beside the package folder: never read.
    std::fs::write(home.at(".codex/config.toml"), "model = \"x\"\n").unwrap();
    home.link(".local/bin/codex", &root.join("current/bin/codex"));
    home.link(
        ".local/bin/codex-code-mode-host",
        &root.join("current/bin/codex-code-mode-host"),
    )
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
            &CODEX,
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
        instance_id: "standalone-codex".to_string(),
        artifact_kind: ArtifactKind::Binary,
        name: "codex".to_string(),
    }
}

#[tokio::test]
async fn test_codex_is_listed_with_the_current_links_version_and_nothing_runs() {
    let home = Home::new("listed");
    install(&home, RELEASE, true);
    let mocks = Mocks::new();
    let adapter = mocks.adapter();

    let instances = adapter.detect(&home.env(Vec::new())).await;
    assert_eq!(instances.len(), 1);
    let inst = &instances[0];
    assert_eq!(inst.id, "standalone-codex");
    assert_eq!(inst.exe_path, home.at(".local/bin/codex"));
    assert_eq!(inst.prefix, home.at(".codex/packages/standalone"));
    assert_eq!(inst.version.as_deref(), Some("0.159.3"));
    assert_eq!(inst.status.unavailable, None);
    // No verified versions are listed, so no version reads as unverified.
    assert_eq!(inst.unverified_version, None);

    let rows = adapter.inventory(inst).await.expect("inventory");
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.key.name, "codex");
    assert_eq!(row.key.kind, ArtifactKind::Binary);
    assert_eq!(row.display_name, "Codex");
    assert_eq!(row.version, "0.159.3");
    assert_eq!(
        row.path.as_deref(),
        Some(
            home.at(&format!(
                ".codex/packages/standalone/releases/{RELEASE}/bin/codex"
            ))
            .as_path()
        )
    );
    // The marker names the release in use: it updates itself.
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
async fn test_a_pinned_install_is_not_said_to_update_itself() {
    // `CODEX_RELEASE=<version>`: the script deletes `auto-update-version`.
    let home = Home::new("pinned");
    install(&home, RELEASE, false);
    let mocks = Mocks::new();
    let adapter = mocks.adapter();
    let inst = adapter.detect(&home.env(Vec::new())).await.remove(0);
    let row = adapter.inventory(&inst).await.expect("inventory").remove(0);
    assert_eq!(row.version, "0.159.3");
    assert!(!row.auto_updates);

    // A marker left from another release is not this one following latest.
    std::fs::write(
        home.at(".codex/packages/standalone/auto-update-version"),
        "0.158.0-aarch64-apple-darwin",
    )
    .unwrap();
    let row = adapter.inventory(&inst).await.expect("inventory").remove(0);
    assert!(!row.auto_updates);
    mocks.assert_untouched();
}

#[tokio::test]
async fn test_no_update_and_no_uninstall_is_planned() {
    let home = Home::new("plans");
    install(&home, RELEASE, true);
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
async fn test_a_missing_current_link_leaves_the_launcher_alone_with_no_version() {
    // `current` gone: the launcher leads nowhere, but its own text still
    // points into the package folder -- the launcher-only state.
    let home = Home::new("no-current");
    install(&home, RELEASE, true);
    std::fs::remove_file(home.at(".codex/packages/standalone/current")).unwrap();
    let mocks = Mocks::new();
    let adapter = mocks.adapter();
    let instances = adapter.detect(&home.env(Vec::new())).await;
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].version, None);
    assert!(instances[0]
        .status
        .notes
        .contains(&InstanceNote::LauncherOnly));
    let rows = adapter.inventory(&instances[0]).await.expect("inventory");
    assert_eq!(rows[0].version, "");
    assert!(!rows[0].auto_updates);
    let outcome = adapter
        .check_updates(&instances[0], &CheckOptions::default())
        .await
        .expect("check");
    assert!(outcome.candidates.is_empty());
    mocks.assert_untouched();
}

#[tokio::test]
async fn test_an_unreadable_release_name_is_listed_with_its_version_unknown_not_as_not_responding()
{
    // The launcher leads into the package folder, but the release folder's
    // name is not one the script writes: the version is unknown, and since
    // nothing was asked, nothing "did not answer".
    let home = Home::new("odd-name");
    install(&home, "nightly", true);
    let mocks = Mocks::new();
    let adapter = mocks.adapter();
    let instances = adapter.detect(&home.env(Vec::new())).await;
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].version, None);
    assert_eq!(instances[0].status.unavailable, None);
    let rows = adapter.inventory(&instances[0]).await.expect("inventory");
    assert_eq!(rows[0].version, "");
    assert!(!rows[0].auto_updates, "no version, no claim");
    let outcome = adapter
        .check_updates(&instances[0], &CheckOptions::default())
        .await
        .expect("check");
    assert!(outcome.candidates.is_empty(), "{outcome:?}");
    mocks.assert_untouched();
}

#[tokio::test]
async fn test_other_codexes_at_the_launcher_path_are_not_this_row() {
    let mocks = Mocks::new();
    let adapter = mocks.adapter();

    // npm with its prefix at `~/.local`: `~/.local/bin/codex` leads into
    // `node_modules`, npm's row.
    let home = Home::new("npm-local");
    home.executable(".local/lib/node_modules/@openai/codex/bin/codex.js");
    home.link(
        ".local/bin/codex",
        Path::new("../lib/node_modules/@openai/codex/bin/codex.js"),
    );
    assert!(adapter.detect(&home.env(Vec::new())).await.is_empty());

    // Installed under another `CODEX_HOME`: the launcher leads into that
    // folder's package folder, which Banager cannot know is Codex's (it
    // inherits no `CODEX_HOME` from the shell).
    let home = Home::new("other-home");
    let other = home.at("work/codex-home/packages/standalone");
    home.executable(&format!(
        "work/codex-home/packages/standalone/releases/{RELEASE}/bin/codex"
    ));
    home.link(
        "work/codex-home/packages/standalone/current",
        &other.join("releases").join(RELEASE),
    );
    home.link(".local/bin/codex", &other.join("current/bin/codex"));
    assert!(adapter.detect(&home.env(Vec::new())).await.is_empty());

    // A plain file of that name is somebody else's.
    let home = Home::new("plain-file");
    home.executable(".local/bin/codex");
    std::fs::create_dir_all(home.at(".codex/packages/standalone")).unwrap();
    assert!(adapter.detect(&home.env(Vec::new())).await.is_empty());

    mocks.assert_untouched();
}

#[tokio::test]
async fn test_the_other_programs_page_stops_listing_codexs_launcher_and_helper() {
    let home = Home::new("unknown");
    install(&home, RELEASE, true);
    home.executable(".local/bin/stray");
    let mocks = Mocks::new();
    let session = Session::with_adapters(
        Arc::new(VecSink::new()),
        vec![Arc::new(mocks.adapter()) as Arc<dyn Adapter>],
        None,
    );
    let env = home.env(vec![home.at(".local/bin")]);
    let listed = |scan: &banager_core::scan::UnknownScan, name: &str| {
        scan.entries
            .iter()
            .any(|e| e.path == Path::new("~/.local/bin").join(name))
    };

    // Before any refresh nobody claims them.
    let before = session.scan_unknown(&env);
    assert!(listed(&before, "codex"), "{before:?}");
    assert!(listed(&before, "codex-code-mode-host"), "{before:?}");

    let snapshot = session.refresh(&env, &CheckOptions::default()).await;
    let row = snapshot
        .artifacts
        .iter()
        .find(|a| a.key.instance_id == "standalone-codex")
        .expect("Codex is listed");
    assert_eq!(row.version, "0.159.3");
    assert_eq!(row.facts.family.as_deref(), Some("codex"));
    assert!(snapshot
        .updates
        .iter()
        .all(|u| u.key.instance_id != "standalone-codex"));

    let after = session.scan_unknown(&env);
    assert!(
        !listed(&after, "codex"),
        "the launcher is Codex's: {after:?}"
    );
    assert!(
        !listed(&after, "codex-code-mode-host"),
        "the helper resolves into Codex's package folder: {after:?}"
    );
    assert!(listed(&after, "stray"), "a stranger is still listed");
    mocks.assert_untouched();
}
