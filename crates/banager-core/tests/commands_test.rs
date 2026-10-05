//! Which copy of a command runs (`commands`), over directory trees each
//! test builds in a temp directory: a Homebrew prefix, an npm prefix, a
//! home with `~/.local/bin`, a Cargo home -- real links and real files,
//! since the judgement answers from `read_dir`, `lstat` and `readlink` and
//! nothing else. No recorded fixture is read but uv's (whose `ruff` line
//! is parsed), and none is written.

use async_trait::async_trait;
use banager_core::adapters::brew::parse::parse_info_installed;
use banager_core::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
use banager_core::commands::{bin_folders, judge, read_folders, CommandBudget, Folders};
use banager_core::diagnostics::PathFolders;
use banager_core::events::{EventSink, OpId, VecSink};
use banager_core::model::{
    ArtifactKey, ArtifactKind, CommandFact, CommandInputs, CommandState, InstallReason,
    InstalledArtifact, ManagerInstance, OpRequest, Outcome, Plan, ProvidedCommand, Reconciled,
    SearchHit,
};
use banager_core::runner::HostEnv;
use banager_core::session::Session;
use banager_core::testing::manager_instance;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// A fresh home for one test, removed when it ends. Canonical, so paths
/// built from it compare equal to what `realpath` answers (`/var` is a
/// link to `/private/var` on a Mac).
struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Home {
        let raw = std::env::temp_dir().join(format!(
            "banager-commands-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&raw).expect("create temp home");
        Home(fs::canonicalize(&raw).expect("canonical temp home"))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn at(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }

    fn dir(&self, rel: &str) -> PathBuf {
        let dir = self.0.join(rel);
        fs::create_dir_all(&dir).expect("create dir");
        dir
    }

    /// An executable regular file at `rel` (its folders created).
    fn exe(&self, rel: &str) -> PathBuf {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).expect("create parent");
        fs::write(&path, b"#!/bin/sh\n").expect("write file");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    /// A regular file at `rel` with no execute bit.
    fn plain(&self, rel: &str) -> PathBuf {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).expect("create parent");
        fs::write(&path, b"text").expect("write file");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");
        path
    }

    /// A symbolic link at `rel` whose text is exactly `target`.
    fn link(&self, rel: &str, target: &Path) -> PathBuf {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).expect("create parent");
        symlink(target, &path).expect("symlink");
        path
    }

    fn env(&self, path_dirs: Vec<PathBuf>) -> HostEnv {
        HostEnv {
            path_dirs,
            home: self.0.clone(),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        }
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn instance(adapter_id: &str, id: &str, prefix: &Path, exe_path: &Path) -> ManagerInstance {
    ManagerInstance {
        prefix: prefix.to_path_buf(),
        exe_path: exe_path.to_path_buf(),
        ..manager_instance(adapter_id, id)
    }
}

fn artifact(instance_id: &str, kind: ArtifactKind, name: &str) -> InstalledArtifact {
    banager_core::testing::installed_artifact(instance_id, kind, name)
}

fn with_family(mut artifact: InstalledArtifact, family: &str) -> InstalledArtifact {
    artifact.facts.family = Some(family.to_string());
    artifact
}

fn provided(name: &str, path: &Path, within: &[&Path]) -> ProvidedCommand {
    ProvidedCommand {
        name: name.to_string(),
        path: path.to_path_buf(),
        within: within.iter().map(|p| p.to_path_buf()).collect(),
    }
}

fn with_provided(
    mut artifact: InstalledArtifact,
    provided: Vec<ProvidedCommand>,
) -> InstalledArtifact {
    artifact.facts.command_inputs.provided = provided;
    artifact
}

/// Every artifact's verdicts, by `PATH`, read and judged with the
/// default budget and the login shell's `PATH` known.
fn verdicts(
    home: &Home,
    path: &[PathBuf],
    instances: &[ManagerInstance],
    artifacts: &[InstalledArtifact],
) -> Vec<Vec<CommandFact>> {
    let folders = read_folders(
        path,
        &bin_folders(instances),
        home.path(),
        CommandBudget::default(),
    );
    assert!(folders.complete());
    judge(
        &folders,
        instances,
        artifacts,
        home.path(),
        true,
        CommandBudget::default(),
    )
    .expect("judged within the budget")
}

fn runs(name: &str) -> CommandFact {
    CommandFact {
        name: name.to_string(),
        state: Some(CommandState::Runs),
    }
}

fn shadowed(name: &str, by: Option<&ArtifactKey>) -> CommandFact {
    CommandFact {
        name: name.to_string(),
        state: Some(CommandState::ShadowedBy { by: by.cloned() }),
    }
}

fn not_on_path(name: &str, dir: &str) -> CommandFact {
    CommandFact {
        name: name.to_string(),
        state: Some(CommandState::NotOnPath {
            dir: dir.to_string(),
        }),
    }
}

fn unjudged(name: &str) -> CommandFact {
    CommandFact {
        name: name.to_string(),
        state: None,
    }
}

/// npm's Claude Code under an npm prefix (`<prefix>/bin/claude` into
/// `lib/node_modules/@anthropic-ai/claude-code`) and the native install
/// (`~/.local/bin/claude` into `~/.local/share/claude/versions/…`), both
/// of the `claude-code` family.
struct TwoClaudes {
    instances: Vec<ManagerInstance>,
    artifacts: Vec<InstalledArtifact>,
    npm_bin: PathBuf,
    local_bin: PathBuf,
}

fn two_claudes(home: &Home) -> TwoClaudes {
    let npm = home.dir("npm");
    home.exe("npm/lib/node_modules/@anthropic-ai/claude-code/cli.js");
    home.link(
        "npm/bin/claude",
        Path::new("../lib/node_modules/@anthropic-ai/claude-code/cli.js"),
    );
    let real = home.exe(".local/share/claude/versions/2.1.281");
    let launcher = home.link(".local/bin/claude", &real);
    let npm_id = format!("npm:{}", npm.display());
    TwoClaudes {
        instances: vec![
            instance("npm", &npm_id, &npm, &npm.join("bin/npm")),
            instance(
                "standalone-claude",
                "standalone-claude",
                &home.at(".local/share/claude"),
                &launcher,
            ),
        ],
        artifacts: vec![
            with_family(
                artifact(&npm_id, ArtifactKind::Package, "@anthropic-ai/claude-code"),
                "claude-code",
            ),
            with_family(
                artifact("standalone-claude", ArtifactKind::Binary, "claude"),
                "claude-code",
            ),
        ],
        npm_bin: npm.join("bin"),
        local_bin: home.at(".local/bin"),
    }
}

#[test]
fn test_which_claude_runs_follows_the_order_of_path() {
    let home = Home::new("two-claudes");
    let setup = two_claudes(&home);
    let npm_key = setup.artifacts[0].key.clone();
    let native_key = setup.artifacts[1].key.clone();

    // npm's folder first: npm's copy runs, the native one waits behind it.
    let npm_first = verdicts(
        &home,
        &[setup.npm_bin.clone(), setup.local_bin.clone()],
        &setup.instances,
        &setup.artifacts,
    );
    assert_eq!(
        npm_first,
        vec![
            vec![runs("claude")],
            vec![shadowed("claude", Some(&npm_key))]
        ]
    );

    // The other order, the other answer.
    let native_first = verdicts(
        &home,
        &[setup.local_bin.clone(), setup.npm_bin.clone()],
        &setup.instances,
        &setup.artifacts,
    );
    assert_eq!(
        native_first,
        vec![
            vec![shadowed("claude", Some(&native_key))],
            vec![runs("claude")]
        ]
    );

    // `~/.local/bin` not on PATH: Terminal does not find the native copy,
    // and the folder it is in is named as the user knows it.
    let only_npm = verdicts(
        &home,
        std::slice::from_ref(&setup.npm_bin),
        &setup.instances,
        &setup.artifacts,
    );
    assert_eq!(
        only_npm,
        vec![
            vec![runs("claude")],
            vec![not_on_path("claude", "~/.local/bin")]
        ]
    );
}

#[test]
fn test_codexs_own_launcher_is_judged_like_the_other_standalone_launchers() {
    // Codex's install script: `~/.local/bin/codex` links (absolute text)
    // through `~/.codex/packages/standalone/current` into
    // `releases/<version>-<target>`; npm's `@openai/codex` sits in an npm
    // prefix. The launcher's `current` hop stays inside the package folder,
    // so the native copy claims `codex` as Claude Code's launcher claims
    // `claude`; its helper `codex-code-mode-host` is no command people
    // type and gets no verdict.
    let home = Home::new("two-codexes");
    let npm = home.dir("npm");
    home.exe("npm/lib/node_modules/@openai/codex/bin/codex.js");
    home.link(
        "npm/bin/codex",
        Path::new("../lib/node_modules/@openai/codex/bin/codex.js"),
    );
    let root = home.at(".codex/packages/standalone");
    let release = root.join("releases/0.159.3-aarch64-apple-darwin");
    home.exe(".codex/packages/standalone/releases/0.159.3-aarch64-apple-darwin/bin/codex");
    home.exe(
        ".codex/packages/standalone/releases/0.159.3-aarch64-apple-darwin/bin/codex-code-mode-host",
    );
    home.link(".codex/packages/standalone/current", &release);
    let launcher = home.link(".local/bin/codex", &root.join("current/bin/codex"));
    home.link(
        ".local/bin/codex-code-mode-host",
        &root.join("current/bin/codex-code-mode-host"),
    );
    let npm_id = format!("npm:{}", npm.display());
    let instances = vec![
        instance("npm", &npm_id, &npm, &npm.join("bin/npm")),
        instance("standalone-codex", "standalone-codex", &root, &launcher),
    ];
    let artifacts = vec![
        with_family(
            artifact(&npm_id, ArtifactKind::Package, "@openai/codex"),
            "codex",
        ),
        with_family(
            artifact("standalone-codex", ArtifactKind::Binary, "codex"),
            "codex",
        ),
    ];
    let npm_key = artifacts[0].key.clone();
    let native_key = artifacts[1].key.clone();

    let npm_first = verdicts(
        &home,
        &[npm.join("bin"), home.at(".local/bin")],
        &instances,
        &artifacts,
    );
    assert_eq!(
        npm_first,
        vec![vec![runs("codex")], vec![shadowed("codex", Some(&npm_key))]]
    );
    let native_first = verdicts(
        &home,
        &[home.at(".local/bin"), npm.join("bin")],
        &instances,
        &artifacts,
    );
    assert_eq!(
        native_first,
        vec![
            vec![shadowed("codex", Some(&native_key))],
            vec![runs("codex")]
        ]
    );
    let only_npm = verdicts(&home, &[npm.join("bin")], &instances, &artifacts);
    assert_eq!(only_npm[1], vec![not_on_path("codex", "~/.local/bin")]);
}

#[test]
fn test_a_grok_build_shaped_cask_provides_both_its_names_from_one_file() {
    // The cask as `brew info --installed --json=v2` lists it, its two
    // links where the stanzas' targets say, both to one staged file.
    let home = Home::new("grok-build");
    let brew = home.dir("brew");
    home.exe("brew/Caskroom/grok-build/1.0.46/grok");
    home.link(
        "brew/bin/grok",
        Path::new("../Caskroom/grok-build/1.0.46/grok"),
    );
    home.link(
        "brew/bin/agent",
        Path::new("../Caskroom/grok-build/1.0.46/grok"),
    );
    let json = format!(
        r#"{{"formulae": [], "casks": [{{
            "token": "grok-build", "full_token": "grok-build", "name": ["Grok Build"],
            "installed": "1.0.46",
            "artifacts": [
                {{ "binary": ["grok"], "target": "{bin}/grok" }},
                {{ "binary": ["grok", {{ "target": "agent" }}], "target": "{bin}/agent" }}
            ]
        }}]}}"#,
        bin = brew.join("bin").display()
    );
    let brew_id = format!("brew:{}", brew.display());
    let cask = with_family(
        parse_info_installed(&json, &brew_id)
            .expect("parse")
            .remove(0),
        "grok-build",
    );
    // Grok Build's own installer too: `~/.grok/bin/grok` and `agent`.
    let download = home.exe(".grok/downloads/grok-1.0.41-macos-aarch64");
    let launcher = home.link(".grok/bin/grok", &download);
    home.link(".grok/bin/agent", &download);
    let instances = vec![
        instance("brew", &brew_id, &brew, &brew.join("bin/brew")),
        instance(
            "standalone-grok",
            "standalone-grok",
            &home.at(".grok"),
            &launcher,
        ),
    ];
    let native = with_family(
        artifact("standalone-grok", ArtifactKind::Binary, "grok"),
        "grok-build",
    );
    let artifacts = vec![cask.clone(), native];

    let found = verdicts(
        &home,
        &[brew.join("bin"), home.at(".grok/bin")],
        &instances,
        &artifacts,
    );
    assert_eq!(found[0], vec![runs("agent"), runs("grok")]);
    assert_eq!(
        found[1],
        vec![
            shadowed("agent", Some(&cask.key)),
            shadowed("grok", Some(&cask.key))
        ]
    );
}

#[test]
fn test_a_broken_link_and_a_file_with_no_execute_bit_are_passed_over() {
    let home = Home::new("passed-over");
    let real = home.exe(".local/share/claude/versions/2.1.281");
    let launcher = home.link(".local/bin/claude", &real);
    // Earlier on PATH: a link to nothing, then a file that cannot run.
    home.link("broken/claude", &home.at("gone/claude"));
    home.plain("text/claude");
    let instances = vec![instance(
        "standalone-claude",
        "standalone-claude",
        &home.at(".local/share/claude"),
        &launcher,
    )];
    let artifacts = vec![artifact(
        "standalone-claude",
        ArtifactKind::Binary,
        "claude",
    )];
    let found = verdicts(
        &home,
        &[home.at("broken"), home.at("text"), home.at(".local/bin")],
        &instances,
        &artifacts,
    );
    assert_eq!(found, vec![vec![runs("claude")]]);

    // A launcher that cannot run is no claim at all: no verdict, no name.
    fs::remove_file(&launcher).unwrap();
    home.plain(".local/bin/claude");
    let found = verdicts(&home, &[home.at(".local/bin")], &instances, &artifacts);
    assert_eq!(found, vec![Vec::<CommandFact>::new()]);
}

#[test]
fn test_a_folder_named_twice_on_path_is_read_once() {
    let home = Home::new("twice");
    let bin = home.dir(".local/bin");
    let alias = home.link("alias-of-local-bin", &bin);
    let folders = read_folders(
        &[bin.clone(), bin.clone(), alias],
        std::slice::from_ref(&bin),
        home.path(),
        CommandBudget::default(),
    );
    assert!(folders.complete());
    assert_eq!(folders.path_folders(), vec![bin.as_path()]);
}

#[test]
fn test_empty_and_relative_path_entries_are_skipped() {
    // A shell would look a relative entry up from its own current folder,
    // which is not Banager's (an app opened from Finder has `/`). The
    // tests run in crates/banager-core, where `src` exists: it is still
    // skipped.
    let home = Home::new("relative");
    let bin = home.dir("bin");
    let folders = read_folders(
        &[
            PathBuf::new(),
            PathBuf::from("src"),
            PathBuf::from("."),
            bin.clone(),
        ],
        &[],
        home.path(),
        CommandBudget::default(),
    );
    assert_eq!(folders.path_folders(), vec![bin.as_path()]);
}

#[test]
fn test_no_verdicts_at_all_without_the_login_shells_path() {
    // `Session::note_login_path(false)`: the names stay, so two copies can
    // still be told apart, but nothing is said about which runs.
    let home = Home::new("unknown-path");
    let setup = two_claudes(&home);
    let folders = read_folders(
        &[setup.npm_bin.clone(), setup.local_bin.clone()],
        &bin_folders(&setup.instances),
        home.path(),
        CommandBudget::default(),
    );
    let found = judge(
        &folders,
        &setup.instances,
        &setup.artifacts,
        home.path(),
        false,
        CommandBudget::default(),
    )
    .expect("judged");
    assert_eq!(
        found,
        vec![vec![unjudged("claude")], vec![unjudged("claude")]]
    );
}

/// A Homebrew prefix with three formulae linked into its `bin`: `jq`,
/// asked for; `curl`, keg-only and linked by hand; `oniguruma`, a
/// dependency.
fn three_formulae(home: &Home) -> (Vec<ManagerInstance>, Vec<InstalledArtifact>, PathBuf) {
    let brew = home.dir("brew");
    for (formula, version, command) in [
        ("jq", "1.8.2", "jq"),
        ("curl", "8.17.0", "curl"),
        ("oniguruma", "6.9.10", "onig-config"),
    ] {
        home.exe(&format!("brew/Cellar/{formula}/{version}/bin/{command}"));
        home.link(
            &format!("brew/bin/{command}"),
            Path::new(&format!("../Cellar/{formula}/{version}/bin/{command}")),
        );
    }
    let id = format!("brew:{}", brew.display());
    let mut curl = artifact(&id, ArtifactKind::Formula, "curl");
    curl.facts.command_inputs = CommandInputs {
        keg_only: true,
        ..Default::default()
    };
    let mut oniguruma = artifact(&id, ArtifactKind::Formula, "oniguruma");
    oniguruma.reason = InstallReason::Dependency;
    (
        vec![instance("brew", &id, &brew, &brew.join("bin/brew"))],
        vec![artifact(&id, ArtifactKind::Formula, "jq"), curl, oniguruma],
        brew.join("bin"),
    )
}

#[test]
fn test_a_keg_only_formula_linked_by_hand_is_judged_but_never_said_not_found() {
    // A keg-only formula has commands in `<prefix>/bin` only when someone
    // linked it there by hand (`brew link --force`, as this Mac's author
    // did with `node@22`): what Terminal runs is then said of it as of any
    // formula. A dependency still gets no verdict.
    let home = Home::new("keg-only");
    let (instances, artifacts, bin) = three_formulae(&home);
    let found = verdicts(&home, std::slice::from_ref(&bin), &instances, &artifacts);
    assert_eq!(
        found,
        vec![
            vec![runs("jq")],
            vec![runs("curl")],
            vec![unjudged("onig-config")]
        ]
    );
    // macOS's own curl first: the hand-linked one waits behind it.
    home.exe("system/bin/curl");
    let found = verdicts(&home, &[home.at("system/bin"), bin], &instances, &artifacts);
    assert_eq!(found[1], vec![shadowed("curl", None)]);
    // Off PATH altogether, still never "not found" for curl's: Homebrew
    // leaves a keg-only formula off it on purpose. (This prefix is under
    // the test's home, so its folder is shown from `~`.)
    let found = verdicts(&home, &[], &instances, &artifacts);
    assert_eq!(
        found,
        vec![
            vec![not_on_path("jq", "~/brew/bin")],
            vec![unjudged("curl")],
            vec![unjudged("onig-config")]
        ]
    );
}

#[test]
fn test_homebrews_node_owns_the_commands_its_corepack_links_lead_to_through_npms_folder() {
    // The layout on the author's Mac (2026-10-01), where Homebrew and npm
    // share one prefix: `node@22` keg-only and linked by hand, whose
    // `corepack` npm lists as a global package of its own. npm's
    // `lib/node_modules/corepack` is links into the keg, file by file, so
    // `bin/pnpm` leads into npm's folder first and into the keg in the end
    // -- and the keg is what runs. npm's own copy of `npm`, a real folder,
    // provides nothing here: `bin/npm` leads into the keg.
    let home = Home::new("homebrew-node");
    let prefix = home.dir("homebrew");
    let keg = "Cellar/node@22/22.23.3";
    home.exe(&format!("homebrew/{keg}/bin/node"));
    home.exe(&format!(
        "homebrew/{keg}/lib/node_modules/npm/bin/npm-cli.js"
    ));
    home.link(
        &format!("homebrew/{keg}/bin/npm"),
        Path::new("../lib/node_modules/npm/bin/npm-cli.js"),
    );
    home.exe(&format!(
        "homebrew/{keg}/lib/node_modules/corepack/dist/pnpm.js"
    ));
    home.link(
        "homebrew/lib/node_modules/corepack/dist/pnpm.js",
        Path::new(&format!(
            "../../../../{keg}/lib/node_modules/corepack/dist/pnpm.js"
        )),
    );
    home.exe("homebrew/lib/node_modules/npm/bin/npm-cli.js");
    home.link(
        "homebrew/bin/node",
        Path::new(&format!("../{keg}/bin/node")),
    );
    home.link("homebrew/bin/npm", Path::new(&format!("../{keg}/bin/npm")));
    home.link(
        "homebrew/bin/pnpm",
        Path::new("../lib/node_modules/corepack/dist/pnpm.js"),
    );
    let brew_id = format!("brew:{}", prefix.display());
    let npm_id = format!("npm:{}", prefix.display());
    let instances = vec![
        instance("brew", &brew_id, &prefix, &prefix.join("bin/brew")),
        instance("npm", &npm_id, &prefix, &prefix.join("bin/npm")),
    ];
    let mut node = artifact(&brew_id, ArtifactKind::Formula, "node@22");
    node.facts.command_inputs.keg_only = true;
    let artifacts = vec![
        node,
        artifact(&npm_id, ArtifactKind::Package, "corepack"),
        artifact(&npm_id, ArtifactKind::Package, "npm"),
    ];
    let found = verdicts(&home, &[prefix.join("bin")], &instances, &artifacts);
    assert_eq!(
        found,
        vec![
            vec![runs("node"), runs("npm"), runs("pnpm")],
            vec![],
            vec![]
        ]
    );
}

#[test]
fn test_a_formula_that_comes_first_is_another_program_with_the_name() {
    // Homebrew's formula `grok`, a regular-expression tool, before Grok
    // Build's own `grok`: what runs is named by its artifact, which is not
    // Grok Build (no family).
    let home = Home::new("namesake");
    let brew = home.dir("brew");
    home.exe("brew/Cellar/grok/1.0.3/bin/grok");
    home.link("brew/bin/grok", Path::new("../Cellar/grok/1.0.3/bin/grok"));
    let download = home.exe(".grok/downloads/grok-1.0.41-macos-aarch64");
    let launcher = home.link(".grok/bin/grok", &download);
    let brew_id = format!("brew:{}", brew.display());
    let instances = vec![
        instance("brew", &brew_id, &brew, &brew.join("bin/brew")),
        instance(
            "standalone-grok",
            "standalone-grok",
            &home.at(".grok"),
            &launcher,
        ),
    ];
    let formula = artifact(&brew_id, ArtifactKind::Formula, "grok");
    let artifacts = vec![
        formula.clone(),
        with_family(
            artifact("standalone-grok", ArtifactKind::Binary, "grok"),
            "grok-build",
        ),
    ];
    let found = verdicts(
        &home,
        &[brew.join("bin"), home.at(".grok/bin")],
        &instances,
        &artifacts,
    );
    // Grok Build's `agent` is not there in this layout: one command.
    assert_eq!(
        found,
        vec![
            vec![runs("grok")],
            vec![shadowed("grok", Some(&formula.key))]
        ]
    );
}

#[test]
fn test_a_file_no_artifact_provides_is_another_program() {
    let home = Home::new("stranger");
    let real = home.exe(".local/share/claude/versions/2.1.281");
    let launcher = home.link(".local/bin/claude", &real);
    home.exe("usr-local-bin/claude");
    let instances = vec![instance(
        "standalone-claude",
        "standalone-claude",
        &home.at(".local/share/claude"),
        &launcher,
    )];
    let artifacts = vec![artifact(
        "standalone-claude",
        ArtifactKind::Binary,
        "claude",
    )];
    let found = verdicts(
        &home,
        &[home.at("usr-local-bin"), home.at(".local/bin")],
        &instances,
        &artifacts,
    );
    assert_eq!(found, vec![vec![shadowed("claude", None)]]);
}

#[test]
fn test_rustups_proxies_are_its_commands_by_name_whether_links_or_hard_links() {
    let home = Home::new("rustup");
    let cargo_home = home.dir(".cargo");
    let rustup = home.exe(".cargo/bin/rustup");
    home.link(".cargo/bin/cargo", Path::new("rustup"));
    fs::hard_link(&rustup, home.at(".cargo/bin/rustc")).expect("hard link");
    let instances = vec![instance(
        "standalone-rustup",
        "standalone-rustup",
        &cargo_home,
        &rustup,
    )];
    let artifacts = vec![artifact(
        "standalone-rustup",
        ArtifactKind::Binary,
        "rustup",
    )];
    let found = verdicts(&home, &[home.at(".cargo/bin")], &instances, &artifacts);
    // The proxies that are not there are no claim.
    assert_eq!(
        found,
        vec![vec![runs("cargo"), runs("rustc"), runs("rustup")]]
    );

    // Homebrew's `rust` first on PATH, with its own `cargo`.
    let brew = home.dir("brew");
    home.exe("brew/Cellar/rust/1.90.0/bin/cargo");
    home.link(
        "brew/bin/cargo",
        Path::new("../Cellar/rust/1.90.0/bin/cargo"),
    );
    let brew_id = format!("brew:{}", brew.display());
    let mut instances = instances;
    instances.insert(0, instance("brew", &brew_id, &brew, &brew.join("bin/brew")));
    let rust = artifact(&brew_id, ArtifactKind::Formula, "rust");
    let artifacts = vec![rust.clone(), artifacts[0].clone()];
    let found = verdicts(
        &home,
        &[brew.join("bin"), home.at(".cargo/bin")],
        &instances,
        &artifacts,
    );
    assert_eq!(
        found,
        vec![
            vec![runs("cargo")],
            vec![
                shadowed("cargo", Some(&rust.key)),
                runs("rustc"),
                runs("rustup")
            ]
        ]
    );
}

#[test]
fn test_a_cargo_crates_bins_are_its_commands() {
    let home = Home::new("cargo-bins");
    let cargo_home = home.dir(".cargo");
    let rg = home.exe(".cargo/bin/rg");
    let id = format!("cargo:{}", cargo_home.display());
    let instances = vec![instance(
        "cargo",
        &id,
        &cargo_home,
        &cargo_home.join("bin/cargo"),
    )];
    let artifacts = vec![with_provided(
        artifact(&id, ArtifactKind::Binary, "ripgrep"),
        vec![provided("rg", &rg, &[])],
    )];
    let found = verdicts(&home, &[home.at(".cargo/bin")], &instances, &artifacts);
    assert_eq!(found, vec![vec![runs("rg")]]);
    let found = verdicts(&home, &[], &instances, &artifacts);
    assert_eq!(found, vec![vec![not_on_path("rg", "~/.cargo/bin")]]);
}

/// uv's `ruff` and pipx's `ruff`, each in its own environment, and one
/// `~/.local/bin/ruff` that leads into `owner`'s.
fn two_ruffs(home: &Home, owner: &str) -> (Vec<ManagerInstance>, Vec<InstalledArtifact>) {
    let uv_venv = home.dir(".local/share/uv/tools/ruff");
    home.exe(".local/share/uv/tools/ruff/bin/ruff");
    let pipx_app = home.exe(".local/pipx/venvs/ruff/bin/ruff");
    let target = match owner {
        "uv" => uv_venv.join("bin/ruff"),
        _ => pipx_app.clone(),
    };
    let link = home.link(".local/bin/ruff", &target);
    // The uv line names the link; pipx names the app in its environment.
    let instances = vec![
        instance("pipx", "pipx", &home.at("bin"), &home.at("bin/pipx")),
        instance("uv", "uv", &home.at("bin"), &home.at("bin/uv")),
    ];
    let artifacts = vec![
        with_provided(
            artifact("pipx", ArtifactKind::Tool, "ruff"),
            vec![provided("ruff", &pipx_app, &[])],
        ),
        with_provided(
            artifact("uv", ArtifactKind::Tool, "ruff"),
            vec![provided("ruff", &link, &[&uv_venv])],
        ),
    ];
    (instances, artifacts)
}

#[test]
fn test_one_local_bin_ruff_is_the_copy_whose_environment_it_leads_into() {
    let home = Home::new("ruffs-uv");
    let (instances, artifacts) = two_ruffs(&home, "uv");
    let found = verdicts(&home, &[home.at(".local/bin")], &instances, &artifacts);
    // pipx's copy is still named, but no link leads to it: no verdict.
    assert_eq!(found, vec![vec![unjudged("ruff")], vec![runs("ruff")]]);

    let home = Home::new("ruffs-pipx");
    let (instances, artifacts) = two_ruffs(&home, "pipx");
    let found = verdicts(&home, &[home.at(".local/bin")], &instances, &artifacts);
    // The link is pipx's now: uv's line names a file that is not uv's.
    assert_eq!(found, vec![vec![runs("ruff")], Vec::<CommandFact>::new()]);
    // Off PATH, pipx's folder is the one its link is in.
    let found = verdicts(&home, &[], &instances, &artifacts);
    assert_eq!(
        found,
        vec![
            vec![not_on_path("ruff", "~/.local/bin")],
            Vec::<CommandFact>::new()
        ]
    );
}

#[test]
fn test_the_recorded_uv_line_names_ruff() {
    // The recording's `- ruff (/Users/brulek/.local/bin/ruff)`, through the
    // adapter's own parse: `ruff`, the link, and the tool's environment.
    let text = fs::read_to_string("../../adapters/fixtures/uv/0.12.17/tool-list-show-paths.txt")
        .expect("read the recorded uv tool list");
    let ruff = text
        .lines()
        .find(|line| line.starts_with("- "))
        .expect("a binary line");
    assert_eq!(ruff, "- ruff (/Users/brulek/.local/bin/ruff)");
}

#[test]
fn test_the_budget_stops_the_read_and_then_nothing_is_judged() {
    let home = Home::new("budget");
    let setup = two_claudes(&home);
    let tight = CommandBudget {
        max_entries: 1,
        max_duration: Duration::from_secs(5),
    };
    let folders = read_folders(
        &[setup.npm_bin.clone(), setup.local_bin.clone()],
        &[],
        home.path(),
        tight,
    );
    assert!(!folders.complete());
    assert_eq!(
        judge(
            &folders,
            &setup.instances,
            &setup.artifacts,
            home.path(),
            true,
            CommandBudget::default()
        ),
        None
    );
    let no_time = CommandBudget {
        max_entries: 20_000,
        max_duration: Duration::ZERO,
    };
    assert!(!read_folders(
        std::slice::from_ref(&setup.npm_bin),
        &[],
        home.path(),
        no_time
    )
    .complete());
    // `Folders::default()`, a read that never ran, judges nothing either.
    assert_eq!(
        judge(
            &Folders::default(),
            &setup.instances,
            &setup.artifacts,
            home.path(),
            true,
            CommandBudget::default()
        ),
        None
    );
}

#[test]
fn test_the_same_disk_gives_the_same_answer() {
    // `Snapshot::same_content` compares the answer: two readings of an
    // unchanged disk must not differ in order or content.
    let home = Home::new("same");
    let setup = two_claudes(&home);
    let (brew_instances, brew_artifacts, bin) = three_formulae(&home);
    let instances: Vec<_> = setup.instances.into_iter().chain(brew_instances).collect();
    let artifacts: Vec<_> = setup.artifacts.into_iter().chain(brew_artifacts).collect();
    let path = [bin, setup.npm_bin, setup.local_bin];
    let first = verdicts(&home, &path, &instances, &artifacts);
    let second = verdicts(&home, &path, &instances, &artifacts);
    assert_eq!(first, second);
    assert!(first
        .iter()
        .all(|commands| commands.windows(2).all(|pair| pair[0].name < pair[1].name)));
}

// ------------------------------------------------------- through a Session

/// One source as a test describes it: its instance and its rows, the same
/// every round.
struct Fixed {
    meta: AdapterMeta,
    instance: ManagerInstance,
    artifacts: Vec<InstalledArtifact>,
    /// How long `detect` takes: a round still detecting while a test
    /// changes what the session was told.
    detect_delay: Duration,
}

impl Fixed {
    fn new(instance: ManagerInstance, artifacts: Vec<InstalledArtifact>) -> Arc<Fixed> {
        Fixed::slow(instance, artifacts, Duration::ZERO)
    }

    fn slow(
        instance: ManagerInstance,
        artifacts: Vec<InstalledArtifact>,
        detect_delay: Duration,
    ) -> Arc<Fixed> {
        Arc::new(Fixed {
            meta: AdapterMeta {
                id: instance.adapter_id.clone(),
                name: instance.adapter_id.clone(),
                kind: "test".to_string(),
                platforms: vec!["macos".to_string()],
                homepage: "https://example.invalid".to_string(),
                schema_version: 1,
                verified_versions: vec![],
            },
            instance,
            artifacts,
            detect_delay,
        })
    }
}

#[async_trait]
impl Adapter for Fixed {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
        tokio::time::sleep(self.detect_delay).await;
        vec![self.instance.clone()]
    }

    async fn inventory(
        &self,
        _inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        Ok(self.artifacts.clone())
    }

    async fn check_updates(
        &self,
        _inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        Ok(CheckOutcome::default())
    }

    async fn search(
        &self,
        _inst: &ManagerInstance,
        _query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        Ok(Vec::new())
    }

    async fn plan(&self, _inst: &ManagerInstance, _req: &OpRequest) -> Result<Plan, AdapterError> {
        Err(AdapterError::Unsupported("test source".to_string()))
    }

    async fn execute(
        &self,
        _plan: &Plan,
        _sink: Arc<dyn EventSink>,
        _op_id: OpId,
        _cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        Ok(Outcome::Succeeded)
    }

    async fn reconcile(
        &self,
        _inst: &ManagerInstance,
        _key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        Err(AdapterError::Unsupported("test source".to_string()))
    }
}

fn session_over(setup: &TwoClaudes) -> Arc<Session> {
    let adapters: Vec<Arc<dyn Adapter>> = setup
        .instances
        .iter()
        .map(|inst| {
            let rows = setup
                .artifacts
                .iter()
                .filter(|a| a.key.instance_id == inst.id)
                .cloned()
                .collect();
            Fixed::new(inst.clone(), rows) as Arc<dyn Adapter>
        })
        .collect();
    Session::with_adapters(Arc::new(VecSink::new()), adapters, None)
}

fn commands_of<'a>(artifacts: &'a [InstalledArtifact], name: &str) -> &'a [CommandFact] {
    &artifacts
        .iter()
        .find(|a| a.key.name == name)
        .expect(name)
        .facts
        .commands
}

#[tokio::test(flavor = "multi_thread")]
async fn test_a_refresh_writes_the_verdicts_and_an_unchanged_disk_does_not_move_the_generation() {
    let home = Home::new("session");
    let setup = two_claudes(&home);
    let session = session_over(&setup);
    let env = home.env(vec![setup.npm_bin.clone(), setup.local_bin.clone()]);

    let first = session.refresh(&env, &CheckOptions::default()).await;
    assert_eq!(
        commands_of(&first.artifacts, "@anthropic-ai/claude-code"),
        &[runs("claude")]
    );
    let npm_key = setup.artifacts[0].key.clone();
    assert_eq!(
        commands_of(&first.artifacts, "claude"),
        &[shadowed("claude", Some(&npm_key))]
    );

    let second = session.refresh(&env, &CheckOptions::default()).await;
    assert_eq!(second.generation, first.generation);
    assert_eq!(second.artifacts, first.artifacts);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_a_session_keeps_what_its_last_round_made_of_the_path_folders() {
    let home = Home::new("session-path-folders");
    let setup = two_claudes(&home);
    let session = session_over(&setup);
    assert_eq!(session.path_folders(), None, "no round yet");
    let env = home.env(vec![setup.npm_bin.clone(), setup.local_bin.clone()]);
    session.refresh(&env, &CheckOptions::default()).await;
    assert_eq!(
        session.path_folders(),
        Some(PathFolders {
            read: 2,
            unread: Vec::new(),
        })
    );
    // A round against a PATH that is not the login shell's reads none of
    // it, and says nothing of it.
    session.note_login_path(false);
    session.refresh(&env, &CheckOptions::default()).await;
    assert_eq!(session.path_folders(), None);
}

#[test]
fn test_a_read_the_budget_stopped_says_nothing_of_the_path_folders() {
    let home = Home::new("path-summary-stopped");
    let bin = home.dir("bin");
    home.exe("bin/one");
    let folders = read_folders(
        std::slice::from_ref(&bin),
        &[],
        home.path(),
        CommandBudget {
            max_entries: 0,
            max_duration: Duration::from_secs(5),
        },
    );
    assert!(!folders.complete());
    assert_eq!(folders.path_summary(home.path()), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_a_session_told_the_path_was_not_restored_says_nothing_about_which_runs() {
    let home = Home::new("session-unknown");
    let setup = two_claudes(&home);
    let session = session_over(&setup);
    session.note_login_path(false);
    let env = home.env(vec![setup.npm_bin.clone(), setup.local_bin.clone()]);
    let snapshot = session.refresh(&env, &CheckOptions::default()).await;
    for artifact in &snapshot.artifacts {
        assert_eq!(artifact.facts.commands, vec![unjudged("claude")]);
    }
}

/// Astra's j2 review, finding 2: a round begun with Finder's few folders
/// (the login shell's `PATH` not read) is still detecting when a read of
/// the login shell works and the session is told so. That round must not
/// judge which copy runs against the few folders it was begun with: whether
/// its `PATH` is the login shell's was taken with it, as it arrived.
#[tokio::test(flavor = "multi_thread")]
async fn test_a_round_begun_without_the_login_path_stays_unjudged_when_a_read_works_meanwhile() {
    let home = Home::new("session-retry-race");
    let setup = two_claudes(&home);
    let adapters: Vec<Arc<dyn Adapter>> = setup
        .instances
        .iter()
        .map(|inst| {
            let rows = setup
                .artifacts
                .iter()
                .filter(|a| a.key.instance_id == inst.id)
                .cloned()
                .collect();
            Fixed::slow(inst.clone(), rows, Duration::from_millis(300)) as Arc<dyn Adapter>
        })
        .collect();
    let session = Session::with_adapters(Arc::new(VecSink::new()), adapters, None);
    // The launch read failed: the round's PATH is the process's own.
    session.note_login_path(false);
    let env = home.env(vec![setup.npm_bin.clone(), setup.local_bin.clone()]);
    let older = tokio::spawn({
        let session = session.clone();
        let env = env.clone();
        async move { session.refresh(&env, &CheckOptions::default()).await }
    });
    // While it detects, Check Again's read works.
    tokio::time::sleep(Duration::from_millis(80)).await;
    session.note_login_path(true);
    let snapshot = older.await.unwrap();
    for artifact in &snapshot.artifacts {
        assert_eq!(
            artifact.facts.commands,
            vec![unjudged("claude")],
            "{}",
            artifact.key.name
        );
    }
    // The next round, begun with the login shell's PATH, judges.
    let next = session.refresh(&env, &CheckOptions::default()).await;
    assert_eq!(
        commands_of(&next.artifacts, "@anthropic-ai/claude-code"),
        &[runs("claude")]
    );
}

#[test]
fn test_a_thousand_links_in_a_bin_folder_take_milliseconds_not_seconds() {
    // The synthesis' bar: a thousand entries is a busy Homebrew `bin`, and
    // the whole check (both halves) must stay far inside its budget. The
    // bound here is loose -- a slow CI disk is not a failure -- the point
    // is the shape: one `read_dir` per folder and a `realpath` per entry,
    // never a lookup per command per folder.
    let home = Home::new("thousand");
    let brew = home.dir("brew");
    let id = format!("brew:{}", brew.display());
    let mut artifacts = Vec::new();
    for i in 0..1000 {
        let name = format!("tool{i:04}");
        home.exe(&format!("brew/Cellar/{name}/1.0/bin/{name}"));
        home.link(
            &format!("brew/bin/{name}"),
            Path::new(&format!("../Cellar/{name}/1.0/bin/{name}")),
        );
        artifacts.push(artifact(&id, ArtifactKind::Formula, &name));
    }
    let instances = vec![instance("brew", &id, &brew, &brew.join("bin/brew"))];
    let path = [home.dir("usr-bin"), brew.join("bin")];
    let started = std::time::Instant::now();
    let found = verdicts(&home, &path, &instances, &artifacts);
    let took = started.elapsed();
    eprintln!("read and judged a thousand links in {took:?}");
    assert_eq!(found.len(), 1000);
    assert!(found
        .iter()
        .zip(&artifacts)
        .all(|(commands, artifact)| commands == &vec![runs(&artifact.key.name)]));
    assert!(
        took < Duration::from_secs(2),
        "a thousand links took {took:?}"
    );
}

#[test]
fn test_a_folder_macos_asks_about_is_never_read_and_no_verdict_rests_on_it() {
    // `~/Documents/bin` on PATH: reading it would put up macOS's "would like
    // to access files in your Documents folder" at a refresh. It is kept,
    // unread, and a name it could hold first gets no verdict; a name whose
    // first copy comes before it still does. `/Volumes` (another disk,
    // perhaps a network one that no longer answers) is not even resolved.
    let home = Home::new("asks-first");
    let setup = two_claudes(&home);
    let documents_bin = home.dir("Documents/bin");
    home.exe("Documents/bin/claude");
    let through_a_link = home.link("docs-bin", &documents_bin);
    let volume = PathBuf::from("/Volumes/Banager-test-no-such-disk/bin");
    let path = [
        setup.npm_bin.clone(),
        documents_bin.clone(),
        through_a_link.clone(),
        volume.clone(),
        setup.local_bin.clone(),
    ];
    let folders = read_folders(
        &path,
        &bin_folders(&setup.instances),
        home.path(),
        CommandBudget::default(),
    );
    assert!(folders.complete());
    assert_eq!(
        folders.unread_path_folders(),
        vec![documents_bin.as_path(), volume.as_path()],
        "the Documents folder once, however it is named, and the other disk"
    );
    assert_eq!(
        folders.path_folders(),
        vec![setup.npm_bin.as_path(), setup.local_bin.as_path()]
    );
    let found = judge(
        &folders,
        &setup.instances,
        &setup.artifacts,
        home.path(),
        true,
        CommandBudget::default(),
    )
    .expect("judged");
    // npm's copy comes first, before the unread folders: it runs whatever
    // they hold, and the native copy, found further along, is behind it.
    let npm_key = setup.artifacts[0].key.clone();
    assert_eq!(
        found,
        vec![
            vec![runs("claude")],
            vec![shadowed("claude", Some(&npm_key))]
        ]
    );

    // The other way round: the native folder first, so the same is said
    // the other way, an unread folder between the two notwithstanding.
    let path = [
        setup.local_bin.clone(),
        documents_bin,
        setup.npm_bin.clone(),
    ];
    let found = verdicts(&home, &path, &setup.instances, &setup.artifacts);
    let native_key = setup.artifacts[1].key.clone();
    assert_eq!(
        found,
        vec![
            vec![shadowed("claude", Some(&native_key))],
            vec![runs("claude")]
        ]
    );

    // An unread folder first: it may hold a `claude` that runs, so neither
    // copy gets a verdict -- and "not found" is not said of the native one,
    // whose folder is off PATH, while that folder could hold a link to it.
    let path = [home.at("Documents/bin"), setup.npm_bin.clone()];
    let found = verdicts(&home, &path, &setup.instances, &setup.artifacts);
    assert_eq!(
        found,
        vec![vec![unjudged("claude")], vec![unjudged("claude")]]
    );
}

#[test]
fn test_a_protected_folder_is_never_read_whatever_case_names_it() {
    // The list is `protected::PROTECTED_IN_HOME`, shared with the size
    // walk, and compared case aside as APFS compares names: `~/documents`
    // is `~/Documents`, and `/volumes` is `/Volumes`. Other apps' data in
    // `~/Library/Containers` and `~/Library/Group Containers` is on it too.
    let home = Home::new("asks-first-case");
    let setup = two_claudes(&home);
    let lower_documents = home.at("documents/bin");
    let containers = home.dir("Library/Containers/com.example.app/Data/bin");
    let group_containers = home.dir("Library/Group Containers/group.example/bin");
    let volume = PathBuf::from("/volumes/Banager-test-no-such-disk/bin");
    let path = [
        lower_documents.clone(),
        containers.clone(),
        group_containers.clone(),
        volume.clone(),
        setup.npm_bin.clone(),
    ];
    let folders = read_folders(
        &path,
        &bin_folders(&setup.instances),
        home.path(),
        CommandBudget::default(),
    );
    assert!(folders.complete());
    assert_eq!(
        folders.unread_path_folders(),
        vec![
            lower_documents.as_path(),
            containers.as_path(),
            group_containers.as_path(),
            volume.as_path()
        ]
    );
    assert_eq!(folders.path_folders(), vec![setup.npm_bin.as_path()]);
}

/// A folder whose permissions are taken away for one test (mode 000:
/// anything that looked inside it would fail with EACCES), given back
/// when the test ends, before its home is removed.
struct Locked(PathBuf);

impl Locked {
    fn new(path: PathBuf) -> Locked {
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("lock");
        Locked(path)
    }
}

impl Drop for Locked {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
    }
}

/// A Cargo crate whose `myproj` is in `~/.cargo/bin`.
fn cargo_myproj(home: &Home) -> (ManagerInstance, InstalledArtifact) {
    let cargo_home = home.dir(".cargo");
    let theirs = home.exe(".cargo/bin/myproj");
    let id = format!("cargo:{}", cargo_home.display());
    (
        instance("cargo", &id, &cargo_home, &cargo_home.join("bin/cargo")),
        with_provided(
            artifact(&id, ArtifactKind::Binary, "myproj"),
            vec![provided("myproj", &theirs, &[])],
        ),
    )
}

#[test]
fn test_an_npm_link_into_documents_is_never_followed_and_no_verdict_rests_on_it() {
    // `npm link` in ~/Documents/myproj: npm's `bin/myproj` leads through
    // `lib/node_modules/myproj` into the Documents folder. Following it
    // would look inside Documents at every refresh. The folder there is
    // locked: had anything under it been looked at, the link would have
    // read as broken, and Cargo's copy as the one that runs.
    let home = Home::new("npm-link-documents");
    let npm = home.dir("npm");
    home.exe("Documents/myproj/bin/cli.js");
    home.link("npm/lib/node_modules/myproj", &home.at("Documents/myproj"));
    home.link(
        "npm/bin/myproj",
        Path::new("../lib/node_modules/myproj/bin/cli.js"),
    );
    let (cargo, crate_row) = cargo_myproj(&home);
    let _locked = Locked::new(home.at("Documents/myproj"));
    let npm_id = format!("npm:{}", npm.display());
    let instances = vec![instance("npm", &npm_id, &npm, &npm.join("bin/npm")), cargo];
    let artifacts = vec![
        artifact(&npm_id, ArtifactKind::Package, "myproj"),
        crate_row,
    ];

    // npm's folder first: its `myproj` may be what runs, and nobody looked
    // where it leads. The package claims nothing; the crate gets no verdict.
    let found = verdicts(
        &home,
        &[npm.join("bin"), home.at(".cargo/bin")],
        &instances,
        &artifacts,
    );
    assert_eq!(found, vec![Vec::new(), vec![unjudged("myproj")]]);

    // Cargo's folder first: its copy runs, whatever npm's leads to.
    let found = verdicts(
        &home,
        &[home.at(".cargo/bin"), npm.join("bin")],
        &instances,
        &artifacts,
    );
    assert_eq!(found, vec![Vec::new(), vec![runs("myproj")]]);
}

#[test]
fn test_a_path_entry_that_leads_into_icloud_drive_is_kept_unread_without_being_resolved() {
    // `~/bin` is a link to a folder in iCloud Drive. Its name is not a
    // protected place; where it leads is. Resolving it would look inside
    // `Library/Mobile Documents` -- locked here, so a look would fail and
    // drop the entry from PATH as if it were not there.
    let home = Home::new("path-into-icloud");
    let cloud = home.dir("Library/Mobile Documents/com~apple~CloudDocs/bin");
    home.exe("Library/Mobile Documents/com~apple~CloudDocs/bin/myproj");
    let bin = home.link("bin", &cloud);
    let (cargo, crate_row) = cargo_myproj(&home);
    let _locked = Locked::new(home.at("Library/Mobile Documents/com~apple~CloudDocs"));
    let path = [bin.clone(), home.at(".cargo/bin")];
    let folders = read_folders(&path, &[], home.path(), CommandBudget::default());
    assert!(folders.complete());
    assert_eq!(folders.unread_path_folders(), vec![bin.as_path()]);
    assert_eq!(
        folders.path_folders(),
        vec![home.at(".cargo/bin").as_path()]
    );
    let found = verdicts(
        &home,
        &path,
        std::slice::from_ref(&cargo),
        std::slice::from_ref(&crate_row),
    );
    assert_eq!(found, vec![vec![unjudged("myproj")]]);
}

#[test]
fn test_links_onto_another_disk_are_never_followed() {
    // A PATH entry that is a link onto `/Volumes` (a network disk that may
    // not answer), and a name in a folder that is read leading there: the
    // first is kept unread, the second may be the file that runs. Neither
    // is resolved, so neither can hang a refresh.
    let home = Home::new("links-onto-volumes");
    let volume = Path::new("/Volumes/Banager-test-no-such-disk");
    let vol = home.link("vol", &volume.join("bin"));
    home.dir(".local/bin");
    home.link(".local/bin/myproj", &volume.join("myproj"));
    let (cargo, crate_row) = cargo_myproj(&home);

    let path = [vol.clone(), home.at(".cargo/bin")];
    let folders = read_folders(&path, &[], home.path(), CommandBudget::default());
    assert_eq!(folders.unread_path_folders(), vec![vol.as_path()]);
    let found = verdicts(
        &home,
        &path,
        std::slice::from_ref(&cargo),
        std::slice::from_ref(&crate_row),
    );
    assert_eq!(found, vec![vec![unjudged("myproj")]]);

    let path = [home.at(".local/bin"), home.at(".cargo/bin")];
    let found = verdicts(
        &home,
        &path,
        std::slice::from_ref(&cargo),
        std::slice::from_ref(&crate_row),
    );
    assert_eq!(found, vec![vec![unjudged("myproj")]]);
}

#[test]
fn test_a_path_folder_that_is_there_but_cannot_be_listed_is_kept_unread() {
    // A shell runs what is in a folder it may search but not list (mode
    // 711): dropping the folder would let a copy further along read as the
    // one that runs. One that is not there at all is still skipped.
    let home = Home::new("unlistable");
    let tools = home.dir("tools/bin");
    home.exe("tools/bin/myproj");
    let (cargo, crate_row) = cargo_myproj(&home);
    fs::set_permissions(&tools, fs::Permissions::from_mode(0o311)).unwrap();
    let _restore = Locked(tools.clone());
    let path = [home.at("missing/bin"), tools.clone(), home.at(".cargo/bin")];
    let folders = read_folders(&path, &[], home.path(), CommandBudget::default());
    assert_eq!(folders.unread_path_folders(), vec![tools.as_path()]);
    assert_eq!(
        folders.path_folders(),
        vec![home.at(".cargo/bin").as_path()]
    );
    // What the window's tool setup check is told: one read, one not, the
    // one not as named on PATH with the home folder as `~`.
    assert_eq!(
        folders.path_summary(home.path()),
        Some(PathFolders {
            read: 1,
            unread: vec!["~/tools/bin".to_string()],
        })
    );
    let found = verdicts(
        &home,
        &path,
        std::slice::from_ref(&cargo),
        std::slice::from_ref(&crate_row),
    );
    assert_eq!(found, vec![vec![unjudged("myproj")]]);
}

#[test]
fn test_a_path_folder_spelled_in_another_case_is_the_same_folder() {
    // On a Mac's disk `~/.CARGO/bin` is `~/.cargo/bin`: a `PATH` entry
    // and a tool's folder that differ only in case are one folder, as
    // `realpath` would say, whatever spelling each was given.
    let home = Home::new("case");
    fs::create_dir_all(home.at(".case-probe")).unwrap();
    if !home.at(".CASE-PROBE").exists() {
        return; // a case-sensitive disk: two folders after all
    }
    let cargo_home = home.dir(".cargo");
    let rg = home.exe(".cargo/bin/rg");
    let id = format!("cargo:{}", cargo_home.display());
    let instances = vec![instance(
        "cargo",
        &id,
        &cargo_home,
        &cargo_home.join("bin/cargo"),
    )];
    let artifacts = vec![with_provided(
        artifact(&id, ArtifactKind::Binary, "ripgrep"),
        vec![provided("rg", &rg, &[])],
    )];
    let found = verdicts(&home, &[home.at(".CARGO/bin")], &instances, &artifacts);
    assert_eq!(found, vec![vec![runs("rg")]]);
    // Named twice, once in each case: read once.
    let folders = read_folders(
        &[home.at(".CARGO/bin"), home.at(".cargo/bin")],
        &bin_folders(&instances),
        home.path(),
        CommandBudget::default(),
    );
    assert_eq!(
        folders.path_folders(),
        vec![home.at(".CARGO/bin").as_path()]
    );
}

#[test]
fn test_a_name_in_another_case_earlier_on_path_is_the_copy_that_runs() {
    // On a Mac's disk, typing `claude` runs `~/bin/CLAUDE`: a name is
    // found whatever case the folder spells it in, so the copy further
    // down `PATH` does not run.
    let home = Home::new("name-case");
    fs::create_dir_all(home.at(".case-probe")).unwrap();
    if !home.at(".CASE-PROBE").exists() {
        return; // a case-sensitive disk: `CLAUDE` is not `claude` there
    }
    let real = home.exe(".local/share/claude/versions/2.1.281");
    let launcher = home.link(".local/bin/claude", &real);
    home.exe("bin/CLAUDE");
    let instances = vec![instance(
        "standalone-claude",
        "standalone-claude",
        &home.at(".local/share/claude"),
        &launcher,
    )];
    let artifacts = vec![artifact(
        "standalone-claude",
        ArtifactKind::Binary,
        "claude",
    )];
    let found = verdicts(
        &home,
        &[home.at("bin"), home.at(".local/bin")],
        &instances,
        &artifacts,
    );
    assert_eq!(found, vec![vec![shadowed("claude", None)]]);
    // And one in another case after it changes nothing.
    let found = verdicts(
        &home,
        &[home.at(".local/bin"), home.at("bin")],
        &instances,
        &artifacts,
    );
    assert_eq!(found, vec![vec![runs("claude")]]);
}

#[test]
fn test_an_unread_folder_after_a_known_copy_can_hide_the_other_installation() {
    let home = Home::new("unread-after-known");
    let setup = two_claudes(&home);
    let documents = home.dir("Documents/bin");
    home.link("Documents/bin/claude", &home.at(".local/bin/claude"));
    let found = verdicts(
        &home,
        &[setup.npm_bin.clone(), documents],
        &setup.instances,
        &setup.artifacts,
    );
    assert_eq!(found, vec![vec![runs("claude")], vec![unjudged("claude")]]);
    let without_unread = verdicts(&home, &[setup.npm_bin], &setup.instances, &setup.artifacts);
    assert_eq!(
        without_unread,
        vec![
            vec![runs("claude")],
            vec![not_on_path("claude", "~/.local/bin")]
        ]
    );
}
