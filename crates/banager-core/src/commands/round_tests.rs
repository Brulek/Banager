//! `judge` through one `protected::Round` per judgement, against `judge`
//! through `protected::resolve` itself -- the walk it replaced, from `/`
//! for every path: the same verdicts, far fewer calls, and nothing looked
//! at in a protected place. Every tree is built under the temp folder for
//! the test and removed after it; no tool is run.

use super::*;
use crate::dirfd::calls;
use crate::model::ProvidedCommand;
use crate::testing::{manager_instance, Rng, TempTree as Tree};
use std::fs;

fn instance(adapter_id: &str, prefix: PathBuf) -> ManagerInstance {
    ManagerInstance {
        prefix,
        ..manager_instance(adapter_id, adapter_id)
    }
}

fn artifact(instance: &str, name: &str, kind: ArtifactKind) -> InstalledArtifact {
    InstalledArtifact {
        display_name: name.rsplit('/').next().unwrap().into(),
        version: "1".into(),
        ..crate::testing::installed_artifact(instance, kind, name)
    }
}

fn provided(name: &str, path: PathBuf, within: Vec<PathBuf>) -> ProvidedCommand {
    ProvidedCommand {
        name: name.into(),
        path,
        within,
    }
}

/// `judge` of `artifacts` under `home`, through one round (`reference`
/// false) or through `protected::resolve` for every path.
fn judged(
    reference: bool,
    home: &Path,
    folders: &Folders,
    instances: &[ManagerInstance],
    artifacts: &[InstalledArtifact],
    path_known: bool,
) -> Option<Vec<Vec<CommandFact>>> {
    let mut look = Look::new(home, CommandBudget::default());
    look.reference = reference;
    judge_with(folders, instances, artifacts, home, path_known, look).map(|answer| answer.commands)
}

/// No call `calls` saw looked at a protected place, or through one.
fn assert_nothing_protected_looked_at(calls: &calls::Calls, protected: &Protected) {
    for (call, path) in &calls.paths {
        for at in path.ancestors() {
            assert!(!protected.contains(at), "{call:?} looked at {at:?}");
        }
    }
}

/// A Homebrew of `formulae` formulae, `per` commands each, linked as
/// Homebrew links them (`bin/<name>` → `../Cellar/<formula>/<version>/
/// bin/<name>`), on `PATH` alone.
fn brew_of(
    tree: &Tree,
    formulae: usize,
    per: usize,
) -> (Vec<ManagerInstance>, Vec<InstalledArtifact>) {
    let instances = vec![instance("brew", tree.dir("brew"))];
    let mut artifacts = Vec::new();
    for f in 0..formulae {
        let formula = format!("formula{f}");
        let mut row = artifact("brew", &formula, ArtifactKind::Formula);
        if f % 3 == 0 {
            row.reason = InstallReason::Dependency;
        }
        artifacts.push(row);
        for c in 0..per {
            let command = format!("cmd{f}_{c}");
            tree.file(&format!("brew/Cellar/{formula}/1.0/bin/{command}"), 0o755);
            tree.link_keeping_first(
                &format!("brew/bin/{command}"),
                format!("../Cellar/{formula}/1.0/bin/{command}"),
            );
        }
    }
    (instances, artifacts)
}

#[test]
fn test_judge_through_a_round_makes_far_fewer_calls_for_the_same_verdicts() {
    let tree = Tree::new("calls");
    let (instances, artifacts) = brew_of(&tree, 500, 1);
    let folders = read_folders(
        &[tree.at("brew/bin")],
        &bin_folders(&instances),
        &tree.root,
        CommandBudget::default(),
    );
    let (before, old) =
        calls::measure(|| judged(true, &tree.root, &folders, &instances, &artifacts, true));
    let (after, new) =
        calls::measure(|| judged(false, &tree.root, &folders, &instances, &artifacts, true));
    assert_eq!(before, after);
    let facts = after.unwrap();
    assert_eq!(facts.iter().map(Vec::len).sum::<usize>(), 500);
    assert!(facts
        .iter()
        .flatten()
        .any(|fact| fact.state == Some(CommandState::Runs)));
    // `/` once to find where each home folder leads (`Protected::new`:
    // this test's, and the account's own, which is another) and once for
    // the round -- not once per path.
    assert!(old.root > 500, "{}", old.root);
    let homes = 1 + usize::from(crate::protected::account_home().is_some());
    assert_eq!(new.root, homes + 1);
    // Each command: its link in `brew/bin` (held open), the link's text,
    // and the rest of the way in one lookup from `brew`; and, once, the
    // folders above them, looked up twice before they are held open.
    // Besides, each folder used again is asked where it now is: `brew/bin`
    // and `brew`, which `..` goes back to.
    let walked = new.total() - new.get_path;
    assert!(walked <= 3 * 500 + 100, "{walked} calls");
    assert!(new.get_path <= 2 * 500 + 20, "{} asked", new.get_path);
    assert!(
        new.total() * 10 < old.total(),
        "before {} calls, after {}",
        old.total(),
        new.total()
    );
    assert_nothing_protected_looked_at(&new, &Protected::new(&tree.root));
}

/// A Mac to judge: one tool of every source -- Homebrew formulae (some
/// through `opt`, some keg-only, some dependencies) and casks, npm
/// packages (some scoped), Cargo, uv and pipx tools, Grok Build's two
/// commands -- with each link leading where it should, or, at random,
/// through a chain, a loop, the data volume's spelling, another case, a
/// folder that cannot be searched, into a protected place or nowhere;
/// and names shared between sources and with a folder earlier on `PATH`.
struct RandomMac {
    tree: Tree,
    instances: Vec<ManagerInstance>,
    artifacts: Vec<InstalledArtifact>,
    path: Vec<PathBuf>,
}

/// Where a command's link at `link` leads: its own file `real`, mostly,
/// by a relative or an absolute path -- or, at random, somewhere else.
fn lead(rng: &mut Rng, tree: &Tree, link: &str, real: &str) -> PathBuf {
    let up = "../".repeat(link.split('/').count() - 1);
    let name = real.rsplit('/').next().unwrap();
    match rng.below(16) {
        0 => tree.at("Documents/bin").join(name),
        1 => tree.at("dOcUmEnTs/bin").join(name),
        2 => tree.on_data_volume("Documents/bin").join(name),
        3 => PathBuf::from("/Volumes/banager-no-such-disk/bin").join(name),
        4 => tree.at("missing").join(name),
        5 => tree.at("cycle-a"),
        6 => tree.link_keeping_first(&format!("chain/{name}"), tree.at(real)),
        7 => tree.on_data_volume(real),
        8 => tree.at(&rng.shout(real)),
        9 => tree.at("locked/in").join(name),
        10 => format!("{up}early/../{real}").into(),
        11 | 12 => tree.at(real),
        _ => format!("{up}{real}").into(),
    }
}

fn random_mac(seed: u64) -> RandomMac {
    let mut rng = Rng(seed);
    let mut tree = Tree::new("random");
    let grok = ManagerInstance {
        exe_path: tree.at(".grok/bin/grok"),
        prefix: tree.at(".grok"),
        ..manager_instance("standalone-grok", "standalone-grok")
    };
    let instances = vec![
        instance("brew", tree.dir("brew")),
        instance("npm", tree.dir("npm")),
        instance("cargo", tree.dir("cargo")),
        instance("uv", tree.dir("uv")),
        instance("pipx", tree.dir("pipx")),
        grok,
    ];
    for dir in [
        "brew/bin",
        "brew/sbin",
        "npm/bin",
        "cargo/bin",
        ".local/bin",
        "early",
        "locked/in",
        "searchonly",
        "Documents/bin",
    ] {
        tree.dir(dir);
    }
    tree.link_keeping_first("alias", "brew/bin");
    tree.link_keeping_first("protected-alias", "Documents/bin");
    tree.link_keeping_first("cycle-a", "cycle-b");
    tree.link_keeping_first("cycle-b", "cycle-a");
    tree.file(".grok/downloads/grok-1.0", 0o755);
    tree.link_keeping_first(".grok/bin/grok", "../downloads/grok-1.0");
    tree.link_keeping_first(
        ".grok/bin/agent",
        if rng.below(2) == 0 {
            tree.at(".grok/downloads/grok-1.0")
        } else {
            tree.on_data_volume(".grok/downloads/grok-1.0")
        },
    );
    let mut artifacts = vec![artifact("standalone-grok", "grok", ArtifactKind::Binary)];
    let names = ["rg", "fd", "bat", "jq", "node", "ruff", "black", "codex"];
    let name_of = |rng: &mut Rng, i: usize| -> String {
        if rng.below(5) == 0 {
            names[rng.below(names.len())].to_string()
        } else {
            format!("c{i}")
        }
    };
    let mode = |rng: &mut Rng| if rng.below(10) == 0 { 0o644 } else { 0o755 };
    for i in 0..60 {
        let command = name_of(&mut rng, i);
        if rng.below(4) == 0 {
            tree.file(&format!("early/{command}"), mode(&mut rng));
        }
        match i % 6 {
            // A Homebrew formula, through its `opt` link at times.
            0 => {
                let formula = format!("f{i}");
                let real = format!("brew/Cellar/{formula}/1.0/bin/{command}");
                tree.file(&real, mode(&mut rng));
                let via = if rng.below(3) == 0 {
                    tree.link_keeping_first(
                        &format!("brew/opt/{formula}"),
                        format!("../Cellar/{formula}/1.0"),
                    );
                    format!("brew/opt/{formula}/bin/{command}")
                } else {
                    real
                };
                let bin = if rng.below(5) == 0 { "sbin" } else { "bin" };
                let link = format!("brew/{bin}/{command}");
                let target = lead(&mut rng, &tree, &link, &via);
                tree.link_keeping_first(&link, target);
                let mut row = artifact("brew", &formula, ArtifactKind::Formula);
                if rng.below(5) == 0 {
                    row.reason = InstallReason::Dependency;
                }
                row.facts.command_inputs.keg_only = rng.below(6) == 0;
                artifacts.push(row);
            }
            // A cask: its binary in its Caskroom folder or in its app.
            1 => {
                let token = format!("k{i}");
                let real = if rng.below(2) == 0 {
                    format!("brew/Caskroom/{token}/1.0/{command}")
                } else {
                    format!("Applications/K{i}.app/Contents/MacOS/{command}")
                };
                tree.file(&real, mode(&mut rng));
                let link = format!("brew/bin/{command}");
                let target = lead(&mut rng, &tree, &link, &real);
                let link = tree.link_keeping_first(&link, target);
                let mut row = artifact("brew", &token, ArtifactKind::Cask);
                row.path = Some(tree.at(&format!("Applications/K{i}.app")));
                let within = if rng.below(2) == 0 {
                    vec![tree.at(&real)]
                } else {
                    vec![]
                };
                row.facts.command_inputs.provided = vec![provided(&command, link, within)];
                artifacts.push(row);
            }
            // An npm package, scoped at times.
            2 => {
                let package = if rng.below(3) == 0 {
                    format!("@s/p{i}")
                } else {
                    format!("p{i}")
                };
                let real = format!("npm/lib/node_modules/{package}/bin/cli.js");
                tree.file(&real, mode(&mut rng));
                let link = format!("npm/bin/{command}");
                let target = lead(&mut rng, &tree, &link, &real);
                tree.link_keeping_first(&link, target);
                artifacts.push(artifact("npm", &package, ArtifactKind::Package));
            }
            // A Cargo binary: a file in its bin folder.
            3 => {
                let path = tree.file(&format!("cargo/bin/{command}"), mode(&mut rng));
                let mut row = artifact("cargo", &format!("crate{i}"), ArtifactKind::Binary);
                row.facts.command_inputs.provided = vec![provided(&command, path, vec![])];
                if rng.below(4) == 0 {
                    // Named twice, and in another case.
                    let again = provided(
                        &command.to_uppercase(),
                        tree.at(&format!("cargo/bin/{command}")),
                        vec![],
                    );
                    row.facts.command_inputs.provided.push(again);
                }
                artifacts.push(row);
            }
            // A uv tool: its link in `~/.local/bin`, which must lead into
            // its environment.
            4 => {
                let tool = format!("u{i}");
                let real = format!("uv/tools/{tool}/bin/{command}");
                tree.file(&real, mode(&mut rng));
                let link = format!(".local/bin/{command}");
                let target = lead(&mut rng, &tree, &link, &real);
                let link = tree.link_keeping_first(&link, target);
                let mut row = artifact("uv", &tool, ArtifactKind::Tool);
                row.facts.command_inputs.provided = vec![provided(
                    &command,
                    link,
                    vec![tree.at(&format!("uv/tools/{tool}"))],
                )];
                artifacts.push(row);
            }
            // A pipx tool: the program in its environment, and its link in
            // `~/.local/bin` when that leads to it.
            _ => {
                let tool = format!("x{i}");
                let real = tree.file(&format!("pipx/venvs/{tool}/bin/{command}"), mode(&mut rng));
                if rng.below(3) != 0 {
                    let link = format!(".local/bin/{command}");
                    let target = lead(
                        &mut rng,
                        &tree,
                        &link,
                        &format!("pipx/venvs/{tool}/bin/{command}"),
                    );
                    tree.link_keeping_first(&link, target);
                }
                let mut row = artifact("pipx", &tool, ArtifactKind::Tool);
                row.facts.command_inputs.provided = vec![provided(&command, real, vec![])];
                artifacts.push(row);
            }
        }
    }
    tree.lock("locked", 0o000);
    tree.lock("searchonly", 0o111);
    let mut path = vec![
        tree.at("early"),
        tree.at("brew/bin"),
        tree.at("brew/sbin"),
        tree.at("npm/bin"),
        tree.at(".local/bin"),
        tree.at("cargo/bin"),
        tree.at(".grok/bin"),
    ];
    for i in (1..path.len()).rev() {
        let other = rng.below(i + 1);
        path.swap(i, other);
    }
    RandomMac {
        tree,
        instances,
        artifacts,
        path,
    }
}

#[test]
fn test_judge_through_a_round_and_through_resolve_agree_on_random_macs() {
    let mut states = HashSet::new();
    for seed in 1..=10u64 {
        let mac = random_mac(seed.wrapping_mul(0x2545_F491_4F6C_DD1D));
        let tree = &mac.tree;
        let protected = Protected::new(&tree.root);
        // `PATH` as it is; cut short; with folders that cannot be read or
        // that are protected first and last; with the same folders named
        // again, through a link, in capitals, from the data volume, and
        // entries no shell would use.
        for scenario in 0..4 {
            let mut path = mac.path.clone();
            match scenario {
                1 => path.truncate(2),
                2 => {
                    path.insert(0, tree.at("locked"));
                    path.insert(1, tree.at("searchonly"));
                    path.insert(2, tree.at("Documents/bin"));
                    path.push(tree.at("protected-alias"));
                }
                3 => {
                    path.extend([
                        path[0].clone(),
                        tree.at("alias"),
                        tree.at("BREW/BIN"),
                        tree.on_data_volume("cargo/bin"),
                        tree.at("missing"),
                        PathBuf::from("relative"),
                        PathBuf::new(),
                    ]);
                    path.rotate_right(3);
                }
                _ => {}
            }
            let folders = read_folders(
                &path,
                &bin_folders(&mac.instances),
                &tree.root,
                CommandBudget::default(),
            );
            assert!(folders.complete());
            for known in [true, false] {
                let old = judged(
                    true,
                    &tree.root,
                    &folders,
                    &mac.instances,
                    &mac.artifacts,
                    known,
                );
                let (new, made) = calls::measure(|| {
                    judged(
                        false,
                        &tree.root,
                        &folders,
                        &mac.instances,
                        &mac.artifacts,
                        known,
                    )
                });
                assert!(old.is_some());
                assert_eq!(old, new, "seed {seed}, scenario {scenario}, known {known}");
                assert_nothing_protected_looked_at(&made, &protected);
                // `contains` itself, against the places as listed and
                // spelled from `/` at every call, for every `PATH` entry
                // and every path a call looked at.
                let raw = protected::places(std::slice::from_ref(&tree.root));
                for at in path.iter().chain(made.paths.iter().map(|(_, at)| at)) {
                    assert_eq!(
                        protected.contains(at),
                        protected::is_within(at, &raw),
                        "{at:?}"
                    );
                }
                for fact in new.unwrap().iter().flatten() {
                    states.insert(match &fact.state {
                        None => "none",
                        Some(CommandState::Runs) => "runs",
                        Some(CommandState::ShadowedBy { by: Some(_) }) => "shadowed by a tool",
                        Some(CommandState::ShadowedBy { by: None }) => "shadowed",
                        Some(CommandState::NotOnPath { .. }) => "not on path",
                    });
                }
            }
        }
    }
    assert_eq!(
        states,
        HashSet::from([
            "none",
            "runs",
            "shadowed by a tool",
            "shadowed",
            "not on path"
        ])
    );
}

#[test]
fn test_a_new_judgement_sees_what_changed_since_the_last() {
    let tree = Tree::new("rounds");
    tree.file("cargo/bin/tool", 0o755);
    let instances = vec![instance("cargo", tree.at("cargo"))];
    let mut row = artifact("cargo", "tool", ArtifactKind::Binary);
    row.facts.command_inputs.provided = vec![provided("tool", tree.at("cargo/bin/tool"), vec![])];
    let rows = [row];
    let folders = read_folders(
        &[tree.at("cargo/bin")],
        &[],
        &tree.root,
        CommandBudget::default(),
    );
    let first = judged(false, &tree.root, &folders, &instances, &rows, true).unwrap();
    assert_eq!(first[0][0].state, Some(CommandState::Runs));
    // Swapped for a link into a protected place: nothing carried over
    // from the round before.
    fs::remove_file(tree.at("cargo/bin/tool")).unwrap();
    tree.link_keeping_first("cargo/bin/tool", tree.at("Documents/secret"));
    let (next, made) =
        calls::measure(|| judged(false, &tree.root, &folders, &instances, &rows, true));
    assert_eq!(
        next,
        judged(true, &tree.root, &folders, &instances, &rows, true)
    );
    assert!(next.unwrap()[0].is_empty());
    assert_nothing_protected_looked_at(&made, &Protected::new(&tree.root));
}

/// A Mac shaped like the one the p1 probe judged (about 500 commands, a
/// third of them a dependency's, 14 folders on `PATH`): Homebrew's
/// formulae in `bin` and `sbin`, a few whose file in the keg is itself a
/// link (Python's), npm packages in the same prefix, Cargo's binaries,
/// two tools with their own installer, and system folders on `PATH`
/// whose names some formulae share.
fn probe_like(tree: &Tree) -> (Vec<ManagerInstance>, Vec<InstalledArtifact>, Vec<PathBuf>) {
    let brew = instance("brew", tree.dir("opt/homebrew"));
    let npm = instance("npm", tree.at("opt/homebrew"));
    let cargo = instance("cargo", tree.dir("home/.cargo"));
    let grok = ManagerInstance {
        exe_path: tree.at("home/.grok/bin/grok"),
        prefix: tree.at("home/.grok"),
        ..manager_instance("standalone-grok", "standalone-grok")
    };
    let mut artifacts = Vec::new();
    let mut command = 0;
    for f in 0..440 {
        let formula = format!("formula{f}");
        let mut row = artifact("brew", &formula, ArtifactKind::Formula);
        if f % 3 != 0 {
            row.reason = InstallReason::Dependency;
        }
        artifacts.push(row);
        let keg = format!("opt/homebrew/Cellar/{formula}/1.2.{f}");
        let commands = if f % 20 == 0 { 3 } else { 1 };
        for _ in 0..commands {
            let name = format!("tool{command}");
            command += 1;
            if f % 40 == 1 {
                tree.file(&format!("{keg}/libexec/bin/{name}"), 0o755);
                tree.link_keeping_first(
                    &format!("{keg}/bin/{name}"),
                    format!("../libexec/bin/{name}"),
                );
            } else {
                tree.file(&format!("{keg}/bin/{name}"), 0o755);
            }
            let bin = if f % 25 == 0 { "sbin" } else { "bin" };
            tree.link_keeping_first(
                &format!("opt/homebrew/{bin}/{name}"),
                format!("../Cellar/{formula}/1.2.{f}/bin/{name}"),
            );
            if command % 9 == 0 {
                tree.file(&format!("usr/bin/{name}"), 0o755);
            }
        }
    }
    for p in 0..12 {
        let package = if p % 4 == 0 {
            format!("@scope/pkg{p}")
        } else {
            format!("pkg{p}")
        };
        tree.file(
            &format!("opt/homebrew/lib/node_modules/{package}/bin/cli.js"),
            0o755,
        );
        tree.link_keeping_first(
            &format!("opt/homebrew/bin/npm{p}"),
            format!("../lib/node_modules/{package}/bin/cli.js"),
        );
        artifacts.push(artifact("npm", &package, ArtifactKind::Package));
    }
    for c in 0..15 {
        let path = tree.file(&format!("home/.cargo/bin/crate{c}"), 0o755);
        let mut row = artifact("cargo", &format!("crate{c}"), ArtifactKind::Binary);
        row.facts.command_inputs.provided = vec![provided(&format!("crate{c}"), path, vec![])];
        artifacts.push(row);
    }
    tree.file("home/.grok/downloads/grok-1.0", 0o755);
    tree.link_keeping_first("home/.grok/bin/grok", "../downloads/grok-1.0");
    tree.link_keeping_first("home/.grok/bin/agent", "../downloads/grok-1.0");
    artifacts.push(artifact("standalone-grok", "grok", ArtifactKind::Binary));
    for s in 0..300 {
        tree.file(&format!("usr/bin/sys{s}"), 0o755);
    }
    let mut path: Vec<PathBuf> = [
        "home/.local/bin",
        "home/.grok/bin",
        "opt/homebrew/bin",
        "opt/homebrew/sbin",
        "usr/local/bin",
        "usr/bin",
        "bin",
        "usr/sbin",
        "sbin",
        "home/.cargo/bin",
        "home/.opencode/bin",
        "home/Library/pnpm",
        "Library/Apple/usr/bin",
        "System/Cryptexes/App/usr/bin",
    ]
    .iter()
    .map(|dir| tree.dir(dir))
    .collect();
    path.dedup();
    (vec![brew, npm, cargo, grok], artifacts, path)
}

/// `judge`'s time, before (through `protected::resolve`) and after
/// (through one round), on `probe_like`'s Mac: run with
/// `cargo test -p banager-core --release --lib commands::round_tests::bench -- --ignored --nocapture`.
#[test]
#[ignore = "benchmark: run in release with --ignored --nocapture"]
fn bench_judge_on_a_mac_like_the_probes() {
    let tree = Tree::new("bench");
    let (instances, artifacts, path) = probe_like(&tree);
    let folders = read_folders(
        &path,
        &bin_folders(&instances),
        &tree.root,
        CommandBudget::default(),
    );
    assert!(folders.complete());
    let (verdicts, old) =
        calls::measure(|| judged(true, &tree.root, &folders, &instances, &artifacts, true));
    let (again, new) =
        calls::measure(|| judged(false, &tree.root, &folders, &instances, &artifacts, true));
    assert_eq!(verdicts, again);
    let verdicts = verdicts.unwrap();
    let commands: usize = verdicts.iter().map(Vec::len).sum();
    let judged_ones = verdicts
        .iter()
        .flatten()
        .filter(|fact| fact.state.is_some())
        .count();
    let mut times = [Vec::new(), Vec::new()];
    for sample in 0..21 {
        for reference in [sample % 2 == 0, sample % 2 != 0] {
            let started = Instant::now();
            let out = judged(
                reference, &tree.root, &folders, &instances, &artifacts, true,
            );
            times[usize::from(reference)].push(started.elapsed());
            assert_eq!(out.as_ref().map(Vec::len), Some(verdicts.len()));
        }
    }
    for times in &mut times {
        times.sort();
    }
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    println!(
        "judge, {commands} commands ({judged_ones} with a verdict), {} PATH folders:",
        path.len()
    );
    println!(
        "  before (resolve from / for every path): {:>6} calls ({} open(\"/\")), median {:.2} ms, fastest {:.2} ms",
        old.total(),
        old.root,
        ms(times[1][10]),
        ms(times[1][0])
    );
    println!(
        "  after  (one round):                     {:>6} calls ({} open(\"/\")), median {:.2} ms, fastest {:.2} ms",
        new.total(),
        new.root,
        ms(times[0][10]),
        ms(times[0][0])
    );
    println!(
        "  before: {old:?}",
        old = calls::Calls {
            paths: Vec::new(),
            ..old
        }
    );
    println!(
        "  after:  {new:?}",
        new = calls::Calls {
            paths: Vec::new(),
            ..new
        }
    );
}

#[tokio::test]
async fn test_finish_carries_dropped_command_coverage_to_the_window() {
    let tree = Tree::new("dropped-claims");
    tree.file("brew/Cellar/jq/1/bin/jq", 0o755);
    tree.link_keeping_first("brew/bin/jq", "../Cellar/jq/1/bin/jq");
    let instances = vec![
        instance("brew", tree.at("brew")),
        instance("pipx", tree.at("pipx")),
        instance("cargo", tree.at(".cargo")),
    ];
    let mut pipx = artifact("pipx", "cowsay", ArtifactKind::Tool);
    pipx.facts.command_inputs.provided = vec![provided(
        "cowsay",
        tree.at("Documents/venvs/cowsay/bin/cowsay"),
        vec![],
    )];
    // Refused resolution also loses the claim: a link loop, not a missing file.
    tree.link_keeping_first(".cargo/bin/loop", "loop");
    let mut refused = artifact("cargo", "loop", ArtifactKind::Package);
    refused.facts.command_inputs.provided =
        vec![provided("loop", tree.at(".cargo/bin/loop"), vec![])];
    let mut missing = artifact("cargo", "gone", ArtifactKind::Package);
    missing.facts.command_inputs.provided =
        vec![provided("gone", tree.at(".cargo/bin/gone"), vec![])];
    let mut rows = vec![
        artifact("brew", "jq", ArtifactKind::Formula),
        pipx,
        refused,
        missing,
    ];
    let env = HostEnv {
        path_dirs: vec![tree.at("brew/bin")],
        home: tree.root.clone(),
        euid: 501,
        cargo_home: None,
        rustup_home: None,
        zdotdir: None,
        ollama_host: None,
    };
    let in_flight = Arc::new(AtomicBool::new(false));
    let budget = CommandBudget::default();
    let reading = start_reading(&env, &instances, true, &in_flight, budget);
    finish(
        reading, &instances, &mut rows, &env.home, true, &in_flight, budget,
    )
    .await;
    assert_eq!(rows[0].facts.commands[0].state, Some(CommandState::Runs));
    assert!(!rows[0].facts.commands_unavailable);
    for row in &rows[1..3] {
        assert!(row.facts.commands.is_empty());
        assert!(row.facts.commands_unavailable);
        let wire = serde_json::to_string(row).unwrap();
        let back: InstalledArtifact = serde_json::from_str(&wire).unwrap();
        assert!(back.facts.commands_unavailable);
    }
    assert!(rows[3].facts.commands.is_empty());
    assert!(
        !rows[3].facts.commands_unavailable,
        "missing is not unreadable"
    );

    // A carried empty row retains its unavailable marker; a fresh inventory
    // can clear it once the path can be checked again.
    let carried = rows[1].clone();
    rows[1].facts.command_inputs.provided.clear();
    let reading = start_reading(&env, &instances, true, &in_flight, budget);
    finish(
        reading, &instances, &mut rows, &env.home, true, &in_flight, budget,
    )
    .await;
    assert_eq!(
        rows[1].facts.commands_unavailable,
        carried.facts.commands_unavailable
    );
    rows[1].facts.commands_unavailable = false;
    let reading = start_reading(&env, &instances, true, &in_flight, budget);
    finish(
        reading, &instances, &mut rows, &env.home, true, &in_flight, budget,
    )
    .await;
    assert!(!rows[1].facts.commands_unavailable);
}

#[test]
fn test_unresolved_shared_prefix_claims_preserve_coverage_without_reading_protected_paths() {
    for (adapter, kind, package_dir) in [
        ("brew", ArtifactKind::Formula, "Cellar/tool/1/bin/tool"),
        (
            "npm",
            ArtifactKind::Package,
            "lib/node_modules/tool/bin/tool",
        ),
    ] {
        let tree = Tree::new("shared-claims");
        tree.file(&format!("prefix/{package_dir}"), 0o755);
        tree.link_keeping_first("prefix/bin/tool", format!("../{package_dir}"));
        let instances = vec![instance(adapter, tree.at("prefix"))];
        let rows = vec![
            artifact(adapter, "tool", kind),
            artifact(adapter, "other", kind),
            artifact(adapter, "app", ArtifactKind::Cask),
        ];
        let check = || {
            let budget = CommandBudget::default();
            let folders = read_folders(
                &[tree.at("prefix/bin")],
                &bin_folders(&instances),
                &tree.root,
                budget,
            );
            judge_with(
                &folders,
                &instances,
                &rows,
                &tree.root,
                true,
                Look::new(&tree.root, budget),
            )
            .unwrap()
        };
        assert!(check().unavailable.is_empty());
        // A link whose first step leaves the prefix for Documents is no
        // formula's or package's (Homebrew and npm link straight into their
        // own folder): every one stays checked (Opus review finding 8).
        tree.link_keeping_first("prefix/bin/hidden", "../../Documents/launcher");
        let (answer, calls) = calls::measure(check);
        assert!(answer.unavailable.is_empty(), "{:?}", answer.unavailable);
        assert_eq!(answer.commands[0][0].state, Some(CommandState::Runs));
        assert!(answer.commands[1].is_empty());
        assert_nothing_protected_looked_at(&calls, &Protected::new(&tree.root));
        // One whose first step goes into `tool`'s own folder, and on from
        // there into Documents: `tool` alone may be missing a command.
        let inside = package_dir.replace("bin/tool", "bin/tool-config");
        let depth = inside.matches('/').count() + 1;
        tree.link_keeping_first(
            &format!("prefix/{inside}"),
            format!("{}Documents/tool-config", "../".repeat(depth)),
        );
        tree.link_keeping_first("prefix/bin/tool-config", format!("../{inside}"));
        let (answer, calls) = calls::measure(check);
        assert_eq!(answer.unavailable, HashSet::from([0]));
        assert_eq!(answer.commands[0][0].state, Some(CommandState::Runs));
        assert!(answer.commands[1].is_empty());
        assert_nothing_protected_looked_at(&calls, &Protected::new(&tree.root));
    }
}

#[test]
fn test_an_unlinked_formulas_keg_is_read_as_its_folders_are_and_never_in_a_protected_place() {
    // r36 V5: an AI coding tool's formula Homebrew did not link has its
    // commands named from its keg (`keg`). Through the round as through
    // `protected::resolve`, the same names; a keg whose `bin` leads into
    // Documents is never looked into, and the formula's names are then
    // said to be incomplete (`commands_unavailable`).
    let tree = Tree::new("unlinked-keg");
    tree.dir("brew/bin");
    tree.file(
        "brew/Cellar/gemini-cli/1/libexec/lib/node_modules/@google/gemini-cli/dist/index.js",
        0o755,
    );
    tree.link_keeping_first(
        "brew/Cellar/gemini-cli/1/libexec/bin/gemini",
        "../lib/node_modules/@google/gemini-cli/dist/index.js",
    );
    tree.link_keeping_first(
        "brew/Cellar/gemini-cli/1/bin/gemini",
        "../libexec/bin/gemini",
    );
    // A file in the keg's `bin` that leads out of the keg is not its.
    tree.file("elsewhere/helper", 0o755);
    tree.link_keeping_first(
        "brew/Cellar/gemini-cli/1/bin/helper",
        "../../../../../elsewhere/helper",
    );
    let instances = vec![instance("brew", tree.at("brew"))];
    let mut gemini = artifact("brew", "gemini-cli", ArtifactKind::Formula);
    gemini.facts.unlinked = true;
    // Unlinked too, but no AI tool's: its keg is not read.
    let mut jq = artifact("brew", "jq", ArtifactKind::Formula);
    jq.facts.unlinked = true;
    tree.file("brew/Cellar/jq/1/bin/jq", 0o755);
    let rows = vec![gemini, jq];
    let budget = CommandBudget::default();
    let folders = read_folders(
        &[tree.at("brew/bin")],
        &bin_folders(&instances),
        &tree.root,
        budget,
    );
    let check = |budget: CommandBudget| {
        judge_with(
            &folders,
            &instances,
            &rows,
            &tree.root,
            true,
            Look::new(&tree.root, budget),
        )
    };
    let (answer, made) = calls::measure(|| check(budget).unwrap());
    assert_eq!(
        answer.commands,
        vec![
            vec![CommandFact {
                name: "gemini".into(),
                state: None
            }],
            vec![]
        ]
    );
    assert!(answer.unavailable.is_empty());
    assert_eq!(
        judged(true, &tree.root, &folders, &instances, &rows, true),
        Some(answer.commands)
    );
    assert_nothing_protected_looked_at(&made, &Protected::new(&tree.root));
    // Its names count against the half's entries, as the first half's do.
    assert!(check(CommandBudget {
        max_entries: 1,
        ..budget
    })
    .is_none());

    // The keg's `bin` a link into Documents: not followed.
    fs::remove_dir_all(tree.at("brew/Cellar/gemini-cli/1/bin")).unwrap();
    tree.file("Documents/kegbin/gemini", 0o755);
    tree.link_keeping_first(
        "brew/Cellar/gemini-cli/1/bin",
        "../../../../Documents/kegbin",
    );
    let (answer, made) = calls::measure(|| check(budget).unwrap());
    assert!(answer.commands[0].is_empty());
    assert_eq!(answer.unavailable, HashSet::from([0]));
    assert_nothing_protected_looked_at(&made, &Protected::new(&tree.root));
}
