//! What a confirmed operation's last check before its command ends in
//! (r20 review of 72818bba), end to end through the real adapters and
//! `OperationManager`, with only the commands' output scripted.
//!
//! uv and Cargo fingerprint what an upgrade's preview was worked out from
//! (`Plan::basis`) and read it again right before the command; npm reads
//! `npm prefix -g` again. The inventories are the recorded fixtures under
//! `adapters/fixtures/` (uv 0.12.17's `tool list --show-paths`, Cargo
//! 1.98.1's `.crates2.json`), with one version edited where the case says
//! the tool moved.
//!
//! R20-1: an update whose tool reached its confirmed target another way
//! before its turn (a `uv tool upgrade` or `cargo install` in Terminal)
//! is the designed `AlreadyUpdated::BeforeItsTurn`, as for Homebrew, npm,
//! pipx and Ollama -- not "changed since shown": the installed version is
//! for the readings around the command to judge, not part of the
//! fingerprint.
//!
//! Every program sits inside the test's own temp folder, so nothing here
//! reads this Mac's Homebrew or home.

use async_trait::async_trait;
use banager_core::adapters::cargo::CargoAdapter;
use banager_core::adapters::uv::UvAdapter;
use banager_core::adapters::Adapter;
use banager_core::events::VecSink;
use banager_core::http::MockHttpClient;
use banager_core::model::{
    AlreadyUpdated, ArtifactKind, ManagerInstance, OpKind, OpRequest, Outcome, Plan, PlanAction,
};
use banager_core::ops::OperationManager;
use banager_core::runner::{CommandOutput, CommandRunner, CommandSpec, LineCallback, RunnerError};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

const FIXTURES: &str = "../../adapters/fixtures";

fn fixture(path: &str) -> String {
    let path = format!("{FIXTURES}/{path}");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

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
/// last one, and keeps every argv it was asked to run.
#[derive(Default)]
struct ScriptedRunner {
    scripts: Mutex<HashMap<Vec<String>, Script>>,
    calls: Mutex<Vec<Vec<String>>>,
}

impl ScriptedRunner {
    fn script(&self, argv: &[&str], outputs: Vec<CommandOutput>) {
        let key = argv.iter().map(|s| s.to_string()).collect();
        self.scripts.lock().unwrap().insert(key, (0, outputs));
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
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
        self.calls.lock().unwrap().push(key.clone());
        let mut scripts = self.scripts.lock().unwrap();
        let Some((calls, outputs)) = scripts.get_mut(&key) else {
            return Err(RunnerError::NoMock(key));
        };
        let output = outputs[(*calls).min(outputs.len() - 1)].clone();
        *calls += 1;
        Ok(output)
    }
}

/// The plan's program and arguments, as the runner is asked for them.
fn argv(plan: &Plan) -> Vec<String> {
    let PlanAction::Command { program, args, .. } = &plan.action else {
        panic!("a command plan");
    };
    std::iter::once(program.to_string_lossy().into_owned())
        .chain(args.iter().cloned())
        .collect()
}

/// The request for `kind` on `name` in `inst`.
fn request(
    inst: &ManagerInstance,
    kind: OpKind,
    artifact_kind: ArtifactKind,
    name: &str,
) -> OpRequest {
    OpRequest {
        kind,
        instance_id: inst.id.clone(),
        artifact_kind,
        name: name.to_string(),
    }
}

/// Plans `request`, hands the plan to `script` (to script its command),
/// runs `between` (what happened on the Mac after the confirmation), then
/// submits the plan toward `target` through a fresh `OperationManager`:
/// its outcome, and how it was already updated.
async fn confirm_then_run(
    adapter: Arc<dyn Adapter>,
    inst: &ManagerInstance,
    request: OpRequest,
    target: Option<&str>,
    script: impl FnOnce(&Plan),
    between: impl FnOnce(),
) -> (Outcome, Option<AlreadyUpdated>) {
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    manager.register_instance(inst.clone());
    let plan = adapter.plan(inst, &request).await.expect("plan");
    script(&plan);
    between();
    let op_id = manager.submit_toward(plan, target.map(str::to_string), None);
    let outcome = manager.wait(op_id).await.expect("an outcome");
    let how = manager
        .summaries()
        .into_iter()
        .find(|s| s.id == op_id)
        .expect("the operation is listed")
        .already_updated;
    (outcome, how)
}

// --- uv -------------------------------------------------------------------

/// The recorded `uv tool list --show-paths` with ruff at `version`, its
/// environment the test's own folder.
fn uv_list(env: &Path, version: &str) -> String {
    let recorded = fixture("uv/0.12.17/tool-list-show-paths.txt");
    let list = recorded
        .replace(
            "/Users/brulek/.local/share/uv/tools/ruff",
            env.to_str().unwrap(),
        )
        .replace("ruff v0.15.0", &format!("ruff v{version}"));
    assert_ne!(list, recorded, "the fixture's path and version were found");
    list
}

/// A uv in `dir/bin/uv` with ruff's environment at `dir/ruff`, its
/// receipt in the shape uv writes one (requirements, entrypoints and
/// `[tool.options]`).
fn uv_setup(dir: &Path) -> (ManagerInstance, std::path::PathBuf) {
    let env = dir.join("ruff");
    std::fs::create_dir_all(&env).unwrap();
    std::fs::write(
        env.join("uv-receipt.toml"),
        format!(
            "[tool]\nrequirements = [{{ name = \"ruff\" }}]\nentrypoints = [\n    {{ name = \"ruff\", install-path = \"{}/bin/ruff\", from = \"ruff\" }},\n]\n\n[tool.options]\nexclude-newer-package = {{}}\n",
            dir.display()
        ),
    )
    .unwrap();
    let inst = ManagerInstance {
        exe_path: dir.join("bin/uv"),
        prefix: dir.join("bin"),
        version: Some("0.12.17".to_string()),
        ..banager_core::testing::manager_instance("uv", "uv")
    };
    (inst, env)
}

#[tokio::test]
async fn test_a_uv_tool_at_its_target_before_its_turn_was_already_updated() {
    // ruff confirmed at 0.15.0 toward 0.16.8 (the recorded
    // `tool list --outdated`'s latest); before its turn a `uv tool
    // upgrade ruff` in Terminal brought it to 0.16.8. uv rewrites the
    // receipt as it was; only the version in the list moved.
    let dir = tempfile::tempdir().unwrap();
    let (inst, env) = uv_setup(dir.path());
    let uv = inst.exe_path.to_str().unwrap().to_string();
    let list = [uv.as_str(), "tool", "list", "--show-paths"];
    let upgrade = [uv.as_str(), "tool", "upgrade", "ruff"];
    let runner = Arc::new(ScriptedRunner::default());
    runner.script(
        &list,
        vec![
            exited(0, &uv_list(&env, "0.15.0"), ""), // plan
            exited(0, &uv_list(&env, "0.16.8"), ""), // before, recheck, after
        ],
    );
    runner.script(&upgrade, vec![exited(0, "", "Nothing to upgrade\n")]);
    let (outcome, how) = confirm_then_run(
        Arc::new(UvAdapter::new(runner.clone())),
        &inst,
        request(&inst, OpKind::Upgrade, ArtifactKind::Tool, "ruff"),
        Some("0.16.8"),
        |_| {},
        || {},
    )
    .await;
    assert_eq!(outcome, Outcome::Succeeded, "{:?}", runner.calls());
    assert_eq!(how, Some(AlreadyUpdated::BeforeItsTurn));
    let upgrades = runner
        .calls()
        .into_iter()
        .filter(|call| call.iter().any(|arg| arg == "upgrade"))
        .count();
    assert_eq!(upgrades, 1, "the confirmed command ran, as previewed");
}

// --- Cargo ----------------------------------------------------------------

const CRATES_IO: &str = "registry+https://github.com/rust-lang/crates.io-index";

/// The recorded `.crates2.json` with hexyl at `version`, built by `rustc`
/// at `release` (the recorded one is 1.98.1).
fn crates2(version: &str, release: &str) -> String {
    let recorded = fixture("cargo/1.98.1/crates2.json");
    let edited = recorded
        .replace("hexyl 0.17.0 (", &format!("hexyl {version} ("))
        .replace("rustc 1.98.1 (", &format!("rustc {release} ("))
        .replace("release: 1.98.1", &format!("release: {release}"));
    assert!(edited.contains(&format!("hexyl {version} ({CRATES_IO})")));
    edited
}

/// Writes hexyl's records at `version` into `root`, as `cargo install`
/// leaves both manifests.
fn write_hexyl(root: &Path, version: &str, release: &str) {
    std::fs::write(root.join(".crates2.json"), crates2(version, release)).unwrap();
    std::fs::write(
        root.join(".crates.toml"),
        format!("[v1]\n\"hexyl {version} ({CRATES_IO})\" = [\"hexyl\"]\n"),
    )
    .unwrap();
}

/// hexyl confirmed at 0.17.0 toward 0.18.0; before its turn a `cargo
/// install hexyl` in Terminal, by a compiler at `release`, brought it to
/// 0.18.0.
async fn hexyl_updated_in_terminal(release: &str) -> (Outcome, Option<AlreadyUpdated>, usize) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_owned();
    write_hexyl(&root, "0.17.0", "1.98.1");
    let inst = ManagerInstance {
        exe_path: root.join("bin/cargo"),
        prefix: root.clone(),
        version: Some("1.98.1".to_string()),
        ..banager_core::testing::manager_instance("cargo", "cargo")
    };
    let runner = Arc::new(ScriptedRunner::default());
    let scripted = runner.clone();
    let (outcome, how) = confirm_then_run(
        Arc::new(CargoAdapter::new(
            runner.clone(),
            Arc::new(MockHttpClient::new()),
        )),
        &inst,
        request(&inst, OpKind::Upgrade, ArtifactKind::Binary, "hexyl"),
        Some("0.18.0"),
        move |plan| {
            let argv = argv(plan);
            let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
            scripted.script(
                &argv,
                vec![exited(
                    0,
                    "",
                    "    Replacing hexyl v0.18.0\n     Replaced package `hexyl v0.18.0` with `hexyl v0.18.0`\n",
                )],
            );
        },
        || write_hexyl(&root, "0.18.0", release),
    )
    .await;
    (outcome, how, runner.calls().len())
}

#[tokio::test]
async fn test_a_cargo_crate_at_its_target_before_its_turn_was_already_updated() {
    let (outcome, how, commands) = hexyl_updated_in_terminal("1.98.1").await;
    assert_eq!(outcome, Outcome::Succeeded, "{how:?}");
    assert_eq!(how, Some(AlreadyUpdated::BeforeItsTurn));
    assert_eq!(commands, 1, "the confirmed command ran, as previewed");
}

#[tokio::test]
async fn test_a_cargo_crate_at_its_target_built_by_a_newer_rust_was_already_updated() {
    // The usual way it happens: the toolchain was updated since hexyl's
    // last install, so the record's `rustc -vV` moved with its version.
    // The compiler's version is no build choice either; its host is.
    let (outcome, how, commands) = hexyl_updated_in_terminal("1.99.0").await;
    assert_eq!(outcome, Outcome::Succeeded, "{how:?}");
    assert_eq!(how, Some(AlreadyUpdated::BeforeItsTurn));
    assert_eq!(commands, 1, "the confirmed command ran, as previewed");
}
