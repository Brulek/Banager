//! Which copy of a command runs when the user types its name in Terminal
//! (`ArtifactFacts.commands`): for every installed artifact, the commands
//! it puts on the Mac, and for each one whether the first executable of
//! that name on `PATH` is this copy (`CommandState::Runs`), another file
//! that comes first while this one waits behind it (`ShadowedBy`, with
//! the artifact that file belongs to when one does), or nothing of this
//! copy's because the folder its command is in is not on `PATH`
//! (`NotOnPath`). The generalisation of `standalone::route::shadow_note`,
//! which answers the same question for one tool's own launcher, to every
//! source.
//!
//! What each source provides:
//!
//! - a Homebrew formula: the links in `<prefix>/bin` and `<prefix>/sbin`
//!   that lead, every link followed, into `<prefix>/Cellar/<name>/`;
//! - a Homebrew cask: its `binary` stanzas' links (`brew/parse.rs`),
//!   when they lead into the cask's folder in `Caskroom`, its app, or the
//!   file the stanza names -- `grok-build` links one file as `grok` and as
//!   `agent`;
//! - an npm package: the links in `<prefix>/bin` that lead into
//!   `<prefix>/lib/node_modules/<package>/`;
//! - a pipx tool: every app in its environment (`app_paths`), found in
//!   `~/.local/bin`, pipx's default bin folder, when the link there leads
//!   to it;
//! - a uv tool: its `- name (path)` lines, when the link leads into the
//!   tool's environment -- pipx and uv can both name `~/.local/bin/ruff`,
//!   and only the one whose environment the file there leads into is its
//!   owner;
//! - a Cargo crate: the `bins` of `.crates2.json`, in `<cargo_home>/bin`;
//! - a tool with its own installer: its launcher and the recipe's
//!   `other_commands` beside it (rustup's proxies by name, as they are hard
//!   links on some Macs).
//!
//! Read-only in the strictest sense, as the unknown-source scan is:
//! `read_dir` of each folder once -- `PATH`'s, and the bin folders of
//! Homebrew's and npm's prefixes -- then `stat` and `realpath` of the
//! entries a command could be, never a file's contents, never a command
//! run (docs/what-we-run.md, "Which copy a command runs"). Bounded
//! (`CommandBudget`) and run on the blocking pool, in two halves around a
//! refresh round's fan-out (`start_reading`, `finish`), so a folder on a
//! network disk that stopped answering costs the rounds its verdicts while
//! it does not answer, never a round itself.
//!
//! No verdict at all (`CommandFact.state: None`) for a Homebrew dependency
//! or keg-only formula -- left off `PATH` on purpose, or never asked for --
//! and for every command while the `PATH` Banager has is not the one its
//! login shell exports (`Session::note_login_path`): judged against the
//! small default `PATH` an app opened from Finder starts with, nearly
//! every tool would read as "not found".

use crate::adapters::standalone::recipe::RouteKind;
use crate::adapters::standalone::recipes::RECIPES;
use crate::model::{
    ArtifactKind, CommandFact, CommandState, InstallReason, InstalledArtifact, ManagerInstance,
};
use crate::protected;
use crate::runner::HostEnv;
use crate::scan::display_path;
use std::collections::{BTreeSet, HashMap};
use std::ffi::{OsStr, OsString};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How much of the file system one round may look at for this, and for
/// how long, before it gives up on its verdicts. Each half -- reading the
/// folders, then judging -- gets `max_duration` of its own, checked before
/// every `read_dir` and every entry; `max_entries` counts the names read.
/// Past either, the round has no verdicts (`ArtifactFacts.commands` empty)
/// rather than ones made from half a `PATH`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandBudget {
    pub max_entries: usize,
    pub max_duration: Duration,
}

impl Default for CommandBudget {
    /// A Mac's `PATH` folders hold a few thousand names between them
    /// (`/usr/bin` alone about a thousand, a busy Homebrew's `bin` as
    /// many again); reading them takes milliseconds.
    fn default() -> CommandBudget {
        CommandBudget {
            max_entries: 20_000,
            max_duration: Duration::from_secs(5),
        }
    }
}

/// One folder: the path it was named by, where that leads, and the names
/// in it -- or, for a `PATH` folder macOS would ask the user about before
/// Banager looked inside (`asks_first`), `read: false` and no names.
#[derive(Clone, Debug)]
struct Folder {
    given: PathBuf,
    canonical: PathBuf,
    names: BTreeSet<OsString>,
    read: bool,
}

/// Whether `path` is in one of `places` (`protected::places`): one of
/// the folders macOS asks the user about before an app looks inside them,
/// under the home folder as named or where it leads, or on another disk,
/// under `/Volumes` -- a network disk that went away does not answer at
/// all. They are never read here: a refresh must not put up a permission
/// request, or wait on a disk. Case aside, as APFS compares names.
fn asks_first(path: &Path, places: &[PathBuf]) -> bool {
    protected::is_within(path, places)
}

/// The folders one round read (`read_folders`).
#[derive(Clone, Debug, Default)]
pub struct Folders {
    /// `PATH`'s folders in `PATH`'s order, each once: an entry that is
    /// empty or relative, that does not exist, or that names a folder an
    /// earlier entry already did (by where it leads) is not here. One
    /// `asks_first` is here, unread: it may hold any name.
    path: Vec<Folder>,
    /// The Homebrew and npm bin folders that are not on `PATH`.
    other: Vec<Folder>,
    /// Whether every folder was read within the budget. A round whose
    /// folders were not makes no verdicts.
    complete: bool,
}

impl Folders {
    fn find(&self, canonical: &Path) -> Option<&Folder> {
        self.path
            .iter()
            .chain(&self.other)
            .find(|folder| folder.canonical == canonical)
    }

    fn on_path(&self, canonical: &Path) -> bool {
        self.path.iter().any(|folder| folder.canonical == canonical)
    }

    /// The `PATH` folders that were read, as named (for the tests).
    pub fn path_folders(&self) -> Vec<&Path> {
        self.path
            .iter()
            .filter(|folder| folder.read)
            .map(|folder| folder.given.as_path())
            .collect()
    }

    /// The `PATH` folders left unread (`asks_first`), as named.
    pub fn unread_path_folders(&self) -> Vec<&Path> {
        self.path
            .iter()
            .filter(|folder| !folder.read)
            .map(|folder| folder.given.as_path())
            .collect()
    }

    pub fn complete(&self) -> bool {
        self.complete
    }
}

/// The bin folders whose links say which formula or npm package a
/// command is: `<prefix>/bin` and `<prefix>/sbin` of every Homebrew, and
/// `<prefix>/bin` of every npm. Read whether or not they are on `PATH`, so
/// a command can be named even where no verdict is made.
pub fn bin_folders(instances: &[ManagerInstance]) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for inst in instances {
        match inst.adapter_id.as_str() {
            "brew" => {
                dirs.push(inst.prefix.join("bin"));
                dirs.push(inst.prefix.join("sbin"));
            }
            "npm" => dirs.push(inst.prefix.join("bin")),
            _ => {}
        }
    }
    dirs
}

/// Why a read stopped: one of the budget's two limits.
struct Stopped;

/// Reads each folder once: the names in every `PATH` entry, in order, and
/// in every `bin_dirs` folder not already on `PATH`. Skips an empty or a
/// relative `PATH` entry (a shell would look it up from its own current
/// folder, which is not Banager's: an app opened from Finder has `/`), one
/// that does not exist or cannot be read, and one naming a folder an
/// earlier one did. A folder `asks_first` names, as named or where it
/// leads, is not read: on `PATH` it is kept, unread, and a bin folder
/// there is skipped. `complete` is false when the budget stopped it.
pub fn read_folders(
    path_dirs: &[PathBuf],
    bin_dirs: &[PathBuf],
    home: &Path,
    budget: CommandBudget,
) -> Folders {
    let started = Instant::now();
    let mut examined = 0usize;
    let mut folders = Folders::default();
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut homes = vec![home.to_path_buf()];
    homes.extend(std::fs::canonicalize(home).ok());
    let places = protected::places(&homes);
    for dir in path_dirs {
        if dir.as_os_str().is_empty() || !dir.is_absolute() {
            continue;
        }
        match read_one(dir, &places, &mut seen, budget, started, &mut examined) {
            Err(Stopped) => return folders,
            Ok(Some(folder)) => folders.path.push(folder),
            Ok(None) => {}
        }
    }
    for dir in bin_dirs {
        match read_one(dir, &places, &mut seen, budget, started, &mut examined) {
            Err(Stopped) => return folders,
            Ok(Some(folder)) if folder.read => folders.other.push(folder),
            Ok(_) => {}
        }
    }
    folders.complete = true;
    folders
}

/// One folder's names, or `None` for one that is not there, cannot be
/// read, or was read already (`seen`, by where it leads). One `asks_first`
/// names comes back unread; as named there, it is not even resolved.
fn read_one(
    dir: &Path,
    places: &[PathBuf],
    seen: &mut Vec<PathBuf>,
    budget: CommandBudget,
    started: Instant,
    examined: &mut usize,
) -> Result<Option<Folder>, Stopped> {
    if started.elapsed() >= budget.max_duration {
        return Err(Stopped);
    }
    let unread = |canonical: PathBuf| Folder {
        given: dir.to_path_buf(),
        canonical,
        names: BTreeSet::new(),
        read: false,
    };
    if asks_first(dir, places) {
        if seen.iter().any(|known| known == dir) {
            return Ok(None);
        }
        seen.push(dir.to_path_buf());
        return Ok(Some(unread(dir.to_path_buf())));
    }
    let Ok(canonical) = std::fs::canonicalize(dir) else {
        return Ok(None);
    };
    if seen.contains(&canonical) {
        return Ok(None);
    }
    seen.push(canonical.clone());
    if asks_first(&canonical, places) {
        return Ok(Some(unread(canonical)));
    }
    let Ok(read) = std::fs::read_dir(&canonical) else {
        return Ok(None);
    };
    let mut names = BTreeSet::new();
    for entry in read {
        if *examined >= budget.max_entries || started.elapsed() >= budget.max_duration {
            return Err(Stopped);
        }
        *examined += 1;
        if let Ok(entry) = entry {
            names.insert(entry.file_name());
        }
    }
    Ok(Some(Folder {
        given: dir.to_path_buf(),
        canonical,
        names,
        read: true,
    }))
}

/// `stat` and `realpath`, each path once, and the clock.
struct Look {
    canonical: HashMap<PathBuf, Option<PathBuf>>,
    started: Instant,
    budget: CommandBudget,
}

impl Look {
    fn canonical(&mut self, path: &Path) -> Option<PathBuf> {
        if let Some(known) = self.canonical.get(path) {
            return known.clone();
        }
        let found = std::fs::canonicalize(path).ok();
        self.canonical.insert(path.to_path_buf(), found.clone());
        found
    }

    fn over(&self) -> bool {
        self.started.elapsed() >= self.budget.max_duration
    }
}

/// A file a shell would run: a regular file, links followed, with an
/// execute bit (`route::shadow_note`'s test, not `resolve_exe`'s, which
/// takes any file).
fn executable(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// What one `PATH` folder holds of a name: an executable file, by where it
/// leads (`None`: it does not resolve), or nobody knows -- a folder left
/// unread (`asks_first`), which may hold one.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Seen {
    Executable(Option<PathBuf>),
    Unread,
}

/// One command an artifact provides: the file it is, every link followed,
/// and the folder its command was put in when Banager knows it (the folder
/// `NotOnPath` names).
struct Claim {
    artifact: usize,
    name: String,
    target: PathBuf,
    folder: Option<PathBuf>,
}

/// Whether Banager says which copy runs for this artifact's commands: not
/// for a Homebrew dependency (nobody typed its name to install it) or a
/// keg-only formula (Homebrew leaves its commands off `PATH` on purpose).
fn judged(artifact: &InstalledArtifact) -> bool {
    artifact.reason != InstallReason::Dependency && !artifact.facts.command_inputs.keg_only
}

/// What every artifact's commands run, in `artifacts`' order, each list
/// sorted by name: `None` when `folders` is incomplete or the time ran
/// out (no verdicts this round), and states of `None` throughout when
/// `path_known` is false. Pure over its arguments and the file system;
/// what the window is sent.
pub fn judge(
    folders: &Folders,
    instances: &[ManagerInstance],
    artifacts: &[InstalledArtifact],
    home: &Path,
    path_known: bool,
    budget: CommandBudget,
) -> Option<Vec<Vec<CommandFact>>> {
    if !folders.complete {
        return None;
    }
    let mut look = Look {
        canonical: HashMap::new(),
        started: Instant::now(),
        budget,
    };
    let claims = claims(folders, instances, artifacts, home, &mut look)?;
    // The artifact each file is, for `ShadowedBy`: the first claim wins.
    let mut owner: HashMap<&Path, usize> = HashMap::new();
    for claim in &claims {
        owner.entry(&claim.target).or_insert(claim.artifact);
    }
    // What each `PATH` folder holds of a name, in `PATH`'s order -- every
    // executable by where it leads, and every unread folder -- looked up
    // once per name.
    let mut on_path: HashMap<&str, Vec<Seen>> = HashMap::new();
    let mut out: Vec<Vec<CommandFact>> = vec![Vec::new(); artifacts.len()];
    for claim in &claims {
        if look.over() {
            return None;
        }
        let state = if !path_known || !judged(&artifacts[claim.artifact]) {
            None
        } else {
            let matches = on_path.entry(&claim.name).or_insert_with(|| {
                folders
                    .path
                    .iter()
                    .filter_map(|folder| {
                        if !folder.read {
                            return Some(Seen::Unread);
                        }
                        if !folder.names.contains(OsStr::new(&claim.name)) {
                            return None;
                        }
                        let path = folder.canonical.join(&claim.name);
                        executable(&path).then(|| Seen::Executable(look.canonical(&path)))
                    })
                    .collect()
            });
            let owner_of = |found: &Option<PathBuf>| {
                found
                    .as_deref()
                    .and_then(|target| owner.get(target))
                    .copied()
            };
            let this = Seen::Executable(Some(claim.target.clone()));
            match matches.first() {
                // An unread folder first: it may hold the name.
                Some(Seen::Unread) => None,
                Some(Seen::Executable(first))
                    if first.as_deref() == Some(claim.target.as_path())
                        || owner_of(first) == Some(claim.artifact) =>
                {
                    Some(CommandState::Runs)
                }
                Some(Seen::Executable(first))
                    if matches.iter().skip(1).any(|seen| *seen == this) =>
                {
                    Some(CommandState::ShadowedBy {
                        by: owner_of(first).map(|i| artifacts[i].key.clone()),
                    })
                }
                // Nothing on `PATH` that was read leads here, and an
                // unread folder could hold a link that does: no verdict.
                _ if matches.contains(&Seen::Unread) => None,
                // Nothing on `PATH` leads here: "not found" is said only
                // of a folder Banager knows and `PATH` lacks. A copy whose
                // folder is on `PATH` and was still not found there (its
                // link replaced, its execute bit gone) gets no verdict.
                _ => claim.folder.as_ref().and_then(|folder| {
                    let canonical = look
                        .canonical(folder)
                        .unwrap_or_else(|| folder.to_path_buf());
                    (!folders.on_path(&canonical)).then(|| CommandState::NotOnPath {
                        dir: display_path(folder, home).to_string_lossy().into_owned(),
                    })
                }),
            }
        };
        out[claim.artifact].push(CommandFact {
            name: claim.name.clone(),
            state,
        });
    }
    for commands in &mut out {
        commands.sort_by(|a, b| a.name.cmp(&b.name));
    }
    Some(out)
}

/// Every command every artifact provides (the module doc's list), each
/// one an executable file, one per name per artifact (the first found),
/// in `artifacts`' order. `None` when the time ran out.
fn claims(
    folders: &Folders,
    instances: &[ManagerInstance],
    artifacts: &[InstalledArtifact],
    home: &Path,
    look: &mut Look,
) -> Option<Vec<Claim>> {
    let mut by_instance: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, artifact) in artifacts.iter().enumerate() {
        by_instance
            .entry(artifact.key.instance_id.as_str())
            .or_default()
            .push(index);
    }
    let mut claims: Vec<Claim> = Vec::new();
    for inst in instances {
        let Some(indices) = by_instance.get(inst.id.as_str()) else {
            continue;
        };
        match inst.adapter_id.as_str() {
            "brew" => {
                linked(
                    folders,
                    inst,
                    indices,
                    artifacts,
                    Linked::Formula,
                    look,
                    &mut claims,
                )?;
                for &index in indices {
                    if artifacts[index].key.kind == ArtifactKind::Cask {
                        cask(inst, index, &artifacts[index], look, &mut claims);
                    }
                }
            }
            "npm" => linked(
                folders,
                inst,
                indices,
                artifacts,
                Linked::Package,
                look,
                &mut claims,
            )?,
            "pipx" => {
                for &index in indices {
                    pipx(index, &artifacts[index], home, look, &mut claims);
                }
            }
            id if id.starts_with("standalone-") => {
                for &index in indices {
                    standalone(inst, index, look, &mut claims);
                }
            }
            // uv, Cargo, and whatever else names its commands itself.
            _ => {
                for &index in indices {
                    named(index, &artifacts[index], &[], look, &mut claims);
                }
            }
        }
        if look.over() {
            return None;
        }
    }
    claims.retain(|claim| executable(&claim.target));
    let mut seen: BTreeSet<(usize, String)> = BTreeSet::new();
    claims.retain(|claim| seen.insert((claim.artifact, claim.name.clone())));
    claims.sort_by_key(|claim| claim.artifact);
    Some(claims)
}

/// Which kind of link `linked` attributes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Linked {
    /// Homebrew's: `<prefix>/{bin,sbin}/x` → `<prefix>/Cellar/<formula>/…`.
    Formula,
    /// npm's: `<prefix>/bin/x` → `<prefix>/lib/node_modules/<package>/…`,
    /// a scoped package's two folders deep.
    Package,
}

/// A Homebrew formula's or an npm package's commands: the links in its
/// prefix's bin folders that lead into its own folder.
fn linked(
    folders: &Folders,
    inst: &ManagerInstance,
    indices: &[usize],
    artifacts: &[InstalledArtifact],
    kind: Linked,
    look: &mut Look,
    claims: &mut Vec<Claim>,
) -> Option<()> {
    let (own, artifact_kind, bins): (PathBuf, ArtifactKind, Vec<PathBuf>) = match kind {
        Linked::Formula => (
            inst.prefix.join("Cellar"),
            ArtifactKind::Formula,
            vec![inst.prefix.join("bin"), inst.prefix.join("sbin")],
        ),
        Linked::Package => (
            inst.prefix.join("lib").join("node_modules"),
            ArtifactKind::Package,
            vec![inst.prefix.join("bin")],
        ),
    };
    let Some(own) = look.canonical(&own) else {
        return Some(());
    };
    // A formula's folder in `Cellar` is its short name (`display_name`);
    // a package's in `node_modules` is its name.
    let by_name: HashMap<&str, usize> = indices
        .iter()
        .filter(|&&index| artifacts[index].key.kind == artifact_kind)
        .map(|&index| {
            let artifact = &artifacts[index];
            let name = match kind {
                Linked::Formula => artifact.display_name.as_str(),
                Linked::Package => artifact.key.name.as_str(),
            };
            (name, index)
        })
        .collect();
    if by_name.is_empty() {
        return Some(());
    }
    for dir in bins {
        let Some(canonical_dir) = look.canonical(&dir) else {
            continue;
        };
        let Some(folder) = folders.find(&canonical_dir) else {
            continue;
        };
        for name in &folder.names {
            if look.over() {
                return None;
            }
            let Some(command) = name.to_str() else {
                continue;
            };
            let Some(target) = look.canonical(&canonical_dir.join(name)) else {
                continue;
            };
            let Ok(inside) = target.strip_prefix(&own) else {
                continue;
            };
            let mut parts = inside.components().filter_map(|part| match part {
                Component::Normal(part) => part.to_str(),
                _ => None,
            });
            let Some(first) = parts.next() else {
                continue;
            };
            let owner_name = match (kind, first.starts_with('@')) {
                (Linked::Package, true) => match parts.next() {
                    Some(second) => format!("{first}/{second}"),
                    None => continue,
                },
                _ => first.to_string(),
            };
            if let Some(&index) = by_name.get(owner_name.as_str()) {
                claims.push(Claim {
                    artifact: index,
                    name: command.to_string(),
                    target,
                    folder: Some(dir.clone()),
                });
            }
        }
    }
    Some(())
}

/// A cask's `binary` links (`brew/parse.rs`), each when it leads into the
/// cask's folder in `Caskroom` (a staged file, `grok-build`'s `grok`), its
/// app, or the file its stanza names.
fn cask(
    inst: &ManagerInstance,
    index: usize,
    artifact: &InstalledArtifact,
    look: &mut Look,
    claims: &mut Vec<Claim>,
) {
    // `Caskroom/<token>`: the short token, which a tapped cask's key
    // (`user/tap/token`) ends with.
    let token = artifact
        .key
        .name
        .rsplit('/')
        .next()
        .unwrap_or(&artifact.key.name);
    let mut roots = vec![inst.prefix.join("Caskroom").join(token)];
    roots.extend(artifact.path.clone());
    named(index, artifact, &roots, look, claims);
}

/// The commands an artifact's source named (`CommandInputs.provided`),
/// each when it leads, every link followed, into one of its `within` or
/// `extra_roots` -- or anywhere, when neither names a place. The folder a
/// command is in is the one its path names.
fn named(
    index: usize,
    artifact: &InstalledArtifact,
    extra_roots: &[PathBuf],
    look: &mut Look,
    claims: &mut Vec<Claim>,
) {
    for provided in &artifact.facts.command_inputs.provided {
        let Some(target) = look.canonical(&provided.path) else {
            continue;
        };
        let roots: Vec<PathBuf> = provided
            .within
            .iter()
            .chain(extra_roots)
            .filter_map(|root| look.canonical(root))
            .collect();
        let places = provided.within.len() + extra_roots.len();
        if places > 0 && !roots.iter().any(|root| target.starts_with(root)) {
            continue;
        }
        claims.push(Claim {
            artifact: index,
            name: provided.name.clone(),
            target,
            folder: provided.path.parent().map(Path::to_path_buf),
        });
    }
}

/// pipx's default bin folder, under the home folder: where `pipx install`
/// puts its links unless `PIPX_BIN_DIR` says otherwise, which Banager's
/// environment does not carry (only `PATH` comes from the login shell).
const PIPX_BIN_DIR: &str = ".local/bin";

/// A pipx tool's apps: the program in its environment is the command's
/// file; the folder is pipx's bin folder, when the link of that name there
/// leads to it.
fn pipx(
    index: usize,
    artifact: &InstalledArtifact,
    home: &Path,
    look: &mut Look,
    claims: &mut Vec<Claim>,
) {
    let bin = home.join(PIPX_BIN_DIR);
    for provided in &artifact.facts.command_inputs.provided {
        let Some(target) = look.canonical(&provided.path) else {
            continue;
        };
        let exposed = look.canonical(&bin.join(&provided.name));
        claims.push(Claim {
            artifact: index,
            name: provided.name.clone(),
            folder: (exposed.as_deref() == Some(target.as_path())).then(|| bin.clone()),
            target,
        });
    }
}

/// A tool with its own installer: its launcher (the instance's
/// `exe_path`, named after the recipe's id) and the recipe's
/// `other_commands` beside it. A launcher that is a link must lead into
/// the tool's own root, as detection requires; rustup's flat file and its
/// proxies are taken by name.
fn standalone(inst: &ManagerInstance, index: usize, look: &mut Look, claims: &mut Vec<Claim>) {
    let Some(recipe) = RECIPES
        .iter()
        .find(|recipe| inst.adapter_id == format!("standalone-{}", recipe.id))
    else {
        return;
    };
    let Some(folder) = inst.exe_path.parent() else {
        return;
    };
    let root = match recipe.route.kind {
        RouteKind::SymlinkIntoRoot => match look.canonical(&inst.prefix) {
            Some(root) => Some(root),
            None => return,
        },
        RouteKind::FlatFile => None,
    };
    let names = std::iter::once(recipe.id).chain(recipe.other_commands.iter().copied());
    for name in names {
        let path = if name == recipe.id {
            inst.exe_path.clone()
        } else {
            folder.join(name)
        };
        let Some(target) = look.canonical(&path) else {
            continue;
        };
        if root.as_ref().is_some_and(|root| !target.starts_with(root)) {
            continue;
        }
        claims.push(Claim {
            artifact: index,
            name: name.to_string(),
            target,
            folder: Some(folder.to_path_buf()),
        });
    }
}

/// How long past its own budget a half may take before the round stops
/// waiting for it: a read stuck in the kernel (a network disk that went
/// away) never comes back to look at the clock.
const GRACE: Duration = Duration::from_secs(1);

/// Clears the in-flight mark when a half's blocking work ends, however
/// it ends.
struct InFlight(Arc<AtomicBool>);

impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// The first half, under way: the folders being read on the blocking pool
/// while a refresh round's fan-out runs (`Session::refresh`).
pub(crate) struct Reading {
    handle: Option<tokio::task::JoinHandle<Folders>>,
    started: Instant,
}

/// Starts reading the folders: `PATH`'s, when it is the login shell's,
/// and the Homebrew and npm bin folders. Nothing is started while an
/// earlier round's read is still running -- one stuck on a folder that
/// stopped answering -- so stuck reads never pile up; that round then has
/// no verdicts.
pub(crate) fn start_reading(
    env: &HostEnv,
    instances: &[ManagerInstance],
    path_known: bool,
    in_flight: &Arc<AtomicBool>,
    budget: CommandBudget,
) -> Reading {
    let started = Instant::now();
    if in_flight.swap(true, Ordering::SeqCst) {
        return Reading {
            handle: None,
            started,
        };
    }
    let path_dirs = if path_known {
        env.path_dirs.clone()
    } else {
        Vec::new()
    };
    let bin_dirs = bin_folders(instances);
    let home = env.home.clone();
    let guard = InFlight(in_flight.clone());
    let handle = tokio::task::spawn_blocking(move || {
        let _guard = guard;
        read_folders(&path_dirs, &bin_dirs, &home, budget)
    });
    Reading {
        handle: Some(handle),
        started,
    }
}

/// The second half: waits for the folders (no longer than their budget
/// and `GRACE` from when they were started), judges every artifact's
/// commands on the blocking pool, and writes `ArtifactFacts.commands` on
/// the rows this round's inventories listed.
///
/// A row carried from an earlier round -- its source did not answer, its
/// inventory failed, an operation holds it (`Session::refresh`) -- is that
/// round's row, and keeps the verdicts it had, as it keeps its version:
/// those are the rows whose `commands` are not empty here, since an
/// inventory never fills them. Every row still counts as the owner of its
/// files. With no answer this round (the budget, a read that did not come
/// back, one still stuck from an earlier round), the rows the inventories
/// listed have no verdicts.
pub(crate) async fn finish(
    reading: Reading,
    instances: &[ManagerInstance],
    artifacts: &mut [InstalledArtifact],
    home: &Path,
    path_known: bool,
    in_flight: &Arc<AtomicBool>,
    budget: CommandBudget,
) {
    let answer = judged_in_background(
        reading, instances, artifacts, home, path_known, in_flight, budget,
    )
    .await;
    if let Some(commands) = answer {
        for (artifact, commands) in artifacts.iter_mut().zip(commands) {
            if artifact.facts.commands.is_empty() {
                artifact.facts.commands = commands;
            }
        }
    }
}

async fn judged_in_background(
    reading: Reading,
    instances: &[ManagerInstance],
    artifacts: &[InstalledArtifact],
    home: &Path,
    path_known: bool,
    in_flight: &Arc<AtomicBool>,
    budget: CommandBudget,
) -> Option<Vec<Vec<CommandFact>>> {
    let handle = reading.handle?;
    let deadline = reading.started + budget.max_duration + GRACE;
    let folders = tokio::time::timeout_at(deadline.into(), handle)
        .await
        .ok()?
        .ok()?;
    if in_flight.swap(true, Ordering::SeqCst) {
        return None;
    }
    let guard = InFlight(in_flight.clone());
    let instances = instances.to_vec();
    let artifacts = artifacts.to_vec();
    let home = home.to_path_buf();
    let handle = tokio::task::spawn_blocking(move || {
        let _guard = guard;
        judge(&folders, &instances, &artifacts, &home, path_known, budget)
    });
    tokio::time::timeout(budget.max_duration + GRACE, handle)
        .await
        .ok()?
        .ok()?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_budget_default_is_the_documented_numbers() {
        // docs/what-we-run.md states both; `what_we_run_test` holds the
        // section to them.
        let budget = CommandBudget::default();
        assert_eq!(budget.max_entries, 20_000);
        assert_eq!(budget.max_duration, Duration::from_secs(5));
    }

    #[test]
    fn test_bin_folders_are_homebrews_two_and_npms_one() {
        let instances = vec![
            ManagerInstance {
                prefix: PathBuf::from("/opt/homebrew"),
                ..crate::testing::manager_instance("brew", "brew:/opt/homebrew")
            },
            ManagerInstance {
                prefix: PathBuf::from("/usr/local"),
                ..crate::testing::manager_instance("npm", "npm:/usr/local")
            },
            crate::testing::manager_instance("pipx", "pipx"),
        ];
        assert_eq!(
            bin_folders(&instances),
            vec![
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/opt/homebrew/sbin"),
                PathBuf::from("/usr/local/bin"),
            ]
        );
    }

    /// A home with Claude Code's native install in it, removed on drop.
    struct Claude {
        home: PathBuf,
        instance: ManagerInstance,
    }

    impl Claude {
        fn new(tag: &str) -> Claude {
            let raw = std::env::temp_dir().join(format!(
                "banager-commands-unit-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(raw.join(".local/share/claude/versions")).unwrap();
            std::fs::create_dir_all(raw.join(".local/bin")).unwrap();
            let home = std::fs::canonicalize(&raw).unwrap();
            let real = home.join(".local/share/claude/versions/2.1.281");
            std::fs::write(&real, b"#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o755)).unwrap();
            let launcher = home.join(".local/bin/claude");
            std::os::unix::fs::symlink(&real, &launcher).unwrap();
            let instance = ManagerInstance {
                exe_path: launcher,
                prefix: home.join(".local/share/claude"),
                ..crate::testing::manager_instance("standalone-claude", "standalone-claude")
            };
            Claude { home, instance }
        }

        fn env(&self) -> HostEnv {
            HostEnv {
                path_dirs: vec![self.home.join(".local/bin")],
                home: self.home.clone(),
                euid: 501,
                cargo_home: None,
                rustup_home: None,
                zdotdir: None,
                ollama_host: None,
            }
        }

        fn row(&self, commands: Vec<CommandFact>) -> InstalledArtifact {
            InstalledArtifact {
                key: crate::model::ArtifactKey {
                    instance_id: "standalone-claude".to_string(),
                    kind: ArtifactKind::Binary,
                    name: "claude".to_string(),
                },
                display_name: "Claude Code".to_string(),
                version: "2.1.281".to_string(),
                reason: InstallReason::Requested,
                description: None,
                homepage: None,
                size_bytes: None,
                installed_at: None,
                path: None,
                auto_updates: false,
                uninstall_blocked: None,
                facts: crate::model::ArtifactFacts {
                    commands,
                    ..Default::default()
                },
            }
        }
    }

    impl Drop for Claude {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.home);
        }
    }

    fn runs(name: &str) -> CommandFact {
        CommandFact {
            name: name.to_string(),
            state: Some(CommandState::Runs),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_folders_read_in_time_are_used_however_long_the_rounds_fan_out_took() {
        // The read starts as the fan-out does; a Homebrew whose `brew
        // update` took minutes comes back long after the read's deadline.
        // A read that had finished by then still counts: the deadline is
        // for a read that has not.
        let claude = Claude::new("late-round");
        let in_flight = Arc::new(AtomicBool::new(false));
        let budget = CommandBudget::default();
        let mut reading = start_reading(
            &claude.env(),
            std::slice::from_ref(&claude.instance),
            true,
            &in_flight,
            budget,
        );
        while in_flight.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        reading.started = Instant::now()
            .checked_sub(Duration::from_secs(600))
            .unwrap_or(reading.started);
        let mut rows = vec![claude.row(Vec::new())];
        finish(
            reading,
            std::slice::from_ref(&claude.instance),
            &mut rows,
            &claude.home,
            true,
            &in_flight,
            budget,
        )
        .await;
        assert_eq!(rows[0].facts.commands, vec![runs("claude")]);
        assert!(!in_flight.load(Ordering::SeqCst));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_a_read_still_running_from_an_earlier_round_is_not_joined_and_carried_rows_keep_theirs(
    ) {
        // An earlier round's read stuck on a folder that stopped answering:
        // this round starts none, says nothing about the rows its
        // inventories listed, and leaves a row carried from before as it
        // was.
        let claude = Claude::new("stuck");
        let in_flight = Arc::new(AtomicBool::new(true));
        let budget = CommandBudget::default();
        let reading = start_reading(
            &claude.env(),
            std::slice::from_ref(&claude.instance),
            true,
            &in_flight,
            budget,
        );
        assert!(reading.handle.is_none());
        let carried = claude.row(vec![CommandFact {
            name: "claude".to_string(),
            state: Some(CommandState::NotOnPath {
                dir: "~/.local/bin".to_string(),
            }),
        }]);
        let mut rows = vec![claude.row(Vec::new()), carried.clone()];
        finish(
            reading,
            std::slice::from_ref(&claude.instance),
            &mut rows,
            &claude.home,
            true,
            &in_flight,
            budget,
        )
        .await;
        assert_eq!(rows[0].facts.commands, Vec::new());
        assert_eq!(rows[1], carried);
        // Still the stuck read's to clear.
        assert!(in_flight.load(Ordering::SeqCst));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_a_round_writes_only_the_rows_its_inventories_listed() {
        // A fresh row is judged; a carried one keeps the verdict it had,
        // even where this round would say otherwise.
        let claude = Claude::new("fresh-and-carried");
        let in_flight = Arc::new(AtomicBool::new(false));
        let budget = CommandBudget::default();
        let reading = start_reading(
            &claude.env(),
            std::slice::from_ref(&claude.instance),
            true,
            &in_flight,
            budget,
        );
        let carried = claude.row(vec![CommandFact {
            name: "claude".to_string(),
            state: Some(CommandState::ShadowedBy { by: None }),
        }]);
        let mut rows = vec![claude.row(Vec::new()), carried.clone()];
        finish(
            reading,
            std::slice::from_ref(&claude.instance),
            &mut rows,
            &claude.home,
            true,
            &in_flight,
            budget,
        )
        .await;
        assert_eq!(rows[0].facts.commands, vec![runs("claude")]);
        assert_eq!(rows[1], carried);
    }
}
