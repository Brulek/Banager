//! A path-list uninstall end to end (phase 4 step C): the real
//! `StandaloneAdapter` over a synthetic native Claude Code layout in a
//! throwaway home, driven the way the window drives it -- `Session::refresh`,
//! `issue_plan` (the actionability gate), `submit`, `run_operation`
//! (`execute`, then the reading after an uninstall) -- with `MockTrasher`
//! standing in for the Trash, so nothing here touches anyone's Trash. The
//! last test is the exception, `#[ignore]`d and gated on `CANAGER_LIVE=1`
//! like `brew_live`'s install test: `RealTrasher` moving five throwaway
//! items it makes into the real Trash of the Mac running it (CI's runner,
//! whose Trash is discarded with it; on a developer's Mac, once, by hand).
//!
//! No recorded fixture: nothing here runs a command but the scripted
//! `--version`, and every layout is built by the test (spec §9.3). Every
//! `#[tokio::test]` here runs on tokio's default current-thread runtime,
//! which the cancel test relies on: a submitted operation does not start
//! until the test awaits.

use canager_core::adapters::standalone::recipes::CLAUDE;
use canager_core::adapters::standalone::StandaloneAdapter;
use canager_core::adapters::{Adapter, CheckOptions};
use canager_core::events::{OpId, VecSink};
use canager_core::http::{HttpResponse, MockHttpClient};
use canager_core::model::{
    ArtifactKind, Fault, InstanceNote, ItemKind, KeptWhat, OpKind, OpRequest, OpStatus, Outcome,
    PlanAction, RemovedWhat, Warning,
};
use canager_core::runner::{CommandOutput, HostEnv, MockRunner};
use canager_core::session::Session;
use canager_core::trash::{MockTrasher, TrashError, Trasher};
use std::collections::BTreeSet;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, Weak};
use std::time::{Duration, Instant};

/// A fresh home directory for one test, removed when the test ends.
/// Canonical, so the paths a test builds compare equal to what the
/// adapter resolves (macOS's `/var/folders` is `/private/var/…`).
struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Home {
        let raw = std::env::temp_dir().join(format!(
            "canager-uninstall-{tag}-{}-{}",
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
    /// the test), with nothing on `PATH`.
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

/// The native Claude Code layout the installer writes (an executable
/// `versions/<v>` that is never run -- `--version` is scripted -- and an
/// absolute link to it), plus the download cache and the settings and
/// history a real one has: returns the launcher.
fn claude_layout(home: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let real = home.join(".local/share/claude/versions/2.1.281");
    std::fs::create_dir_all(real.parent().unwrap()).expect("versions dir");
    std::fs::write(&real, b"#!/bin/sh\n").expect("the program");
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o755)).expect("executable");
    let launcher = home.join(".local/bin/claude");
    std::fs::create_dir_all(launcher.parent().unwrap()).expect("bin dir");
    std::os::unix::fs::symlink(&real, &launcher).expect("the launcher");
    std::fs::create_dir_all(home.join(".claude/downloads")).expect("the cache");
    std::fs::create_dir_all(home.join(".claude/projects/p")).expect("projects");
    std::fs::write(home.join(".claude/projects/p/session.jsonl"), b"{}\n").expect("history");
    std::fs::write(home.join(".claude.json"), b"{}\n").expect("settings");
    launcher
}

/// Every entry under `root`, relative, links not followed: what "nothing
/// else in the home changed" is compared by.
fn tree(root: &Path) -> BTreeSet<PathBuf> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            out.insert(path.strip_prefix(root).unwrap().to_path_buf());
            if std::fs::symlink_metadata(&path).unwrap().is_dir() {
                walk(root, &path, out);
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(root, root, &mut out);
    out
}

/// A session over one standalone Claude Code adapter whose `--version` is
/// scripted, whose update check answers "no newer version", and whose Trash
/// is `trasher`, with no pause after each item.
fn session_with(launcher: &Path, trasher: Arc<dyn Trasher>) -> Arc<Session> {
    let runner = Arc::new(MockRunner::new());
    runner.respond(
        vec![launcher.to_str().unwrap(), "--version"],
        CommandOutput {
            exit_code: Some(0),
            stdout: "2.1.281 (Claude Code)\n".to_string(),
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        },
    );
    let http = Arc::new(MockHttpClient::new());
    http.respond(
        "https://downloads.claude.ai/claude-code-releases/latest",
        HttpResponse {
            status: 200,
            body: "2.1.281\n".to_string(),
        },
    );
    let adapter =
        StandaloneAdapter::new(&CLAUDE, runner, http, trasher).with_trash_gap(Duration::ZERO);
    Session::with_adapters(
        Arc::new(VecSink::new()),
        vec![Arc::new(adapter) as Arc<dyn Adapter>],
        None,
    )
}

fn uninstall() -> OpRequest {
    OpRequest {
        kind: OpKind::Uninstall,
        instance_id: "standalone-claude".to_string(),
        artifact_kind: ArtifactKind::Binary,
        name: "claude".to_string(),
    }
}

/// Waits for `op_id` to finish and returns its outcome.
async fn outcome_of(session: &Arc<Session>, op_id: OpId) -> Outcome {
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
async fn test_uninstalling_claude_code_moves_its_three_paths_and_keeps_its_settings() {
    let home = Home::new("full");
    let launcher = claude_layout(home.path());
    let trasher = Arc::new(MockTrasher::new());
    let session = session_with(&launcher, trasher.clone());

    let snapshot = session.refresh(&home.env(), &CheckOptions::default()).await;
    let row = snapshot
        .artifacts
        .iter()
        .find(|a| a.key.instance_id == "standalone-claude")
        .expect("Claude Code is listed");
    assert_eq!(row.uninstall_blocked, None, "the row offers Uninstall");
    let before = tree(home.path());

    let issued = session
        .issue_plan(&uninstall())
        .await
        .expect("the gate lets it through and the preview is built");
    let moved = vec![
        home.path().join(".local/share/claude"),
        home.path().join(".claude/downloads"),
        launcher.clone(),
    ];
    let PlanAction::TrashPaths { paths, previewed } = &issued.plan.action else {
        panic!("a path list, not a command: {:?}", issued.plan.action);
    };
    assert_eq!(paths, &moved);
    assert_eq!(
        previewed.len(),
        moved.len(),
        "what the preview saw, one per path"
    );
    // What the window receives: the paths alone (Ruling 10).
    assert_eq!(
        serde_json::to_value(&issued).unwrap()["plan"]["action"],
        serde_json::json!({ "TrashPaths": { "paths": moved } })
    );
    assert_eq!(
        issued.plan.warnings,
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
    let op_id = session.submit(issued.id).expect("submit");

    assert_eq!(outcome_of(&session, op_id).await, Outcome::Succeeded);
    // Exactly the previewed paths, in order, and nothing else.
    assert_eq!(trasher.calls(), moved);
    // Both `claude`s are in the Trash: the program directory under its own
    // name, the launcher -- a link, not its target -- under a suffixed one.
    assert!(trasher.bin().join("claude/versions/2.1.281").is_file());
    let link = trasher.bin().join("claude 2");
    assert!(std::fs::symlink_metadata(link)
        .unwrap()
        .file_type()
        .is_symlink());
    // Everything else in the home is exactly as it was.
    let expected: BTreeSet<PathBuf> = before
        .into_iter()
        .filter(|rel| !moved.iter().any(|m| home.path().join(rel).starts_with(m)))
        .collect();
    assert_eq!(tree(home.path()), expected);
    assert!(home
        .path()
        .join(".claude/projects/p/session.jsonl")
        .is_file());
    // And the next refresh has no Claude Code row.
    let snapshot = session.refresh(&home.env(), &CheckOptions::default()).await;
    assert!(snapshot
        .instances
        .iter()
        .all(|i| i.id != "standalone-claude"));
}

#[tokio::test]
async fn test_a_path_changed_after_the_preview_stops_the_uninstall_before_it_moves_anything() {
    // Between the preview and the click the launcher was re-pointed at a
    // Homebrew copy: not what the user confirmed. Nothing moves.
    let home = Home::new("changed");
    let launcher = claude_layout(home.path());
    let trasher = Arc::new(MockTrasher::new());
    let session = session_with(&launcher, trasher.clone());
    session.refresh(&home.env(), &CheckOptions::default()).await;
    let issued = session.issue_plan(&uninstall()).await.expect("preview");

    let cask = home
        .path()
        .join("opt/homebrew/Caskroom/claude-code/2.1.267/claude");
    std::fs::create_dir_all(cask.parent().unwrap()).unwrap();
    std::fs::write(&cask, b"#!/bin/sh\n").unwrap();
    std::fs::remove_file(&launcher).unwrap();
    std::os::unix::fs::symlink(&cask, &launcher).unwrap();
    let op_id = session.submit(issued.id).expect("submit");

    assert_eq!(
        outcome_of(&session, op_id).await,
        Outcome::CanagerFailed(Fault::PathChanged {
            path: "~/.local/bin/claude".to_string()
        })
    );
    assert!(trasher.calls().is_empty());
    assert!(home
        .path()
        .join(".local/share/claude/versions/2.1.281")
        .is_file());
}

#[tokio::test]
async fn test_a_self_update_between_the_preview_and_the_click_stops_the_uninstall_before_it_moves_anything(
) {
    // Ruling 10 end to end: what the preview saw travels in the plan
    // `issue_plan` stores and `submit` hands to the operation -- the window
    // never sees it. Claude Code updating itself in between re-points the
    // launcher at a new version inside its root: every check still passes
    // and the path is the same, but the link is not the one the user was
    // shown. Nothing moves, and the user previews again.
    let home = Home::new("self-updated");
    let launcher = claude_layout(home.path());
    let trasher = Arc::new(MockTrasher::new());
    let session = session_with(&launcher, trasher.clone());
    session.refresh(&home.env(), &CheckOptions::default()).await;
    let issued = session.issue_plan(&uninstall()).await.expect("preview");

    let newer = home.path().join(".local/share/claude/versions/2.1.282");
    std::fs::write(&newer, b"#!/bin/sh\n").unwrap();
    std::fs::remove_file(&launcher).unwrap();
    std::os::unix::fs::symlink(&newer, &launcher).unwrap();
    let op_id = session.submit(issued.id).expect("submit");

    assert_eq!(
        outcome_of(&session, op_id).await,
        Outcome::CanagerFailed(Fault::PathChanged {
            path: "~/.local/bin/claude".to_string()
        })
    );
    assert!(trasher.calls().is_empty());
    assert!(home
        .path()
        .join(".local/share/claude/versions/2.1.281")
        .is_file());
}

#[tokio::test]
async fn test_an_uninstall_macos_refuses_partway_leaves_a_launcher_only_row_that_a_second_uninstall_finishes(
) {
    // Review Focus 4: macOS refuses the second item. The launcher (last)
    // is still there, so the next refresh shows the launcher-only row --
    // with an Uninstall -- and a second uninstall lists the program
    // directory as already gone and finishes.
    let home = Home::new("refused");
    let launcher = claude_layout(home.path());
    let trasher = Arc::new(MockTrasher::new());
    trasher.refuse_call(
        1,
        "“downloads” couldn’t be moved to the Trash because you don’t have permission to access it.",
    );
    let session = session_with(&launcher, trasher.clone());
    session.refresh(&home.env(), &CheckOptions::default()).await;

    let first = session.issue_plan(&uninstall()).await.expect("preview");
    let op_id = session.submit(first.id).expect("submit");
    assert_eq!(
        outcome_of(&session, op_id).await,
        Outcome::Failed {
            exit_code: None,
            summary: "“downloads” couldn’t be moved to the Trash because you don’t have permission to access it.".to_string()
        }
    );

    let snapshot = session.refresh(&home.env(), &CheckOptions::default()).await;
    let row = snapshot
        .instances
        .iter()
        .find(|i| i.id == "standalone-claude")
        .expect("the row is still there");
    assert_eq!(row.status.notes, vec![InstanceNote::LauncherOnly]);
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|a| a.key.instance_id == "standalone-claude")
        .expect("its artifact");
    assert_eq!(artifact.version, "");
    assert_eq!(artifact.uninstall_blocked, None, "and it offers Uninstall");

    let second = session
        .issue_plan(&uninstall())
        .await
        .expect("second preview");
    assert_eq!(
        second.plan.warnings[..3],
        [
            Warning::AlreadyGone {
                path: "~/.local/share/claude".to_string()
            },
            Warning::WillTrash {
                path: "~/.claude/downloads".to_string(),
                what: RemovedWhat::Cache
            },
            Warning::WillTrash {
                path: "~/.local/bin/claude".to_string(),
                what: RemovedWhat::Launcher
            },
        ]
    );
    let op_id = session.submit(second.id).expect("submit");
    assert_eq!(outcome_of(&session, op_id).await, Outcome::Succeeded);
    let snapshot = session.refresh(&home.env(), &CheckOptions::default()).await;
    assert!(snapshot
        .instances
        .iter()
        .all(|i| i.id != "standalone-claude"));
}

/// A `MockTrasher` that presses Cancel on the running uninstall right after
/// it moves its first item. The operation to cancel is named after
/// `submit` returns and before the test awaits, which on the current-thread
/// runtime is before the operation starts.
struct CancellingTrasher {
    inner: MockTrasher,
    op: OnceLock<(Weak<Session>, OpId)>,
}

impl Trasher for CancellingTrasher {
    fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError> {
        let moved = self.inner.trash(path, kind)?;
        if self.inner.calls().len() == 1 {
            let (session, op_id) = self.op.get().expect("the test named its operation");
            session
                .upgrade()
                .expect("the session is alive")
                .cancel(*op_id)
                .expect("a Running uninstall accepts Cancel");
        }
        Ok(moved)
    }
}

#[tokio::test]
async fn test_an_uninstall_cancelled_between_items_is_reported_cancelled_and_a_second_uninstall_finishes(
) {
    // Review Focus 4, the other half, and the retry Astra's finding 6 asked
    // for: the user's Cancel lands after the program directory went to the
    // Trash. The launcher is still there -- one link into its root, so
    // launcher-only rather than gone -- so the reading after the uninstall
    // says the item is present and the cancel is what happened
    // (`Cancelled`; never `Succeeded`: Task 2's reading), the next refresh
    // shows the launcher-only row, and pressing Uninstall again finishes
    // the job.
    let home = Home::new("cancelled");
    let launcher = claude_layout(home.path());
    let trasher = Arc::new(CancellingTrasher {
        inner: MockTrasher::new(),
        op: OnceLock::new(),
    });
    let session = session_with(&launcher, trasher.clone());
    session.refresh(&home.env(), &CheckOptions::default()).await;
    let issued = session.issue_plan(&uninstall()).await.expect("preview");

    let op_id = session.submit(issued.id).expect("submit");
    trasher
        .op
        .set((Arc::downgrade(&session), op_id))
        .expect("named once");

    assert_eq!(outcome_of(&session, op_id).await, Outcome::Cancelled);
    assert_eq!(
        trasher.inner.calls(),
        vec![home.path().join(".local/share/claude")]
    );
    assert!(std::fs::symlink_metadata(&launcher)
        .unwrap()
        .file_type()
        .is_symlink());
    let snapshot = session.refresh(&home.env(), &CheckOptions::default()).await;
    let row = snapshot
        .instances
        .iter()
        .find(|i| i.id == "standalone-claude")
        .expect("the row is still there");
    assert_eq!(row.status.notes, vec![InstanceNote::LauncherOnly]);

    // The retry: the program directory is already gone, the cache and the
    // launcher are what is left, and this time nothing stops it (the
    // trasher presses Cancel after its first call only).
    let second = session
        .issue_plan(&uninstall())
        .await
        .expect("second preview");
    assert_eq!(
        second.plan.warnings[..3],
        [
            Warning::AlreadyGone {
                path: "~/.local/share/claude".to_string()
            },
            Warning::WillTrash {
                path: "~/.claude/downloads".to_string(),
                what: RemovedWhat::Cache
            },
            Warning::WillTrash {
                path: "~/.local/bin/claude".to_string(),
                what: RemovedWhat::Launcher
            },
        ]
    );
    let op_id = session.submit(second.id).expect("submit");
    assert_eq!(outcome_of(&session, op_id).await, Outcome::Succeeded);
    assert_eq!(
        trasher.inner.calls(),
        vec![
            home.path().join(".local/share/claude"),
            home.path().join(".claude/downloads"),
            launcher.clone(),
        ]
    );
    let snapshot = session.refresh(&home.env(), &CheckOptions::default()).await;
    assert!(snapshot
        .instances
        .iter()
        .all(|i| i.id != "standalone-claude"));
}

/// A `MockTrasher` that, right after its `lock_after_call`-th move, takes
/// every permission off `folder` -- so the reading after the uninstall
/// cannot tell whether the launcher is there -- and gives `0o755` back
/// when dropped, so the test home can be removed.
struct LockingTrasher {
    inner: MockTrasher,
    lock_after_call: usize,
    folder: PathBuf,
}

impl Trasher for LockingTrasher {
    fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError> {
        use std::os::unix::fs::PermissionsExt;
        let moved = self.inner.trash(path, kind)?;
        if self.inner.calls().len() == self.lock_after_call {
            std::fs::set_permissions(&self.folder, std::fs::Permissions::from_mode(0o000))
                .expect("take the folder's permissions away");
        }
        Ok(moved)
    }
}

impl Drop for LockingTrasher {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&self.folder, std::fs::Permissions::from_mode(0o755));
    }
}

#[tokio::test]
async fn test_an_uninstall_whose_last_reading_cannot_tell_is_unconfirmed_not_succeeded() {
    // Ruling 27 end to end: every item moved and `execute` said
    // `Succeeded`, but the reading after it cannot look into `~/.local/bin`
    // (a permission error). "Could not tell" is not "gone": the outcome is
    // `Unconfirmed`, never `Succeeded` on `execute`'s word. (Permissions do
    // not stop root, so the check is skipped, and says so, when the tests
    // run as root.)
    let home = Home::new("unreadable-after");
    if std::fs::metadata(home.path()).expect("stat home").uid() == 0 {
        eprintln!("running as root: permissions stop nothing, check skipped");
        return;
    }
    let launcher = claude_layout(home.path());
    let trasher = Arc::new(LockingTrasher {
        inner: MockTrasher::new(),
        lock_after_call: 3,
        folder: home.path().join(".local/bin"),
    });
    let session = session_with(&launcher, trasher.clone());
    session.refresh(&home.env(), &CheckOptions::default()).await;
    let issued = session.issue_plan(&uninstall()).await.expect("preview");

    let op_id = session.submit(issued.id).expect("submit");

    assert_eq!(outcome_of(&session, op_id).await, Outcome::Unconfirmed);
    assert_eq!(trasher.inner.calls().len(), 3, "every item was moved");
}

/// Review Focus 8: the real call, on each kind of item a path-list
/// uninstall can move -- a file, a directory, a link to a file, a link to
/// a directory, and a dangling link (what the launcher is when every
/// uninstall moves it, last) -- each of which must land in `~/.Trash`, a link as the
/// link itself with its target left where it was (spec §9.4). The items
/// are made under the temp directory, which on a Mac is on the home
/// folder's volume, so the system moves them to `~/.Trash` (the spike saw
/// the same from `$TMPDIR`). Each call is told the item's kind, as the
/// removal's last check tells it -- a link as `Symlink` whatever it points
/// at. It changes the machine -- it leaves five
/// throwaway items, named `canager-trash-smoke-…`, in the Trash of the Mac
/// running it -- so, like `brew_live`'s install test, it also requires
/// `CANAGER_LIVE=1` and skips loudly without it.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "moves five throwaway items into the real Trash; run with CANAGER_LIVE=1 cargo test -p canager-core --test standalone_uninstall_test -- --ignored"]
fn test_real_trasher_moves_each_kind_of_item_and_links_as_links() {
    use canager_core::trash::RealTrasher;

    if std::env::var("CANAGER_LIVE").as_deref() != Ok("1") {
        eprintln!("CANAGER_LIVE is not 1; skipping the real Trash smoke test");
        return;
    }
    let trash = PathBuf::from(std::env::var_os("HOME").expect("HOME is set")).join(".Trash");
    let scratch = Home::new("real-trash");
    let stem = format!(
        "canager-trash-smoke-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = scratch.path();
    let file = dir.join(format!("{stem}-file.txt"));
    std::fs::write(&file, b"smoke\n").unwrap();
    let folder = dir.join(format!("{stem}-dir"));
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("inner.txt"), b"inner\n").unwrap();
    let link_to_file = dir.join(format!("{stem}-link-to-file"));
    std::os::unix::fs::symlink(&file, &link_to_file).unwrap();
    let link_to_dir = dir.join(format!("{stem}-link-to-dir"));
    std::os::unix::fs::symlink(&folder, &link_to_dir).unwrap();
    let dangling = dir.join(format!("{stem}-dangling"));
    std::os::unix::fs::symlink(dir.join("gone"), &dangling).unwrap();

    let trasher = RealTrasher::new();
    // Links first, while their targets are still in place.
    for (link, target) in [
        (&link_to_file, Some(&file)),
        (&link_to_dir, Some(&folder)),
        (&dangling, None),
    ] {
        let trashed = trasher
            .trash(link, ItemKind::Symlink)
            .expect("moved to the Trash");
        assert!(
            trashed.starts_with(&trash),
            "{} went to {}, not under {}",
            link.display(),
            trashed.display(),
            trash.display()
        );
        assert!(
            std::fs::symlink_metadata(&trashed)
                .unwrap()
                .file_type()
                .is_symlink(),
            "{} arrived as a link",
            link.display()
        );
        assert!(
            std::fs::symlink_metadata(link).is_err(),
            "{} left its place",
            link.display()
        );
        if let Some(target) = target {
            assert!(target.exists(), "{}'s target stayed put", link.display());
        }
    }
    let trashed = trasher
        .trash(&folder, ItemKind::Dir)
        .expect("the directory");
    assert!(trashed.starts_with(&trash));
    assert!(trashed.join("inner.txt").is_file());
    let trashed = trasher.trash(&file, ItemKind::File).expect("the file");
    assert!(trashed.starts_with(&trash));
    assert!(std::fs::symlink_metadata(&trashed).unwrap().is_file());
}
