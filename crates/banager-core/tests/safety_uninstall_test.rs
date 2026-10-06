//! Promise 2 of `docs/what-we-run.md`: an uninstall runs only the command
//! its preview showed, or moves only the paths its preview listed; it
//! never deletes the data the preview says stays; and it never asks a
//! package manager to remove anything beyond the tool (no autoremove, no
//! cleanup, no `--zap`).
//!
//! Each command source's uninstall is planned and then carried out by its
//! own adapter against a runner that records every command: the one
//! command run is the plan's, argument for argument, and the plan holds
//! none of the words that would remove more; the preview itself runs
//! only the read-only commands its source's section shows. The path-list uninstalls
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

fn cases(npm_prefix: &str) -> Vec<Case> {
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
        instance("npm", "/opt/homebrew/bin/npm", npm_prefix),
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

/// Each command source by adapter id: how `docs/what-we-run.md` writes its
/// program, and the heading of its section.
const SECTIONS: [(&str, &str, &str); 6] = [
    ("npm", "<npm>", "npm"),
    ("pipx", "<pipx>", "pipx"),
    ("uv", "<uv>", "uv"),
    ("cargo", "<cargo>", "Cargo"),
    ("ollama", "<ollama>", "Ollama"),
    ("brew", "<brew>", "Homebrew"),
];

/// `docs/what-we-run.md`'s section under `## {heading}`.
fn section(doc: &str, heading: &str) -> String {
    let start = doc
        .find(&format!("\n## {heading}\n"))
        .unwrap_or_else(|| panic!("a section ## {heading}"));
    let rest = &doc[start + 1..];
    let end = rest[3..].find("\n## ").map_or(rest.len(), |at| at + 3);
    rest[..end].to_string()
}

/// The commands `section` writes starting with `program`: those outside
/// its write tables (the tables with a "Needs a password" column), and
/// those in them.
fn commands_in(section: &str, program: &str) -> (Vec<String>, Vec<String>) {
    let (mut reads, mut writes) = (Vec::new(), Vec::new());
    let mut in_write_table = false;
    for line in section.lines() {
        if !line.starts_with('|') {
            in_write_table = false;
        } else if line.contains("Needs a password") {
            in_write_table = true;
        }
        for (index, span) in line.split('`').enumerate() {
            if index % 2 == 1 && span.starts_with(program) {
                if in_write_table {
                    writes.push(span.to_string());
                } else {
                    reads.push(span.to_string());
                }
            }
        }
    }
    (reads, writes)
}

/// Whether `argv` is `written`, a `{...}` there standing for one argument.
fn is(argv: &[String], written: &str) -> bool {
    let words: Vec<&str> = written.split_whitespace().collect();
    words.len() == argv.len()
        && words
            .iter()
            .zip(argv)
            .all(|(word, arg)| word == arg || (word.starts_with('{') && word.ends_with('}')))
}

#[tokio::test]
async fn test_an_uninstall_runs_exactly_the_command_its_preview_showed_and_nothing_more() {
    let doc = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/what-we-run.md"),
    )
    .unwrap();
    // npm checks prefix writability even with a MockRunner. Use a test-owned
    // root so this command-safety test does not depend on the Mac's Homebrew.
    let npm_root = tempfile::tempdir().unwrap();
    for case in cases(npm_root.path().to_str().unwrap()) {
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

        // The preview itself runs only read-only commands its source's
        // section shows (Homebrew's `brew uses --installed {name}`).
        let (placeholder, heading) = SECTIONS
            .iter()
            .find(|(adapter_id, _, _)| *adapter_id == instance.adapter_id)
            .map(|(_, placeholder, heading)| (*placeholder, *heading))
            .unwrap();
        let (reads, writes) = commands_in(&section(&doc, heading), placeholder);
        for call in runner.calls() {
            let mut shown = vec![placeholder.to_string()];
            shown.extend(call[1..].iter().cloned());
            assert!(
                reads.iter().any(|read| is(&shown, read))
                    && writes.iter().all(|write| !is(&shown, write)),
                "{what}: the preview ran {shown:?}, not a read-only command of ## {heading}"
            );
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

#[tokio::test]
async fn test_a_homebrew_formula_with_two_versions_is_uninstalled_whole_and_only_it() {
    // U9 (r6): with more than one version of the formula installed, the
    // uninstall passes `--force`, Homebrew's own way to delete every one of
    // them -- the one word of `REMOVES_MORE` it may carry, which removes
    // more of the tool and nothing beside it: the rest of the argv is the
    // verb, the kind flag and the name, as its row in Homebrew's write
    // table shows it, and nothing else runs.
    let prefix = std::env::temp_dir().join(format!(
        "banager-safety-two-versions-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&prefix);
    for version in ["1.24.0", "1.25.0"] {
        std::fs::create_dir_all(prefix.join("Cellar/wget").join(version)).unwrap();
    }
    let runner = Arc::new(MockRunner::new());
    let adapter = BrewAdapter::new(runner.clone());
    let inst = instance("brew", "/opt/homebrew/bin/brew", prefix.to_str().unwrap());
    runner.respond(
        vec!["/opt/homebrew/bin/brew", "uses", "--installed", "wget"],
        exited_0(),
    );
    let request = OpRequest {
        kind: OpKind::Uninstall,
        instance_id: inst.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: "wget".to_string(),
    };
    let plan = adapter.plan(&inst, &request).await.expect("plan");
    let PlanAction::Command { program, args, .. } = &plan.action else {
        panic!("an uninstall that runs a command");
    };
    assert_eq!(args, &["uninstall", "--formula", "--force", "wget"]);
    let mut argv = vec![program.to_string_lossy().into_owned()];
    argv.extend(args.iter().cloned());
    let doc = std::fs::read_to_string("../../docs/what-we-run.md").expect("read the document");
    let (_, writes) = commands_in(&section(&doc, "Homebrew"), "<brew>");
    let mut shown = vec!["<brew>".to_string()];
    shown.extend(args.iter().cloned());
    assert!(
        writes.iter().any(|write| is(&shown, write)),
        "{shown:?} is no row of Homebrew's write table: {writes:?}"
    );
    let planned = runner.calls().len();
    runner.respond(argv.iter().map(String::as_str).collect(), exited_0());
    adapter
        .execute(&plan, Arc::new(VecSink::new()), 1, CancellationToken::new())
        .await
        .expect("execute");
    assert_eq!(runner.calls()[planned..].to_vec(), vec![argv]);
    std::fs::remove_dir_all(&prefix).unwrap();
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
