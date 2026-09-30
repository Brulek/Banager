//! The two step D recipes through `Session`: the gate refusing agy's
//! upgrade from its candidate, grok's uninstall from the refresh through
//! the gate, the operation manager and the reading after, and the Unknown
//! page's rule 4 claiming a backup agy's updater left once the refresh
//! lists agy -- so the seams the unit tests exercise one at a time are
//! proven joined. Every layout is synthetic, in a temp home; every name is
//! invented; the Trash is a `MockTrasher` (a temp directory).

use banager_core::adapters::standalone::recipes::{AGY, GROK};
use banager_core::adapters::standalone::StandaloneAdapter;
use banager_core::adapters::{Adapter, AdapterError, CheckOptions};
use banager_core::events::{OpId, VecSink};
use banager_core::http::{HttpResponse, MockHttpClient};
use banager_core::model::{
    ArtifactKind, KeptWhat, OpKind, OpRequest, OpStatus, Outcome, PlanAction, UpdateBlocked,
    Warning,
};
use banager_core::runner::{CommandOutput, HostEnv, MockRunner};
use banager_core::session::Session;
use banager_core::trash::MockTrasher;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A fresh, canonical home directory for one test, removed when the test
/// ends (macOS's `/var/folders` is `/private/var/…`).
struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Home {
        let raw = std::env::temp_dir().join(format!(
            "banager-agy-grok-{tag}-{}-{}",
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

    /// A small regular file at `rel` (parents created).
    fn file(&self, rel: &str) -> PathBuf {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create parent");
        std::fs::write(&path, b"x").expect("write file");
        path
    }

    /// An executable regular file at `rel`; it is never run.
    fn executable(&self, rel: &str) -> PathBuf {
        let path = self.file(rel);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    /// A symbolic link at `rel` whose text is `target` exactly (parents
    /// created).
    fn link(&self, rel: &str, target: &Path) -> PathBuf {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create parent");
        std::os::unix::fs::symlink(target, &path).expect("symlink");
        path
    }

    /// `HostEnv` for this home, as the user who owns it, with nothing on
    /// `PATH`.
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

fn exited_0(stdout: &str) -> CommandOutput {
    CommandOutput {
        exit_code: Some(0),
        stdout: stdout.to_string(),
        stderr: String::new(),
        timed_out: false,
        cancelled: false,
    }
}

const AGY_MANIFEST_URL: &str =
    "https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/darwin_arm64.json";

/// Both tools installed in `home`; agy at 1.2.10 with a 1.2.11 manifest,
/// grok at 1.0.41 with its own check saying nothing is newer; the Trash is
/// `trasher`. Returns the session and grok's launcher.
fn session_with(home: &Home, trasher: Arc<MockTrasher>) -> (Arc<Session>, PathBuf) {
    let agy = home.executable(".local/bin/agy");
    home.file(".gemini/antigravity-cli/conversations/c1.jsonl");
    home.executable(".grok/downloads/grok-1.0.41-macos-aarch64");
    let target = Path::new("../downloads/grok-1.0.41-macos-aarch64");
    let grok = home.link(".grok/bin/grok", target);
    home.link(".grok/bin/agent", target);
    home.file(".grok/bundled/agents/default.md");
    home.file(".grok/config.toml");
    home.file(".grok/auth.json");
    home.file(".grok/sessions/s1.jsonl");
    home.file(".zshrc");

    let runner = Arc::new(MockRunner::new());
    runner.respond(
        vec![agy.to_str().unwrap(), "--version"],
        exited_0("1.2.10\n"),
    );
    runner.respond(
        vec![grok.to_str().unwrap(), "--version"],
        exited_0("grok 1.0.41 (4220f3b224a6)\n"),
    );
    runner.respond(
        vec![grok.to_str().unwrap(), "update", "--check", "--json"],
        exited_0(r#"{"currentVersion":"1.0.41","latestVersion":"1.0.41","updateAvailable":false}"#),
    );
    let http = Arc::new(MockHttpClient::new());
    http.respond(
        AGY_MANIFEST_URL,
        HttpResponse {
            status: 200,
            body: r#"{"version":"1.2.11","url":"x","sha512":"y"}"#.to_string(),
        },
    );
    let agy_adapter = StandaloneAdapter::new(&AGY, runner.clone(), http.clone(), trasher.clone())
        .with_trash_gap(Duration::ZERO)
        .with_arch("aarch64");
    let grok_adapter =
        StandaloneAdapter::new(&GROK, runner, http, trasher).with_trash_gap(Duration::ZERO);
    let session = Session::with_adapters(
        Arc::new(VecSink::new()),
        vec![
            Arc::new(agy_adapter) as Arc<dyn Adapter>,
            Arc::new(grok_adapter) as Arc<dyn Adapter>,
        ],
        None,
    );
    (session, grok)
}

fn request(instance_id: &str, kind: OpKind, name: &str) -> OpRequest {
    OpRequest {
        kind,
        instance_id: instance_id.to_string(),
        artifact_kind: ArtifactKind::Binary,
        name: name.to_string(),
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
        assert!(Instant::now() < deadline, "the operation never finished");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn test_the_gate_refuses_an_upgrade_of_agy_as_self_updating_and_the_row_carries_the_reason() {
    // Spec §4.4 D5 item 4 through the whole path: the refresh lists agy's
    // newer version with `SelfUpdatesOnly`, the Installed row says nothing
    // about being up to date, and `issue_plan` refuses the upgrade from the
    // candidate before the adapter is asked.
    let home = Home::new("agy-gate");
    let (session, _) = session_with(&home, Arc::new(MockTrasher::new()));

    let snapshot = session.refresh(&home.env(), &CheckOptions::default()).await;

    let candidate = snapshot
        .updates
        .iter()
        .find(|u| u.key.instance_id == "standalone-agy")
        .expect("agy's newer version is listed");
    assert_eq!(candidate.current, "1.2.10");
    assert_eq!(candidate.target, "1.2.11");
    assert!(candidate.checkable);
    assert_eq!(candidate.blocked, Some(UpdateBlocked::SelfUpdatesOnly));
    let row = snapshot
        .artifacts
        .iter()
        .find(|a| a.key.instance_id == "standalone-agy")
        .expect("agy is listed");
    assert!(row.auto_updates);
    assert_eq!(row.uninstall_blocked, None, "the uninstall is offered");

    let refused = session
        .issue_plan(&request("standalone-agy", OpKind::Upgrade, "agy"))
        .await;
    assert!(
        matches!(
            refused,
            Err(AdapterError::UpdateBlocked {
                reason: UpdateBlocked::SelfUpdatesOnly
            })
        ),
        "{refused:?}"
    );
    assert!(
        snapshot
            .updates
            .iter()
            .all(|u| u.key.instance_id != "standalone-grok"),
        "grok said nothing is newer"
    );
}

#[tokio::test]
async fn test_uninstalling_grok_through_the_session_moves_its_folders_and_keeps_its_home() {
    // The whole path for grok: refresh, the gate, the preview (two folders
    // present, the fish file and the fallback links absent, the two bin
    // links last), submit, `run_operation`'s reading after, the next
    // refresh with no grok row -- and `~/.grok`'s settings, login and
    // sessions untouched. A `/usr/local/bin/grok` on the machine running
    // this test is not a link into the temp home, so no sentence names it.
    let home = Home::new("grok-uninstall");
    let trasher = Arc::new(MockTrasher::new());
    let (session, launcher) = session_with(&home, trasher.clone());
    session.refresh(&home.env(), &CheckOptions::default()).await;

    let issued = session
        .issue_plan(&request("standalone-grok", OpKind::Uninstall, "grok"))
        .await
        .expect("the gate lets it through and the preview is built");
    let PlanAction::TrashPaths { paths, .. } = &issued.plan.action else {
        panic!("a path list, not a command: {:?}", issued.plan.action);
    };
    assert_eq!(
        paths,
        &vec![
            home.path().join(".grok/downloads"),
            home.path().join(".grok/bundled"),
            home.path().join(".grok/bin/agent"),
            launcher.clone(),
        ]
    );

    let op_id = session.submit(issued.id).expect("submit");
    assert_eq!(outcome_of(&session, op_id).await, Outcome::Succeeded);
    assert_eq!(&trasher.calls(), paths);
    assert!(!launcher.exists() && std::fs::symlink_metadata(&launcher).is_err());
    assert!(
        home.path().join(".grok/bin").is_dir(),
        "the emptied PATH folder stays"
    );
    assert!(home.path().join(".grok/config.toml").is_file());
    assert!(home.path().join(".grok/auth.json").is_file());
    assert!(home.path().join(".grok/sessions/s1.jsonl").is_file());
    assert!(home.path().join(".zshrc").is_file());

    let after = session.refresh(&home.env(), &CheckOptions::default()).await;
    assert!(after.instances.iter().all(|i| i.id != "standalone-grok"));
    assert!(
        after.instances.iter().any(|i| i.id == "standalone-agy"),
        "agy is untouched"
    );
}

#[tokio::test]
async fn test_uninstalling_grok_past_another_programs_agent_link_the_preview_kept_succeeds() {
    // Spec §十三 #27 through the whole path: `~/.local/bin/agent` is another
    // program's link, so the preview keeps it and says so, and the
    // uninstall moves everything else. The link is still there afterwards
    // and this run never moved it; the preview's own rule keeps it as not
    // grok's, so neither the run's own last look nor the reading after it
    // counts it as grok left behind: `Succeeded`, the link untouched, and
    // no grok row on the next refresh.
    let home = Home::new("grok-foreign-agent");
    let other = home.executable("other-cli/agent");
    let agent = home.link(".local/bin/agent", &other);
    let trasher = Arc::new(MockTrasher::new());
    let (session, launcher) = session_with(&home, trasher.clone());
    session.refresh(&home.env(), &CheckOptions::default()).await;

    let issued = session
        .issue_plan(&request("standalone-grok", OpKind::Uninstall, "grok"))
        .await
        .expect("the preview keeps the link and goes on");
    let PlanAction::TrashPaths { paths, .. } = &issued.plan.action else {
        panic!("a path list, not a command: {:?}", issued.plan.action);
    };
    assert!(!paths.contains(&agent));
    assert!(issued.plan.warnings.contains(&Warning::WillKeep {
        path: "~/.local/bin/agent".to_string(),
        what: KeptWhat::NotOurs,
    }));

    let op_id = session.submit(issued.id).expect("submit");
    assert_eq!(outcome_of(&session, op_id).await, Outcome::Succeeded);
    assert_eq!(&trasher.calls(), paths);
    assert_eq!(paths.last(), Some(&launcher));
    assert_eq!(
        std::fs::read_link(&agent).expect("the link is still there"),
        other,
        "and untouched"
    );
    let after = session.refresh(&home.env(), &CheckOptions::default()).await;
    assert!(after.instances.iter().all(|i| i.id != "standalone-grok"));
}

#[tokio::test]
async fn test_a_backup_agys_updater_left_is_agys_on_the_unknown_page_once_the_refresh_lists_agy() {
    // Spec §8.3 rule 4 in production: `Session::scan_unknown` hands the
    // scan every registered recipe's backup patterns
    // (`recipes::backup_globs`), so a regular `agy.<time>.old` beside the
    // launcher is agy's while the snapshot lists agy -- and, before any
    // refresh, a stranger like any other file (the spec's own words for a
    // leftover with no instance). Registration is what puts agy's pattern
    // in that list. The scan reads only directory entries and metadata.
    let home = Home::new("agy-backup");
    let (session, _) = session_with(&home, Arc::new(MockTrasher::new()));
    home.executable(".local/bin/agy.20260926T100000.old");
    home.executable(".local/bin/stray");
    let env = home.env();
    let backup = Path::new("~/.local/bin/agy.20260926T100000.old");
    let stray = Path::new("~/.local/bin/stray");

    let before = session.scan_unknown(&env);
    assert!(
        before.entries.iter().any(|e| e.path == backup),
        "{before:?}"
    );

    session.refresh(&env, &CheckOptions::default()).await;
    let after = session.scan_unknown(&env);
    assert!(
        !after.entries.iter().any(|e| e.path == backup),
        "agy's updater left it: {after:?}"
    );
    assert!(
        !after
            .entries
            .iter()
            .any(|e| e.path == Path::new("~/.local/bin/agy")),
        "the launcher is agy's own"
    );
    assert!(
        after.entries.iter().any(|e| e.path == stray),
        "a stranger is still listed: {after:?}"
    );
}
