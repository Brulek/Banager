//! Which other sources run on a Homebrew formula or cask an uninstall would
//! remove (`Warning::NeededBySource`), for the uninstall preview.
//!
//! `brew uses --installed` names the formulae and casks that need a
//! package, and nothing else: not npm, whose `npm` and every global package
//! run on the `node` a Homebrew `node` or `node@22` put on `PATH`; not pip,
//! a module of the Python it runs in; not a pipx or uv tool, whose own
//! environment's Python is a Homebrew `python@3.x`; not Ollama, whose
//! models Homebrew's `ollama` runs. Uninstalling one of those left every
//! tool of that source unable to run, or with nothing in Banager able to
//! update or uninstall it, and Homebrew did not refuse.
//!
//! What runs on a package, for each source of the kinds that run their
//! tools through a program of their own (`HOSTED`):
//! - the source itself, when the program Banager runs for it
//!   (`ManagerInstance::exe_path`) leads, every link followed, into the
//!   package's own folder -- npm's `npm` into a linked `node@22`'s keg,
//!   pip's Python into `python@3.14`'s, Homebrew's `pipx`, `uv` and
//!   `ollama` into their own -- or when the interpreter that program is run
//!   with does: npm's `bin/npm-cli.js` begins `#!/usr/bin/env node`, so the
//!   `node` first on `PATH` (`interpreter`), which is a Homebrew `node`'s
//!   even where npm itself is a copy outside its keg;
//! - some of its tools, for pipx and uv, each of whose tools has an
//!   environment of its own (`InstalledArtifact::path`): those whose
//!   `bin/python` leads into it.
//!
//! A package's own folder: a formula's keg folder, `<prefix>/Cellar/<name>`
//! -- its `opt/<name>` link and every alias Homebrew keeps in `opt`
//! (`python@3`, `python3`) lead into it, so a path spelled through `opt` is
//! followed there -- or a cask's folder, `<prefix>/Caskroom/<token>`, and the
//! app it moved into place, into which its `binary` links lead
//! (`/opt/homebrew/bin/ollama` → `/Applications/Ollama.app/…`). A keg-only
//! `node@22` that is not linked is nobody's `node`: npm's `npm` and the
//! `node` on `PATH` lead into another keg, or none, and nothing is said of
//! it.
//!
//! Which tools count (`counts`): every one the source lists, but what it
//! lists of its own program and what was installed for another package --
//! npm's `npm` and `corepack`, which Node.js ships with npm (on the
//! author's Mac they live in the `node@22` keg, linked into
//! `lib/node_modules`); the `pip`, `setuptools` and `wheel` Homebrew's
//! Python formulae install into `site-packages` themselves (`post_install`,
//! python@3.14.rb:301-317 in its keg's `.brew` copy; python@3.11.rb adds
//! setuptools); and pip's dependencies (`InstallReason::Dependency`). A
//! source with none that count loses nothing, and is not named: Node.js can
//! be uninstalled with only its own npm left.
//!
//! How: read-only, as the command check is (`protected::resolve`: `lstat`
//! and `readlink` of each step, from the folder before it held open, and
//! never a step into one of the places macOS asks about first, nor onto
//! another disk), plus the same `PATH` look `resolve_exe` makes for the
//! interpreter. Only folders are opened, to follow each link; no file's
//! contents are read, nothing is written and no command runs.
//! Bounded (`BUDGET`): a look it stopped short of is reported as one that
//! did not finish (`NeededBy::complete`), never as "nothing runs on it".
//! So is one that met a path it may not or cannot follow (`Look::unknown`):
//! a source's program, a tool's environment or a `PATH` folder that is, or
//! leads into, a protected place, one on the way to which a folder could
//! not be searched, and a pipx or uv tool with no environment Banager
//! knows of. What is there is not known, so neither is whether it runs on
//! the package; a path that is known not to be there (`Missing`) is known
//! not to.
//! Run on the blocking pool by `Session::issue_plan`
//! (`session/needed_by.rs`).

use crate::model::{ArtifactKind, InstallReason, InstalledArtifact, ManagerInstance, Warning};
use crate::protected::{self, Protected, Resolution};
use crate::runner::HostEnv;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

/// The kinds of source whose tools run through a program of the source's
/// own, which a Homebrew package can be: the package managers. Not
/// Homebrew itself, whose dependents are `brew uses`'s, nor a tool with its
/// own installer, whose one row is the tool.
pub const HOSTED: [&str; 6] = ["npm", "pip", "pipx", "uv", "cargo", "ollama"];

/// The interpreter a source's program is run with, found on `PATH` as
/// `env` finds it: npm's `bin/npm-cli.js` begins `#!/usr/bin/env node`
/// (npm 10.9.9 and 12.0.2 alike).
fn interpreter(adapter_id: &str) -> Option<&'static str> {
    (adapter_id == "npm").then_some("node")
}

/// What a source lists of its own program: npm's `npm` and `corepack`,
/// and the `pip`, `setuptools` and `wheel` of a Homebrew Python, spelled
/// as PyPI normalizes a name (`normalized`).
fn comes_with_program(adapter_id: &str) -> &'static [&'static str] {
    match adapter_id {
        "npm" => &["npm", "corepack"],
        "pip" => &["pip", "setuptools", "wheel"],
        _ => &[],
    }
}

/// The sources each of whose tools has an environment of its own, at the
/// tool's `path`, run by that environment's `bin/python`: pipx's venvs, uv's
/// tool environments.
fn has_environments(adapter_id: &str) -> bool {
    matches!(adapter_id, "pipx" | "uv")
}

/// Whether `source`'s tools run on this Mac at all. An Ollama whose
/// `OLLAMA_HOST` names another machine keeps its models, and runs them,
/// there: the `ollama` here is only a client, and uninstalling it costs
/// those models nothing (`models_on_this_mac`).
fn runs_here(source: &ManagerInstance) -> bool {
    source.adapter_id != "ollama" || crate::adapters::ollama::models_on_this_mac(source)
}

/// `name` as PyPI compares names (PEP 503): lower case, with `_` and `.`
/// read as `-`. Harmless for npm's names, which are lower case already.
fn normalized(name: &str) -> String {
    name.to_ascii_lowercase().replace(['_', '.'], "-")
}

/// Whether `tool`, one of `adapter_id`'s rows, counts as a tool that needs
/// the package its source runs on (the module doc says which do not).
fn counts(adapter_id: &str, tool: &InstalledArtifact) -> bool {
    tool.reason != InstallReason::Dependency
        && !comes_with_program(adapter_id).contains(&normalized(&tool.key.name).as_str())
}

/// How much one preview may look at, and for how long, before it gives up
/// on what it has not reached: links followed one path at a time, each
/// path one look. A Mac's sources take a few dozen; a pipx or uv with
/// hundreds of tools, a few hundred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub max_looks: usize,
    pub max_duration: Duration,
}

/// What one uninstall preview may spend: 2,000 paths, 1 second.
pub const BUDGET: Budget = Budget {
    max_looks: 2_000,
    max_duration: Duration::from_secs(1),
};

/// What `needed_by` found: a `Warning::NeededBySource` for each source that
/// runs on the package, and whether the look finished. One that did not --
/// the budget ran out, the package's own folder could not be read, or a
/// path that could have led into it was not followed (`Look::unknown`) --
/// is no proof that nothing else does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeededBy {
    pub warnings: Vec<Warning>,
    pub complete: bool,
}

/// The sources of `instances` that run on `package`, a formula or cask of
/// `brew`, and how many of each one's tools (`artifacts`) need it, in
/// `instances`' order (the module doc says what counts). `env` is the one
/// the last refresh had: its home folder decides the places never looked
/// into, its `PATH` which `node` npm is run with. Empty for any other kind
/// of package. Blocking: run it on the blocking pool.
pub fn needed_by(
    package: &InstalledArtifact,
    brew: &ManagerInstance,
    instances: &[ManagerInstance],
    artifacts: &[InstalledArtifact],
    env: &HostEnv,
    budget: Budget,
) -> NeededBy {
    let mut look = Look::new(&env.home, budget);
    let Some(roots) = own_folders(package, brew, &mut look) else {
        return NeededBy {
            warnings: Vec::new(),
            complete: false,
        };
    };
    let mut warnings = Vec::new();
    if roots.is_empty() {
        return NeededBy {
            warnings,
            complete: true,
        };
    }
    for source in instances {
        if source.id == brew.id
            || !HOSTED.contains(&source.adapter_id.as_str())
            || !runs_here(source)
        {
            continue;
        }
        let tools: Vec<&InstalledArtifact> = artifacts
            .iter()
            .filter(|tool| tool.key.instance_id == source.id && counts(&source.adapter_id, tool))
            .collect();
        if tools.is_empty() {
            continue;
        }
        let program = look.leads_into(&source.exe_path, &roots)
            || interpreter(&source.adapter_id)
                .and_then(|name| look.on_path(name, env))
                .is_some_and(|found| look.leads_into(&found, &roots));
        if program {
            warnings.push(Warning::NeededBySource {
                instance_id: source.id.clone(),
                program: true,
                tools: tools.len(),
            });
            continue;
        }
        if has_environments(&source.adapter_id) {
            let on_it = tools
                .iter()
                .filter(|tool| match tool.path.as_deref() {
                    Some(environment) => {
                        look.leads_into(&environment.join("bin").join("python"), &roots)
                    }
                    // No environment to look at: whether its Python is
                    // the package's is not known.
                    None => {
                        look.unknown = true;
                        false
                    }
                })
                .count();
            if on_it > 0 {
                warnings.push(Warning::NeededBySource {
                    instance_id: source.id.clone(),
                    program: false,
                    tools: on_it,
                });
            }
        }
    }
    NeededBy {
        warnings,
        complete: !look.over && !look.unknown,
    }
}

/// The folders whose files are `package`'s, every link followed: a
/// formula's keg folder, a cask's folder in `Caskroom` and its app. Empty
/// for any other kind of package; `None` when `brew`'s `Cellar` or
/// `Caskroom` could not be read, or the budget ran out first.
fn own_folders(
    package: &InstalledArtifact,
    brew: &ManagerInstance,
    look: &mut Look,
) -> Option<Vec<PathBuf>> {
    let folder = match package.key.kind {
        ArtifactKind::Formula => "Cellar",
        ArtifactKind::Cask => "Caskroom",
        _ => return Some(Vec::new()),
    };
    // `Cellar/<name>`, `Caskroom/<token>`: the short name, which a tapped
    // package's key (`user/tap/name`) ends with.
    let Some(short) = short_name(&package.key.name) else {
        return Some(Vec::new());
    };
    let mut roots = vec![look.resolve(&brew.prefix.join(folder))?.join(short)];
    if package.key.kind == ArtifactKind::Cask {
        if let Some(app) = package.path.as_deref().filter(|path| is_app(path)) {
            roots.extend(look.resolve(app));
        }
    }
    Some(roots)
}

/// The last part of a Homebrew name, when it names one folder: not empty,
/// `.` or `..`.
fn short_name(name: &str) -> Option<&str> {
    let short = name.rsplit('/').next()?;
    let mut parts = Path::new(short).components();
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(_)), None) => Some(short),
        _ => None,
    }
}

/// Whether `path` names an app bundle, `/Applications/Ollama.app`: what a
/// cask's `path` is (`parse_info_installed`), and nothing broader -- a
/// folder such as `/Applications` would claim every app in it.
fn is_app(path: &Path) -> bool {
    path.is_absolute()
        && path.components().count() > 2
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
}

/// Where each path leads, and what has been spent on finding out.
struct Look {
    protected: Protected,
    started: Instant,
    looks: usize,
    budget: Budget,
    over: bool,
    /// Whether a path was met whose end is not known: one that is, or
    /// leads into, a protected place, which is never looked into; one on
    /// the way to which a folder could not be searched, or was replaced
    /// while it was looked at (`Resolution::Refused`); or a tool with no
    /// environment path. It may lead into the package, so the look did not
    /// finish. A path that is not there (`Resolution::Missing`) is known
    /// to lead nowhere, and is not this.
    unknown: bool,
}

impl Look {
    fn new(home: &Path, budget: Budget) -> Look {
        Look {
            protected: Protected::new(home),
            started: Instant::now(),
            looks: 0,
            budget,
            over: false,
            unknown: false,
        }
    }

    /// Whether the budget allows one more look, counting it if so.
    fn one_more(&mut self) -> bool {
        if self.looks >= self.budget.max_looks || self.started.elapsed() >= self.budget.max_duration
        {
            self.over = true;
        }
        if !self.over {
            self.looks += 1;
        }
        !self.over
    }

    /// Where `path` leads, every link followed and never into a protected
    /// place (`protected::resolve`); `None` for one that leads nowhere or
    /// into such a place, or that the budget did not reach. One that leads
    /// into such a place, or that could not be followed, is `unknown`.
    fn resolve(&mut self, path: &Path) -> Option<PathBuf> {
        if !self.one_more() {
            return None;
        }
        match protected::resolve(path, &self.protected, true) {
            Resolution::Found(real, _) => Some(real),
            Resolution::Missing => None,
            Resolution::Protected(_) | Resolution::Refused => {
                self.unknown = true;
                None
            }
        }
    }

    /// Whether `path` leads, every link followed, into one of `roots`.
    fn leads_into(&mut self, path: &Path, roots: &[PathBuf]) -> bool {
        self.resolve(path).is_some_and(|real| {
            let real = protected::without_data_volume(&real);
            roots.iter().any(|root| {
                protected::starts_with_folded(&real, &protected::without_data_volume(root))
            })
        })
    }

    /// The first `name` on `env`'s `PATH`, as `env` would find it: what
    /// `resolve_exe` answers, each `PATH` folder looked at the same way
    /// (`protected::resolve`) and one look each. A folder passed over that
    /// is, or leads into, a protected place, or could not be searched, may
    /// hold the `name` `env` would run: the answer is `unknown` then, and
    /// the one after it is still the one returned, as `resolve_exe`'s. A
    /// relative folder is passed over as `resolve_exe` passes it over: it
    /// names a folder only from wherever the program is run.
    fn on_path(&mut self, name: &str, env: &HostEnv) -> Option<PathBuf> {
        for dir in env.path_dirs.iter().filter(|dir| dir.is_absolute()) {
            let candidate = dir.join(name);
            if !self.one_more() {
                return None;
            }
            match protected::resolve(&candidate, &self.protected, true) {
                Resolution::Found(_, meta) if meta.is_file() => return Some(candidate),
                Resolution::Found(..) | Resolution::Missing => {}
                Resolution::Protected(_) | Resolution::Refused => self.unknown = true,
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ArtifactKey, InstanceStatus, Scope};
    use std::os::unix::fs::{symlink, PermissionsExt};

    /// A folder of a test's own, standing in for `/`: a Homebrew prefix at
    /// `opt/homebrew`, a home folder at `home`, an app folder at
    /// `Applications`. Removed when dropped.
    struct Root(PathBuf);

    impl Root {
        fn new(tag: &str) -> Root {
            let dir = std::env::temp_dir().join(format!(
                "banager-needed-by-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Root(std::fs::canonicalize(&dir).unwrap())
        }

        fn path(&self, relative: &str) -> PathBuf {
            self.0.join(relative)
        }

        /// An executable file at `relative`.
        fn program(&self, relative: &str) -> PathBuf {
            let path = self.path(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }

        /// A link at `relative` whose text is `target`, as written.
        fn link(&self, relative: &str, target: &str) -> PathBuf {
            let path = self.path(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            symlink(target, &path).unwrap();
            path
        }

        fn dir(&self, relative: &str) -> PathBuf {
            let path = self.path(relative);
            std::fs::create_dir_all(&path).unwrap();
            path
        }

        fn prefix(&self) -> PathBuf {
            self.path("opt/homebrew")
        }

        fn home(&self) -> PathBuf {
            self.dir("home")
        }

        /// A `HostEnv` whose `PATH` is these folders, under this root.
        fn env(&self, path: &[&str]) -> HostEnv {
            HostEnv {
                path_dirs: path.iter().map(|dir| self.path(dir)).collect(),
                home: self.home(),
                euid: 501,
                cargo_home: None,
                rustup_home: None,
                zdotdir: None,
                ollama_host: None,
            }
        }

        /// Homebrew's `python@3.13` keg, linked as `bin/python3.13` and with
        /// its `opt` link, as Homebrew lays it out: the program inside the
        /// framework, `bin/python3.13` a link to it.
        fn python_313(&self) {
            self.program(
                "opt/homebrew/Cellar/python@3.13/3.13.15/Frameworks/Python.framework/Versions/3.13/bin/python3.13",
            );
            self.link(
                "opt/homebrew/Cellar/python@3.13/3.13.15/bin/python3.13",
                "../Frameworks/Python.framework/Versions/3.13/bin/python3.13",
            );
            self.link(
                "opt/homebrew/opt/python@3.13",
                "../Cellar/python@3.13/3.13.15",
            );
            self.link(
                "opt/homebrew/bin/python3.13",
                "../Cellar/python@3.13/3.13.15/bin/python3.13",
            );
        }

        /// A keg-only `node@22`: its `node`, and npm inside it, `bin/npm` a
        /// link to npm's `npm-cli.js`, as the formula lays them out; and
        /// its `opt` link. Not linked into `bin` until `link_node_22`.
        fn node_22(&self) {
            self.program("opt/homebrew/Cellar/node@22/22.23.3/bin/node");
            self.program("opt/homebrew/Cellar/node@22/22.23.3/lib/node_modules/npm/bin/npm-cli.js");
            self.link(
                "opt/homebrew/Cellar/node@22/22.23.3/bin/npm",
                "../lib/node_modules/npm/bin/npm-cli.js",
            );
            self.link("opt/homebrew/opt/node@22", "../Cellar/node@22/22.23.3");
        }

        /// `brew link --force node@22`, as on the author's Mac on 2026-10-02.
        fn link_node_22(&self) {
            self.link(
                "opt/homebrew/bin/node",
                "../Cellar/node@22/22.23.3/bin/node",
            );
            self.link("opt/homebrew/bin/npm", "../Cellar/node@22/22.23.3/bin/npm");
        }

        /// Homebrew's `node`, linked: its `bin/npm` an absolute link to the
        /// copy of npm its `post_install` puts in the prefix's
        /// `lib/node_modules`, outside the keg.
        fn node_linked(&self) {
            self.program("opt/homebrew/Cellar/node/24.9.0/bin/node");
            self.program("opt/homebrew/lib/node_modules/npm/bin/npm-cli.js");
            let copy = self.path("opt/homebrew/lib/node_modules/npm/bin/npm-cli.js");
            self.link(
                "opt/homebrew/Cellar/node/24.9.0/bin/npm",
                copy.to_str().unwrap(),
            );
            self.link("opt/homebrew/bin/node", "../Cellar/node/24.9.0/bin/node");
            let keg_npm = self.path("opt/homebrew/Cellar/node/24.9.0/bin/npm");
            self.link("opt/homebrew/bin/npm", keg_npm.to_str().unwrap());
        }
    }

    impl Drop for Root {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn instance(adapter_id: &str, id: &str, exe_path: PathBuf, prefix: PathBuf) -> ManagerInstance {
        ManagerInstance {
            id: id.to_string(),
            adapter_id: adapter_id.to_string(),
            exe_path,
            prefix,
            scope: Scope::User,
            version: Some("1.0".to_string()),
            answered_at: None,
            unverified_version: None,
            read_only_reason: None,
            status: InstanceStatus::default(),
        }
    }

    fn row(instance_id: &str, kind: ArtifactKind, name: &str) -> InstalledArtifact {
        InstalledArtifact {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind,
                name: name.to_string(),
            },
            display_name: name.to_string(),
            version: "1.0".to_string(),
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path: None,
            auto_updates: false,
            uninstall_blocked: None,
            facts: Default::default(),
        }
    }

    fn with_path(mut artifact: InstalledArtifact, path: PathBuf) -> InstalledArtifact {
        artifact.path = Some(path);
        artifact
    }

    fn with_reason(mut artifact: InstalledArtifact, reason: InstallReason) -> InstalledArtifact {
        artifact.reason = reason;
        artifact
    }

    const BREW: &str = "brew:/opt/homebrew";
    const NPM: &str = "npm:/opt/homebrew";

    fn brew(root: &Root) -> ManagerInstance {
        instance(
            "brew",
            BREW,
            root.path("opt/homebrew/bin/brew"),
            root.prefix(),
        )
    }

    fn formula(name: &str) -> InstalledArtifact {
        row(BREW, ArtifactKind::Formula, name)
    }

    fn needed(warnings: &[Warning]) -> Vec<(String, bool, usize)> {
        warnings
            .iter()
            .map(|warning| match warning {
                Warning::NeededBySource {
                    instance_id,
                    program,
                    tools,
                } => (instance_id.clone(), *program, *tools),
                other => panic!("expected NeededBySource, got {other:?}"),
            })
            .collect()
    }

    /// npm's global packages as `npm ls -g` listed them on the author's Mac
    /// (`adapters/fixtures/npm/12.0.2/ls-global.json`): four of the user's,
    /// and npm's own `npm` and `corepack`.
    fn npm_rows() -> Vec<InstalledArtifact> {
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
            .expect("read adapters/fixtures/npm/12.0.2/ls-global.json");
        crate::adapters::npm::parse_ls_global(&json, NPM).expect("parse")
    }

    #[test]
    fn test_a_linked_node_22_that_npm_runs_on_is_needed_by_npm_and_its_four_tools() {
        let root = Root::new("linked");
        root.node_22();
        root.link_node_22();
        let npm = instance("npm", NPM, root.path("opt/homebrew/bin/npm"), root.prefix());
        let instances = vec![brew(&root), npm];
        let found = needed_by(
            &formula("node@22"),
            &instances[0],
            &instances,
            &npm_rows(),
            &root.env(&["opt/homebrew/bin", "usr/bin"]),
            BUDGET,
        );
        assert!(found.complete);
        // The recorded list's six packages, less npm's own two.
        assert_eq!(needed(&found.warnings), vec![(NPM.to_string(), true, 4)]);
        // A formula whose name `node@22` merely starts with is another
        // folder: folders are compared whole.
        root.dir("opt/homebrew/Cellar/node@2/1");
        let near = needed_by(
            &formula("node@2"),
            &instances[0],
            &instances,
            &npm_rows(),
            &root.env(&["opt/homebrew/bin", "usr/bin"]),
            BUDGET,
        );
        assert_eq!(
            near,
            NeededBy {
                warnings: Vec::new(),
                complete: true
            }
        );
    }

    #[test]
    fn test_a_keg_only_node_22_that_is_not_linked_is_nobodys() {
        // Homebrew's `node` is what npm runs, and the `node` on `PATH`;
        // `node@22` beside it, keg-only, is not linked.
        let root = Root::new("unlinked");
        root.node_22();
        root.node_linked();
        let npm = instance("npm", NPM, root.path("opt/homebrew/bin/npm"), root.prefix());
        let instances = vec![brew(&root), npm];
        let env = root.env(&["opt/homebrew/bin"]);
        let node_22 = needed_by(
            &formula("node@22"),
            &instances[0],
            &instances,
            &npm_rows(),
            &env,
            BUDGET,
        );
        assert_eq!(
            node_22,
            NeededBy {
                warnings: Vec::new(),
                complete: true
            }
        );
        // `node` itself is: npm's `npm` is a copy outside its keg, but the
        // `node` npm is run with is in it.
        let node = needed_by(
            &formula("node"),
            &instances[0],
            &instances,
            &npm_rows(),
            &env,
            BUDGET,
        );
        assert_eq!(needed(&node.warnings), vec![(NPM.to_string(), true, 4)]);
    }

    #[test]
    fn test_an_npm_whose_node_is_not_homebrews_needs_neither_formula() {
        // An npm and a node of their own (nvm's layout), first on `PATH`.
        let root = Root::new("nvm");
        root.node_22();
        root.link_node_22();
        root.program("home/.nvm/versions/node/v22.20.0/bin/node");
        root.program("home/.nvm/versions/node/v22.20.0/lib/node_modules/npm/bin/npm-cli.js");
        root.link(
            "home/.nvm/versions/node/v22.20.0/bin/npm",
            "../lib/node_modules/npm/bin/npm-cli.js",
        );
        let npm = instance(
            "npm",
            "npm:nvm",
            root.path("home/.nvm/versions/node/v22.20.0/bin/npm"),
            root.path("home/.nvm/versions/node/v22.20.0"),
        );
        let rows: Vec<InstalledArtifact> = npm_rows()
            .into_iter()
            .map(|mut artifact| {
                artifact.key.instance_id = "npm:nvm".to_string();
                artifact
            })
            .collect();
        let instances = vec![brew(&root), npm];
        let env = root.env(&["home/.nvm/versions/node/v22.20.0/bin", "opt/homebrew/bin"]);
        let found = needed_by(
            &formula("node@22"),
            &instances[0],
            &instances,
            &rows,
            &env,
            BUDGET,
        );
        assert_eq!(
            found,
            NeededBy {
                warnings: Vec::new(),
                complete: true
            }
        );
    }

    #[test]
    fn test_an_npm_with_nothing_but_its_own_packages_loses_nothing() {
        let root = Root::new("bare");
        root.node_22();
        root.link_node_22();
        let npm = instance("npm", NPM, root.path("opt/homebrew/bin/npm"), root.prefix());
        let own: Vec<InstalledArtifact> = npm_rows()
            .into_iter()
            .filter(|artifact| artifact.key.name == "npm" || artifact.key.name == "corepack")
            .collect();
        let instances = vec![brew(&root), npm];
        let found = needed_by(
            &formula("node@22"),
            &instances[0],
            &instances,
            &own,
            &root.env(&["opt/homebrew/bin"]),
            BUDGET,
        );
        assert_eq!(
            found,
            NeededBy {
                warnings: Vec::new(),
                complete: true
            }
        );
    }

    #[test]
    fn test_pipx_venvs_whose_python_leads_into_python_313_through_opt_or_cellar_need_it() {
        let root = Root::new("venvs");
        root.python_313();
        // A uv-managed Python, not Homebrew's.
        root.program(
            "home/.local/share/uv/python/cpython-3.13.7-macos-aarch64-none/bin/python3.13",
        );
        let opt = root.path("opt/homebrew/opt/python@3.13/bin/python3.13");
        let cellar = root.path("opt/homebrew/Cellar/python@3.13/3.13.15/bin/python3.13");
        let uv_python = root
            .path("home/.local/share/uv/python/cpython-3.13.7-macos-aarch64-none/bin/python3.13");
        let venvs = "home/.local/pipx/venvs";
        root.link(&format!("{venvs}/httpie/bin/python"), opt.to_str().unwrap());
        root.link(
            &format!("{venvs}/poetry/bin/python"),
            cellar.to_str().unwrap(),
        );
        root.link(
            &format!("{venvs}/black/bin/python"),
            uv_python.to_str().unwrap(),
        );
        // A venv whose Python is gone: nothing there to say it is 3.13's.
        root.link(
            &format!("{venvs}/gone/bin/python"),
            "/nonexistent/python3.13",
        );
        // Homebrew's pipx itself runs on its own keg, not on python@3.13.
        root.program("opt/homebrew/Cellar/pipx/1.17.3/libexec/bin/pipx");
        root.link(
            "opt/homebrew/Cellar/pipx/1.17.3/bin/pipx",
            "../libexec/bin/pipx",
        );
        root.link("opt/homebrew/bin/pipx", "../Cellar/pipx/1.17.3/bin/pipx");
        let pipx = instance(
            "pipx",
            "pipx",
            root.path("opt/homebrew/bin/pipx"),
            root.path("opt/homebrew/bin"),
        );
        let tools: Vec<InstalledArtifact> = ["httpie", "poetry", "black", "gone"]
            .into_iter()
            .map(|name| {
                with_path(
                    row("pipx", ArtifactKind::Tool, name),
                    root.path(&format!("{venvs}/{name}")),
                )
            })
            // One with no app, so no environment Banager knows of.
            .chain([row("pipx", ArtifactKind::Tool, "no-app")])
            .collect();
        let instances = vec![brew(&root), pipx];
        let env = root.env(&["opt/homebrew/bin"]);
        let found = needed_by(
            &formula("python@3.13"),
            &instances[0],
            &instances,
            &tools,
            &env,
            BUDGET,
        );
        assert_eq!(
            needed(&found.warnings),
            vec![("pipx".to_string(), false, 2)]
        );
        // `no-app`'s environment may run on it too, for all Banager knows:
        // the look did not finish. Without it, it did.
        assert!(!found.complete);
        let known = needed_by(
            &formula("python@3.13"),
            &instances[0],
            &instances,
            &tools[..4],
            &env,
            BUDGET,
        );
        assert_eq!(known.warnings, found.warnings);
        assert!(known.complete);
        // Homebrew's `pipx` is what the pipx source runs: all five need it.
        let pipx_formula = needed_by(
            &formula("pipx"),
            &instances[0],
            &instances,
            &tools,
            &env,
            BUDGET,
        );
        assert_eq!(
            needed(&pipx_formula.warnings),
            vec![("pipx".to_string(), true, 5)]
        );
    }

    #[test]
    fn test_pip_runs_on_its_python_and_counts_what_the_user_installed() {
        let root = Root::new("pip");
        root.python_313();
        let pip = instance(
            "pip",
            "pip:python3.13",
            root.path("opt/homebrew/bin/python3.13"),
            root.path("opt/homebrew/bin"),
        );
        let rows = vec![
            with_reason(
                row("pip:python3.13", ArtifactKind::Package, "pip"),
                InstallReason::Unknown,
            ),
            with_reason(
                row("pip:python3.13", ArtifactKind::Package, "wheel"),
                InstallReason::Unknown,
            ),
            with_reason(
                row("pip:python3.13", ArtifactKind::Package, "setuptools"),
                InstallReason::Unknown,
            ),
            with_reason(
                row("pip:python3.13", ArtifactKind::Package, "requests"),
                InstallReason::Unknown,
            ),
            with_reason(
                row("pip:python3.13", ArtifactKind::Package, "PyYAML"),
                InstallReason::Unknown,
            ),
            with_reason(
                row("pip:python3.13", ArtifactKind::Package, "urllib3"),
                InstallReason::Dependency,
            ),
        ];
        let instances = vec![brew(&root), pip];
        let found = needed_by(
            &formula("python@3.13"),
            &instances[0],
            &instances,
            &rows,
            &root.env(&["opt/homebrew/bin"]),
            BUDGET,
        );
        assert_eq!(
            needed(&found.warnings),
            vec![("pip:python3.13".to_string(), true, 2)]
        );
    }

    #[test]
    fn test_ollamas_app_cask_and_formula_each_need_their_models_kept() {
        let root = Root::new("ollama");
        root.dir("opt/homebrew/Cellar");
        root.dir("opt/homebrew/Caskroom/ollama-app/0.34.1");
        root.program("Applications/Ollama.app/Contents/Resources/ollama");
        let resources = root.path("Applications/Ollama.app/Contents/Resources/ollama");
        root.link("opt/homebrew/bin/ollama", resources.to_str().unwrap());
        let ollama = instance(
            "ollama",
            "ollama:http://127.0.0.1:11434",
            root.path("opt/homebrew/bin/ollama"),
            root.path("home/.ollama"),
        );
        let models = vec![
            row(&ollama.id, ArtifactKind::Model, "llama3.2:3b"),
            row(&ollama.id, ArtifactKind::Model, "qwen3.8:27b-mlx"),
        ];
        let instances = vec![brew(&root), ollama.clone()];
        let env = root.env(&["opt/homebrew/bin"]);
        let cask = with_path(
            row(BREW, ArtifactKind::Cask, "ollama-app"),
            root.path("Applications/Ollama.app"),
        );
        let found = needed_by(&cask, &instances[0], &instances, &models, &env, BUDGET);
        assert_eq!(needed(&found.warnings), vec![(ollama.id.clone(), true, 2)]);
        // Another app is not Ollama's.
        let other = with_path(
            row(BREW, ArtifactKind::Cask, "iterm2"),
            root.dir("Applications/iTerm.app"),
        );
        assert!(
            needed_by(&other, &instances[0], &instances, &models, &env, BUDGET)
                .warnings
                .is_empty()
        );
        // A cask whose path is a whole folder of apps claims none of them.
        let broad = with_path(
            row(BREW, ArtifactKind::Cask, "odd"),
            root.path("Applications"),
        );
        assert!(
            needed_by(&broad, &instances[0], &instances, &models, &env, BUDGET)
                .warnings
                .is_empty()
        );
        // With the formula instead, its `ollama` linked from its keg: the
        // formula is the one the Ollama source runs on, and the cask is not.
        std::fs::remove_file(root.path("opt/homebrew/bin/ollama")).unwrap();
        root.program("opt/homebrew/Cellar/ollama/0.34.1/bin/ollama");
        root.link(
            "opt/homebrew/bin/ollama",
            "../Cellar/ollama/0.34.1/bin/ollama",
        );
        let found = needed_by(
            &formula("ollama"),
            &instances[0],
            &instances,
            &models,
            &env,
            BUDGET,
        );
        assert_eq!(needed(&found.warnings), vec![(ollama.id.clone(), true, 2)]);
        assert!(
            needed_by(&cask, &instances[0], &instances, &models, &env, BUDGET)
                .warnings
                .is_empty()
        );
    }

    #[test]
    fn test_an_ollama_whose_models_are_on_another_machine_needs_nothing_here() {
        let root = Root::new("ollama-remote");
        root.program("opt/homebrew/Cellar/ollama/0.34.1/bin/ollama");
        root.link(
            "opt/homebrew/bin/ollama",
            "../Cellar/ollama/0.34.1/bin/ollama",
        );
        let env = root.env(&["opt/homebrew/bin"]);
        let models_of =
            |ollama: &ManagerInstance| vec![row(&ollama.id, ArtifactKind::Model, "llama3.2:3b")];
        // `OLLAMA_HOST` names another machine: its models are there, and the
        // `ollama` this formula put here is only a client.
        let remote = instance(
            "ollama",
            "ollama:http://studio.local:11434",
            root.path("opt/homebrew/bin/ollama"),
            root.path("home/.ollama"),
        );
        let instances = vec![brew(&root), remote.clone()];
        let found = needed_by(
            &formula("ollama"),
            &instances[0],
            &instances,
            &models_of(&remote),
            &env,
            BUDGET,
        );
        assert!(found.warnings.is_empty());
        assert!(found.complete);
        // The same Mac spelled another way still counts.
        let local = instance(
            "ollama",
            "ollama:http://localhost:11434",
            root.path("opt/homebrew/bin/ollama"),
            root.path("home/.ollama"),
        );
        let instances = vec![brew(&root), local.clone()];
        let found = needed_by(
            &formula("ollama"),
            &instances[0],
            &instances,
            &models_of(&local),
            &env,
            BUDGET,
        );
        assert_eq!(needed(&found.warnings), vec![(local.id.clone(), true, 1)]);
    }

    #[test]
    fn test_a_tool_environment_in_a_protected_place_is_not_looked_into() {
        // `~/Documents` is a place macOS asks about before an app reads it:
        // its environment is not looked into, and so not counted -- nor
        // taken for one that does not run on it: the look did not finish.
        let root = Root::new("protected");
        root.python_313();
        let opt = root.path("opt/homebrew/opt/python@3.13/bin/python3.13");
        root.link(
            "home/Documents/venvs/tool/bin/python",
            opt.to_str().unwrap(),
        );
        let uv = instance(
            "uv",
            "uv",
            root.path("home/.local/bin/uv"),
            root.path("home/.local/bin"),
        );
        let tools = vec![with_path(
            row("uv", ArtifactKind::Tool, "tool"),
            root.path("home/Documents/venvs/tool"),
        )];
        let instances = vec![brew(&root), uv];
        let found = needed_by(
            &formula("python@3.13"),
            &instances[0],
            &instances,
            &tools,
            &root.env(&["opt/homebrew/bin"]),
            BUDGET,
        );
        assert_eq!(
            found,
            NeededBy {
                warnings: Vec::new(),
                complete: false
            }
        );
    }

    #[test]
    fn test_a_look_the_budget_cut_short_is_not_finished() {
        let root = Root::new("budget");
        root.node_22();
        root.link_node_22();
        let npm = instance("npm", NPM, root.path("opt/homebrew/bin/npm"), root.prefix());
        let instances = vec![brew(&root), npm];
        for budget in [
            Budget {
                max_looks: 0,
                max_duration: Duration::from_secs(1),
            },
            Budget {
                max_looks: 1,
                max_duration: Duration::from_secs(1),
            },
            Budget {
                max_looks: 2_000,
                max_duration: Duration::ZERO,
            },
        ] {
            let found = needed_by(
                &formula("node@22"),
                &instances[0],
                &instances,
                &npm_rows(),
                &root.env(&["opt/homebrew/bin"]),
                budget,
            );
            assert!(!found.complete, "{budget:?}");
            assert!(found.warnings.is_empty(), "{budget:?}");
        }
    }

    #[test]
    fn test_only_a_formula_or_cask_of_a_readable_homebrew_is_looked_at() {
        let root = Root::new("kinds");
        root.node_22();
        root.link_node_22();
        let npm = instance("npm", NPM, root.path("opt/homebrew/bin/npm"), root.prefix());
        let instances = vec![brew(&root), npm];
        let env = root.env(&["opt/homebrew/bin"]);
        // An npm package is no Homebrew package.
        let package = row(NPM, ArtifactKind::Package, "node@22");
        assert_eq!(
            needed_by(
                &package,
                &instances[0],
                &instances,
                &npm_rows(),
                &env,
                BUDGET
            ),
            NeededBy {
                warnings: Vec::new(),
                complete: true
            }
        );
        // A name that names no single folder.
        for name in ["..", "user/tap/..", ".", ""] {
            assert_eq!(
                needed_by(
                    &formula(name),
                    &instances[0],
                    &instances,
                    &npm_rows(),
                    &env,
                    BUDGET
                ),
                NeededBy {
                    warnings: Vec::new(),
                    complete: true
                },
                "{name:?}"
            );
        }
        // A tapped formula is found by its short name.
        assert_eq!(
            needed(
                &needed_by(
                    &formula("homebrew/core/node@22"),
                    &instances[0],
                    &instances,
                    &npm_rows(),
                    &env,
                    BUDGET
                )
                .warnings
            ),
            vec![(NPM.to_string(), true, 4)]
        );
        // A Homebrew whose `Cellar` cannot be read: no answer, not "nothing".
        let gone = instance("brew", BREW, root.path("gone/bin/brew"), root.path("gone"));
        let instances = vec![gone, instances[1].clone()];
        assert_eq!(
            needed_by(
                &formula("node@22"),
                &instances[0],
                &instances,
                &npm_rows(),
                &env,
                BUDGET
            ),
            NeededBy {
                warnings: Vec::new(),
                complete: false
            }
        );
    }

    #[test]
    fn test_a_tool_environment_banager_may_not_or_cannot_look_into_leaves_the_look_unfinished() {
        // A pipx whose own Python is not python@3.13, with tools whose
        // environments may run on it: one kept in `~/Documents` -- its
        // `bin/python` a link to Homebrew's python@3.13, which Banager
        // never sees -- one whose `bin` cannot be searched, and one with
        // no environment Banager knows of. Neither "needs it" nor
        // "nothing needs it" is known of any of them: the look did not
        // finish (`Warning::DependentsUnknown` in the preview).
        let root = Root::new("unknown-env");
        root.python_313();
        let opt = root.path("opt/homebrew/opt/python@3.13/bin/python3.13");
        root.program("opt/homebrew/Cellar/pipx/1.17.3/libexec/bin/pipx");
        root.link(
            "opt/homebrew/bin/pipx",
            "../Cellar/pipx/1.17.3/libexec/bin/pipx",
        );
        let pipx = instance(
            "pipx",
            "pipx",
            root.path("opt/homebrew/bin/pipx"),
            root.path("opt/homebrew/bin"),
        );
        root.link(
            "home/Documents/pipx/venvs/httpie/bin/python",
            opt.to_str().unwrap(),
        );
        root.link(
            "home/.local/pipx/venvs/poetry/bin/python",
            opt.to_str().unwrap(),
        );
        // A venv whose Python is known to be gone: known not to need it.
        root.link(
            "home/.local/pipx/venvs/gone/bin/python",
            "/nonexistent/python3.13",
        );
        let instances = vec![brew(&root), pipx];
        let env = root.env(&["opt/homebrew/bin"]);
        let tool = |name: &str, environment: Option<&str>| match environment {
            Some(environment) => with_path(
                row("pipx", ArtifactKind::Tool, name),
                root.path(environment),
            ),
            None => row("pipx", ArtifactKind::Tool, name),
        };
        let look = |tools: &[InstalledArtifact]| {
            needed_by(
                &formula("python@3.13"),
                &instances[0],
                &instances,
                tools,
                &env,
                BUDGET,
            )
        };
        // What is known either way finishes the look.
        assert_eq!(
            look(&[tool("gone", Some("home/.local/pipx/venvs/gone"))]),
            NeededBy {
                warnings: Vec::new(),
                complete: true
            }
        );
        // In `~/Documents`: not looked into, and not finished.
        assert_eq!(
            look(&[
                tool("gone", Some("home/.local/pipx/venvs/gone")),
                tool("httpie", Some("home/Documents/pipx/venvs/httpie")),
            ]),
            NeededBy {
                warnings: Vec::new(),
                complete: false
            }
        );
        // No environment path at all: nothing to look at, not finished.
        assert_eq!(
            look(&[tool("no-app", None)]),
            NeededBy {
                warnings: Vec::new(),
                complete: false
            }
        );
        // One that needs it is still named beside one that may.
        let found = look(&[
            tool("poetry", Some("home/.local/pipx/venvs/poetry")),
            tool("httpie", Some("home/Documents/pipx/venvs/httpie")),
        ]);
        assert_eq!(
            needed(&found.warnings),
            vec![("pipx".to_string(), false, 1)]
        );
        assert!(!found.complete);
        // A `bin` this account may not search: not finished either.
        let bin = root.path("home/.local/pipx/venvs/poetry/bin");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o000)).unwrap();
        let unreadable = look(&[tool("poetry", Some("home/.local/pipx/venvs/poetry"))]);
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            unreadable,
            NeededBy {
                warnings: Vec::new(),
                complete: false
            }
        );
    }

    #[test]
    fn test_a_source_program_or_path_folder_in_a_protected_place_leaves_the_look_unfinished() {
        let root = Root::new("unknown-program");
        root.node_22();
        root.node_linked();
        // npm's `npm` is a copy outside the `node` keg; the `node` it is run
        // with is the first on `PATH`. A `PATH` folder in `~/Documents`
        // comes first: whether a `node` there would be the one is not
        // known, so the look does not finish -- and Homebrew's, found after
        // it, is still named.
        let npm = instance("npm", NPM, root.path("opt/homebrew/bin/npm"), root.prefix());
        let instances = vec![brew(&root), npm];
        let found = needed_by(
            &formula("node"),
            &instances[0],
            &instances,
            &npm_rows(),
            &root.env(&["home/Documents/bin", "opt/homebrew/bin"]),
            BUDGET,
        );
        assert_eq!(needed(&found.warnings), vec![(NPM.to_string(), true, 4)]);
        assert!(!found.complete);
        // A `PATH` folder that is not there is known to hold no `node`.
        let missing = needed_by(
            &formula("node"),
            &instances[0],
            &instances,
            &npm_rows(),
            &root.env(&["home/bin", "opt/homebrew/bin"]),
            BUDGET,
        );
        assert_eq!(needed(&missing.warnings), vec![(NPM.to_string(), true, 4)]);
        assert!(missing.complete);
        // A source whose program leads into `~/Documents`: whether it runs
        // on the package is not known. Its one tool's environment is known
        // to hold no Python, so only the program is in doubt.
        root.python_313();
        root.link(
            "home/.local/bin/uv",
            root.path("home/Documents/uv/bin/uv").to_str().unwrap(),
        );
        let uv = instance(
            "uv",
            "uv",
            root.path("home/.local/bin/uv"),
            root.path("home/.local/bin"),
        );
        let tools = vec![with_path(
            row("uv", ArtifactKind::Tool, "ruff"),
            root.path("home/.local/share/uv/tools/ruff"),
        )];
        let instances = vec![brew(&root), uv];
        let found = needed_by(
            &formula("python@3.13"),
            &instances[0],
            &instances,
            &tools,
            &root.env(&["opt/homebrew/bin"]),
            BUDGET,
        );
        assert_eq!(
            found,
            NeededBy {
                warnings: Vec::new(),
                complete: false
            }
        );
    }

    /// A recorded fixture, read as it is, with the author's paths moved
    /// under `root`: `/Users/brulek` to its home folder, `/opt/homebrew`
    /// to its prefix. Nothing is edited but where things are.
    fn fixture_under(root: &Root, relative: &str) -> String {
        let text = std::fs::read_to_string(format!("../../adapters/fixtures/{relative}"))
            .unwrap_or_else(|e| panic!("read adapters/fixtures/{relative}: {e}"));
        text.replace("/Users/brulek", root.home().to_str().unwrap())
            .replace("/opt/homebrew", root.prefix().to_str().unwrap())
    }

    #[test]
    fn test_the_recorded_sources_need_python_3_14_and_ollama_and_not_the_unlinked_node_22() {
        // Every row from the recordings: Homebrew 7.0.3's list (`node@22`
        // keg-only and not linked, `python@3.11`, `python@3.14`, `ollama`),
        // npm's six packages, pip's seven, pipx's `cowsay` -- whose venv
        // pipx made with `/opt/homebrew/opt/python@3.14/bin/python3.14`
        // (`source_interpreter`) -- uv's `ruff`, and Ollama's one model.
        // Laid out as that Mac's Homebrew lays them out, with the npm and
        // node first on `PATH` an nvm's, and uv's `ruff` on a Python uv
        // downloaded itself.
        let root = Root::new("fixtures");
        let brew_rows = crate::adapters::brew::parse::parse_info_installed(
            &fixture_under(&root, "brew/7.0.3/info-installed.json"),
            BREW,
        )
        .expect("parse brew");
        let node_22 = brew_rows
            .iter()
            .find(|row| row.key.name == "node@22")
            .expect("node@22 is recorded");
        assert!(node_22.facts.command_inputs.keg_only, "and keg-only");
        root.node_22();
        root.program(
            "opt/homebrew/Cellar/python@3.14/3.14.7/Frameworks/Python.framework/Versions/3.14/bin/python3.14",
        );
        root.link(
            "opt/homebrew/Cellar/python@3.14/3.14.7/bin/python3.14",
            "../Frameworks/Python.framework/Versions/3.14/bin/python3.14",
        );
        root.link(
            "opt/homebrew/Cellar/python@3.14/3.14.7/bin/python3",
            "python3.14",
        );
        root.link(
            "opt/homebrew/opt/python@3.14",
            "../Cellar/python@3.14/3.14.7",
        );
        root.link(
            "opt/homebrew/bin/python3",
            "../Cellar/python@3.14/3.14.7/bin/python3",
        );
        root.program("opt/homebrew/Cellar/python@3.11/3.11.16/bin/python3.11");
        root.program("opt/homebrew/Cellar/ollama/0.34.1/bin/ollama");
        root.link(
            "opt/homebrew/bin/ollama",
            "../Cellar/ollama/0.34.1/bin/ollama",
        );
        root.program("home/.nvm/versions/node/v22.20.0/bin/node");
        root.program("home/.nvm/versions/node/v22.20.0/lib/node_modules/npm/bin/npm-cli.js");
        root.link(
            "home/.nvm/versions/node/v22.20.0/bin/npm",
            "../lib/node_modules/npm/bin/npm-cli.js",
        );
        let pipx_rows = crate::adapters::pipx::parse_list(
            &fixture_under(&root, "pipx/1.17.3/list.json"),
            "pipx",
        )
        .expect("parse pipx");
        let cowsay = pipx_rows[0].path.clone().expect("cowsay's venv");
        let interpreter = root.path("opt/homebrew/opt/python@3.14/bin/python3.14");
        root.link(
            cowsay
                .join("bin/python")
                .strip_prefix(&root.0)
                .unwrap()
                .to_str()
                .unwrap(),
            interpreter.to_str().unwrap(),
        );
        let uv_rows = crate::adapters::uv::parse_tool_list_show_paths(
            &fixture_under(&root, "uv/0.12.17/tool-list-show-paths.txt"),
            "uv",
        );
        let ruff = uv_rows[0].path.clone().expect("ruff's environment");
        let uv_python = root.program(
            "home/.local/share/uv/python/cpython-3.14.0-macos-aarch64-none/bin/python3.14",
        );
        root.link(
            ruff.join("bin/python")
                .strip_prefix(&root.0)
                .unwrap()
                .to_str()
                .unwrap(),
            uv_python.to_str().unwrap(),
        );
        const OLLAMA: &str = "ollama:http://127.0.0.1:11434";
        let ollama_rows = crate::adapters::ollama::parse::parse_tags(
            &std::fs::read_to_string("../../adapters/fixtures/ollama/0.34.1/api-tags.json")
                .expect("read ollama"),
            OLLAMA,
        )
        .expect("parse ollama");
        // pip's rows as its inventory makes them: what `--not-required`
        // lists is the user's (`Unknown`), the rest dependencies.
        let read = |name: &str| {
            crate::adapters::pip::parse_pip_list(&fixture_under(&root, name)).expect("parse pip")
        };
        let leaves: Vec<String> = read("pip/26.2.1/list-not-required.json")
            .into_iter()
            .map(|package| package.name)
            .collect();
        const PIP: &str = "pip:/opt/homebrew/bin/python3";
        let pip_rows: Vec<InstalledArtifact> = read("pip/26.2.1/list.json")
            .into_iter()
            .map(|package| {
                let reason = if leaves.contains(&package.name) {
                    InstallReason::Unknown
                } else {
                    InstallReason::Dependency
                };
                with_reason(row(PIP, ArtifactKind::Package, &package.name), reason)
            })
            .collect();
        let npm_rows: Vec<InstalledArtifact> = npm_rows()
            .into_iter()
            .map(|mut artifact| {
                artifact.key.instance_id = "npm:nvm".to_string();
                artifact
            })
            .collect();
        // As a refresh orders them: by adapter id.
        let instances = vec![
            brew(&root),
            instance(
                "npm",
                "npm:nvm",
                root.path("home/.nvm/versions/node/v22.20.0/bin/npm"),
                root.path("home/.nvm/versions/node/v22.20.0"),
            ),
            instance(
                "ollama",
                OLLAMA,
                root.path("opt/homebrew/bin/ollama"),
                root.path("home/.ollama"),
            ),
            instance(
                "pip",
                PIP,
                root.path("opt/homebrew/bin/python3"),
                root.path("opt/homebrew/bin"),
            ),
            instance(
                "pipx",
                "pipx",
                root.path("opt/homebrew/bin/pipx"),
                root.path("opt/homebrew/bin"),
            ),
            instance(
                "uv",
                "uv",
                root.path("home/.local/bin/uv"),
                root.path("home/.local/bin"),
            ),
        ];
        let artifacts: Vec<InstalledArtifact> =
            [npm_rows, ollama_rows, pip_rows, pipx_rows, uv_rows]
                .into_iter()
                .flatten()
                .collect();
        let env = root.env(&[
            "home/.nvm/versions/node/v22.20.0/bin",
            "opt/homebrew/bin",
            "usr/bin",
        ]);
        let of = |name: &str| {
            let package = brew_rows
                .iter()
                .find(|row| row.key.name == name)
                .unwrap_or_else(|| panic!("{name} is recorded"));
            let found = needed_by(package, &instances[0], &instances, &artifacts, &env, BUDGET);
            assert!(found.complete, "{name}");
            needed(&found.warnings)
        };
        assert_eq!(of("node@22"), Vec::new());
        assert_eq!(of("python@3.11"), Vec::new());
        // pip's seven, less Homebrew Python's own `pip` and `wheel`; and
        // pipx's `cowsay`, by its venv's Python, though pipx itself is not
        // on this Mac.
        assert_eq!(
            of("python@3.14"),
            vec![(PIP.to_string(), true, 5), ("pipx".to_string(), false, 1)]
        );
        assert_eq!(of("ollama"), vec![(OLLAMA.to_string(), true, 1)]);
    }
}
