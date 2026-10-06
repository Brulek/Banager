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
//! So is one that met a path it may not or cannot follow (`Doubt`) -- a
//! source's program, a tool's environment or a `PATH` folder that is, or
//! leads into, a protected place, one on the way to which a folder could
//! not be searched, and a pipx or uv tool with no environment Banager
//! knows of -- when the package could be what that path leads to
//! (`Look::could_be`): what is there is not known, so neither is whether
//! it runs on such a package. What the path would have to lead to is
//! judged by what it is for, not only by its name, for a link can change
//! both: pip's program is a Python whatever it is called (`~/bin/python3`
//! may lead to python@3.13's `bin/python3.13`, which has no `python3`),
//! npm's lives in a Node.js, a tool's environment runs a Python. A package
//! could be it when it is named for it (`uv`, `node@22`, `python@3.13`),
//! or has, of its own, a program of such a name. jq, or a font, can be
//! none of these, and their previews say nothing of it. A path that is
//! known not to be there (`Missing`) is known not to run on it.
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
/// path that could have led into it, were it what the package is, was
/// not followed (`Doubt`, `Look::could_be`) -- is no proof that nothing
/// else does.
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
        // What the source's program, and the interpreter it is run with,
        // lead to: into the package, or not, or not known (`Doubt`). Its
        // program not followed is moot when its interpreter leads in.
        let mut doubts = Vec::new();
        let mut program = match look.leads_into(&source.exe_path, &roots) {
            Some(into) => into,
            None => {
                doubts.extend(Doubt::of_program(&source.adapter_id, &source.exe_path));
                false
            }
        };
        if let Some(name) = interpreter(&source.adapter_id).filter(|_| !program) {
            let (found, passed_over) = look.on_path(name, env);
            // A folder passed over may hold the `node` `env` runs: in
            // doubt even when the one found after it leads into the
            // package, for then it may not be the one that runs.
            if passed_over {
                look.doubts.push(Doubt::Program(name.to_string()));
            }
            if let Some(found) = found {
                match look.leads_into(&found, &roots) {
                    Some(into) => program = into,
                    None => doubts.push(Doubt::Program(name.to_string())),
                }
            }
        }
        if program {
            warnings.push(Warning::NeededBySource {
                instance_id: source.id.clone(),
                program: true,
                tools: tools.len(),
            });
            continue;
        }
        look.doubts.extend(doubts);
        if has_environments(&source.adapter_id) {
            let on_it = tools
                .iter()
                .filter(|tool| {
                    let into = tool.path.as_deref().and_then(|environment| {
                        look.leads_into(&environment.join("bin").join("python"), &roots)
                    });
                    // No environment to look at, or one not followed:
                    // whether its Python is the package's is not known.
                    if into.is_none() {
                        look.doubts.push(Doubt::Python);
                    }
                    into == Some(true)
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
    let doubted = look.could_be(package, brew, &roots);
    NeededBy {
        warnings,
        complete: !look.over && !doubted,
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
    let Leads::To(folder) = look.resolve(&brew.prefix.join(folder)) else {
        return None;
    };
    let mut roots = vec![folder.join(short)];
    if package.key.kind == ArtifactKind::Cask {
        if let Some(app) = package.path.as_deref().filter(|path| is_app(path)) {
            match look.resolve(app) {
                Leads::To(app) => roots.push(app),
                Leads::Nowhere => {}
                // Its own app, not followed: anything may lead into it.
                Leads::Unknown => look.doubts.push(Doubt::Anything),
            }
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

/// Where a path leads, as far as a look can tell.
enum Leads {
    /// Every link followed, to here.
    To(PathBuf),
    /// Nowhere: it is not there (`Resolution::Missing`).
    Nowhere,
    /// Not known: it is, or leads into, a protected place, which is never
    /// looked into; a folder on the way could not be searched, or was
    /// replaced while it was looked at (`Resolution::Refused`); or the
    /// budget ran out first (`Look::over`).
    Unknown,
}

/// A path whose end is not known (`Leads::Unknown`), or a tool with no
/// environment path, by what it would have to lead to for the source to
/// run on the package: the look did not finish only when the package
/// could be that (`Look::could_be`).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Doubt {
    /// A program of this name: a source's own program, by its file name,
    /// or the interpreter it is run with (npm's `node`).
    Program(String),
    /// A Python: a pipx or uv tool's environment's `bin/python`, or pip's
    /// own program, whatever it is called.
    Python,
    /// Anything: the package's own app could not be followed.
    Anything,
}

impl Doubt {
    /// The doubts about `adapter_id`'s program at `path`, not followed:
    /// what it is for, not only what it is called, for a link may lead
    /// to a program of another name (`~/bin/python3` to python@3.13's
    /// `bin/python3.13`; npm's `npm` to `lib/node_modules/npm/bin/npm-cli.js`).
    /// pip's program, or any program named as a Python is, is a Python;
    /// npm's lives in a Node.js, with its `node`; any other is a program
    /// of its own name. Anything, for a path with no name.
    fn of_program(adapter_id: &str, path: &Path) -> Vec<Doubt> {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return vec![Doubt::Anything];
        };
        if adapter_id == "pip" || is_python(name) {
            return vec![Doubt::Python];
        }
        let mut doubts = vec![Doubt::Program(name.to_string())];
        if let Some(interpreter) = interpreter(adapter_id) {
            doubts.push(Doubt::Program(interpreter.to_string()));
        }
        doubts
    }
}

/// Whether `name` is a Python's, as pip's candidates are named
/// (`PipAdapter::CANDIDATE_INTERPRETERS`): `python`, `python3`,
/// `python3.N`.
fn is_python(name: &str) -> bool {
    name == "python"
        || name.strip_prefix("python3").is_some_and(|rest| {
            rest.is_empty()
                || rest.strip_prefix('.').is_some_and(|minor| {
                    !minor.is_empty() && minor.bytes().all(|b| b.is_ascii_digit())
                })
        })
}

/// Whether a package of short name `short` is named for the program
/// `name`: Homebrew's `uv` for `uv`, `node@22` for `node` -- a formula
/// named for a program is that program, wherever in its keg it keeps it.
fn named_for(short: &str, name: &str) -> bool {
    short
        .strip_prefix(name)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('@'))
}

/// Where each path leads, and what has been spent on finding out.
struct Look {
    protected: Protected,
    started: Instant,
    looks: usize,
    budget: Budget,
    over: bool,
    /// The paths met whose end is not known, by what each would have to
    /// lead to (`Doubt`). A path that is not there is known to lead
    /// nowhere, and is not one of these.
    doubts: Vec<Doubt>,
}

impl Look {
    fn new(home: &Path, budget: Budget) -> Look {
        Look {
            protected: Protected::new(home),
            started: Instant::now(),
            looks: 0,
            budget,
            over: false,
            doubts: Vec::new(),
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
    /// place (`protected::resolve`).
    fn resolve(&mut self, path: &Path) -> Leads {
        if !self.one_more() {
            return Leads::Unknown;
        }
        match protected::resolve(path, &self.protected, true) {
            Resolution::Found(real, _) => Leads::To(real),
            Resolution::Missing => Leads::Nowhere,
            Resolution::Protected(_) | Resolution::Refused => Leads::Unknown,
        }
    }

    /// Whether `path` leads, every link followed, into one of `roots`:
    /// `None` when where it leads is not known.
    fn leads_into(&mut self, path: &Path, roots: &[PathBuf]) -> Option<bool> {
        match self.resolve(path) {
            Leads::To(real) => {
                let real = protected::without_data_volume(&real);
                Some(roots.iter().any(|root| {
                    protected::starts_with_folded(&real, &protected::without_data_volume(root))
                }))
            }
            Leads::Nowhere => Some(false),
            Leads::Unknown => None,
        }
    }

    /// Whether `package` could be what one of the doubts met leads to
    /// (`Doubt`), so that the look did not finish: whether it is named for
    /// what a doubt would have to end at (`named_for`: `uv`, `node@22`,
    /// a `python@3.N` for a Python), or has, of its own, a program of that
    /// name -- a formula in `<prefix>/opt/<name>/bin` (where `opt/<name>`
    /// leads into its keg, linked or keg-only), a cask in `<prefix>/bin`
    /// (where its `binary` links go) -- leading into its own folders
    /// (`roots`). A Python is `python3`, or `python3.N` for
    /// `python@3.N`, whose keg has no `python3` unless it is Homebrew's
    /// default Python. One look a name, and none without a doubt or for a
    /// package named for one; one that is not known itself counts as a
    /// yes.
    fn could_be(
        &mut self,
        package: &InstalledArtifact,
        brew: &ManagerInstance,
        roots: &[PathBuf],
    ) -> bool {
        let doubts = std::mem::take(&mut self.doubts);
        if doubts.is_empty() {
            return false;
        }
        let Some(short) = short_name(&package.key.name) else {
            return true;
        };
        let mut names: Vec<String> = Vec::new();
        let mut named = false;
        for doubt in doubts {
            match doubt {
                Doubt::Anything => return true,
                Doubt::Program(name) => {
                    named |= named_for(short, &name);
                    names.push(name);
                }
                Doubt::Python => {
                    named |= named_for(short, "python");
                    names.push("python3".to_string());
                    if let Some(version) = short.strip_prefix("python@") {
                        names.push(format!("python{version}"));
                    }
                }
            }
        }
        if named {
            return true;
        }
        let bin = match package.key.kind {
            ArtifactKind::Formula => brew.prefix.join("opt").join(short).join("bin"),
            _ => brew.prefix.join("bin"),
        };
        names.sort();
        names.dedup();
        names
            .iter()
            .filter(|name| short_name(name) == Some(name.as_str()))
            .any(|name| self.leads_into(&bin.join(name), roots) != Some(false))
    }

    /// The first `name` on `env`'s `PATH`, as `env` would find it: what
    /// `resolve_exe` answers, each `PATH` folder looked at the same way
    /// (`protected::resolve`) and one look each; and whether a folder was
    /// passed over on the way that is, or leads into, a protected place,
    /// or could not be searched, and so may hold the `name` `env` would
    /// run. The one after it is still the one returned, as `resolve_exe`'s.
    /// A relative folder is passed over as `resolve_exe` passes it over: it
    /// names a folder only from wherever the program is run.
    fn on_path(&mut self, name: &str, env: &HostEnv) -> (Option<PathBuf>, bool) {
        let mut passed_over = false;
        for dir in env.path_dirs.iter().filter(|dir| dir.is_absolute()) {
            let candidate = dir.join(name);
            if !self.one_more() {
                return (None, passed_over);
            }
            match protected::resolve(&candidate, &self.protected, true) {
                Resolution::Found(_, meta) if meta.is_file() => {
                    return (Some(candidate), passed_over)
                }
                Resolution::Found(..) | Resolution::Missing => {}
                Resolution::Protected(_) | Resolution::Refused => passed_over = true,
            }
        }
        (None, passed_over)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{InstanceStatus, Scope};
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn test_the_per_source_tables_are_the_ones_the_window_mirrors() {
        // `needed_by_tables.json` is read here and by
        // src/lib/neededBy.test.ts, which holds `COMES_WITH_PROGRAM`,
        // `MANAGES_ONLY` (src/lib/neededBy.ts), `HOSTED_SOURCES` and
        // `RUNS_ON` (src/lib/batchUninstall.ts) to it. `manages_only` is
        // the window's own list, with no table here: it is held to these
        // tables for consistency, and each hosted source outside it says
        // why (`manages_only_leaves_out`).
        #[derive(serde::Deserialize)]
        struct Tables {
            hosted: Vec<String>,
            comes_with_program: std::collections::BTreeMap<String, Vec<String>>,
            interpreter: std::collections::BTreeMap<String, String>,
            has_environments: Vec<String>,
            manages_only: Vec<String>,
            manages_only_leaves_out: std::collections::BTreeMap<String, String>,
            batch_hosted_leaves_out: std::collections::BTreeMap<String, String>,
        }
        let tables: Tables =
            serde_json::from_str(include_str!("needed_by_tables.json")).expect("tables parse");
        assert_eq!(tables.hosted, HOSTED);
        for id in HOSTED.iter().chain(&["brew", "standalone-claude", "pip3"]) {
            let listed: Vec<&str> = tables
                .comes_with_program
                .get(*id)
                .map(|names| names.iter().map(String::as_str).collect())
                .unwrap_or_default();
            assert_eq!(comes_with_program(id), listed.as_slice(), "{id}");
            assert_eq!(
                interpreter(id),
                tables.interpreter.get(*id).map(String::as_str),
                "{id}"
            );
            assert_eq!(
                has_environments(id),
                tables.has_environments.iter().any(|each| each == id),
                "{id}"
            );
        }
        // Whose tools keep running without the package: a source of
        // `HOSTED` with no interpreter of its own and nothing it lists of
        // its own program, every source with environments among them.
        for id in &tables.manages_only {
            assert!(HOSTED.contains(&id.as_str()), "{id}");
            assert_eq!(interpreter(id), None, "{id}");
            assert!(comes_with_program(id).is_empty(), "{id}");
        }
        for id in &tables.has_environments {
            assert!(tables.manages_only.contains(id), "{id}");
        }
        // Every hosted source is either in it or left out with why, never
        // both: Ollama moved in by mistake fails here as well as in the
        // window's test.
        for id in HOSTED {
            let listed = tables.manages_only.iter().any(|each| each == id);
            let left_out = tables.manages_only_leaves_out.contains_key(id);
            assert!(
                listed != left_out,
                "{id}: in manages_only or left out with why"
            );
        }
        for id in tables.batch_hosted_leaves_out.keys() {
            assert!(HOSTED.contains(&id.as_str()), "{id}");
        }
    }

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
        /// `lib/node_modules`, outside the keg; and its `opt` link.
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
            self.link("opt/homebrew/opt/node", "../Cellar/node/24.9.0");
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
        crate::testing::installed_artifact(instance_id, kind, name)
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

    /// `jq`, a formula nothing else runs on, laid out as Homebrew lays it
    /// out (its keg, its `opt` link, linked into `bin`), and a font cask,
    /// which has no app: packages no doubt about a `node`, a Python or a
    /// source's own program can be about.
    fn bystanders(root: &Root) -> [InstalledArtifact; 2] {
        root.program("opt/homebrew/Cellar/jq/1.8.1/bin/jq");
        root.link("opt/homebrew/opt/jq", "../Cellar/jq/1.8.1");
        root.link("opt/homebrew/bin/jq", "../Cellar/jq/1.8.1/bin/jq");
        root.dir("opt/homebrew/Caskroom/font-fira-code/6.2");
        [
            formula("jq"),
            row(BREW, ArtifactKind::Cask, "font-fira-code"),
        ]
    }

    fn unfinished() -> NeededBy {
        NeededBy {
            warnings: Vec::new(),
            complete: false,
        }
    }

    fn finished() -> NeededBy {
        NeededBy {
            warnings: Vec::new(),
            complete: true,
        }
    }

    #[test]
    fn test_a_tool_environment_banager_may_not_or_cannot_look_into_leaves_a_pythons_look_unfinished(
    ) {
        // A pipx whose own Python is not python@3.13, with tools whose
        // environments may run on it: one kept in `~/Documents` -- its
        // `bin/python` a link to Homebrew's python@3.13, which Banager
        // never sees -- one whose `bin` cannot be searched, and one with
        // no environment Banager knows of. Neither "needs it" nor
        // "nothing needs it" is known of a Python: its look did not finish
        // (`Warning::DependentsUnknown` in the preview). Of jq or a font,
        // which no environment's `bin/python` can be, it is.
        let root = Root::new("unknown-env");
        root.python_313();
        let others = bystanders(&root);
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
        let look = |package: &InstalledArtifact, tools: &[InstalledArtifact]| {
            needed_by(package, &instances[0], &instances, tools, &env, BUDGET)
        };
        let python = formula("python@3.13");
        let gone = tool("gone", Some("home/.local/pipx/venvs/gone"));
        let in_documents = tool("httpie", Some("home/Documents/pipx/venvs/httpie"));
        let no_app = tool("no-app", None);
        // What is known either way finishes the look.
        assert_eq!(look(&python, std::slice::from_ref(&gone)), finished());
        // In `~/Documents`, not looked into; or with no environment path
        // at all: python@3.13's look is not finished -- its keg has
        // `python3.13`, not `python3` -- and jq's and the font's are.
        for tools in [vec![gone.clone(), in_documents.clone()], vec![no_app]] {
            assert_eq!(look(&python, &tools), unfinished(), "{tools:?}");
            for other in &others {
                assert_eq!(look(other, &tools), finished(), "{other:?} {tools:?}");
            }
        }
        // One that needs it is still named beside one that may.
        let found = look(
            &python,
            &[
                tool("poetry", Some("home/.local/pipx/venvs/poetry")),
                in_documents.clone(),
            ],
        );
        assert_eq!(
            needed(&found.warnings),
            vec![("pipx".to_string(), false, 1)]
        );
        assert!(!found.complete);
        // A `bin` this account may not search: not finished either, of a
        // Python alone.
        let bin = root.path("home/.local/pipx/venvs/poetry/bin");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o000)).unwrap();
        let poetry = [tool("poetry", Some("home/.local/pipx/venvs/poetry"))];
        let unreadable = look(&python, &poetry);
        let jq = look(&others[0], &poetry);
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(unreadable, unfinished());
        assert_eq!(jq, finished());
        // Homebrew's default Python has `python3` too, and is a Python by
        // it whatever its name.
        root.program("opt/homebrew/Cellar/python@3.14/3.14.0/bin/python3");
        root.link(
            "opt/homebrew/opt/python@3.14",
            "../Cellar/python@3.14/3.14.0",
        );
        assert_eq!(look(&formula("python@3.14"), &[in_documents]), unfinished());
    }

    #[test]
    fn test_a_path_folder_in_a_protected_place_before_node_leaves_a_nodes_look_unfinished() {
        let root = Root::new("unknown-path");
        root.node_22();
        root.node_linked();
        let others = bystanders(&root);
        // npm's `npm` is a copy outside the `node` keg; the `node` it is run
        // with is the first on `PATH`. A `PATH` folder in `~/Documents`
        // comes first: whether a `node` there would be the one is not
        // known, so the look of a Node.js does not finish -- and
        // Homebrew's, found after it, is still named. Of jq or a font,
        // which no `node` can be, it does.
        let npm = instance("npm", NPM, root.path("opt/homebrew/bin/npm"), root.prefix());
        let instances = vec![brew(&root), npm];
        let protected_first = root.env(&["home/Documents/bin", "opt/homebrew/bin"]);
        let look = |package: &InstalledArtifact, env: &HostEnv| {
            needed_by(package, &instances[0], &instances, &npm_rows(), env, BUDGET)
        };
        let found = look(&formula("node"), &protected_first);
        assert_eq!(needed(&found.warnings), vec![(NPM.to_string(), true, 4)]);
        assert!(!found.complete);
        // node@22, keg-only and not linked, has a `node` too: whether the
        // one in `~/Documents` leads into it is not known either.
        assert_eq!(look(&formula("node@22"), &protected_first), unfinished());
        for other in &others {
            assert_eq!(look(other, &protected_first), finished(), "{other:?}");
        }
        // A `PATH` folder that is not there is known to hold no `node`.
        let missing = look(
            &formula("node"),
            &root.env(&["home/bin", "opt/homebrew/bin"]),
        );
        assert_eq!(needed(&missing.warnings), vec![(NPM.to_string(), true, 4)]);
        assert!(missing.complete);
    }

    #[test]
    fn test_a_source_program_in_a_protected_place_leaves_only_its_own_packages_look_unfinished() {
        // uv's program leads into `~/Documents`: whether it is Homebrew's
        // `uv` is not known. Its one tool's environment is known to hold
        // no Python, so only the program is in doubt -- which only a
        // package that could be a `uv` can settle.
        let root = Root::new("unknown-program");
        root.python_313();
        let others = bystanders(&root);
        root.program("opt/homebrew/Cellar/uv/0.9.2/bin/uv");
        root.link("opt/homebrew/opt/uv", "../Cellar/uv/0.9.2");
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
        let env = root.env(&["opt/homebrew/bin"]);
        let look = |package: &InstalledArtifact| {
            needed_by(package, &instances[0], &instances, &tools, &env, BUDGET)
        };
        assert_eq!(look(&formula("uv")), unfinished());
        assert_eq!(look(&formula("python@3.13")), finished());
        for other in &others {
            assert_eq!(look(other), finished(), "{other:?}");
        }
    }

    #[test]
    fn test_pips_python_not_followed_leaves_every_pythons_look_unfinished_whatever_its_name() {
        // Astra's late review, finding 2: pip's program is a launcher of
        // the user's own, `~/bin/python3`, that led into python@3.13's
        // `bin/python3.13` -- a keg with no `python3` of its own. When it
        // is rerouted through `~/Documents`, or a folder on the way cannot
        // be searched, what runs pip is not known: it is a Python, so
        // every Python's look does not finish, whatever the launcher is
        // called. jq, a font and uv can be no Python, and theirs do.
        let root = Root::new("pip-alias");
        root.python_313();
        root.link(
            "opt/homebrew/Cellar/python@3.13/3.13.15/libexec/bin/python3",
            "../../Frameworks/Python.framework/Versions/3.13/bin/python3.13",
        );
        root.program("opt/homebrew/Cellar/python@3.14/3.14.0/bin/python3.14");
        root.link(
            "opt/homebrew/Cellar/python@3.14/3.14.0/bin/python3",
            "python3.14",
        );
        root.link(
            "opt/homebrew/opt/python@3.14",
            "../Cellar/python@3.14/3.14.0",
        );
        root.program("opt/homebrew/Cellar/uv/0.9.2/bin/uv");
        root.link("opt/homebrew/opt/uv", "../Cellar/uv/0.9.2");
        let others = bystanders(&root);
        let real = root.path("opt/homebrew/Cellar/python@3.13/3.13.15/bin/python3.13");
        const PIP: &str = "pip:~/bin/python3";
        let tools = vec![row(PIP, ArtifactKind::Package, "requests")];
        let env = root.env(&["home/bin", "opt/homebrew/bin"]);
        let look = |launcher: &str, package: &InstalledArtifact| {
            let pip = instance("pip", PIP, root.path(launcher), root.path("home/bin"));
            let instances = vec![brew(&root), pip];
            needed_by(package, &instances[0], &instances, &tools, &env, BUDGET)
        };
        // Followed: python@3.13 runs pip, and pip's one package needs it.
        root.link("home/bin/python3", real.to_str().unwrap());
        let found = look("home/bin/python3", &formula("python@3.13"));
        assert_eq!(needed(&found.warnings), vec![(PIP.to_string(), true, 1)]);
        assert!(found.complete);
        // Into `~/Documents`, a link to itself, and one named `python`.
        std::fs::remove_file(root.path("home/bin/python3")).unwrap();
        root.link(
            "home/bin/python3",
            root.path("home/Documents/py/bin/python3").to_str().unwrap(),
        );
        root.link("home/bin/loop/python3", "python3");
        root.link(
            "home/bin/python",
            root.path("home/Documents/py/bin/python3.13")
                .to_str()
                .unwrap(),
        );
        for launcher in [
            "home/bin/python3",
            "home/bin/loop/python3",
            "home/bin/python",
        ] {
            for python in ["python@3.13", "python@3.14"] {
                assert_eq!(
                    look(launcher, &formula(python)),
                    unfinished(),
                    "{launcher} {python}"
                );
            }
            assert_eq!(look(launcher, &formula("uv")), finished(), "{launcher}");
            for other in &others {
                assert_eq!(look(launcher, other), finished(), "{launcher} {other:?}");
            }
        }
        // Known not to be there: known to run on nothing.
        root.link("home/bin/gone/python3", "/nonexistent/python3");
        assert_eq!(
            look("home/bin/gone/python3", &formula("python@3.13")),
            finished()
        );
    }

    #[test]
    fn test_a_package_named_for_a_program_not_followed_could_be_it_wherever_its_keg_keeps_it() {
        // A link may lead to a program of another name, in another folder
        // of the keg. A formula named for the program -- `uv` for uv's,
        // `node@22` for npm's `node` -- could be it however it is laid
        // out; `libuv`, whose name only contains it, and jq are not.
        let root = Root::new("named-for");
        let others = bystanders(&root);
        root.program("opt/homebrew/Cellar/uv/0.9.2/libexec/vendor-launcher");
        root.link("opt/homebrew/opt/uv", "../Cellar/uv/0.9.2");
        root.program("opt/homebrew/Cellar/libuv/1.51.0/lib/libuv.1.dylib");
        root.link("opt/homebrew/opt/libuv", "../Cellar/libuv/1.51.0");
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
        let env = root.env(&["opt/homebrew/bin"]);
        let instances = vec![brew(&root), uv];
        let look = |package: &InstalledArtifact| {
            needed_by(package, &instances[0], &instances, &tools, &env, BUDGET)
        };
        assert_eq!(look(&formula("uv")), unfinished());
        assert_eq!(look(&formula("libuv")), finished());
        for other in &others {
            assert_eq!(look(other), finished(), "{other:?}");
        }
        // npm's program not followed: it lives in a Node.js. The `node` on
        // `PATH` is Homebrew's `node`, so that one is named; the keg-only
        // `node@22` could hold the npm itself, and is not known either
        // way. jq and the font can hold neither.
        let root = Root::new("npm-not-followed");
        root.node_22();
        root.node_linked();
        let others = bystanders(&root);
        root.link(
            "home/bin/npm",
            root.path("home/Documents/npm/bin/npm-cli.js")
                .to_str()
                .unwrap(),
        );
        let npm = instance("npm", NPM, root.path("home/bin/npm"), root.prefix());
        let instances = vec![brew(&root), npm];
        let env = root.env(&["opt/homebrew/bin"]);
        let look = |package: &InstalledArtifact| {
            needed_by(
                package,
                &instances[0],
                &instances,
                &npm_rows(),
                &env,
                BUDGET,
            )
        };
        let node = look(&formula("node"));
        assert_eq!(needed(&node.warnings), vec![(NPM.to_string(), true, 4)]);
        assert!(node.complete);
        assert_eq!(look(&formula("node@22")), unfinished());
        for other in &others {
            assert_eq!(look(other), finished(), "{other:?}");
        }
    }

    #[test]
    fn test_the_p8_control_layouts_leave_only_a_runtime_the_path_could_reach_unfinished() {
        // The real-data recheck p8 (item 7, finding F1), on a Homebrew laid
        // out as Homebrew lays one out: twelve formulae -- `node@22`
        // keg-only but linked by force, three Pythons (only 3.14 with a
        // `python3`), pipx, uv, Ollama and five that are no runtime -- and
        // two casks, a font and an app. npm runs on `node@22`, Ollama has
        // one model, pipx no tool. In each layout, the packages whose look
        // does not finish are exactly the ones the unfollowed path could
        // reach; 8d437178 left all but one or two unfinished in each.
        let root = Root::new("p8-layouts");
        let keg = |name: &str, programs: &[&str]| {
            for program in programs {
                root.program(&format!("opt/homebrew/Cellar/{name}/1.0/bin/{program}"));
                root.link(
                    &format!("opt/homebrew/bin/{program}"),
                    &format!("../Cellar/{name}/1.0/bin/{program}"),
                );
            }
            root.dir(&format!("opt/homebrew/Cellar/{name}/1.0"));
            root.link(
                &format!("opt/homebrew/opt/{name}"),
                &format!("../Cellar/{name}/1.0"),
            );
        };
        for (name, programs) in [
            ("jq", &["jq"][..]),
            ("wget", &["wget"]),
            ("git", &["git"]),
            ("ffmpeg", &["ffmpeg"]),
            ("libuv", &[]),
            ("python@3.11", &["python3.11"]),
            ("python@3.13", &["python3.13"]),
            ("python@3.14", &["python3.14", "python3"]),
            ("uv", &["uv", "uvx"]),
            ("ollama", &["ollama"]),
        ] {
            keg(name, programs);
        }
        root.program("opt/homebrew/Cellar/pipx/1.0/libexec/bin/pipx");
        root.link(
            "opt/homebrew/Cellar/pipx/1.0/bin/pipx",
            "../libexec/bin/pipx",
        );
        root.link("opt/homebrew/bin/pipx", "../Cellar/pipx/1.0/bin/pipx");
        root.link("opt/homebrew/opt/pipx", "../Cellar/pipx/1.0");
        root.node_22();
        root.link_node_22();
        root.dir("opt/homebrew/Caskroom/font-fira-code/6.2");
        root.dir("opt/homebrew/Caskroom/libreoffice/26.2.0");
        root.dir("Applications/LibreOffice.app/Contents/MacOS");
        let mut packages: Vec<InstalledArtifact> = [
            "jq",
            "wget",
            "git",
            "ffmpeg",
            "libuv",
            "node@22",
            "python@3.11",
            "python@3.13",
            "python@3.14",
            "pipx",
            "uv",
            "ollama",
        ]
        .iter()
        .map(|name| formula(name))
        .collect();
        packages.push(row(BREW, ArtifactKind::Cask, "font-fira-code"));
        packages.push(with_path(
            row(BREW, ArtifactKind::Cask, "libreoffice"),
            root.path("Applications/LibreOffice.app"),
        ));
        root.program("usr/bin/true");
        const OLLAMA: &str = "ollama:http://127.0.0.1:11434";
        const PIP: &str = "pip:~/bin/python3";
        let mut rows = npm_rows();
        rows.push(row(OLLAMA, ArtifactKind::Model, "qwen3:8b"));
        // pip run by a launcher of the user's own, and uv by one, each
        // leading into `~/Documents`.
        root.link(
            "home/bin/python3",
            root.path("home/Documents/py/bin/python3").to_str().unwrap(),
        );
        root.link(
            "home/.local/bin/uv",
            root.path("home/Documents/uv/bin/uv").to_str().unwrap(),
        );
        let base = vec![
            brew(&root),
            instance("npm", NPM, root.path("opt/homebrew/bin/npm"), root.prefix()),
            instance(
                "ollama",
                OLLAMA,
                root.path("opt/homebrew/bin/ollama"),
                root.path("home/.ollama"),
            ),
            instance(
                "pipx",
                "pipx",
                root.path("opt/homebrew/bin/pipx"),
                root.path("opt/homebrew/bin"),
            ),
        ];
        let login = root.env(&["opt/homebrew/bin", "usr/bin"]);
        let protected_first = root.env(&["home/Documents/bin", "opt/homebrew/bin", "usr/bin"]);
        let unfinished_of = |env: &HostEnv, layout: &str| -> Vec<String> {
            let mut instances = base.clone();
            let mut artifacts = rows.clone();
            if layout == "npm-copy" || layout == "both" {
                instances[1].exe_path = root.path("usr/bin/true");
            }
            let pipx_tool = layout == "pipx-no-env" || layout == "both";
            if pipx_tool {
                artifacts.push(row("pipx", ArtifactKind::Tool, "no-env"));
            }
            if layout == "pip-launcher" {
                instances.push(instance(
                    "pip",
                    PIP,
                    root.path("home/bin/python3"),
                    root.path("home/bin"),
                ));
                artifacts.push(row(PIP, ArtifactKind::Package, "requests"));
            }
            if layout == "uv-launcher" {
                instances.push(instance(
                    "uv",
                    "uv",
                    root.path("home/.local/bin/uv"),
                    root.path("home/.local/bin"),
                ));
                artifacts.push(with_path(
                    row("uv", ArtifactKind::Tool, "ruff"),
                    root.path("home/.local/share/uv/tools/ruff"),
                ));
            }
            let mut names = Vec::new();
            for package in &packages {
                let found = needed_by(package, &instances[0], &instances, &artifacts, env, BUDGET);
                // What is found is still named, in every layout: npm on
                // `node@22` (its `npm`, or the `node` on `PATH`), Ollama on
                // `ollama`, and pipx, once it has a tool, on `pipx`.
                let expected = match package.key.name.as_str() {
                    "node@22" => vec![(NPM.to_string(), true, 4)],
                    "ollama" => vec![(OLLAMA.to_string(), true, 1)],
                    "pipx" if pipx_tool => vec![("pipx".to_string(), true, 1)],
                    _ => Vec::new(),
                };
                assert_eq!(needed(&found.warnings), expected, "{layout} {package:?}");
                if !found.complete {
                    names.push(package.key.name.clone());
                }
            }
            names
        };
        let pythons = ["python@3.11", "python@3.13", "python@3.14"];
        for (env, label) in [(&login, "login"), (&protected_first, "protected first")] {
            let protected = label == "protected first";
            assert_eq!(unfinished_of(env, "as-is"), Vec::<String>::new(), "{label}");
            let npm_copy: Vec<&str> = if protected { vec!["node@22"] } else { vec![] };
            assert_eq!(unfinished_of(env, "npm-copy"), npm_copy, "{label}");
            assert_eq!(unfinished_of(env, "pipx-no-env"), pythons, "{label}");
            let mut both = pythons.to_vec();
            if protected {
                both.insert(0, "node@22");
            }
            assert_eq!(unfinished_of(env, "both"), both, "{label}");
            assert_eq!(unfinished_of(env, "pip-launcher"), pythons, "{label}");
            assert_eq!(unfinished_of(env, "uv-launcher"), vec!["uv"], "{label}");
        }
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
