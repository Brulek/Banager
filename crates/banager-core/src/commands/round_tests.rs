//! `judge` through one `protected::Round` per judgement, against `judge`
//! through `protected::resolve` itself -- the walk it replaced, from `/`
//! for every path: the same verdicts, far fewer calls, and nothing looked
//! at in a protected place. Every tree is built under the temp folder for
//! the test and removed after it; no tool is run.

use super::*;
use crate::dirfd::calls;
use crate::model::{ArtifactFacts, ArtifactKey, ProvidedCommand};
use crate::protected::DATA_VOLUME;
use crate::testing::manager_instance;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};

/// A fresh folder, canonical (`/var` is a link on a Mac), removed with
/// everything in it -- folders locked by the test unlocked first.
struct Tree {
    root: PathBuf,
    locked: Vec<PathBuf>,
}

impl Tree {
    fn new(tag: &str) -> Tree {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let raw = std::env::temp_dir().join(format!(
            "banager-judge-{tag}-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&raw).unwrap();
        Tree {
            root: fs::canonicalize(raw).unwrap(),
            locked: Vec::new(),
        }
    }

    fn at(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn dir(&self, rel: &str) -> PathBuf {
        let path = self.at(rel);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn file(&self, rel: &str, mode: u32) -> PathBuf {
        let path = self.at(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"never run").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    fn link(&self, rel: &str, target: impl AsRef<Path>) -> PathBuf {
        let path = self.at(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // A name linked twice keeps its first link.
        let _ = symlink(target, &path);
        path
    }

    fn lock(&mut self, rel: &str, mode: u32) {
        let path = self.at(rel);
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        self.locked.push(path);
    }

    /// `rel` under the tree, spelled from the data volume.
    fn on_data_volume(&self, rel: &str) -> PathBuf {
        Path::new(DATA_VOLUME)
            .join(self.root.strip_prefix("/").unwrap())
            .join(rel)
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        for path in self.locked.iter().rev() {
            let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o755));
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn instance(adapter_id: &str, prefix: PathBuf) -> ManagerInstance {
    ManagerInstance {
        prefix,
        ..manager_instance(adapter_id, adapter_id)
    }
}

fn artifact(instance: &str, name: &str, kind: ArtifactKind) -> InstalledArtifact {
    InstalledArtifact {
        key: ArtifactKey {
            instance_id: instance.into(),
            name: name.into(),
            kind,
        },
        display_name: name.rsplit('/').next().unwrap().into(),
        version: "1".into(),
        reason: InstallReason::Requested,
        description: None,
        homepage: None,
        size_bytes: None,
        installed_at: None,
        path: None,
        auto_updates: false,
        uninstall_blocked: None,
        facts: ArtifactFacts::default(),
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
    judge_with(folders, instances, artifacts, home, path_known, look)
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
            tree.link(
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

/// Reproducible numbers, with no new dependency.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }

    /// `rel` with one of its names, picked at random, spelled another
    /// way a Mac's disk takes for the same name: in capitals, or with a
    /// long s, a Kelvin sign or an `st` ligature (`protected::AS_ASCII`).
    fn shout(&mut self, rel: &str) -> String {
        let mut names: Vec<String> = rel.split('/').map(str::to_string).collect();
        let at = self.below(names.len());
        names[at] = match self.below(4) {
            0 => names[at].replace(['s', 'S'], "\u{17F}"),
            1 => names[at].replace(['k', 'K'], "\u{212A}"),
            2 => names[at]
                .replace("st", "\u{FB06}")
                .replace("St", "\u{FB05}"),
            _ => names[at].to_ascii_uppercase(),
        };
        names.join("/")
    }
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
        6 => tree.link(&format!("chain/{name}"), tree.at(real)),
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
    tree.link("alias", "brew/bin");
    tree.link("protected-alias", "Documents/bin");
    tree.link("cycle-a", "cycle-b");
    tree.link("cycle-b", "cycle-a");
    tree.file(".grok/downloads/grok-1.0", 0o755);
    tree.link(".grok/bin/grok", "../downloads/grok-1.0");
    tree.link(
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
                    tree.link(
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
                tree.link(&link, target);
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
                let link = tree.link(&link, target);
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
                tree.link(&link, target);
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
                let link = tree.link(&link, target);
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
                    tree.link(&link, target);
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
    tree.link("cargo/bin/tool", tree.at("Documents/secret"));
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
                tree.link(
                    &format!("{keg}/bin/{name}"),
                    format!("../libexec/bin/{name}"),
                );
            } else {
                tree.file(&format!("{keg}/bin/{name}"), 0o755);
            }
            let bin = if f % 25 == 0 { "sbin" } else { "bin" };
            tree.link(
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
        tree.link(
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
    tree.link("home/.grok/bin/grok", "../downloads/grok-1.0");
    tree.link("home/.grok/bin/agent", "../downloads/grok-1.0");
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
