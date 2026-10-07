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
use banager_core::adapters::Adapter;
use banager_core::events::VecSink;
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
