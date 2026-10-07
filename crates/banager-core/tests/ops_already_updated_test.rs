//! An update whose package was already at the version its confirmed plan
//! targeted when its turn came (r6 y3-batch, finding 2).
//!
//! Homebrew upgrades a formula's outdated dependencies before the formula
//! itself, so in an Update all an earlier update often brings a later one's
//! package to its new version first: `brew upgrade --formula libpng` then
//! finds libpng current, says so, and exits 0. The two readings around that
//! command are equal, which `run_operation` used to report as
//! `UnchangedAfterUpgrade` ("didn't update: same version") -- for twelve of
//! the author's 34 updates on 2026-10-07, every one of them updated.
//!
//! Now, when the plan was confirmed with a target version
//! (`OperationManager::submit_toward`, the candidate's `target` that
//! `Session::submit` passes on), an unchanged reading at or past that target
//! is an update that is done: `Succeeded`, with `OpSummary::already_updated`
//! saying how -- by an earlier update of the same source that ended after
//! this one was confirmed, or already before its turn for no reason Banager
//! saw. Below the target it is `UnchangedAfterUpgrade` as before, and a plan
//! with no target keeps the old rule.
//!
//! The runner is scripted, and the inventory is the recorded
//! `brew/7.0.3/info-installed.json` with one version edited per reading.

use async_trait::async_trait;
use banager_core::adapters::brew::BrewAdapter;
use banager_core::adapters::pipx::PipxAdapter;
use banager_core::adapters::Adapter;
use banager_core::events::VecSink;
use banager_core::http::MockHttpClient;
use banager_core::model::{
    AlreadyUpdated, ArtifactKind, Attention, ManagerInstance, OpKind, OpRequest, Outcome,
};
use banager_core::ops::OperationManager;
use banager_core::runner::{CommandOutput, CommandRunner, CommandSpec, LineCallback, RunnerError};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

const FIXTURES: &str = "../../adapters/fixtures";
const BREW: &str = "/opt/homebrew/bin/brew";
const BREW_INFO: [&str; 4] = [BREW, "info", "--installed", "--json=v2"];

fn exited(code: i32, stdout: &str, stderr: &str) -> CommandOutput {
    CommandOutput {
        stderr_cause: Default::default(),
        exit_code: Some(code),
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
        timed_out: false,
        cancelled: false,
    }
}

/// How many times an argv has been answered, and its outputs in order.
type Script = (usize, Vec<CommandOutput>);

/// Answers each argv with its scripted outputs in order, repeating the
/// last one, so the inventory can read one way before an update and
/// another way after it.
///
/// An argv with a gate (`gate`) is not answered until the test opens it.
#[derive(Default)]
struct ScriptedRunner {
    scripts: Mutex<HashMap<Vec<String>, Script>>,
    gates: Mutex<HashMap<Vec<String>, Arc<Notify>>>,
}

impl ScriptedRunner {
    fn script(&self, argv: &[&str], outputs: Vec<CommandOutput>) {
        let key = argv.iter().map(|s| s.to_string()).collect();
        self.scripts.lock().unwrap().insert(key, (0, outputs));
    }

    /// Holds `argv` until the returned gate is opened (`notify_one`).
    fn gate(&self, argv: &[&str]) -> Arc<Notify> {
        let key = argv.iter().map(|s| s.to_string()).collect();
        let gate = Arc::new(Notify::new());
        self.gates.lock().unwrap().insert(key, gate.clone());
        gate
    }
}

#[async_trait]
impl CommandRunner for ScriptedRunner {
    async fn run(
        &self,
        spec: CommandSpec,
        _on_line: Option<LineCallback>,
        _cancel: CancellationToken,
    ) -> Result<CommandOutput, RunnerError> {
        let mut key = vec![spec.program.to_string_lossy().to_string()];
        key.extend(spec.args.iter().cloned());
        let gate = self.gates.lock().unwrap().get(&key).cloned();
        if let Some(gate) = gate {
            gate.notified().await;
        }
        let mut scripts = self.scripts.lock().unwrap();
        let Some((calls, outputs)) = scripts.get_mut(&key) else {
            return Err(RunnerError::NoMock(key));
        };
        let output = outputs[(*calls).min(outputs.len() - 1)].clone();
        *calls += 1;
        Ok(output)
    }
}

fn brew_instance() -> ManagerInstance {
    ManagerInstance {
        exe_path: PathBuf::from(BREW),
        prefix: PathBuf::from("/opt/homebrew"),
        version: Some("7.0.3".to_string()),
        ..banager_core::testing::manager_instance("brew", "brew:/opt/homebrew")
    }
}

/// The recorded `brew info --installed --json=v2`, with each formula in
/// `versions` installed (and linked) at the version given.
fn brew_info(versions: &[(&str, &str)]) -> String {
    let path = format!("{FIXTURES}/brew/7.0.3/info-installed.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let mut info: serde_json::Value = serde_json::from_str(&text).expect("fixture");
    for (name, version) in versions {
        let formula = info["formulae"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|f| f["name"] == *name)
            .unwrap_or_else(|| panic!("no formula {name} in the fixture"));
        let mut keg = formula["installed"][0].clone();
        keg["version"] = (*version).into();
        formula["installed"] = serde_json::Value::Array(vec![keg]);
        formula["linked_keg"] = (*version).into();
    }
    info.to_string()
}

fn manager(runner: &Arc<ScriptedRunner>) -> (Arc<OperationManager>, Arc<dyn Adapter>) {
    let adapter: Arc<dyn Adapter> = Arc::new(BrewAdapter::new(runner.clone()));
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    manager.register_instance(brew_instance());
    (manager, adapter)
}

async fn plan_upgrade(adapter: &Arc<dyn Adapter>, name: &str) -> banager_core::model::Plan {
    let req = OpRequest {
        kind: OpKind::Upgrade,
        instance_id: "brew:/opt/homebrew".to_string(),
        artifact_kind: ArtifactKind::Formula,
        name: name.to_string(),
    };
    adapter.plan(&brew_instance(), &req).await.expect("plan")
}

fn already_updated(manager: &OperationManager, op_id: u64) -> Option<AlreadyUpdated> {
    manager
        .summaries()
        .into_iter()
        .find(|s| s.id == op_id)
        .expect("the operation is listed")
        .already_updated
}

/// libpng's update alone, its inventory reading `before` and then `after`,
/// submitted toward `target`.
async fn libpng_alone(
    before: &str,
    after: &str,
    target: Option<&str>,
) -> (Outcome, Option<AlreadyUpdated>) {
    let runner = Arc::new(ScriptedRunner::default());
    runner.script(
        &[BREW, "upgrade", "--formula", "libpng"],
        vec![exited(
            0,
            "",
            &format!("Warning: libpng {after} already installed\n"),
        )],
    );
    runner.script(
        &BREW_INFO,
        vec![
            exited(0, &brew_info(&[("libpng", before)]), ""),
            exited(0, &brew_info(&[("libpng", after)]), ""),
        ],
    );
    let (manager, adapter) = manager(&runner);
    let plan = plan_upgrade(&adapter, "libpng").await;
    let op_id = manager.submit_toward(plan, target.map(str::to_string), None);
    let outcome = manager.wait(op_id).await.expect("an outcome");
    (outcome, already_updated(&manager, op_id))
}

#[tokio::test]
async fn test_an_update_already_at_its_target_when_its_turn_came_is_updated() {
    let (outcome, how) = libpng_alone("1.6.59", "1.6.59", Some("1.6.59")).await;
    assert_eq!(outcome, Outcome::Succeeded);
    // No earlier update of this source ended in between: Banager does not
    // know what brought it there, and says only that it was there.
    assert_eq!(how, Some(AlreadyUpdated::BeforeItsTurn));
}

#[tokio::test]
async fn test_an_update_still_below_its_target_did_not_update() {
    let (outcome, how) = libpng_alone("1.6.58", "1.6.58", Some("1.6.59")).await;
    assert_eq!(
        outcome,
        Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade)
    );
    assert_eq!(how, None);
}

#[tokio::test]
async fn test_an_update_with_no_target_keeps_the_rule_it_had() {
    let (outcome, how) = libpng_alone("1.6.59", "1.6.59", None).await;
    assert_eq!(
        outcome,
        Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade)
    );
    assert_eq!(how, None);
}

#[tokio::test]
async fn test_a_version_past_the_target_has_reached_it_and_a_prerelease_has_not() {
    // A newer version came out between the check and the update, and an
    // earlier update brought it in: past the target is at least it.
    let (outcome, how) = libpng_alone("1.6.60", "1.6.60", Some("1.6.59")).await;
    assert_eq!(outcome, Outcome::Succeeded);
    assert_eq!(how, Some(AlreadyUpdated::BeforeItsTurn));
    // Only versions made of numbers are ordered: "1.7.0-rc1" is not taken
    // for at least "1.7.0", whatever order its characters sort in.
    let (outcome, how) = libpng_alone("1.7.0-rc1", "1.7.0-rc1", Some("1.7.0")).await;
    assert_eq!(
        outcome,
        Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade)
    );
    assert_eq!(how, None);
    // A Homebrew revision is a number like the rest.
    let (outcome, _) = libpng_alone("1.6.59_1", "1.6.59_1", Some("1.6.59")).await;
    assert_eq!(outcome, Outcome::Succeeded);
}

#[tokio::test]
async fn test_an_update_an_earlier_update_of_the_batch_completed_says_so() {
    // Update all of harfbuzz and libpng, in that order: `brew upgrade
    // harfbuzz` upgrades libpng first, as a dependency, so when libpng's
    // turn comes it is already at 1.6.59.
    let runner = Arc::new(ScriptedRunner::default());
    runner.script(
        &[BREW, "upgrade", "--formula", "harfbuzz"],
        vec![exited(0, "==> Upgrading harfbuzz\n", "")],
    );
    runner.script(
        &[BREW, "upgrade", "--formula", "libpng"],
        vec![exited(0, "", "Warning: libpng 1.6.59 already installed\n")],
    );
    let old = brew_info(&[]);
    let new = brew_info(&[("harfbuzz", "14.5.0"), ("libpng", "1.6.59")]);
    runner.script(
        &BREW_INFO,
        vec![
            exited(0, &old, ""), // harfbuzz, before
            exited(0, &new, ""), // harfbuzz, after
            exited(0, &new, ""), // libpng, before
            exited(0, &new, ""), // libpng, after
        ],
    );
    let (manager, adapter) = manager(&runner);
    let harfbuzz = plan_upgrade(&adapter, "harfbuzz").await;
    let libpng = plan_upgrade(&adapter, "libpng").await;
    let first = manager.submit_toward(harfbuzz, Some("14.5.0".to_string()), None);
    // The history is handed the same: what `Session::submit` records.
    let told = Arc::new(Mutex::new(None));
    let tell = told.clone();
    let second = manager.submit_toward(
        libpng,
        Some("1.6.59".to_string()),
        Some(Box::new(move |ended| {
            *tell.lock().unwrap() = Some((ended.outcome.clone(), ended.already_updated));
        })),
    );
    assert_eq!(manager.wait(first).await, Some(Outcome::Succeeded));
    assert_eq!(manager.wait(second).await, Some(Outcome::Succeeded));
    assert_eq!(
        *told.lock().unwrap(),
        Some((Outcome::Succeeded, Some(AlreadyUpdated::ByEarlierUpdate)))
    );
    assert_eq!(already_updated(&manager, first), None, "harfbuzz moved");
    assert_eq!(
        already_updated(&manager, second),
        Some(AlreadyUpdated::ByEarlierUpdate)
    );
}

#[tokio::test]
async fn test_an_update_of_another_source_does_not_count_as_an_earlier_one() {
    // An update that ended on another source changed nothing of this
    // one's: only the same source's count. libpng's update is confirmed
    // first and held at its command until an update of another Homebrew
    // has ended.
    let runner = Arc::new(ScriptedRunner::default());
    runner.script(
        &[BREW, "upgrade", "--formula", "libpng"],
        vec![exited(0, "", "Warning: libpng 1.6.59 already installed\n")],
    );
    let held = runner.gate(&[BREW, "upgrade", "--formula", "libpng"]);
    runner.script(
        &BREW_INFO,
        vec![exited(0, &brew_info(&[("libpng", "1.6.59")]), "")],
    );
    let (manager, adapter) = manager(&runner);
    let other = ManagerInstance {
        prefix: PathBuf::from("/usr/local"),
        exe_path: PathBuf::from("/usr/local/bin/brew"),
        ..banager_core::testing::manager_instance("brew", "brew:/usr/local")
    };
    manager.register_instance(other.clone());
    runner.script(
        &["/usr/local/bin/brew", "upgrade", "--formula", "harfbuzz"],
        vec![exited(0, "==> Upgrading harfbuzz\n", "")],
    );
    runner.script(
        &["/usr/local/bin/brew", "info", "--installed", "--json=v2"],
        vec![
            exited(0, &brew_info(&[]), ""),
            exited(0, &brew_info(&[("harfbuzz", "14.5.0")]), ""),
        ],
    );
    let req = OpRequest {
        kind: OpKind::Upgrade,
        instance_id: "brew:/usr/local".to_string(),
        artifact_kind: ArtifactKind::Formula,
        name: "harfbuzz".to_string(),
    };
    let elsewhere = adapter.plan(&other, &req).await.expect("plan");
    let libpng = plan_upgrade(&adapter, "libpng").await;
    let first = manager.submit_toward(libpng, Some("1.6.59".to_string()), None);
    let second = manager.submit_toward(elsewhere, Some("14.5.0".to_string()), None);
    assert_eq!(manager.wait(second).await, Some(Outcome::Succeeded));
    held.notify_one();
    assert_eq!(manager.wait(first).await, Some(Outcome::Succeeded));
    assert_eq!(
        already_updated(&manager, first),
        Some(AlreadyUpdated::BeforeItsTurn)
    );
}

// --- Review of y3-batch, finding 1 -----------------------------------------
//
// An update counts as one that may have brought a later one along only
// when it may have changed something itself: its version moved (or there
// was nothing to compare), it failed (Homebrew upgrades the dependencies
// before the formula, so a failed upgrade may still have moved them), or
// it was stopped partway. One that was already at its target, or that the
// tool skipped, changed nothing, and an update after it that finds its
// package already new was not done by it. And only on a source where one
// update can update another package at all: Homebrew.

#[tokio::test]
async fn test_an_update_after_one_that_changed_nothing_was_not_done_by_it() {
    // The person ran `brew upgrade` in Terminal, then pressed Update all on
    // the check from before: libpng and libtiff are both at their new
    // versions before the batch starts. Neither command changes anything,
    // so neither was done by the other.
    let runner = Arc::new(ScriptedRunner::default());
    runner.script(
        &[BREW, "upgrade", "--formula", "libpng"],
        vec![exited(0, "", "Warning: libpng 1.6.59 already installed\n")],
    );
    runner.script(
        &[BREW, "upgrade", "--formula", "libtiff"],
        vec![exited(0, "", "Warning: libtiff 4.7.3 already installed\n")],
    );
    let new = brew_info(&[("libpng", "1.6.59"), ("libtiff", "4.7.3")]);
    runner.script(&BREW_INFO, vec![exited(0, &new, "")]);
    let (manager, adapter) = manager(&runner);
    let libpng = plan_upgrade(&adapter, "libpng").await;
    let libtiff = plan_upgrade(&adapter, "libtiff").await;
    let first = manager.submit_toward(libpng, Some("1.6.59".to_string()), None);
    let second = manager.submit_toward(libtiff, Some("4.7.3".to_string()), None);
    assert_eq!(manager.wait(first).await, Some(Outcome::Succeeded));
    assert_eq!(manager.wait(second).await, Some(Outcome::Succeeded));
    assert_eq!(
        already_updated(&manager, first),
        Some(AlreadyUpdated::BeforeItsTurn)
    );
    assert_eq!(
        already_updated(&manager, second),
        Some(AlreadyUpdated::BeforeItsTurn),
        "the update before it changed nothing"
    );
}

#[tokio::test]
async fn test_an_update_after_one_the_tool_skipped_was_not_done_by_it() {
    // harfbuzz's upgrade exits 0 and moves nothing, below its target (a
    // pinned or disabled formula): `UnchangedAfterUpgrade`, and it brought
    // nothing along either.
    let runner = Arc::new(ScriptedRunner::default());
    runner.script(
        &[BREW, "upgrade", "--formula", "harfbuzz"],
        vec![exited(0, "", "")],
    );
    runner.script(
        &[BREW, "upgrade", "--formula", "libpng"],
        vec![exited(0, "", "Warning: libpng 1.6.59 already installed\n")],
    );
    let info = brew_info(&[("libpng", "1.6.59")]);
    runner.script(&BREW_INFO, vec![exited(0, &info, "")]);
    let (manager, adapter) = manager(&runner);
    let harfbuzz = plan_upgrade(&adapter, "harfbuzz").await;
    let libpng = plan_upgrade(&adapter, "libpng").await;
    let first = manager.submit_toward(harfbuzz, Some("14.5.0".to_string()), None);
    let second = manager.submit_toward(libpng, Some("1.6.59".to_string()), None);
    assert_eq!(
        manager.wait(first).await,
        Some(Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade))
    );
    assert_eq!(manager.wait(second).await, Some(Outcome::Succeeded));
    assert_eq!(
        already_updated(&manager, second),
        Some(AlreadyUpdated::BeforeItsTurn)
    );
}

#[tokio::test]
async fn test_a_failed_homebrew_update_may_have_done_a_later_one() {
    // `brew upgrade harfbuzz` upgrades libpng, as a dependency, and then
    // fails on harfbuzz itself: libpng was moved all the same.
    let runner = Arc::new(ScriptedRunner::default());
    runner.script(
        &[BREW, "upgrade", "--formula", "harfbuzz"],
        vec![exited(
            1,
            "",
            "Error: harfbuzz: Failed executing: meson compile -C build\n",
        )],
    );
    runner.script(
        &[BREW, "upgrade", "--formula", "libpng"],
        vec![exited(0, "", "Warning: libpng 1.6.59 already installed\n")],
    );
    let old = brew_info(&[]);
    let new = brew_info(&[("libpng", "1.6.59")]);
    runner.script(
        &BREW_INFO,
        vec![
            exited(0, &old, ""), // harfbuzz, before
            exited(0, &new, ""), // harfbuzz, after
            exited(0, &new, ""), // libpng, before
            exited(0, &new, ""), // libpng, after
        ],
    );
    let (manager, adapter) = manager(&runner);
    let harfbuzz = plan_upgrade(&adapter, "harfbuzz").await;
    let libpng = plan_upgrade(&adapter, "libpng").await;
    let first = manager.submit_toward(harfbuzz, Some("14.5.0".to_string()), None);
    let second = manager.submit_toward(libpng, Some("1.6.59".to_string()), None);
    assert!(matches!(
        manager.wait(first).await,
        Some(Outcome::Failed { .. })
    ));
    assert_eq!(manager.wait(second).await, Some(Outcome::Succeeded));
    assert_eq!(
        already_updated(&manager, second),
        Some(AlreadyUpdated::ByEarlierUpdate)
    );
}

const PIPX: &str = "/opt/homebrew/bin/pipx";

/// The recorded `pipx list --json`, with a second venv, httpie, copied
/// from cowsay's, and each venv's version as given.
fn pipx_list(cowsay: &str, httpie: &str) -> String {
    let path = format!("{FIXTURES}/pipx/1.17.3/list.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let mut list: serde_json::Value = serde_json::from_str(&text).expect("fixture");
    let venvs = list["venvs"].as_object_mut().expect("venvs");
    let mut copy = venvs["cowsay"].clone();
    let main = &mut copy["metadata"]["main_package"];
    main["package"] = "httpie".into();
    main["package_or_url"] = "httpie".into();
    main["package_version"] = httpie.into();
    venvs.insert("httpie".to_string(), copy);
    venvs["cowsay"]["metadata"]["main_package"]["package_version"] = cowsay.into();
    list.to_string()
}

#[tokio::test]
async fn test_on_a_source_where_one_update_never_updates_another_none_is_done_by_an_earlier_one() {
    // pipx keeps each tool in a venv of its own: upgrading cowsay changes
    // nothing of httpie's. cowsay's update moved it; httpie was at its new
    // version before its turn all the same (updated in Terminal, say), and
    // is not said to be done by cowsay's.
    let runner = Arc::new(ScriptedRunner::default());
    runner.script(
        &[PIPX, "upgrade", "cowsay"],
        vec![exited(0, "upgraded package cowsay\n", "")],
    );
    runner.script(
        &[PIPX, "upgrade", "httpie"],
        vec![exited(0, "httpie is already at latest version 3.3\n", "")],
    );
    runner.script(
        &[PIPX, "list", "--json"],
        vec![
            exited(0, &pipx_list("5.0", "3.3"), ""), // cowsay plan: pin check
            exited(0, &pipx_list("5.0", "3.3"), ""), // httpie plan: pin check
            exited(0, &pipx_list("5.0", "3.3"), ""), // cowsay, before
            exited(0, &pipx_list("6.1", "3.3"), ""), // cowsay, after; httpie's two
        ],
    );
    let inst = ManagerInstance {
        exe_path: PathBuf::from(PIPX),
        ..banager_core::testing::manager_instance("pipx", "pipx")
    };
    let adapter: Arc<dyn Adapter> = Arc::new(PipxAdapter::new(
        runner.clone(),
        Arc::new(MockHttpClient::new()),
    ));
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    manager.register_instance(inst.clone());
    let plan = |name: &str| OpRequest {
        kind: OpKind::Upgrade,
        instance_id: inst.id.clone(),
        artifact_kind: ArtifactKind::Tool,
        name: name.to_string(),
    };
    let cowsay = adapter.plan(&inst, &plan("cowsay")).await.expect("plan");
    let httpie = adapter.plan(&inst, &plan("httpie")).await.expect("plan");
    let first = manager.submit_toward(cowsay, Some("6.1".to_string()), None);
    let second = manager.submit_toward(httpie, Some("3.3".to_string()), None);
    assert_eq!(manager.wait(first).await, Some(Outcome::Succeeded));
    assert_eq!(already_updated(&manager, first), None, "cowsay moved");
    assert_eq!(manager.wait(second).await, Some(Outcome::Succeeded));
    assert_eq!(
        already_updated(&manager, second),
        Some(AlreadyUpdated::BeforeItsTurn)
    );
}

/// `brew_info(&[])` with the cask `token` installed at `version`.
fn brew_info_with_cask(token: &str, version: &str, formulae: &[(&str, &str)]) -> String {
    let mut info: serde_json::Value = serde_json::from_str(&brew_info(formulae)).expect("json");
    let cask = info["casks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["token"] == token)
        .unwrap_or_else(|| panic!("no cask {token} in the fixture"));
    cask["installed"] = version.into();
    info.to_string()
}

#[tokio::test]
async fn test_a_cask_update_does_not_count_as_one_that_brought_a_formula_along() {
    // Homebrew upgrades a formula's dependencies, which are formulae: a
    // cask's update that moved its version brought no formula along.
    // onyx moved; libpng was at its new version before its turn all the
    // same.
    let runner = Arc::new(ScriptedRunner::default());
    runner.script(
        &[BREW, "upgrade", "--cask", "onyx"],
        vec![exited(0, "==> Upgrading onyx\n", "")],
    );
    runner.script(
        &[BREW, "upgrade", "--formula", "libpng"],
        vec![exited(0, "", "Warning: libpng 1.6.59 already installed\n")],
    );
    let old = brew_info_with_cask("onyx", "5.0.2", &[("libpng", "1.6.59")]);
    let new = brew_info_with_cask("onyx", "5.1.0", &[("libpng", "1.6.59")]);
    runner.script(
        &BREW_INFO,
        vec![
            exited(0, &old, ""), // onyx, before
            exited(0, &new, ""), // onyx, after
            exited(0, &new, ""), // libpng, before
            exited(0, &new, ""), // libpng, after
        ],
    );
    let (manager, adapter) = manager(&runner);
    let req = OpRequest {
        kind: OpKind::Upgrade,
        instance_id: "brew:/opt/homebrew".to_string(),
        artifact_kind: ArtifactKind::Cask,
        name: "onyx".to_string(),
    };
    let onyx = adapter.plan(&brew_instance(), &req).await.expect("plan");
    let libpng = plan_upgrade(&adapter, "libpng").await;
    let first = manager.submit_toward(onyx, Some("5.1.0".to_string()), None);
    let second = manager.submit_toward(libpng, Some("1.6.59".to_string()), None);
    assert_eq!(manager.wait(first).await, Some(Outcome::Succeeded));
    assert_eq!(already_updated(&manager, first), None, "onyx moved");
    assert_eq!(manager.wait(second).await, Some(Outcome::Succeeded));
    assert_eq!(
        already_updated(&manager, second),
        Some(AlreadyUpdated::BeforeItsTurn)
    );
}

#[tokio::test]
async fn test_a_numeric_prerelease_an_update_left_as_it_was_is_below_its_release() {
    // r11 F1: npm 1.2.3-1 is offered 1.2.3, its stable release (SemVer
    // puts a prerelease before its release, digits only or not). With
    // `dry-run=true` in the person's `.npmrc`, `npm install -g` exits 0 and
    // installs nothing; both readings say 1.2.3-1. That is not an update
    // already done: by its digits 1.2.3-1 would sort after 1.2.3.
    use banager_core::adapters::npm::NpmAdapter;
    let runner = Arc::new(ScriptedRunner::default());
    let npm = "/opt/homebrew/bin/npm";
    let prefix =
        std::env::temp_dir().join(format!("banager-prerelease-npm-{}", std::process::id()));
    std::fs::create_dir_all(&prefix).unwrap();
    let prefix_text = prefix.to_str().unwrap();
    let inst = ManagerInstance {
        exe_path: npm.into(),
        prefix: prefix.clone(),
        ..banager_core::testing::manager_instance("npm", "npm:/opt/homebrew")
    };
    // `npm ls`'s shape, the same before and after the install.
    runner.script(
        &[
            npm,
            "ls",
            "-g",
            "--depth=0",
            "--json",
            "--prefix",
            prefix_text,
        ],
        vec![exited(
            0,
            r#"{"name":"lib","dependencies":{"example-cli":{"version":"1.2.3-1"}}}"#,
            "",
        )],
    );
    runner.script(
        &[
            npm,
            "install",
            "-g",
            "example-cli@latest",
            "--prefix",
            prefix_text,
        ],
        vec![exited(0, "", "")],
    );
    let adapter = Arc::new(NpmAdapter::new(runner));
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    manager.register_adapter(adapter.clone());
    manager.register_instance(inst.clone());
    let manager = Arc::new(manager);
    let plan = adapter
        .plan(
            &inst,
            &OpRequest {
                kind: OpKind::Upgrade,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Package,
                name: "example-cli".into(),
            },
        )
        .await
        .unwrap();
    let told = Arc::new(Mutex::new(None));
    let tell = told.clone();
    let id = manager.submit_toward(
        plan,
        Some("1.2.3".into()),
        Some(Box::new(move |ended| {
            *tell.lock().unwrap() = Some((ended.outcome.clone(), ended.already_updated));
        })),
    );
    let expected = Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade);
    assert_eq!(manager.wait(id).await, Some(expected.clone()));
    assert_eq!(already_updated(&manager, id), None);
    assert_eq!(*told.lock().unwrap(), Some((expected, None)));
    std::fs::remove_dir_all(prefix).unwrap();
}

#[tokio::test]
async fn test_a_homebrew_version_with_a_hyphen_at_its_target_is_still_updated() {
    // ImageMagick numbers its releases 7.1.1-47: Homebrew's version and
    // `brew outdated`'s target are the same string, which is enough. Only
    // ordering past the target is given up where there is a hyphen.
    let (outcome, how) = libpng_alone("7.1.1-47", "7.1.1-47", Some("7.1.1-47")).await;
    assert_eq!(outcome, Outcome::Succeeded);
    assert_eq!(how, Some(AlreadyUpdated::BeforeItsTurn));
    let (outcome, how) = libpng_alone("7.1.1-47_1", "7.1.1-47_1", Some("7.1.1-47_1")).await;
    assert_eq!(outcome, Outcome::Succeeded);
    assert_eq!(how, Some(AlreadyUpdated::BeforeItsTurn));
    let (outcome, how) = libpng_alone("7.1.1-46", "7.1.1-46", Some("7.1.1-47")).await;
    assert_eq!(
        outcome,
        Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade)
    );
    assert_eq!(how, None);
}
