//! Promise 3 of `docs/what-we-run.md`: no command runs without a plan the
//! user confirmed, except the read-only commands a refresh runs, which
//! each source's section lists. Here a whole refresh runs through
//! `Session` over the real adapters -- npm, pipx, uv, Cargo, pip, Ollama
//! and the six tools with their own installer -- in a temp home, against
//! a runner that answers from the recorded fixtures and records every
//! command. Each command it was asked to run must be one its source's
//! section shows (a `{name}` there stands for one argument), and none may
//! be one of the section's write commands. And the refresh, and the
//! Other Programs scan after it, leave the home folder they read exactly
//! as it was (promise 4).
//!
//! Homebrew is left out here only because it is found at fixed paths on
//! this Mac's own disk (`/opt/homebrew`), which a test out here cannot
//! point elsewhere. The same check for it is a unit test in
//! `adapters/brew/mod.rs`
//! (`test_a_refresh_runs_only_the_read_only_commands_homebrews_section_shows`),
//! with `brew update`, which has a table of its own, checked apart.

use async_trait::async_trait;
use banager_core::adapters::cargo::CargoAdapter;
use banager_core::adapters::npm::NpmAdapter;
use banager_core::adapters::ollama::OllamaAdapter;
use banager_core::adapters::pip::PipAdapter;
use banager_core::adapters::pipx::PipxAdapter;
use banager_core::adapters::standalone::recipes::RECIPES;
use banager_core::adapters::standalone::StandaloneAdapter;
use banager_core::adapters::uv::UvAdapter;
use banager_core::adapters::{Adapter, CheckOptions};
use banager_core::events::VecSink;
use banager_core::http::MockHttpClient;
use banager_core::runner::{
    CommandOutput, CommandRunner, CommandSpec, HostEnv, LineCallback, RunnerError,
};
use banager_core::session::Session;
use banager_core::trash::MockTrasher;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A recorded fixture's text.
fn fixture(rel: &str) -> String {
    std::fs::read_to_string(root().join("adapters/fixtures").join(rel))
        .unwrap_or_else(|e| panic!("fixture {rel}: {e}"))
}

/// Answers each command from the recordings, by its program's file name
/// and its arguments; anything else exits 1 with nothing printed. Every
/// command asked of it is recorded, answered or not.
struct Recorder {
    answers: Vec<(String, String, i32, String)>,
    calls: Mutex<Vec<(PathBuf, Vec<String>)>>,
}

impl Recorder {
    fn answer(&mut self, program: &str, args: &str, exit: i32, stdout: String) {
        self.answers
            .push((program.to_string(), args.to_string(), exit, stdout));
    }
}

#[async_trait]
impl CommandRunner for Recorder {
    async fn run(
        &self,
        spec: CommandSpec,
        _on_line: Option<LineCallback>,
        _cancel: CancellationToken,
    ) -> Result<CommandOutput, RunnerError> {
        self.calls
            .lock()
            .unwrap()
            .push((spec.program.clone(), spec.args.clone()));
        let name = spec
            .program
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let args = spec.args.join(" ");
        let found = self
            .answers
            .iter()
            .find(|(program, wanted, _, _)| *program == name && *wanted == args);
        Ok(match found {
            Some((_, _, exit, stdout)) => CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(*exit),
                stdout: stdout.clone(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
            None => CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(1),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        })
    }
}

struct Home(PathBuf);

impl Home {
    fn new() -> Home {
        let raw = std::env::temp_dir().join(format!(
            "banager-x3-refresh-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&raw).unwrap();
        Home(std::fs::canonicalize(&raw).unwrap())
    }

    /// An executable at `rel`; never run.
    fn exe(&self, rel: &str) -> PathBuf {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn link(&self, rel: &str, target: &Path) {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        symlink(target, path).unwrap();
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Every entry under `dir`, links not followed: its path, kind, length
/// and modification time.
fn tree(dir: &Path) -> Vec<(PathBuf, String, u64, i64, i64)> {
    let mut all = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&next)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            let meta = std::fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                stack.push(path.clone());
            }
            all.push((
                path,
                format!("{:?}", meta.file_type()),
                meta.len(),
                meta.mtime(),
                meta.mtime_nsec(),
            ));
        }
    }
    all
}

/// Each source's program, by file name: the name `docs/what-we-run.md`
/// writes it as, and the heading of its section.
const SOURCES: [(&str, &str, &str); 10] = [
    ("npm", "<npm>", "npm"),
    ("pipx", "<pipx>", "pipx"),
    ("uv", "<uv>", "uv"),
    ("cargo", "<cargo>", "Cargo"),
    ("python3", "<python>", "pip (read-only)"),
    ("ollama", "<ollama>", "Ollama"),
    ("claude", "<claude>", "Claude Code"),
    ("agy", "<agy>", "Antigravity CLI"),
    ("grok", "<grok>", "Grok Build"),
    ("rustup", "<rustup>", "rustup"),
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

/// Every command written in `section` that starts with `program` (the
/// read ones and the write ones apart): the text between backticks.
fn commands_in(section: &str, program: &str) -> (Vec<String>, Vec<String>) {
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    let mut in_write_table = false;
    for line in section.lines() {
        if !line.starts_with('|') {
            in_write_table = false;
        } else if line.contains("Needs a password") {
            in_write_table = true;
        }
        for (index, span) in line.split('`').enumerate() {
            if index % 2 == 1 && (span.starts_with(program) || span.starts_with("/usr/bin/")) {
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

/// Whether `argv` is `written`, a `{...}` there standing for any one
/// argument.
fn is(argv: &[String], written: &str) -> bool {
    let words: Vec<&str> = written.split_whitespace().collect();
    words.len() == argv.len()
        && words
            .iter()
            .zip(argv)
            .all(|(word, arg)| word == arg || (word.starts_with('{') && word.ends_with('}')))
}

#[tokio::test]
async fn test_a_refresh_runs_only_the_read_only_commands_each_sources_section_shows() {
    let home = Home::new();
    let bin = home.0.join("bin");
    for program in ["npm", "pipx", "uv", "cargo", "python3", "ollama"] {
        home.exe(&format!("bin/{program}"));
    }
    std::fs::create_dir_all(home.0.join("npm-prefix/lib/node_modules")).unwrap();
    std::fs::create_dir_all(home.0.join(".cargo/bin")).unwrap();
    std::fs::write(
        home.0.join(".cargo/.crates2.json"),
        fixture("cargo/1.98.1/crates2.json"),
    )
    .unwrap();
    // Claude Code's native install, Antigravity CLI, Grok Build and rustup.
    let claude = home.exe(".local/share/claude/versions/2.1.282");
    home.link(".local/bin/claude", &claude);
    home.exe(".local/bin/agy");
    home.exe(".grok/downloads/grok-1.0.41-macos-aarch64");
    home.link(
        ".grok/bin/grok",
        Path::new("../downloads/grok-1.0.41-macos-aarch64"),
    );
    home.exe(".cargo/bin/rustup");
    std::fs::create_dir_all(home.0.join(".rustup/toolchains")).unwrap();

    let mut recorder = Recorder {
        answers: Vec::new(),
        calls: Mutex::new(Vec::new()),
    };
    // The recordings name the author's Mac's folders (`/Users/brulek/...`,
    // `/opt/homebrew/...`), which a refresh looks at: the same paths under
    // a folder beside the home that is never made, so that what this Mac
    // has there changes nothing, and the home stays as it was.
    let recorded_root = home.0.with_extension("recorded-root");
    let away = |text: String| {
        let root = recorded_root.display();
        text.replace("/Users/brulek/", &format!("{root}/Users/brulek/"))
            .replace("/opt/homebrew/", &format!("{root}/opt/homebrew/"))
    };
    let prefix = home.0.join("npm-prefix").to_string_lossy().into_owned();
    recorder.answer("npm", "--version", 0, fixture("npm/12.0.2/version.txt"));
    recorder.answer("npm", "prefix -g", 0, format!("{prefix}\n"));
    recorder.answer(
        "npm",
        &format!("ls -g --depth=0 --json --prefix {prefix}"),
        0,
        fixture("npm/12.0.2/ls-global.json"),
    );
    recorder.answer(
        "npm",
        &format!("outdated -g --json --prefix {prefix}"),
        1,
        away(fixture("npm/12.0.2/outdated-global.json")),
    );
    recorder.answer("pipx", "--version", 0, fixture("pipx/1.17.3/version.txt"));
    recorder.answer(
        "pipx",
        "list --json",
        0,
        away(fixture("pipx/1.17.3/list.json")),
    );
    recorder.answer(
        "pipx",
        "list --outdated",
        0,
        fixture("pipx/1.17.3/list-outdated.txt"),
    );
    recorder.answer("uv", "--version", 0, fixture("uv/0.12.17/version.txt"));
    recorder.answer(
        "uv",
        "tool list --show-paths",
        0,
        away(fixture("uv/0.12.17/tool-list-show-paths.txt")),
    );
    recorder.answer(
        "uv",
        "tool list --outdated",
        0,
        fixture("uv/0.12.17/tool-list-outdated.txt"),
    );
    recorder.answer("cargo", "--version", 0, fixture("cargo/1.98.1/version.txt"));
    recorder.answer(
        "python3",
        "-m pip --version",
        0,
        away(fixture("pip/26.2.1/version.txt")),
    );
    recorder.answer(
        "python3",
        "-m pip list --format=json",
        0,
        fixture("pip/26.2.1/list.json"),
    );
    recorder.answer(
        "python3",
        "-m pip list --format=json --not-required",
        0,
        fixture("pip/26.2.1/list-not-required.json"),
    );
    recorder.answer(
        "python3",
        "-m pip list --outdated --format=json",
        0,
        fixture("pip/26.2.1/list-outdated.json"),
    );
    recorder.answer(
        "ollama",
        "--version",
        0,
        fixture("ollama/0.34.1/version.txt"),
    );
    recorder.answer(
        "claude",
        "--version",
        0,
        fixture("standalone-claude/2.1.282/version.txt"),
    );
    recorder.answer(
        "agy",
        "--version",
        0,
        fixture("standalone-agy/1.2.11/version.txt"),
    );
    recorder.answer(
        "grok",
        "--version",
        0,
        fixture("standalone-grok/1.0.41/version.txt"),
    );
    recorder.answer(
        "grok",
        "update --check --json",
        0,
        fixture("standalone-grok/1.0.41/update-check.json"),
    );
    recorder.answer(
        "rustup",
        "--version",
        0,
        fixture("standalone-rustup/1.29.1/version.txt"),
    );
    recorder.answer(
        "npm",
        "search --json --searchlimit 20 jq",
        0,
        fixture("npm/12.0.2/search-jq.json"),
    );
    let runner = Arc::new(recorder);
    let http = Arc::new(MockHttpClient::new());
    let mut adapters: Vec<Arc<dyn Adapter>> = vec![
        Arc::new(NpmAdapter::new(runner.clone()).looking_at_no_homebrew_prefix()),
        Arc::new(PipxAdapter::new(runner.clone(), http.clone())),
        // Not the `UV_TOOL_DIR` of the Mac running the test.
        Arc::new(UvAdapter::new(runner.clone()).with_tool_dir_fn(|| None)),
        Arc::new(CargoAdapter::new(runner.clone(), http.clone())),
        Arc::new(PipAdapter::new(runner.clone())),
        // Its daemon does not answer, and the app that would start it is
        // looked for nowhere: not in this Mac's `/Applications`.
        Arc::new(OllamaAdapter::new(runner.clone(), http.clone()).with_app_present_fn(|_| false)),
    ];
    for recipe in RECIPES {
        adapters.push(Arc::new(
            StandaloneAdapter::new(
                recipe,
                runner.clone(),
                http.clone(),
                Arc::new(MockTrasher::new()),
            )
            .with_machine_root(&recorded_root),
        ));
    }
    let session = Session::with_adapters(Arc::new(VecSink::new()), adapters.clone(), None);
    let env = HostEnv {
        path_dirs: vec![bin.clone(), home.0.join(".local/bin")],
        home: home.0.clone(),
        euid: std::fs::metadata(&home.0).unwrap().uid(),
        cargo_home: Some(home.0.join(".cargo")),
        rustup_home: Some(home.0.join(".rustup")),
        zdotdir: None,
        ollama_host: None,
    };
    // `/usr/local/bin`, the one folder the scan reads outside the home
    // folder, stood in for by one that is never made: never this Mac's.
    session.set_unknown_scan_system_bin(&recorded_root.join("usr/local/bin"));
    let before = tree(&home.0);
    let snapshot = session.refresh(&env, &CheckOptions::default()).await;
    // Promise 4 too: the refresh, and the Other Programs scan after it,
    // wrote, made, moved and deleted nothing in the home they read.
    session.scan_unknown(&env);
    assert_eq!(tree(&home.0), before, "the home folder changed");

    // Search, the one other call that runs without a plan (nothing in the
    // window asks for it yet): over every source found, its commands are
    // held to the same tables.
    for adapter in &adapters {
        for inst in snapshot
            .instances
            .iter()
            .filter(|inst| inst.adapter_id == adapter.meta().id)
        {
            let _ = adapter.search(inst, "jq").await;
        }
    }

    let doc = std::fs::read_to_string(root().join("docs/what-we-run.md")).unwrap();
    let calls = runner.calls.lock().unwrap().clone();
    let mut ran: Vec<&str> = Vec::new();
    for (program, args) in &calls {
        let name = program.file_name().unwrap().to_string_lossy().into_owned();
        let (_, written, heading) = SOURCES
            .iter()
            .find(|(file, _, _)| *file == name)
            .unwrap_or_else(|| panic!("a command of no known source: {program:?} {args:?}"));
        let (reads, writes) = commands_in(&section(&doc, heading), written);
        let mut argv = vec![written.to_string()];
        argv.extend(args.iter().cloned());
        assert!(
            writes.iter().all(|write| !is(&argv, write)),
            "a refresh or search ran {argv:?}, a write command of {heading}"
        );
        assert!(
            reads.iter().any(|read| is(&argv, read)),
            "a refresh or search ran {argv:?}, which ## {heading} does not show as read-only: \
             {reads:?}"
        );
        ran.push(heading);
    }
    // Not a test that ran nothing: each source answered, and was read.
    for heading in [
        "npm",
        "pipx",
        "uv",
        "Cargo",
        "pip (read-only)",
        "Ollama",
        "Claude Code",
        "Antigravity CLI",
        "Grok Build",
        "rustup",
    ] {
        assert!(
            ran.contains(&heading),
            "no command of {heading} ran: {calls:?}"
        );
    }
    for listed in ["npm", "pipx", "uv", "pip", "cargo"] {
        let ids: Vec<&str> = snapshot
            .instances
            .iter()
            .filter(|i| i.adapter_id == listed)
            .map(|i| i.id.as_str())
            .collect();
        assert!(
            snapshot
                .artifacts
                .iter()
                .any(|a| ids.contains(&a.key.instance_id.as_str())),
            "{listed}'s inventory was read: {:?}",
            snapshot.errors
        );
    }
    assert!(
        calls
            .iter()
            .any(|(p, a)| p.ends_with("grok") && a.join(" ") == "update --check --json"),
        "Grok Build's own update check ran"
    );
    assert!(
        calls
            .iter()
            .any(|(p, a)| p.ends_with("npm") && a.first().map(String::as_str) == Some("search")),
        "npm's search ran"
    );
}
