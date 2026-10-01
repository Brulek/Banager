//! Promise 2 of `docs/what-we-run.md`: an uninstall runs only the command
//! its preview showed, or moves only the paths its preview listed; it
//! never deletes the data the preview says stays; and it never asks a
//! package manager to remove anything beyond the tool (no autoremove, no
//! cleanup, no `--zap`).
//!
//! Each command source's uninstall is planned and then carried out by its
//! own adapter against a runner that records every command: the one
//! command run is the plan's, argument for argument, and the plan holds
//! none of the words that would remove more. The path-list uninstalls
//! (Claude Code, Antigravity CLI, Grok Build) are checked against every
//! tool's kept data as a property of the recipes.

use banager_core::adapters::brew::BrewAdapter;
use banager_core::adapters::cargo::CargoAdapter;
use banager_core::adapters::npm::NpmAdapter;
use banager_core::adapters::ollama::OllamaAdapter;
use banager_core::adapters::pipx::PipxAdapter;
use banager_core::adapters::standalone::recipe::Uninstall;
use banager_core::adapters::standalone::recipes::RECIPES;
use banager_core::adapters::uv::UvAdapter;
use banager_core::adapters::Adapter;
use banager_core::events::VecSink;
use banager_core::families::families;
use banager_core::http::MockHttpClient;
use banager_core::model::{ArtifactKind, ManagerInstance, OpKind, OpRequest, PlanAction};
use banager_core::runner::{CommandOutput, MockRunner};
use banager_core::testing::manager_instance;
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

fn exited_0() -> CommandOutput {
    CommandOutput {
        exit_code: Some(0),
        stdout: String::new(),
        stderr: String::new(),
        timed_out: false,
        cancelled: false,
    }
}

/// Words that, in an uninstall's argv, would remove more than the tool.
const REMOVES_MORE: [&str; 8] = [
    "autoremove",
    "cleanup",
    "--zap",
    "--force",
    "--ignore-dependencies",
    "--purge",
    "prune",
    "--all",
];

/// A prefix that is not there: the preview looks at Homebrew's update
/// lock under it, and this Mac's own must not decide the test.
const BREW_PREFIX: &str = "/nonexistent-banager-x3/homebrew";

struct Case {
    what: &'static str,
    adapter: Arc<dyn Adapter>,
    runner: Arc<MockRunner>,
    instance: ManagerInstance,
    kind: ArtifactKind,
    name: &'static str,
}

fn instance(adapter_id: &str, exe: &str, prefix: &str) -> ManagerInstance {
    ManagerInstance {
        exe_path: PathBuf::from(exe),
        prefix: PathBuf::from(prefix),
        ..manager_instance(adapter_id, &format!("{adapter_id}:{prefix}"))
    }
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut add = |what, adapter: fn(Arc<MockRunner>) -> Arc<dyn Adapter>, inst, kind, name| {
        let runner = Arc::new(MockRunner::new());
        cases.push(Case {
            what,
            adapter: adapter(runner.clone()),
            runner,
            instance: inst,
            kind,
            name,
        });
    };
    add(
        "npm",
        |r| Arc::new(NpmAdapter::new(r)),
        instance("npm", "/opt/homebrew/bin/npm", "/opt/homebrew"),
        ArtifactKind::Package,
        "prettier",
    );
    add(
        "pipx",
        |r| Arc::new(PipxAdapter::new(r, Arc::new(MockHttpClient::new()))),
        instance("pipx", "/opt/homebrew/bin/pipx", "/opt/homebrew"),
        ArtifactKind::Tool,
        "black",
    );
    add(
        "uv",
        |r| Arc::new(UvAdapter::new(r)),
        instance("uv", "/opt/homebrew/bin/uv", "/opt/homebrew"),
        ArtifactKind::Tool,
        "ruff",
    );
    add(
        "cargo",
        |r| Arc::new(CargoAdapter::new(r, Arc::new(MockHttpClient::new()))),
        instance("cargo", "/Users/you/.cargo/bin/cargo", "/Users/you/.cargo"),
        ArtifactKind::Binary,
        "ripgrep",
    );
    add(
        "ollama",
        |r| Arc::new(OllamaAdapter::new(r, Arc::new(MockHttpClient::new()))),
        instance(
            "ollama",
            "/opt/homebrew/bin/ollama",
            "http://127.0.0.1:11434",
        ),
        ArtifactKind::Model,
        "llama3.2:3b",
    );
    add(
        "Homebrew formula",
        |r| Arc::new(BrewAdapter::new(r)),
        instance("brew", "/opt/homebrew/bin/brew", BREW_PREFIX),
        ArtifactKind::Formula,
        "jq",
    );
    add(
        "Homebrew cask",
        |r| Arc::new(BrewAdapter::new(r)),
        instance("brew", "/opt/homebrew/bin/brew", BREW_PREFIX),
        ArtifactKind::Cask,
        "docker",
    );
    cases
}

#[tokio::test]
async fn test_an_uninstall_runs_exactly_the_command_its_preview_showed_and_nothing_more() {
    for case in cases() {
        let Case {
            what,
            adapter,
            runner,
            instance,
            kind,
            name,
        } = case;
        // Homebrew's preview asks which installed formulae need this one.
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", name],
            exited_0(),
        );
        let request = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: instance.id.clone(),
            artifact_kind: kind,
            name: name.to_string(),
        };
        let plan = adapter
            .plan(&instance, &request)
            .await
            .unwrap_or_else(|e| panic!("{what}: plan: {e}"));
        let PlanAction::Command { program, args, env } = &plan.action else {
            panic!("{what}: an uninstall that runs a command");
        };
        let mut argv = vec![program.to_string_lossy().into_owned()];
        argv.extend(args.iter().cloned());
        for word in REMOVES_MORE {
            assert!(
                !argv.iter().any(|arg| arg == word),
                "{what}: {argv:?} carries {word}"
            );
        }
        assert_eq!(
            args.last().map(String::as_str),
            Some(name),
            "{what}: the tool's own name ends the command"
        );
        if program.ends_with("brew") {
            for switch in ["HOMEBREW_NO_AUTOREMOVE", "HOMEBREW_NO_INSTALL_CLEANUP"] {
                assert!(
                    env.iter().any(|(k, v)| k == switch && v == "1"),
                    "{what}: {switch}=1 is set, got {env:?}"
                );
            }
        }

        let planned = runner.calls().len();
        runner.respond(argv.iter().map(String::as_str).collect(), exited_0());
        adapter
            .execute(&plan, Arc::new(VecSink::new()), 1, CancellationToken::new())
            .await
            .unwrap_or_else(|e| panic!("{what}: execute: {e}"));
        assert_eq!(
            runner.calls()[planned..].to_vec(),
            vec![argv.clone()],
            "{what}: the one command run is the one previewed"
        );
    }
}

#[test]
fn test_no_path_list_uninstall_moves_a_path_any_tool_keeps_or_a_folder_holding_one() {
    // `kept_data`'s table (`families`) is what the preview says stays,
    // for every tool; the path-list uninstalls move `remove`. A removed
    // path that is a kept one, or a folder holding one, would move to the
    // Trash data the preview promised to leave.
    let kept: Vec<(String, String)> = families()
        .iter()
        .flat_map(|family| {
            family
                .data_paths
                .iter()
                .map(move |path| (family.id.clone(), path.trim_end_matches('/').to_string()))
        })
        .collect();
    assert!(
        kept.iter().any(|(_, path)| path == "~/.claude"),
        "the table is read: {kept:?}"
    );
    let mut checked = 0;
    for recipe in RECIPES {
        let Some(Uninstall::Paths { remove, keep }) = &recipe.uninstall else {
            continue;
        };
        for spec in remove.iter() {
            let moved = spec.path.trim_end_matches('/');
            for (family, data) in &kept {
                assert!(
                    data != moved && !data.starts_with(&format!("{moved}/")),
                    "{}: moving {moved} would move {data}, which {family} keeps",
                    recipe.id
                );
            }
            for stays in keep.iter() {
                let stays = stays.path.trim_end_matches('/');
                assert!(
                    stays != moved && !stays.starts_with(&format!("{moved}/")),
                    "{}: moving {moved} would move {stays}, which its own preview keeps",
                    recipe.id
                );
            }
            checked += 1;
        }
    }
    assert!(checked >= 3, "the three path-list uninstalls were read");
}
