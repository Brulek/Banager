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
//! fingerprint, and neither are the commands uv installed for it (a new
//! version can bring one) nor the compiler Cargo built it with.
//!
//! R20-2: when the read before the command does not answer -- the
//! program is gone, its launcher cannot start (`env: node: No such file
//! or directory`, npm's `#!/usr/bin/env node` with no `node`), or it ran
//! out of time -- nothing changed after the confirmation: the operation
//! ends as the program's own failure would, with the runner's error or
//! the read's own exit and words (its stderr in the operation's log, as
//! a command's is), or as taking too long, and nothing written. Only a
//! read that answers with something else (another prefix, a changed
//! receipt) is "changed since shown".
//!
//! Every program sits inside the test's own temp folder, so nothing here
//! reads this Mac's Homebrew or home.

use async_trait::async_trait;
use banager_core::adapters::cargo::CargoAdapter;
use banager_core::adapters::npm::NpmAdapter;
use banager_core::adapters::uv::UvAdapter;
use banager_core::adapters::Adapter;
use banager_core::events::{OperationEvent, Stream, VecSink};
use banager_core::history::FailureCause;
use banager_core::http::MockHttpClient;
use banager_core::model::{
    AlreadyUpdated, ArtifactKind, Fault, ManagerInstance, OpKind, OpRequest, Outcome, Plan,
    PlanAction,
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

/// A command the runner stopped at its deadline, as `RealRunner` reports
/// one: no exit code, `timed_out`, nothing written.
fn timed_out() -> CommandOutput {
    CommandOutput {
        exit_code: None,
        timed_out: true,
        ..exited(0, "", "")
    }
}

/// What npm's launcher (`#!/usr/bin/env node`) does with no `node` on
/// `PATH`: macOS's `env` says so and exits 127 -- the 2026-10-07 shape,
/// after a `brew upgrade node` that could not link `node` again.
fn no_node() -> CommandOutput {
    exited(127, "", "env: node: No such file or directory\n")
}

/// Whether any of `calls` is a write: the operation's own command.
fn wrote(calls: &[Vec<String>]) -> bool {
    calls.iter().any(|call| {
        call.iter()
            .any(|arg| matches!(arg.as_str(), "install" | "uninstall" | "upgrade"))
    })
}

/// An argv's answer: an output, or the runner's own error -- the program
/// is not there, as `RealRunner` says when the spawn finds no file.
#[derive(Clone)]
enum Answer {
    Output(CommandOutput),
    NotFound,
}

/// How many times an argv has been answered, and its answers in order.
type Script = (usize, Vec<Answer>);

/// Answers each argv with its scripted answers in order, repeating the
/// last one, and any other argv with `otherwise` when one is set; keeps
/// every argv it was asked to run.
#[derive(Default)]
struct ScriptedRunner {
    scripts: Mutex<HashMap<Vec<String>, Script>>,
    otherwise: Mutex<Option<Answer>>,
    calls: Mutex<Vec<Vec<String>>>,
}

impl ScriptedRunner {
    fn script(&self, argv: &[&str], outputs: Vec<CommandOutput>) {
        self.script_answers(argv, outputs.into_iter().map(Answer::Output).collect());
    }

    fn script_answers(&self, argv: &[&str], answers: Vec<Answer>) {
        let key = argv.iter().map(|s| s.to_string()).collect();
        self.scripts.lock().unwrap().insert(key, (0, answers));
    }

    /// Every argv with no script of its own is answered with `answer`.
    fn otherwise(&self, answer: Answer) {
        *self.otherwise.lock().unwrap() = Some(answer);
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
        let answer = match self.scripts.lock().unwrap().get_mut(&key) {
            Some((calls, answers)) => {
                let answer = answers[(*calls).min(answers.len() - 1)].clone();
                *calls += 1;
                answer
            }
            None => match self.otherwise.lock().unwrap().clone() {
                Some(answer) => answer,
                None => return Err(RunnerError::NoMock(key)),
            },
        };
        match answer {
            Answer::Output(output) => Ok(output),
            Answer::NotFound => Err(RunnerError::NotFound(spec.program)),
        }
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

/// How a confirmed operation ended: its outcome, how it was already
/// updated, and the lines its log was given (the window's operation log).
struct Ran {
    outcome: Outcome,
    how: Option<AlreadyUpdated>,
    log: Vec<(Stream, String)>,
}

/// Plans `request`, hands the plan to `script` (to script its command),
/// runs `between` (what happened on the Mac after the confirmation), then
/// submits the plan toward `target` through a fresh `OperationManager`.
async fn confirm_then_run(
    adapter: Arc<dyn Adapter>,
    inst: &ManagerInstance,
    request: OpRequest,
    target: Option<&str>,
    script: impl FnOnce(&Plan),
    between: impl FnOnce(),
) -> Ran {
    let sink = Arc::new(VecSink::new());
    let mut manager = OperationManager::new(sink.clone());
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
    let log = sink
        .snapshot()
        .into_iter()
        .filter_map(|event| match event {
            OperationEvent::Log {
                op_id: id,
                stream,
                line,
            } if id == op_id => Some((stream, line)),
            _ => None,
        })
        .collect();
    Ran { outcome, how, log }
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
        ruff_receipt(dir, &[("ruff", "ruff")]),
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

/// ruff confirmed at 0.15.0 toward 0.16.8 (the recorded `tool list
/// --outdated`'s latest); before its turn a `uv tool upgrade ruff` in
/// Terminal brought it to 0.16.8 and wrote its receipt again, which
/// `rewrite` does to the receipt `uv_setup` wrote (`dir`, then ruff's
/// environment): the outcome, how it was already updated, and how many
/// upgrade commands ran.
async fn ruff_updated_in_terminal(
    rewrite: impl FnOnce(&Path, &Path),
) -> (Outcome, Option<AlreadyUpdated>, usize) {
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
    let ran = confirm_then_run(
        Arc::new(UvAdapter::new(runner.clone())),
        &inst,
        request(&inst, OpKind::Upgrade, ArtifactKind::Tool, "ruff"),
        Some("0.16.8"),
        |_| {},
        || rewrite(dir.path(), &env),
    )
    .await;
    let upgrades = runner
        .calls()
        .into_iter()
        .filter(|call| call.iter().any(|arg| arg == "upgrade"))
        .count();
    (ran.outcome, ran.how, upgrades)
}

/// ruff's receipt as `uv_setup` writes it, with `entrypoints` in its
/// place: what uv writes after an upgrade, the commands it installed
/// for the version it installed (`finalize_tool_install`).
fn ruff_receipt(dir: &Path, entrypoints: &[(&str, &str)]) -> String {
    let entrypoints: String = entrypoints
        .iter()
        .map(|(name, from)| {
            format!(
                "    {{ name = \"{name}\", install-path = \"{}/bin/{name}\", from = \"{from}\" }},\n",
                dir.display()
            )
        })
        .collect();
    format!(
        "[tool]\nrequirements = [{{ name = \"ruff\" }}]\nentrypoints = [\n{entrypoints}]\n\n[tool.options]\nexclude-newer-package = {{}}\n"
    )
}

#[tokio::test]
async fn test_a_uv_tool_at_its_target_before_its_turn_was_already_updated() {
    // uv wrote the receipt again as it was; only the version in the list
    // moved.
    let (outcome, how, upgrades) = ruff_updated_in_terminal(|dir, env| {
        std::fs::write(
            env.join("uv-receipt.toml"),
            ruff_receipt(dir, &[("ruff", "ruff")]),
        )
        .unwrap()
    })
    .await;
    assert_eq!(outcome, Outcome::Succeeded, "{how:?}");
    assert_eq!(how, Some(AlreadyUpdated::BeforeItsTurn));
    assert_eq!(upgrades, 1, "the confirmed command ran, as previewed");
}

#[tokio::test]
async fn test_a_uv_tool_whose_new_version_added_a_command_was_already_updated() {
    // The new version brought a command the old one did not have, as
    // huggingface_hub 0.34 brought `hf`: uv's receipt lists the commands
    // it installed for the version it installed, so it moved with the
    // version. Where they come from (ruff itself) did not.
    let (outcome, how, upgrades) = ruff_updated_in_terminal(|dir, env| {
        std::fs::write(
            env.join("uv-receipt.toml"),
            ruff_receipt(dir, &[("ruff", "ruff"), ("ruff-lsp", "ruff")]),
        )
        .unwrap()
    })
    .await;
    assert_eq!(outcome, Outcome::Succeeded, "{how:?}");
    assert_eq!(how, Some(AlreadyUpdated::BeforeItsTurn));
    assert_eq!(upgrades, 1, "the confirmed command ran, as previewed");
}

#[tokio::test]
async fn test_a_uv_tool_now_installing_another_packages_commands_is_still_changed_since_shown() {
    // The receipt now has uv install `black`'s commands with ruff's (a
    // `uv tool install --with-executables-from black ruff`): the upgrade
    // would install them again, which the preview did not say.
    let (outcome, _, upgrades) = ruff_updated_in_terminal(|dir, env| {
        std::fs::write(
            env.join("uv-receipt.toml"),
            ruff_receipt(dir, &[("black", "black"), ("ruff", "ruff")]),
        )
        .unwrap()
    })
    .await;
    assert_eq!(outcome, Outcome::BanagerFailed(Fault::ChangedSinceShown));
    assert_eq!(upgrades, 0, "nothing written");
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
    let ran = confirm_then_run(
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
    (ran.outcome, ran.how, runner.calls().len())
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

// --- npm: the prefix read before the command (R20-2) -----------------------

/// npm at `dir/bin/npm`, its global prefix `dir`.
fn npm_instance(dir: &Path) -> ManagerInstance {
    ManagerInstance {
        exe_path: dir.join("bin/npm"),
        prefix: dir.to_owned(),
        version: Some("12.0.2".to_string()),
        ..banager_core::testing::manager_instance("npm", &format!("npm:{}", dir.display()))
    }
}

/// typescript's update toward 5.9.3, confirmed while npm answered, run
/// with `runner` answering as the Mac does when its turn comes.
async fn npm_update(dir: &Path, runner: &Arc<ScriptedRunner>) -> Ran {
    let inst = npm_instance(dir);
    confirm_then_run(
        Arc::new(NpmAdapter::new(runner.clone())),
        &inst,
        request(&inst, OpKind::Upgrade, ArtifactKind::Package, "typescript"),
        Some("5.9.3"),
        |_| {},
        || {},
    )
    .await
}

/// `lines` as lines npm or uv wrote to stderr, in the operation's log.
fn stderr_lines(lines: &[&str]) -> Vec<(Stream, String)> {
    lines
        .iter()
        .map(|line| (Stream::Stderr, line.to_string()))
        .collect()
}

#[tokio::test]
async fn test_npm_with_no_node_at_its_turn_says_what_is_missing() {
    // Update all: Homebrew's `node` first, which could not be linked
    // again, then npm's packages under the same prefix. Every npm command
    // now fails as its launcher does.
    let dir = tempfile::tempdir().unwrap();
    let runner = Arc::new(ScriptedRunner::default());
    runner.otherwise(Answer::Output(no_node()));
    let ran = npm_update(dir.path(), &runner).await;
    assert_eq!(
        ran.outcome,
        Outcome::Failed {
            exit_code: Some(127),
            summary: "env: node: No such file or directory".to_string(),
            cause: Some(FailureCause::NotFound),
        }
    );
    // The line is in the operation's log too, as npm's command would
    // have put it there: the log is not "no longer available", it has
    // what npm said.
    assert_eq!(
        ran.log,
        stderr_lines(&["env: node: No such file or directory"])
    );
    assert!(!wrote(&runner.calls()), "{:?}", runner.calls());
}

#[tokio::test]
async fn test_npm_gone_at_its_turn_is_a_missing_program() {
    let dir = tempfile::tempdir().unwrap();
    let runner = Arc::new(ScriptedRunner::default());
    runner.otherwise(Answer::NotFound);
    let ran = npm_update(dir.path(), &runner).await;
    assert_eq!(
        ran.outcome,
        Outcome::BanagerFailed(Fault::ProgramMissing {
            program: dir.path().join("bin/npm").display().to_string(),
        })
    );
    assert!(!wrote(&runner.calls()), "{:?}", runner.calls());
}

#[tokio::test]
async fn test_npm_that_does_not_answer_its_prefix_in_time_did_not_update() {
    let dir = tempfile::tempdir().unwrap();
    let runner = Arc::new(ScriptedRunner::default());
    runner.otherwise(Answer::Output(timed_out()));
    let ran = npm_update(dir.path(), &runner).await;
    // Banager stopped waiting for it: that it took too long, which is
    // true, rather than that npm gave no reason over an empty log.
    assert_eq!(
        ran.outcome,
        Outcome::Failed {
            exit_code: None,
            summary: String::new(),
            cause: Some(FailureCause::TimedOut),
        }
    );
    assert_eq!(ran.log, []);
    assert!(!wrote(&runner.calls()), "{:?}", runner.calls());
}

#[tokio::test]
async fn test_npm_answering_another_prefix_is_still_changed_since_shown() {
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let npm = dir.path().join("bin/npm");
    let runner = Arc::new(ScriptedRunner::default());
    runner.script(
        &[npm.to_str().unwrap(), "prefix", "-g"],
        vec![exited(0, &format!("{}\n", other.path().display()), "")],
    );
    runner.otherwise(Answer::Output(exited(1, "", "")));
    let ran = npm_update(dir.path(), &runner).await;
    assert_eq!(
        ran.outcome,
        Outcome::BanagerFailed(Fault::ChangedSinceShown)
    );
    // A read that answered is compared, not logged.
    assert_eq!(ran.log, []);
    assert!(!wrote(&runner.calls()), "{:?}", runner.calls());
}

// --- uv: the list read before the command (R20-2) -------------------------

/// ruff's update toward 0.16.8, confirmed while uv listed it at 0.15.0.
/// When its turn comes, `uv tool list` gives what `at_its_turn` makes of
/// ruff's environment (to the reading before the command and the recheck
/// alike), after `between` ran on it: how it ended, and whether anything
/// was written.
async fn uv_update(
    at_its_turn: impl FnOnce(&Path) -> Answer,
    between: impl FnOnce(&Path),
) -> (Ran, bool) {
    let dir = tempfile::tempdir().unwrap();
    let (inst, env) = uv_setup(dir.path());
    let uv = inst.exe_path.to_str().unwrap().to_string();
    let runner = Arc::new(ScriptedRunner::default());
    runner.script_answers(
        &[uv.as_str(), "tool", "list", "--show-paths"],
        vec![
            Answer::Output(exited(0, &uv_list(&env, "0.15.0"), "")),
            at_its_turn(&env),
        ],
    );
    runner.script(
        &[uv.as_str(), "tool", "upgrade", "ruff"],
        vec![exited(0, "", "Updated ruff v0.15.0 -> v0.16.8\n")],
    );
    let ran = confirm_then_run(
        Arc::new(UvAdapter::new(runner.clone())),
        &inst,
        request(&inst, OpKind::Upgrade, ArtifactKind::Tool, "ruff"),
        Some("0.16.8"),
        |_| {},
        || between(&env),
    )
    .await;
    if let Outcome::BanagerFailed(Fault::ProgramMissing { program }) = &ran.outcome {
        assert_eq!(program, &uv, "the plan's own uv");
    }
    (ran, wrote(&runner.calls()))
}

#[tokio::test]
async fn test_uv_gone_at_its_turn_is_a_missing_program() {
    // `brew upgrade uv` beside it in Update all, between its unlink and
    // its link: uv's tool updates hold only uv's own lock.
    let (ran, wrote) = uv_update(|_| Answer::NotFound, |_| {}).await;
    assert!(
        matches!(
            ran.outcome,
            Outcome::BanagerFailed(Fault::ProgramMissing { .. })
        ),
        "{:?}",
        ran.outcome
    );
    assert!(!wrote);
}

#[tokio::test]
async fn test_uv_list_failing_at_its_turn_says_why() {
    let (ran, wrote) = uv_update(
        |_| {
            Answer::Output(exited(
                2,
                "",
                "error: No such file or directory (os error 2)\n",
            ))
        },
        |_| {},
    )
    .await;
    assert_eq!(
        ran.outcome,
        Outcome::Failed {
            exit_code: Some(2),
            summary: "error: No such file or directory (os error 2)".to_string(),
            cause: Some(FailureCause::NotFound),
        }
    );
    assert_eq!(
        ran.log,
        stderr_lines(&["error: No such file or directory (os error 2)"])
    );
    assert!(!wrote);
}

#[tokio::test]
async fn test_uv_list_not_answering_in_time_at_its_turn_did_not_update() {
    let (ran, wrote) = uv_update(|_| Answer::Output(timed_out()), |_| {}).await;
    assert_eq!(
        ran.outcome,
        Outcome::Failed {
            exit_code: None,
            summary: String::new(),
            cause: Some(FailureCause::TimedOut),
        }
    );
    assert_eq!(ran.log, []);
    assert!(!wrote);
}

#[tokio::test]
async fn test_uv_receipt_changed_after_the_confirmation_is_still_changed_since_shown() {
    // The list answers as it did; the receipt now pins ruff.
    let (ran, wrote) = uv_update(
        |env| Answer::Output(exited(0, &uv_list(env, "0.15.0"), "")),
        |env| {
            std::fs::write(
                env.join("uv-receipt.toml"),
                "[tool]\nrequirements = [{ name = \"ruff\", specifier = \"==0.15.0\" }]\n",
            )
            .unwrap()
        },
    )
    .await;
    assert_eq!(
        ran.outcome,
        Outcome::BanagerFailed(Fault::ChangedSinceShown)
    );
    assert_eq!(ran.log, []);
    assert!(!wrote);
}
