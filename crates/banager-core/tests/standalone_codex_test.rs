//! Codex installed by its own script (`recipes::CODEX`): the version comes
//! from the `current` link's release folder, nothing is ever run or asked,
//! there is no update, the uninstall moves the script's two links and its
//! package folder to the Trash and keeps the rest of `~/.codex` (the
//! author's decision U8), and the Other Programs page stops calling its
//! launcher a stranger. Every layout is synthetic, built in a temp home the
//! way the install script writes it (read as text,
//! `adapters/fixtures/standalone-codex/…/README.md`); the runner and the
//! HTTP client are mocks that record every call, and every test checks
//! there was none; the Trash is `MockTrasher`'s folder.

use banager_core::adapters::standalone::recipes::CODEX;
use banager_core::adapters::standalone::StandaloneAdapter;
use banager_core::adapters::{Adapter, AdapterError, CheckOptions};
use banager_core::events::VecSink;
use banager_core::http::MockHttpClient;
use banager_core::model::{
    ArtifactKind, InstanceNote, KeptWhat, OpKind, OpRequest, OpStatus, Outcome, PlanAction,
    RemovedWhat, UpdateBlocked, Warning,
};
use banager_core::runner::{HostEnv, MockRunner};
use banager_core::session::Session;
use banager_core::trash::MockTrasher;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    trasher: Arc<MockTrasher>,
}

impl Mocks {
    fn new() -> Mocks {
        Mocks {
            runner: Arc::new(MockRunner::new()),
            http: Arc::new(MockHttpClient::new()),
            trasher: Arc::new(MockTrasher::new()),
        }
    }

    /// With no pause after each move to the Trash.
    fn adapter(&self) -> StandaloneAdapter {
        StandaloneAdapter::new(
            &CODEX,
            self.runner.clone(),
            self.http.clone(),
            self.trasher.clone(),
        )
        .with_trash_gap(Duration::ZERO)
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
    // Its uninstall is a path list: the row offers Uninstall.
    assert_eq!(row.uninstall_blocked, None);

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
async fn test_a_codex_kept_in_a_protected_place_is_not_listed_and_not_looked_into() {
    // `~/.codex` (or only its `releases/`) moved into `~/Documents` or
    // iCloud Drive and linked back: the launcher leads there through
    // `current`, so it is one Banager cannot look at, and the row is not
    // listed -- not listed with a version unknown (docs/what-we-run.md,
    // Codex). Kept anywhere else, it is listed with its version.
    for keep in [
        "elsewhere",
        "Documents",
        "Library/Mobile Documents/com~apple~CloudDocs",
    ] {
        for moved in [".codex", ".codex/packages/standalone/releases"] {
            let home = Home::new("kept");
            install(&home, RELEASE, true);
            let kept = home.at(keep).join(moved);
            std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
            std::fs::rename(home.at(moved), &kept).unwrap();
            std::os::unix::fs::symlink(&kept, home.at(moved)).unwrap();
            let mocks = Mocks::new();
            let instances = mocks.adapter().detect(&home.env(Vec::new())).await;
            if keep == "elsewhere" && moved == ".codex" {
                assert_eq!(instances.len(), 1, "{moved}");
                assert_eq!(instances[0].version.as_deref(), Some("0.159.3"));
            } else {
                // `releases/` moved anywhere: the program then resolves
                // outside the root, which is not the installer's layout
                // whether or not the place is protected.
                assert!(instances.is_empty(), "{keep} {moved}: {instances:?}");
            }
            mocks.assert_untouched();
        }
    }
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
async fn test_no_update_and_no_install_is_planned() {
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
        adapter.plan(&inst, &request(OpKind::Install)).await,
        Err(AdapterError::Unsupported(_))
    ));
    mocks.assert_untouched();
}

/// The three paths the uninstall moves, in order: the helper link, the
/// package folder, the launcher last.
fn moved(home: &Home) -> Vec<PathBuf> {
    vec![
        home.at(".local/bin/codex-code-mode-host"),
        home.at(".codex/packages/standalone"),
        home.at(".local/bin/codex"),
    ]
}

#[tokio::test]
async fn test_the_uninstall_preview_lists_exactly_what_moves_and_names_codex_as_what_stays() {
    let home = Home::new("preview");
    install(&home, RELEASE, true);
    std::fs::write(
        home.at(".zprofile"),
        "export PATH=\"$HOME/.local/bin:$PATH\"\n",
    )
    .unwrap();
    let mocks = Mocks::new();
    let adapter = mocks.adapter();
    let inst = adapter.detect(&home.env(Vec::new())).await.remove(0);

    let plan = adapter
        .plan(&inst, &request(OpKind::Uninstall))
        .await
        .expect("a path list, not a refusal");
    let PlanAction::TrashPaths { paths, previewed } = &plan.action else {
        panic!("a path list, not a command: {:?}", plan.action);
    };
    assert_eq!(paths, &moved(&home));
    assert_eq!(previewed.len(), 3);
    assert!(!plan.needs_password);
    assert!(plan.affected.is_empty());
    assert_eq!(
        plan.warnings,
        vec![
            Warning::WillTrash {
                path: "~/.local/bin/codex-code-mode-host".to_string(),
                what: RemovedWhat::Program
            },
            Warning::WillTrash {
                path: "~/.codex/packages/standalone".to_string(),
                what: RemovedWhat::Program
            },
            Warning::WillTrash {
                path: "~/.local/bin/codex".to_string(),
                what: RemovedWhat::Launcher
            },
            Warning::WillKeep {
                path: "~/.codex".to_string(),
                what: KeptWhat::SettingsAndHistory
            },
            Warning::WillKeep {
                path: "~/.zprofile".to_string(),
                what: KeptWhat::ShellConfigLines
            },
        ]
    );
    // A preview moves nothing.
    assert!(mocks.trasher.calls().is_empty());
    mocks.assert_untouched();
}

#[tokio::test]
async fn test_without_the_helper_the_preview_lists_the_package_folder_and_the_launcher() {
    // An older release, or one without the helper: the script makes no
    // `codex-code-mode-host` link, and nothing is said of one.
    let home = Home::new("no-helper");
    install(&home, RELEASE, false);
    std::fs::remove_file(home.at(".local/bin/codex-code-mode-host")).unwrap();
    let mocks = Mocks::new();
    let adapter = mocks.adapter();
    let inst = adapter.detect(&home.env(Vec::new())).await.remove(0);
    let plan = adapter
        .plan(&inst, &request(OpKind::Uninstall))
        .await
        .expect("plan");
    let PlanAction::TrashPaths { paths, .. } = &plan.action else {
        panic!("a path list: {:?}", plan.action);
    };
    assert_eq!(paths, &moved(&home)[1..]);
    assert!(!plan
        .warnings
        .iter()
        .any(|w| format!("{w:?}").contains("codex-code-mode-host")));
    mocks.assert_untouched();
}

#[tokio::test]
async fn test_a_helper_of_that_name_that_is_not_codexs_is_kept_and_said() {
    // A file of the user's own at the helper's path, or a link elsewhere:
    // not the script's, so it stays where it is and the preview says so.
    for theirs in ["file", "link"] {
        let home = Home::new("not-ours");
        install(&home, RELEASE, true);
        let helper = home.at(".local/bin/codex-code-mode-host");
        std::fs::remove_file(&helper).unwrap();
        if theirs == "file" {
            home.executable(".local/bin/codex-code-mode-host");
        } else {
            let elsewhere = home.executable("tools/codex-code-mode-host");
            std::os::unix::fs::symlink(&elsewhere, &helper).unwrap();
        }
        let mocks = Mocks::new();
        let adapter = mocks.adapter();
        let inst = adapter.detect(&home.env(Vec::new())).await.remove(0);
        let plan = adapter
            .plan(&inst, &request(OpKind::Uninstall))
            .await
            .expect("plan");
        let PlanAction::TrashPaths { paths, .. } = &plan.action else {
            panic!("a path list: {:?}", plan.action);
        };
        assert_eq!(paths, &moved(&home)[1..], "{theirs}");
        assert!(
            plan.warnings.contains(&Warning::WillKeep {
                path: "~/.local/bin/codex-code-mode-host".to_string(),
                what: KeptWhat::NotOurs
            }),
            "{theirs}: {:?}",
            plan.warnings
        );
        mocks.assert_untouched();
    }
}

/// Waits for `op_id` to finish and returns its outcome.
async fn outcome_of(session: &Arc<Session>, op_id: banager_core::events::OpId) -> Outcome {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(op) = session.operations().into_iter().find(|op| op.id == op_id) {
            if op.status == OpStatus::Done {
                return op.outcome.expect("a finished operation has an outcome");
            }
        }
        assert!(Instant::now() < deadline, "the uninstall never finished");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn test_uninstalling_codex_moves_its_files_to_the_trash_and_keeps_settings_and_sessions() {
    let home = Home::new("uninstall");
    install(&home, RELEASE, true);
    std::fs::create_dir_all(home.at(".codex/sessions/2026/10")).unwrap();
    std::fs::write(home.at(".codex/sessions/2026/10/rollout.jsonl"), "{}\n").unwrap();
    std::fs::write(home.at(".codex/auth.json"), "{}\n").unwrap();
    // Claude Code's launcher shares the folder: never touched.
    home.executable(".local/bin/claude");
    let mocks = Mocks::new();
    let session = Session::with_adapters(
        Arc::new(VecSink::new()),
        vec![Arc::new(mocks.adapter()) as Arc<dyn Adapter>],
        None,
    );
    let env = home.env(Vec::new());
    let snapshot = session.refresh(&env, &CheckOptions::default()).await;
    let row = snapshot
        .artifacts
        .iter()
        .find(|a| a.key.instance_id == "standalone-codex")
        .expect("Codex is listed");
    assert_eq!(row.uninstall_blocked, None, "the row offers Uninstall");

    let issued = session
        .issue_plan(&request(OpKind::Uninstall))
        .await
        .expect("the gate lets it through and the preview is built");
    let op_id = session.submit(issued.id).expect("submit");
    assert_eq!(outcome_of(&session, op_id).await, Outcome::Succeeded);

    // Exactly the previewed paths, in order, and nothing else.
    assert_eq!(mocks.trasher.calls(), moved(&home));
    assert!(mocks
        .trasher
        .bin()
        .join(format!("standalone/releases/{RELEASE}/bin/codex"))
        .is_file());
    // Settings, login and sessions stay, and so does the shared folder.
    assert!(home.at(".codex/config.toml").is_file());
    assert!(home.at(".codex/auth.json").is_file());
    assert!(home.at(".codex/sessions/2026/10/rollout.jsonl").is_file());
    assert!(home.at(".codex/packages").is_dir());
    assert!(home.at(".local/bin/claude").is_file());
    assert!(!home.at(".local/bin/codex").exists());
    // And the next refresh has no Codex row.
    let snapshot = session.refresh(&env, &CheckOptions::default()).await;
    assert!(snapshot
        .instances
        .iter()
        .all(|i| i.id != "standalone-codex"));
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
    // `/usr/local/bin`, the one folder the scan reads outside the home
    // folder, stood in for by the test's own: never this Mac's.
    session.set_unknown_scan_system_bin(&env.home.join("usr/local/bin"));
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
