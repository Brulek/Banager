//! An upgrade's outcome, end to end through a real adapter: its own `plan`,
//! its own `execute`, and its own `reconcile` reading its own inventory
//! command, with only the commands' output scripted (and, in the Claude
//! Code section, its install built in a temp home).
//!
//! The four "exited 0 and changed nothing" cases in the Homebrew, pipx and
//! uv sections are the ones the per-package actionability work found
//! (docs/superpowers/backlog.md, "假「成功」"). Each tool skips the package
//! and still exits 0, so the only thing that can tell them apart from a
//! real upgrade is that the version its inventory reports is the same
//! after as before. The inventories are the recorded fixtures under
//! `adapters/fixtures/`, read unchanged before and after, which is what a
//! skipped upgrade leaves. The tools' own messages are copied from their
//! source (named on each) for a realistic log; the outcome depends only on
//! the exit code and the inventories.
//!
//! The "stopped partway" cases at the end of the Homebrew and pipx sections
//! are the opposite: a command the user cancelled, or the timeout stopped,
//! is `Unconfirmed` whatever its inventory reads, because these tools write
//! the version it reports partway through an upgrade (the
//! `Ok(Outcome::Unconfirmed)` arm of `run_operation` cites their lines).
//!
//! The Claude Code section reads no recorded inventory: its adapter's
//! inventory probes a real launcher link and runs `--version`, so the
//! install is real files in a temp home and only `update` and `--version`
//! are scripted; one case's runner also removes the program file as
//! `update` returns, leaving the launcher dangling. Its stopped case is
//! `Unconfirmed` by the same arm, which does not depend on how the command
//! writes; `claude update` is compiled and its steps were not read
//! (claude.md §6, §8).

use async_trait::async_trait;
use canager_core::adapters::brew::BrewAdapter;
use canager_core::adapters::pipx::PipxAdapter;
use canager_core::adapters::standalone::recipes::CLAUDE;
use canager_core::adapters::standalone::StandaloneAdapter;
use canager_core::adapters::uv::UvAdapter;
use canager_core::adapters::Adapter;
use canager_core::events::VecSink;
use canager_core::http::MockHttpClient;
use canager_core::model::{
    ArtifactKind, Attention, InstanceNote, ManagerInstance, OpKind, OpRequest, Outcome,
};
use canager_core::ops::OperationManager;
use canager_core::runner::{
    CommandOutput, CommandRunner, CommandSpec, HostEnv, LineCallback, RunnerError,
};
use canager_core::trash::MockTrasher;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

const FIXTURES: &str = "../../adapters/fixtures";

fn fixture(path: &str) -> String {
    let path = format!("{FIXTURES}/{path}");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

fn exited_0(stdout: &str, stderr: &str) -> CommandOutput {
    CommandOutput {
        exit_code: Some(0),
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
        timed_out: false,
        cancelled: false,
    }
}

/// How a command was stopped partway, as the runner reports it.
#[derive(Clone, Copy, Debug)]
enum Stop {
    /// The user pressed Cancel.
    Cancel,
    /// The command ran past its timeout.
    Timeout,
}

impl Stop {
    fn output(self) -> CommandOutput {
        CommandOutput {
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: matches!(self, Stop::Timeout),
            cancelled: matches!(self, Stop::Cancel),
        }
    }
}

/// Answers each argv with its scripted outputs in order, repeating the
/// last one -- so an inventory command can read one way before an upgrade
/// and another way after it. `MockRunner` answers an argv the same way
/// every time, which covers only the "nothing changed" half.
///
/// An output scripted as cancelled (`Stop::Cancel`) is not returned until
/// the op's Cancel arrives, as a real run is not: `upgrade` presses Cancel
/// once `awaiting_cancel` says such a command is running.
#[derive(Default)]
struct ScriptedRunner {
    scripts: Mutex<HashMap<Vec<String>, Script>>,
    awaiting_cancel: Notify,
}

/// How many times an argv has been answered, and its outputs in order.
type Script = (usize, Vec<CommandOutput>);

impl ScriptedRunner {
    fn script(&self, argv: &[&str], outputs: Vec<CommandOutput>) {
        let key = argv.iter().map(|s| s.to_string()).collect();
        self.scripts.lock().unwrap().insert(key, (0, outputs));
    }
}

#[async_trait]
impl CommandRunner for ScriptedRunner {
    async fn run(
        &self,
        spec: CommandSpec,
        _on_line: Option<LineCallback>,
        cancel: CancellationToken,
    ) -> Result<CommandOutput, RunnerError> {
        let mut key = vec![spec.program.to_string_lossy().to_string()];
        key.extend(spec.args.iter().cloned());
        let output = {
            let mut scripts = self.scripts.lock().unwrap();
            let Some((calls, outputs)) = scripts.get_mut(&key) else {
                return Err(RunnerError::NoMock(key));
            };
            let output = outputs[(*calls).min(outputs.len() - 1)].clone();
            *calls += 1;
            output
        };
        if output.cancelled {
            self.awaiting_cancel.notify_one();
            cancel.cancelled().await;
        }
        Ok(output)
    }
}

/// Submits an upgrade of `name` through a fresh `OperationManager` and
/// returns its outcome. If `runner` is running a command scripted as
/// cancelled, presses Cancel on the op, as the user would.
async fn upgrade(
    runner: &ScriptedRunner,
    adapter: Arc<dyn Adapter>,
    inst: ManagerInstance,
    kind: ArtifactKind,
    name: &str,
) -> Outcome {
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    manager.register_instance(inst.clone());
    let req = OpRequest {
        kind: OpKind::Upgrade,
        instance_id: inst.id.clone(),
        artifact_kind: kind,
        name: name.to_string(),
    };
    let plan = adapter.plan(&inst, &req).await.expect("plan");
    let op_id = manager.submit(plan);
    tokio::select! {
        outcome = manager.wait(op_id) => return outcome.expect("an outcome"),
        _ = runner.awaiting_cancel.notified() => manager.cancel(op_id).expect("cancel a Running op"),
    }
    manager.wait(op_id).await.expect("an outcome")
}

// --- Homebrew -------------------------------------------------------------

const BREW: &str = "/opt/homebrew/bin/brew";
const BREW_INFO: [&str; 4] = [BREW, "info", "--installed", "--json=v2"];

fn brew_instance() -> ManagerInstance {
    ManagerInstance {
        exe_path: PathBuf::from(BREW),
        prefix: PathBuf::from("/opt/homebrew"),
        version: Some("7.0.3".to_string()),
        ..canager_core::testing::manager_instance("brew", "brew:/opt/homebrew")
    }
}

/// `brew info --installed --json=v2` as recorded, with `edit` applied.
fn brew_info(edit: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut info: serde_json::Value =
        serde_json::from_str(&fixture("brew/7.0.3/info-installed.json")).expect("fixture");
    edit(&mut info);
    info.to_string()
}

fn formula<'a>(info: &'a mut serde_json::Value, name: &str) -> &'a mut serde_json::Value {
    info["formulae"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|f| f["name"] == name)
        .unwrap_or_else(|| panic!("no formula {name} in the fixture"))
}

fn cask<'a>(info: &'a mut serde_json::Value, token: &str) -> &'a mut serde_json::Value {
    info["casks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["token"] == token)
        .unwrap_or_else(|| panic!("no cask {token} in the fixture"))
}

async fn brew_upgrade_cask(
    token: &str,
    upgrade_output: CommandOutput,
    infos: Vec<String>,
) -> Outcome {
    let runner = Arc::new(ScriptedRunner::default());
    runner.script(&[BREW, "upgrade", "--cask", token], vec![upgrade_output]);
    runner.script(&BREW_INFO, infos.iter().map(|i| exited_0(i, "")).collect());
    upgrade(
        &runner,
        Arc::new(BrewAdapter::new(runner.clone())),
        brew_instance(),
        ArtifactKind::Cask,
        token,
    )
    .await
}

async fn brew_upgrade_formula(
    name: &str,
    upgrade_output: CommandOutput,
    infos: Vec<String>,
) -> Outcome {
    let runner = Arc::new(ScriptedRunner::default());
    runner.script(&[BREW, "upgrade", "--formula", name], vec![upgrade_output]);
    runner.script(&BREW_INFO, infos.iter().map(|i| exited_0(i, "")).collect());
    upgrade(
        &runner,
        Arc::new(BrewAdapter::new(runner.clone())),
        brew_instance(),
        ArtifactKind::Formula,
        name,
    )
    .await
}

#[tokio::test]
async fn test_a_disabled_cask_homebrew_skipped_is_not_reported_as_updated() {
    // C2: `Cask::Upgrade` skips a disabled cask with a warning, not a
    // failure (`opoo "Not upgrading #{cask.token}, it is ..."`,
    // `cask/upgrade.rb:60-63` in Homebrew 7.0.6), and exits 0. `onyx` is
    // outdated in the recording (5.0.2, tap at 5.1.0).
    let recorded = brew_info(|_| {});
    let outcome = brew_upgrade_cask(
        "onyx",
        exited_0(
            "",
            "Warning: Not upgrading onyx, it is disabled because it is discontinued upstream!\n",
        ),
        vec![recorded.clone(), recorded],
    )
    .await;
    assert_eq!(
        outcome,
        Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade)
    );
}

#[tokio::test]
async fn test_a_cask_whose_installed_recipe_cannot_be_loaded_is_not_reported_as_updated() {
    // C4: when the installed caskfile cannot be loaded, `Cask::Upgrade`
    // warns "cannot be upgraded as-is" and skips it (`cask/upgrade.rb:208-213`
    // in Homebrew 7.0.6), exiting 0. `codexbar` is outdated in the recording.
    let recorded = brew_info(|_| {});
    let outcome = brew_upgrade_cask(
        "codexbar",
        exited_0(
            "",
            "Warning: The cask 'codexbar' cannot be upgraded as-is. To fix this, run:\n\
             brew reinstall --cask --force codexbar\n",
        ),
        vec![recorded.clone(), recorded],
    )
    .await;
    assert_eq!(
        outcome,
        Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade)
    );
}

#[tokio::test]
async fn test_a_cask_upgrade_that_moved_its_version_succeeded() {
    let after = brew_info(|info| cask(info, "onyx")["installed"] = "5.1.0".into());
    let outcome = brew_upgrade_cask(
        "onyx",
        exited_0("==> Upgrading onyx\n", ""),
        vec![brew_info(|_| {}), after],
    )
    .await;
    assert_eq!(outcome, Outcome::Succeeded);
}

#[tokio::test]
async fn test_a_latest_cask_reinstalled_under_the_same_name_is_not_called_unchanged() {
    // A `version :latest` cask is installed as "latest" before and after
    // every upgrade, so an equal version says nothing about whether the
    // upgrade did anything (`BrewAdapter::reconcile`). It must fall back to
    // presence -- Succeeded -- not claim nothing changed.
    let latest = brew_info(|info| cask(info, "onyx")["installed"] = "latest".into());
    let outcome = brew_upgrade_cask(
        "onyx",
        exited_0("==> Upgrading onyx\n", ""),
        vec![latest.clone(), latest],
    )
    .await;
    assert_eq!(outcome, Outcome::Succeeded);
}

#[tokio::test]
async fn test_a_formula_upgrade_that_left_its_old_keg_behind_still_reads_as_moved() {
    // Canager runs brew with `HOMEBREW_NO_INSTALL_CLEANUP=1`, so after an
    // upgrade the old keg stays and `installed` lists both. The version
    // read is the linked keg's (`parse_info_installed`), which the upgrade
    // moved to the new one. `aria2` is at 1.37.0_2 in the recording.
    let after = brew_info(|info| {
        let aria2 = formula(info, "aria2");
        let mut new_keg = aria2["installed"][0].clone();
        new_keg["version"] = "1.37.0_3".into();
        aria2["installed"].as_array_mut().unwrap().push(new_keg);
        aria2["linked_keg"] = "1.37.0_3".into();
    });
    let outcome = brew_upgrade_formula(
        "aria2",
        exited_0("==> Upgrading aria2\n", ""),
        vec![brew_info(|_| {}), after],
    )
    .await;
    assert_eq!(outcome, Outcome::Succeeded);
}

#[tokio::test]
async fn test_a_formula_upgrade_stopped_before_its_new_keg_was_linked_is_unconfirmed() {
    // I1 in .superpowers/honest-review.md. Homebrew unlinks the old keg,
    // pours the new one, then links it in `finish` (install.rb:634-637 in
    // Homebrew 7.0.6). Stopped between pour and link, `installed` lists
    // both kegs and `linked_keg` is null, so the version read is the last
    // keg's (`parse_info_installed`): the new one. The version moved, and
    // the upgrade did not finish: nothing is linked.
    let after = brew_info(|info| {
        let aria2 = formula(info, "aria2");
        let mut new_keg = aria2["installed"][0].clone();
        new_keg["version"] = "1.37.0_3".into();
        aria2["installed"].as_array_mut().unwrap().push(new_keg);
        aria2["linked_keg"] = serde_json::Value::Null;
    });
    for stop in [Stop::Cancel, Stop::Timeout] {
        let outcome = brew_upgrade_formula(
            "aria2",
            stop.output(),
            vec![brew_info(|_| {}), after.clone()],
        )
        .await;
        assert_eq!(outcome, Outcome::Unconfirmed, "{stop:?}");
    }
}

#[tokio::test]
async fn test_a_cask_upgrade_stopped_after_it_wrote_the_new_version_is_unconfirmed() {
    // I1, cask. `stage` writes the new version's metadata
    // (cask/upgrade.rb:460, cask/installer.rb:589-603) before
    // `install_artifacts` puts the new app in place (cask/upgrade.rb:462).
    // Stopped in between, the version reads as the new one and the app is
    // not installed.
    let after = brew_info(|info| cask(info, "onyx")["installed"] = "5.1.0".into());
    for stop in [Stop::Cancel, Stop::Timeout] {
        let outcome = brew_upgrade_cask(
            "onyx",
            stop.output(),
            vec![brew_info(|_| {}), after.clone()],
        )
        .await;
        assert_eq!(outcome, Outcome::Unconfirmed, "{stop:?}");
    }
}

#[tokio::test]
async fn test_a_cask_upgrade_cancelled_before_it_wrote_the_new_version_is_unconfirmed() {
    // I2, cask. `start_upgrade` has already moved the old app out of
    // /Applications (cask/upgrade.rb:455), and only `rescue => e` puts it
    // back (cask/upgrade.rb:502), which SIGTERM's `SignalException` skips.
    // Cancelled before `stage`, the version reads as before and the app is
    // gone: "You cancelled this" would say nothing happened.
    let recorded = brew_info(|_| {});
    let outcome = brew_upgrade_cask(
        "onyx",
        Stop::Cancel.output(),
        vec![recorded.clone(), recorded],
    )
    .await;
    assert_eq!(outcome, Outcome::Unconfirmed);
}

// --- pipx -----------------------------------------------------------------

const PIPX: &str = "/opt/homebrew/bin/pipx";

async fn pipx_upgrade(upgrade_output: CommandOutput, lists: Vec<String>) -> Outcome {
    let runner = Arc::new(ScriptedRunner::default());
    runner.script(&[PIPX, "upgrade", "cowsay"], vec![upgrade_output]);
    runner.script(
        &[PIPX, "list", "--json"],
        lists.iter().map(|l| exited_0(l, "")).collect(),
    );
    let inst = ManagerInstance {
        exe_path: PathBuf::from(PIPX),
        ..canager_core::testing::manager_instance("pipx", "pipx")
    };
    upgrade(
        &runner,
        Arc::new(PipxAdapter::new(
            runner.clone(),
            Arc::new(MockHttpClient::new()),
        )),
        inst,
        ArtifactKind::Tool,
        "cowsay",
    )
    .await
}

#[tokio::test]
async fn test_a_locked_pipx_tool_pipx_skipped_is_not_reported_as_updated() {
    // pipx 1.17.3 returns `UpgradeStatus.LOCKED` for a venv with a lock
    // file, reporting `version=main_package.package_version` unchanged
    // (`commands/upgrade.py:270-283`), prints `locked_package_message`
    // (`commands/common.py:720-721`) and exits 0. The recorded venv has no
    // lock file (`list.json`, `"lock_file": null`); a locked one differs
    // only in that field, which the inventory does not read, and its
    // version reads the same before and after.
    let recorded = fixture("pipx/1.17.3/list.json");
    let outcome = pipx_upgrade(
        exited_0(
            "Not upgrading locked package cowsay. Update its lock file and run `pipx reinstall cowsay`.\n",
            "",
        ),
        vec![recorded.clone(), recorded],
    )
    .await;
    assert_eq!(
        outcome,
        Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade)
    );
}

#[tokio::test]
async fn test_a_pipx_upgrade_that_moved_its_version_succeeded() {
    let before = fixture("pipx/1.17.3/list.json");
    let after = before.replace(r#""package_version": "5.0""#, r#""package_version": "6.1""#);
    assert_ne!(after, before, "the fixture's version field was not found");
    let outcome = pipx_upgrade(
        exited_0("upgraded package cowsay\n", ""),
        vec![before, after],
    )
    .await;
    assert_eq!(outcome, Outcome::Succeeded);
}

#[tokio::test]
async fn test_a_pipx_upgrade_cancelled_before_it_wrote_the_new_version_is_unconfirmed() {
    // I2 in .superpowers/honest-review.md. pipx 1.17.3 installs the new
    // package first (`venv.py:778`) and writes the version `pipx list
    // --json` reports last (`update_package_metadata`, `venv.py:790`), with
    // no signal handler in between. Cancelled there, the version reads as
    // before and the venv has already changed.
    let recorded = fixture("pipx/1.17.3/list.json");
    let outcome = pipx_upgrade(Stop::Cancel.output(), vec![recorded.clone(), recorded]).await;
    assert_eq!(outcome, Outcome::Unconfirmed);
}

// --- uv -------------------------------------------------------------------

const UV: &str = "/opt/homebrew/bin/uv";

async fn uv_upgrade(upgrade_output: CommandOutput, lists: Vec<String>) -> Outcome {
    let runner = Arc::new(ScriptedRunner::default());
    runner.script(&[UV, "tool", "upgrade", "ruff"], vec![upgrade_output]);
    runner.script(
        &[UV, "tool", "list", "--show-paths"],
        lists.iter().map(|l| exited_0(l, "")).collect(),
    );
    let inst = ManagerInstance {
        exe_path: PathBuf::from(UV),
        ..canager_core::testing::manager_instance("uv", "uv")
    };
    upgrade(
        &runner,
        Arc::new(UvAdapter::new(runner.clone())),
        inst,
        ArtifactKind::Tool,
        "ruff",
    )
    .await
}

#[tokio::test]
async fn test_a_uv_tool_installed_with_an_exact_pin_is_not_reported_as_updated() {
    // uv 0.12.17 re-resolves a tool installed as `ruff==X` to `X`, prints
    // "Nothing to upgrade" and a hint, and returns `ExitStatus::Success`
    // (`upgrade.rs:189-191`, `:217`; see .superpowers/actionability-facts.md).
    let recorded = fixture("uv/0.12.17/tool-list-show-paths.txt");
    let outcome = uv_upgrade(
        exited_0("", "Nothing to upgrade\n"),
        vec![recorded.clone(), recorded],
    )
    .await;
    assert_eq!(
        outcome,
        Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade)
    );
}

#[tokio::test]
async fn test_a_uv_upgrade_that_moved_its_version_succeeded() {
    let before = fixture("uv/0.12.17/tool-list-show-paths.txt");
    let after = before.replace("ruff v0.15.0", "ruff v0.15.1");
    assert_ne!(after, before, "the fixture's version was not found");
    let outcome = uv_upgrade(
        exited_0("", "Updated ruff v0.15.0 -> v0.15.1\n"),
        vec![before, after],
    )
    .await;
    assert_eq!(outcome, Outcome::Succeeded);
}

// --- Claude Code (standalone, phase 4 step B) ------------------------------

/// A native Claude Code layout in a temp home: `~/.local/bin/claude`
/// linking into `~/.local/share/claude/versions/<version>`. The adapter's
/// inventory probes the disk (a real link resolving into a real root), so
/// a scripted runner alone cannot stand in for the install; the runner
/// scripts only the two commands.
fn claude_home(version: &str) -> (PathBuf, ManagerInstance) {
    // The Claude Code tests run in parallel in one process, and two of them
    // can read the same time (macOS's realtime clock counts whole
    // microseconds), so a sequence number keeps each call's home its own.
    static NEXT_HOME: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let home = std::env::temp_dir().join(format!(
        "canager-ops-claude-{}-{}-{}",
        std::process::id(),
        NEXT_HOME.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = home.join(".local/share/claude");
    let real = root.join("versions").join(version);
    std::fs::create_dir_all(real.parent().unwrap()).expect("versions dir");
    std::fs::write(&real, b"#!/bin/sh\n").expect("real binary");
    let bin = home.join(".local/bin");
    std::fs::create_dir_all(&bin).expect("bin dir");
    let launcher = bin.join("claude");
    std::os::unix::fs::symlink(&real, &launcher).expect("launcher link");
    let inst = ManagerInstance {
        exe_path: launcher,
        prefix: root,
        version: Some(version.to_string()),
        ..canager_core::testing::manager_instance("standalone-claude", "standalone-claude")
    };
    (home, inst)
}

struct ClaudeMutationRunner {
    inner: Arc<ScriptedRunner>,
    remove_target_on_update: Option<PathBuf>,
}

#[async_trait]
impl CommandRunner for ClaudeMutationRunner {
    async fn run(
        &self,
        spec: CommandSpec,
        on_line: Option<LineCallback>,
        cancel: CancellationToken,
    ) -> Result<CommandOutput, RunnerError> {
        let is_update = spec.args == ["update"];
        let output = self.inner.run(spec, on_line, cancel).await?;
        if is_update {
            if let Some(target) = &self.remove_target_on_update {
                std::fs::remove_file(target).expect("update left launcher dangling");
            }
        }
        Ok(output)
    }
}

async fn claude_upgrade_outputs(
    update_output: CommandOutput,
    versions: Vec<CommandOutput>,
    dangling_after_update: bool,
) -> Outcome {
    let (home, inst) = claude_home("2.1.281");
    let launcher = inst.exe_path.to_string_lossy().to_string();
    let runner = Arc::new(ScriptedRunner::default());
    let mutating = Arc::new(ClaudeMutationRunner {
        inner: runner.clone(),
        remove_target_on_update: dangling_after_update
            .then(|| std::fs::canonicalize(&inst.exe_path).unwrap()),
    });
    let adapter = Arc::new(StandaloneAdapter::new(
        &CLAUDE,
        mutating,
        Arc::new(MockHttpClient::new()),
        Arc::new(MockTrasher::new()),
    ));
    // Detect first, as `Session` does before it plans anything: right
    // before it runs `update`, the adapter's `execute` looks at the
    // launcher again under the home detect recorded (B's Astra finding
    // B-1). Detect's own `--version` read comes before that command is
    // scripted, so the runner refuses it (`NoMock`), which the adapter
    // reads as no version: none of the before/after answers below is
    // consumed. The instance the op runs against is `inst`, built above.
    let detected = adapter
        .detect(&HostEnv {
            path_dirs: vec![],
            home: home.clone(),
            euid: 501,
            cargo_home: None,
            ollama_host: None,
        })
        .await;
    assert_eq!(detected.len(), 1, "detect sees the native layout");
    runner.script(&[launcher.as_str(), "update"], vec![update_output]);
    runner.script(&[launcher.as_str(), "--version"], versions);
    let outcome = upgrade(
        &runner,
        adapter,
        inst.clone(),
        ArtifactKind::Binary,
        "claude",
    )
    .await;
    if dangling_after_update {
        use canager_core::adapters::standalone::recipe::RouteKind;
        use canager_core::adapters::standalone::route::{probe, Probe};
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &inst.exe_path, &inst.prefix),
            Probe::LauncherOnly
        );
        // The next refresh: detect lists the launcher alone, and the
        // inventory of that row is the one an Uninstall finishes. (The
        // inventory of `inst`, detected whole, refuses instead: the install
        // is no longer what that detect listed -- B's Astra finding B-2.)
        let adapter = StandaloneAdapter::new(
            &CLAUDE,
            runner.clone(),
            Arc::new(MockHttpClient::new()),
            Arc::new(MockTrasher::new()),
        );
        let listed = adapter
            .detect(&HostEnv {
                path_dirs: vec![],
                home: home.clone(),
                euid: 501,
                cargo_home: None,
                ollama_host: None,
            })
            .await;
        assert_eq!(
            listed.len(),
            1,
            "step C can still discover the remaining launcher"
        );
        assert_eq!(listed[0].status.notes, vec![InstanceNote::LauncherOnly]);
        let artifacts = adapter.inventory(&listed[0]).await.unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].path, None);
    }
    let _ = std::fs::remove_dir_all(&home);
    outcome
}

async fn claude_upgrade(update_output: CommandOutput, versions: Vec<&str>) -> Outcome {
    claude_upgrade_outputs(
        update_output,
        versions
            .iter()
            .map(|v| exited_0(&format!("{v} (Claude Code)\n"), ""))
            .collect(),
        false,
    )
    .await
}

#[tokio::test]
async fn test_a_claude_update_exiting_zero_with_a_failed_version_read_is_unconfirmed() {
    let failed = CommandOutput {
        exit_code: Some(1),
        stdout: String::new(),
        stderr: "dyld: Library not loaded".to_string(),
        timed_out: false,
        cancelled: false,
    };
    for after in [failed, exited_0("", "")] {
        assert_eq!(
            claude_upgrade_outputs(
                exited_0("updated", ""),
                vec![exited_0("2.1.281 (Claude Code)\n", ""), after],
                false,
            )
            .await,
            Outcome::Unconfirmed
        );
    }
}

#[tokio::test]
async fn test_a_claude_update_exiting_zero_with_a_dangling_launcher_is_unconfirmed() {
    assert_eq!(
        claude_upgrade_outputs(
            exited_0("updated", ""),
            vec![exited_0("2.1.281 (Claude Code)\n", "")],
            true,
        )
        .await,
        Outcome::Unconfirmed
    );
}

#[tokio::test]
async fn test_a_claude_update_exiting_zero_with_no_version_before_it_falls_back_to_the_reading_after(
) {
    // `run_operation` reads `--version` before an upgrade as well as after
    // it. A failed reading after is `Unconfirmed`
    // (`test_a_claude_update_exiting_zero_with_a_failed_version_read_is_unconfirmed`);
    // a failed reading before only leaves nothing to compare, so an update
    // that exits 0 is judged by the reading after alone, as for every
    // source (`VersionChange::Unknown` in ops/mod.rs, and
    // `test_succeeded_upgrade_with_no_before_reading_falls_back_to_presence`
    // in tests/ops_outcome_test.rs). With a version after, that is
    // `Succeeded` -- even here, where `claude update` found nothing to
    // install and the version after is the one installed before.
    let timed_out = CommandOutput {
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
        timed_out: true,
        cancelled: false,
    };
    let outcome = claude_upgrade_outputs(
        exited_0("Claude Code is up to date (2.1.281)\n", ""),
        vec![timed_out, exited_0("2.1.281 (Claude Code)\n", "")],
        false,
    )
    .await;
    assert_eq!(outcome, Outcome::Succeeded);
}

#[tokio::test]
async fn test_a_stopped_claude_upgrade_stays_unconfirmed_even_if_the_version_moves() {
    for stop in [Stop::Cancel, Stop::Timeout] {
        assert_eq!(
            claude_upgrade(stop.output(), vec!["2.1.281", "2.1.290"]).await,
            Outcome::Unconfirmed
        );
    }
}

#[tokio::test]
async fn test_a_claude_update_that_reports_up_to_date_is_not_reported_as_updated() {
    // `claude update` prints `Claude Code is up to date (X)` and exits 0
    // when there is nothing to install (doc text, VERIFIED in
    // .superpowers/phase4/claude.md §6) -- which is also what the race
    // with its own background updater looks like from here. The version
    // read before and after is the same `--version`, so the outcome is
    // the honest one a skipped brew or pipx upgrade gets (spec §4.4 item
    // 5, D5).
    let outcome = claude_upgrade(
        exited_0("Claude Code is up to date (2.1.281)\n", ""),
        vec!["2.1.281", "2.1.281"],
    )
    .await;
    assert_eq!(
        outcome,
        Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade)
    );
}

#[tokio::test]
async fn test_a_claude_update_that_moved_the_version_succeeded() {
    // `Successfully updated from <old> to version <new>` (doc text,
    // VERIFIED, claude.md §6), and the launcher now answers the new
    // version.
    let outcome = claude_upgrade(
        exited_0("Successfully updated from 2.1.281 to version 2.1.290\n", ""),
        vec!["2.1.281", "2.1.290"],
    )
    .await;
    assert_eq!(outcome, Outcome::Succeeded);
}
