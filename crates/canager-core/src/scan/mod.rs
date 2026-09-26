//! The unknown-source scan: the command-line programs on this Mac that
//! none of the registered sources installed (design spec §4.2; phase 4
//! spec §八).
//!
//! Not an `Adapter`, on purpose. An adapter has a `plan`, an `execute`, a
//! `reconcile`, an instance and a fixture directory of recorded output,
//! and this has none of them; registering it as one would trip the
//! fixture-set equality test, put it in the operation manager's registry
//! and make the plan gate answer "is this actionable?" for something
//! that can never be. The decisive reason is simpler: deciding who owns a
//! program needs *every other* source's instances and artifacts, and an
//! adapter's `inventory(&self, inst)` sees only its own. So this is a
//! pure function over a clone of the snapshot (`Session::scan_unknown`),
//! run on demand from the Unknown page -- never from a refresh, never
//! into the `Snapshot`, never under a lock.
//!
//! Read-only in the strictest sense: `read_dir`, `symlink_metadata`,
//! `metadata`, `read_link` and `canonicalize`, one level deep, over a
//! fixed list of bin directories. No `CommandRunner`, so nothing it finds
//! is ever run; no write of any kind.
//!
//! Under `scan/`, not `adapters/unknown.rs` as the design spec's §3 drew
//! it: someone reading `adapters/` should not find a module there with no
//! `impl Adapter` (phase 4 spec §8.1, Q12; its appendix C records the
//! deviation).

use crate::model::{InstalledArtifact, InstanceId, ManagerInstance, RemovedWhat};
use crate::runner::HostEnv;
use serde::{Deserialize, Serialize};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

/// How much of the file system one scan may look at before it stops and
/// says so. Runtime values rather than constants, so the two numbers the
/// user reads in the "this list may be incomplete" banner come from the
/// same place the scan enforced them (`ScanStop` carries them out;
/// `Fault::HomebrewStillUpdating { minutes }` in model.rs is the
/// precedent for putting the number in the payload).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanBudget {
    /// Directory entries examined, counting the ones a known source
    /// claimed and the ones skipped as subdirectories or non-executables.
    pub max_entries: usize,
    /// Wall-clock time from the start of the walk -- the clock starts
    /// after the known sources are indexed, so canonicalising their paths
    /// is not charged to it -- checked before every `read_dir` and before
    /// every entry.
    pub max_duration: Duration,
}

impl Default for ScanBudget {
    /// The design spec's §4.2 numbers. Sized for a `~/bin` of a few
    /// thousand files; the seven directories on the research machine held
    /// 26 entries between them and took 64 ms.
    fn default() -> ScanBudget {
        ScanBudget {
            max_entries: 2000,
            max_duration: Duration::from_secs(10),
        }
    }
}

/// Why a scan stopped before it had looked at everything. Read by the
/// Unknown page's banner (`unknown.stopped.FileLimit` /
/// `unknown.stopped.TimeLimit`), which prints the number carried here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScanStop {
    /// `ScanBudget::max_entries` entries had been examined and there was
    /// another.
    FileLimit { max_entries: u32 },
    /// `ScanBudget::max_duration` had elapsed before the next `read_dir`
    /// or the next entry.
    TimeLimit { max_secs: u32 },
}

/// One directory the scan actually read, and how many of its entries it
/// examined (claimed, listed or skipped alike). The page's "Looked in:"
/// footer, so an empty list reads as "looked in seven places", not
/// "didn't look".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScannedDir {
    /// With the home directory abbreviated to `~`, on this side, for the
    /// reason `Warning`'s `{{path}}` payloads are: the front end has no
    /// `HOME` to strip, and this is data, not a sentence.
    pub path: PathBuf,
    pub entries: u32,
}

/// A file-name pattern for the backup copies a tool's own updater leaves
/// beside its launcher -- `~/.local/bin/agy.<time>.old` (agy.md §4, spec
/// §3.5) -- as `prefix` + something + `suffix` on a regular file directly
/// in `dir`; no glob crate. Defined here rather than beside the recipes
/// because this scan reads it (rule 4) and `adapters` depends on `scan`,
/// never the reverse (spec §3.1). Read by `Known::index`/`claimant` (rule 4,
/// through `Recipe.backup_globs` via `Session::scan_unknown`) and by
/// `adapters::standalone::removal::listed_items` (check 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glob {
    /// `~/…`: the one directory the pattern applies to, never recursed.
    pub dir: &'static str,
    pub prefix: &'static str,
    pub suffix: &'static str,
    /// What a match is, for the uninstall preview's sentence
    /// (`Warning::WillTrash`).
    pub what: RemovedWhat,
}

impl Glob {
    /// `dir` under `home`, joined the way a recipe path is
    /// (`adapters::standalone::route::expand`): the same spelling rule, so
    /// an instance's raw `exe_path` and a pattern's directory come from
    /// one home. A `dir` not starting with `~/` is a programming error in a
    /// recipe constant, which
    /// `recipes::tests::test_every_backup_glob_is_under_home_and_names_a_pattern`
    /// catches before this can.
    pub fn dir_under(&self, home: &Path) -> PathBuf {
        let rest = self
            .dir
            .strip_prefix("~/")
            .unwrap_or_else(|| panic!("glob dir {:?} must start with ~/", self.dir));
        home.join(rest)
    }

    /// Whether a file named `name` is one of this pattern's: `prefix`
    /// first, `suffix` last, and at least one character between -- so a
    /// name that is exactly `prefix + suffix` (`agy..old`) is not.
    pub fn matches_name(&self, name: &str) -> bool {
        name.len() > self.prefix.len() + self.suffix.len()
            && name.starts_with(self.prefix)
            && name.ends_with(self.suffix)
    }
}

/// What one listed entry is. Read by the page's kind badge
/// (`unknown.kind.*`, through a `Record<EntryKind, string>` so a variant
/// added here without copy fails `tsc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    File,
    Symlink,
    /// A symlink whose target `canonicalize` could not reach. `link_target`
    /// still carries what it says; the research machine had one pointing
    /// into an app that had since been deleted.
    BrokenSymlink,
}

/// One program no registered source accounts for. Every field is read by
/// the Unknown page (`src/pages/UnknownPage.tsx`); the comment on each
/// says where.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnknownEntry {
    /// The entry as found, `~`-abbreviated like `ScannedDir::path`. The
    /// row's name is its last component; the row's first line is the path.
    pub path: PathBuf,
    /// The badge.
    pub kind: EntryKind,
    /// `canonicalize` of the entry: every link hop followed, absolute, never
    /// abbreviated. `None` for a broken link, and for the rare regular
    /// file whose parent cannot be resolved. Shown under technical details
    /// as "Links to …" for a `Symlink`.
    pub resolved: Option<PathBuf>,
    /// `readlink`'s text, verbatim, for links only -- relative or absolute
    /// as the installer wrote it. The `{{target}}` of the broken-link
    /// sentence.
    pub link_target: Option<String>,
    /// The target's size. `None` for a broken link: there is no target to
    /// measure. Formatted by `formatBytes` into the size · date subtitle.
    pub size_bytes: Option<u64>,
    /// The target's modification time, unix seconds. `None` for a broken
    /// link, whose own `mtime` would only say when the link was made.
    /// Formatted with `Intl.DateTimeFormat` (an absolute date; this
    /// repository deliberately has no relative-time formatter).
    pub modified_at: Option<i64>,
    /// Whether the entry itself belongs to the user Canager runs as
    /// (`st_uid == euid`, of the entry, not its target: the question is
    /// who put it here). `false` renders "Put here by an installer with
    /// administrator rights".
    pub owned_by_me: bool,
    /// The `.app` bundle any component of the path runs inside, without
    /// the `.app`; tried on `resolved`, then on a link's own text (the
    /// only path a broken link has), then on the entry's path. Renders
    /// "Part of {{app}}".
    pub app_bundle: Option<String>,
}

/// The result of one scan. Not the `Snapshot`'s: produced on demand by
/// `Session::scan_unknown`, returned by the `scan_unknown` IPC command,
/// held only by the Unknown page's query.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnknownScan {
    /// Every directory actually read, in scan order. Directories that do
    /// not exist are not here (29 of the research machine's 36 candidates
    /// did not).
    pub scanned: Vec<ScannedDir>,
    /// The programs nobody claimed: the rows.
    pub entries: Vec<UnknownEntry>,
    /// How many examined programs a registered source accounted for and
    /// are therefore not listed. The page's "N more programs came from
    /// sources Canager knows" sentence.
    pub attributed: u32,
    /// `Some` when the scan hit its budget; the page's banner. What was
    /// scanned before that point is still in the fields above.
    pub stopped: Option<ScanStop>,
}

/// The directories one scan looks at: the design spec's §4.2 seven,
/// `$CARGO_HOME/bin` when the host sets `CARGO_HOME` -- resolved the way
/// `CargoAdapter` resolves it (cargo.rs:133-136) but *added alongside*
/// `~/.cargo/bin` where cargo substitutes it; with it set the proxies
/// live under the override and `~/.cargo/bin` is usually absent, and an
/// absent directory costs nothing -- and every `PATH` entry under the
/// home directory. Raw, in this order,
/// duplicates included: `scan_dirs` drops the ones that do not exist and
/// reads each distinct directory once, by canonical path, so a `PATH`
/// that names `~/.local/bin` twice, or through a link, costs one read.
///
/// Only `PATH` entries under `home` are taken. The rest --
/// `/opt/homebrew/bin`, `/usr/bin` -- are Homebrew's and macOS's, and
/// not what this page is for. Which `PATH` that is depends on how Canager
/// was launched (`fix_path_env` restores a login shell's for a Finder
/// launch; a terminal launch inherits that terminal's, temporary agent
/// directories and all); the research machine's `PATH` held 23 entries
/// that did not exist a session later, which is why missing directories
/// are silently skipped rather than reported.
fn candidate_dirs(env: &HostEnv) -> Vec<PathBuf> {
    let home = &env.home;
    let mut dirs = vec![
        home.join(".local/bin"),
        home.join("bin"),
        PathBuf::from("/usr/local/bin"),
        home.join(".cargo/bin"),
        home.join("go/bin"),
        home.join(".bun/bin"),
        home.join(".deno/bin"),
    ];
    if let Some(cargo_home) = &env.cargo_home {
        dirs.push(cargo_home.join("bin"));
    }
    dirs.extend(
        env.path_dirs
            .iter()
            .filter(|dir| dir.starts_with(home))
            .cloned(),
    );
    dirs
}

/// `path` with the home directory replaced by `~`, for the two wire
/// fields the page shows as they are (`ScannedDir::path`,
/// `UnknownEntry::path`). `resolved` is never passed through this: it is
/// the technical detail, and stays canonical and absolute. Attribution
/// compares absolute paths; only the output is abbreviated.
/// Also the one `~` rule for the sentences a path-list uninstall sends
/// (`adapters::standalone::removal`): data the user reads, never a path
/// anything acts on.
pub(crate) fn display_path(path: &Path, home: &Path) -> PathBuf {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => PathBuf::from("~"),
        Ok(rest) => Path::new("~").join(rest),
        Err(_) => path.to_path_buf(),
    }
}

/// The name of the `.app` bundle a path runs inside, if any component of
/// any candidate ends in `.app`: `/Applications/Helper.app/Contents/x`
/// gives `Helper`. Candidates are tried in order -- the real path first,
/// then a broken link's own text (the only path a broken link has), then
/// the entry's own path.
fn app_bundle<'a>(candidates: impl IntoIterator<Item = &'a Path>) -> Option<String> {
    for path in candidates {
        for component in path.components() {
            if let Component::Normal(part) = component {
                let part = part.to_string_lossy();
                if let Some(name) = part.strip_suffix(".app") {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

/// The directories a source *owns*: whatever resolves to a path under one
/// of them was put there by that source. Keyed by adapter id here,
/// pending the `Adapter::owned_roots()` trait method the phase 4 spec's
/// §十一 records as this table's eventual home.
///
/// Deliberately not `ManagerInstance.prefix`. For uv, pipx and pip the
/// prefix is `exe_path.parent()` (uv.rs:138-141, pipx.rs:213-216,
/// pip.rs:125-128): a pip instance detected through
/// `~/.local/bin/python3.12` has prefix `~/.local/bin`, and treating that
/// as owned would claim every unrelated program in the one directory
/// this scan exists to look at. Homebrew's prefix is all of
/// `/opt/homebrew` or `/usr/local`, which on an Intel Mac would swallow
/// whatever a third-party installer dropped into `/usr/local/bin` --
/// programs `brew info --installed` never lists. npm's prefix is the
/// global prefix root `npm prefix -g` reports (npm.rs:157-158; the
/// `exe_path.parent()` at npm.rs:194-197 is its `NotResponding` arm
/// only), and what npm owns under it is `lib/node_modules`, not `bin`.
///
/// The standalone adapters own their tool roots -- `standalone-claude` →
/// `~/.local/share/claude`, `standalone-agy` →
/// `~/.gemini/antigravity-cli`, `standalone-grok` → `~/.grok`, each the
/// instance's `prefix`; `standalone-rustup` nothing (its root is the
/// Cargo home, whose `bin/` is scanned; rule 1 has the launcher and its
/// proxies, rule 2 the `cargo install`ed programs). A row here with no
/// adapter that can produce its instance would be a definition without a
/// producer (spec §十).
pub fn owned_roots(inst: &ManagerInstance) -> Vec<PathBuf> {
    match inst.adapter_id.as_str() {
        // Not `/Applications`: a cask claims its own `.app` through rule
        // 2 (`InstalledArtifact.path`, brew/parse.rs), and the directory
        // as a whole holds every other installer's apps too.
        "brew" => vec![
            inst.prefix.join("Cellar"),
            inst.prefix.join("Caskroom"),
            inst.prefix.join("opt"),
        ],
        // `~/.ollama`: nothing in a bin directory resolves into it today;
        // listed so the table says what Ollama owns, not by omission.
        "ollama" => vec![inst.prefix.clone()],
        // npm unpacks every global package under `<prefix>/lib/node_modules`
        // (npm.rs:50, `real_prefix_is_writable`), and `<prefix>/bin/<tool>`
        // is a link into it -- with a home prefix such as `~/.npm-global`
        // that bin directory is on `PATH` and scanned. Not `<prefix>/bin`
        // itself: on `/usr/local` it is where third-party installers drop
        // things, exactly as for Homebrew above. A `NotResponding` npm has
        // `prefix = exe_path.parent()`; the root derived from that does
        // not exist and is simply absent from the index.
        "npm" => vec![inst.prefix.join("lib").join("node_modules")],
        // A tool installed by its own installer owns its root: Claude Code's
        // `~/.local/share/claude` (the `versions/<v>` store its launcher
        // links into), Antigravity's `~/.gemini/antigravity-cli`, Grok's
        // `~/.grok` (whose `downloads/` its two links resolve into). The
        // launcher itself is the instance's `exe_path` and rules 0/1 have
        // it; this row is for anything else that resolves under the root.
        // `standalone-rustup` never joins: its root is the Cargo home, whose
        // `bin/` is scanned; rule 1 has its launcher and proxies, rule 2 the
        // `cargo install`ed programs.
        "standalone-claude" | "standalone-agy" | "standalone-grok" => vec![inst.prefix.clone()],
        // cargo: `$CARGO_HOME` holds `bin/`, the very directory being
        // scanned; rule 1 places the proxies and rule 2 places
        // `cargo install`ed binaries. uv and (from Task 3b) pipx: rule 2,
        // through the tool venv their artifacts carry. pip: a
        // `parent()`-derived prefix, never a root.
        _ => Vec::new(),
    }
}

/// What the registered sources have said is theirs, indexed once per scan
/// so the rules are lookups rather than a `canonicalize` per entry per
/// instance. Built from a clone of the snapshot (`Session::scan_unknown`):
/// a refresh committing meanwhile does not move it.
///
/// The rules, in order; the first that matches wins (spec §8.3):
///
/// 0. The entry *is* an instance's `exe_path`, byte for byte, no
///    `canonicalize`. This is what catches a launcher that is a dangling
///    symlink (step B's `InstanceNote::LauncherOnly`, the state a
///    stopped uninstall leaves): `canonicalize` fails on it, so rules
///    1-3 cannot see it, and it would otherwise be listed as a broken
///    link here while also being a source on the Installed page.
/// 1. The entry resolves to the same file an instance's `exe_path`
///    resolves to. rustup's thirteen proxies in `~/.cargo/bin` are
///    relative links to `rustup`, and so is the cargo instance's own
///    `cargo`; grok's `agent` and `grok` links resolve to one download.
/// 2. The entry resolves to a path *under* an artifact's
///    `InstalledArtifact.path` (equal, when that path is a file). uv is
///    the first real input: its `path` is the tool's venv directory
///    (uv.rs:65) and the shim resolves to `<venv>/bin/<tool>`, so equality
///    would never match (§十三 #35). brew fills it for a cask with the
///    `.app` the cask's `app` stanza was moved to (`brew/parse.rs`, from
///    `brew info`'s `artifacts`): the cask's `binary` link in
///    `<prefix>/bin` -- `code ->
///    /Applications/Visual Studio Code.app/Contents/Resources/app/bin/code`
///    -- resolves into that bundle and under none of the roots rule 3
///    gives Homebrew, in a directory every scan reads. cargo fills `path`
///    with the program each crate installed, which only this rule places
///    (`hexyl` resolves to no instance's `exe_path`); the standalone
///    adapters fill it from step B, and for those rules 1 and 2 compare
///    the same file and rule 2 decides nothing new.
/// 3. The entry resolves to a path under a directory the instance's
///    adapter *owns* -- `owned_roots`, the longest matching root when
///    roots nest (`owned` below).
/// 4. The entry is a regular file in a directory an *installed* tool's
///    `backup_globs` name, and its name matches one of them (`agy.<time>.old`
///    in `~/.local/bin`): the tool's own updater left it. Read from
///    `Recipe.backup_globs` (phase 4 step D); without the instance, listed.
struct Known {
    exe_raw: Vec<(PathBuf, InstanceId)>,
    exe_canonical: Vec<(PathBuf, InstanceId)>,
    artifact_roots: Vec<(PathBuf, InstanceId)>,
    /// Rule 3: every `owned_roots` of every instance, canonical, with the
    /// instance that owns it. A root that does not exist (Homebrew with
    /// no casks has no `Caskroom`) is simply absent.
    owned: Vec<(PathBuf, InstanceId)>,
    /// Rule 4: for every instance whose adapter declares backup-file
    /// patterns (`Recipe.backup_globs`, handed in by `Session::scan_unknown`
    /// keyed by adapter id), each pattern's directory, canonical, with the
    /// pattern and the instance. A directory that does not exist is simply
    /// absent; a tool with no instance contributes nothing, so its leftover
    /// backup is listed.
    backups: Vec<(PathBuf, Glob, InstanceId)>,
}

impl Known {
    fn index(
        instances: &[ManagerInstance],
        artifacts: &[InstalledArtifact],
        globs: &[(String, &'static [Glob])],
        home: &Path,
    ) -> Known {
        let mut exe_raw = Vec::with_capacity(instances.len());
        let mut exe_canonical = Vec::with_capacity(instances.len());
        for inst in instances {
            exe_raw.push((inst.exe_path.clone(), inst.id.clone()));
            if let Ok(canonical) = std::fs::canonicalize(&inst.exe_path) {
                exe_canonical.push((canonical, inst.id.clone()));
            }
        }
        let artifact_roots = artifacts
            .iter()
            .filter_map(|artifact| {
                let path = artifact.path.as_ref()?;
                let canonical = std::fs::canonicalize(path).ok()?;
                Some((canonical, artifact.key.instance_id.clone()))
            })
            .collect();
        let owned = instances
            .iter()
            .flat_map(|inst| {
                owned_roots(inst).into_iter().filter_map(move |root| {
                    let canonical = std::fs::canonicalize(root).ok()?;
                    Some((canonical, inst.id.clone()))
                })
            })
            .collect();
        let backups = instances
            .iter()
            .flat_map(|inst| {
                globs
                    .iter()
                    .filter(move |(adapter_id, _)| *adapter_id == inst.adapter_id)
                    .flat_map(move |(_, patterns)| {
                        patterns.iter().filter_map(move |glob| {
                            let dir = std::fs::canonicalize(glob.dir_under(home)).ok()?;
                            Some((dir, *glob, inst.id.clone()))
                        })
                    })
            })
            .collect();
        Known {
            exe_raw,
            exe_canonical,
            artifact_roots,
            owned,
            backups,
        }
    }

    /// The source that put `raw` (in the canonical directory `dir`; real
    /// path `resolved`, `None` for a broken link; `kind` what it is) there,
    /// by the first rule that matches -- or `None`: unknown.
    fn claimant(
        &self,
        raw: &Path,
        dir: &Path,
        resolved: Option<&Path>,
        kind: EntryKind,
    ) -> Option<&InstanceId> {
        if let Some((_, id)) = self.exe_raw.iter().find(|(exe, _)| exe == raw) {
            return Some(id);
        }
        if let Some(resolved) = resolved {
            if let Some((_, id)) = self.exe_canonical.iter().find(|(exe, _)| exe == resolved) {
                return Some(id);
            }
            if let Some((_, id)) = self
                .artifact_roots
                .iter()
                .find(|(root, _)| resolved.starts_with(root))
            {
                return Some(id);
            }
            // The longest matching root: the closest owner when roots nest.
            if let Some((_, id)) = self
                .owned
                .iter()
                .filter(|(root, _)| resolved.starts_with(root))
                .max_by_key(|(root, _)| root.as_os_str().len())
            {
                return Some(id);
            }
        }
        // Rule 4: a backup the tool's own updater left, by name, in the
        // pattern's directory, a regular file -- a link of that name is
        // somebody's link, not the updater's copy.
        if kind == EntryKind::File {
            if let Some(name) = raw.file_name().and_then(|name| name.to_str()) {
                if let Some((_, _, id)) = self
                    .backups
                    .iter()
                    .find(|(glob_dir, glob, _)| glob_dir == dir && glob.matches_name(name))
                {
                    return Some(id);
                }
            }
        }
        None
    }
}

/// One directory entry as the page will describe it, or `None` for the
/// ones the scan does not list at all: a subdirectory (depth 1, never
/// recursed -- `~/Library/pnpm` on the research machine held `bin/` and
/// `store/`), a link to a directory, a file with no execute bit in any
/// position (checked on the target; a theoretical boundary, the
/// research machine had none), anything that is neither a file nor a
/// link, and an entry whose `lstat` failed -- one failed `stat` costs that
/// entry and nothing else. Every file-system read of the scan is here or
/// in `scan_dirs`'s `read_dir`.
fn examine(raw: &Path, home: &Path, euid: u32) -> Option<UnknownEntry> {
    let lstat = std::fs::symlink_metadata(raw).ok()?;
    let file_type = lstat.file_type();
    let (kind, resolved, link_target) = if file_type.is_symlink() {
        let link_target = std::fs::read_link(raw)
            .ok()
            .map(|target| target.to_string_lossy().into_owned());
        match std::fs::canonicalize(raw) {
            Ok(resolved) => (EntryKind::Symlink, Some(resolved), link_target),
            Err(_) => (EntryKind::BrokenSymlink, None, link_target),
        }
    } else if file_type.is_file() {
        (EntryKind::File, std::fs::canonicalize(raw).ok(), None)
    } else {
        return None;
    };
    // Size, date and the executable check are the target's: a link's own
    // say only when the installer made the link.
    let target = match kind {
        EntryKind::BrokenSymlink => None,
        EntryKind::File | EntryKind::Symlink => Some(std::fs::metadata(raw).ok()?),
    };
    if let Some(target) = &target {
        if target.is_dir() || (target.mode() & 0o111) == 0 {
            return None;
        }
    }
    let (size_bytes, modified_at) = match &target {
        Some(target) => (Some(target.len()), Some(target.mtime())),
        None => (None, None),
    };
    let mut bundle_candidates: Vec<&Path> = Vec::new();
    if let Some(resolved) = &resolved {
        bundle_candidates.push(resolved);
    }
    if let Some(target) = &link_target {
        bundle_candidates.push(Path::new(target));
    }
    bundle_candidates.push(raw);
    let app_bundle = app_bundle(bundle_candidates);
    Some(UnknownEntry {
        path: display_path(raw, home),
        kind,
        resolved,
        link_target,
        size_bytes,
        modified_at,
        owned_by_me: lstat.uid() == euid,
        app_bundle,
    })
}

/// The scan over an explicit directory list. `scan_unknown` is what
/// production calls; this is what the synthetic-tree tests call, so a
/// test never reads the `/usr/local/bin` of the machine running it.
///
/// Directories that do not exist are skipped without a trace; each
/// distinct directory (by canonical path) is read once; entries are taken
/// in name order so a stop at the budget is reproducible. The time budget
/// is checked before every `read_dir`, and both limits before every entry
/// (`ScanBudget`); when one trips, what was examined so far is returned
/// as it is, with `stopped` saying which limit -- a directory whose first
/// entry tripped it is not reported as read. `globs` are the installed
/// tools' backup-file patterns by adapter id (rule 4).
pub fn scan_dirs(
    dirs: &[PathBuf],
    env: &HostEnv,
    instances: &[ManagerInstance],
    artifacts: &[InstalledArtifact],
    globs: &[(String, &'static [Glob])],
    budget: ScanBudget,
) -> UnknownScan {
    let file_stop = ScanStop::FileLimit {
        max_entries: u32::try_from(budget.max_entries).unwrap_or(u32::MAX),
    };
    let time_stop = ScanStop::TimeLimit {
        max_secs: u32::try_from(budget.max_duration.as_secs()).unwrap_or(u32::MAX),
    };
    let known = Known::index(instances, artifacts, globs, &env.home);
    // The clock starts here, after indexing: `ScanBudget::max_duration`
    // bounds the walk, not the `canonicalize` per known path above.
    let started = Instant::now();
    let mut scanned = Vec::new();
    let mut entries = Vec::new();
    let mut attributed = 0u32;
    let mut stopped = None;
    let mut examined = 0usize;
    let mut seen: Vec<PathBuf> = Vec::new();
    'dirs: for dir in dirs {
        let Ok(canonical) = std::fs::canonicalize(dir) else {
            continue;
        };
        if seen.contains(&canonical) {
            continue;
        }
        seen.push(canonical.clone());
        if started.elapsed() >= budget.max_duration {
            stopped = Some(time_stop.clone());
            break;
        }
        // Unreadable (permissions) is not "read": it is not reported either.
        let Ok(read) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut names: Vec<_> = read
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .collect();
        names.sort();
        let mut count = 0u32;
        for name in names {
            let over_budget = if examined >= budget.max_entries {
                Some(file_stop.clone())
            } else if started.elapsed() >= budget.max_duration {
                Some(time_stop.clone())
            } else {
                None
            };
            if let Some(stop) = over_budget {
                stopped = Some(stop);
                if count > 0 {
                    scanned.push(ScannedDir {
                        path: display_path(dir, &env.home),
                        entries: count,
                    });
                }
                break 'dirs;
            }
            examined += 1;
            count += 1;
            let raw = dir.join(name);
            let Some(entry) = examine(&raw, &env.home, env.euid) else {
                continue;
            };
            match known.claimant(&raw, &canonical, entry.resolved.as_deref(), entry.kind) {
                Some(_) => attributed += 1,
                None => entries.push(entry),
            }
        }
        scanned.push(ScannedDir {
            path: display_path(dir, &env.home),
            entries: count,
        });
    }
    UnknownScan {
        scanned,
        entries,
        attributed,
        stopped,
    }
}

/// The unknown-source scan: `scan_dirs` over `candidate_dirs(env)`. Pure
/// over its arguments and the file system; synchronous, and blocking for
/// up to `budget.max_duration` -- the Tauri shell runs it on the blocking
/// pool (`ipc::scan_unknown`). `instances` and `artifacts` are the
/// snapshot's, cloned by `Session::scan_unknown`. `globs` are the
/// installed tools' backup-file patterns by adapter id (rule 4).
pub fn scan_unknown(
    env: &HostEnv,
    instances: &[ManagerInstance],
    artifacts: &[InstalledArtifact],
    globs: &[(String, &'static [Glob])],
    budget: ScanBudget,
) -> UnknownScan {
    scan_dirs(
        &candidate_dirs(env),
        env,
        instances,
        artifacts,
        globs,
        budget,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::HostEnv;
    use std::path::Path;

    #[test]
    fn test_scan_budget_default_is_the_spec_numbers() {
        let budget = ScanBudget::default();
        assert_eq!(budget.max_entries, 2000);
        assert_eq!(budget.max_duration, Duration::from_secs(10));
    }

    #[test]
    fn test_scan_wire_shapes_match_the_hand_written_ts_mirror() {
        // `src/lib/types.ts` spells `EntryKind` as bare strings and
        // `ScanStop` as externally tagged single-key objects carrying the
        // limit the scan really enforced -- so `unknown.stopped.*` can
        // print that number rather than a copy typed into the locale
        // files (`Fault::HomebrewStillUpdating { minutes }` is the
        // precedent, model.rs).
        assert_eq!(
            serde_json::to_string(&EntryKind::File).unwrap(),
            r#""File""#
        );
        assert_eq!(
            serde_json::to_string(&EntryKind::Symlink).unwrap(),
            r#""Symlink""#
        );
        assert_eq!(
            serde_json::to_string(&EntryKind::BrokenSymlink).unwrap(),
            r#""BrokenSymlink""#
        );
        assert_eq!(
            serde_json::to_string(&ScanStop::FileLimit { max_entries: 2000 }).unwrap(),
            r#"{"FileLimit":{"max_entries":2000}}"#
        );
        assert_eq!(
            serde_json::to_string(&ScanStop::TimeLimit { max_secs: 10 }).unwrap(),
            r#"{"TimeLimit":{"max_secs":10}}"#
        );

        let scan = UnknownScan {
            scanned: vec![ScannedDir {
                path: PathBuf::from("~/.local/bin"),
                entries: 5,
            }],
            entries: vec![UnknownEntry {
                path: PathBuf::from("~/.local/bin/old-script"),
                kind: EntryKind::BrokenSymlink,
                resolved: None,
                link_target: Some(
                    "/Applications/Removed.app/Contents/Resources/index.js".to_string(),
                ),
                size_bytes: None,
                modified_at: None,
                owned_by_me: true,
                app_bundle: Some("Removed".to_string()),
            }],
            attributed: 4,
            stopped: None,
        };
        let json = serde_json::to_string(&scan).expect("serialize");
        assert!(
            json.contains(r#""stopped":null"#),
            "a complete scan carries an explicit null, not a missing key: {json}"
        );
        assert!(json.contains(r#""kind":"BrokenSymlink""#), "{json}");
        assert!(json.contains(r#""resolved":null"#), "{json}");
        assert!(json.contains(r#""owned_by_me":true"#), "{json}");
        assert_eq!(
            serde_json::from_str::<UnknownScan>(&json).expect("deserialize"),
            scan
        );

        let stopped = UnknownScan {
            stopped: Some(ScanStop::TimeLimit { max_secs: 10 }),
            ..scan
        };
        let json = serde_json::to_string(&stopped).expect("serialize");
        assert!(
            json.contains(r#""stopped":{"TimeLimit":{"max_secs":10}}"#),
            "{json}"
        );
        assert_eq!(
            serde_json::from_str::<UnknownScan>(&json).expect("deserialize"),
            stopped
        );
    }

    fn env(home: &str, path_dirs: &[&str], cargo_home: Option<&str>) -> HostEnv {
        HostEnv {
            path_dirs: path_dirs.iter().map(PathBuf::from).collect(),
            home: PathBuf::from(home),
            euid: 501,
            cargo_home: cargo_home.map(PathBuf::from),
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        }
    }

    #[test]
    fn test_candidate_dirs_are_the_seven_fixed_ones_plus_path_entries_under_home() {
        let dirs = candidate_dirs(&env(
            "/Users/someone",
            &[
                "/Users/someone/.opencode/bin",
                "/opt/homebrew/bin",
                "/usr/bin",
                "/Users/someone/.local/bin",
            ],
            None,
        ));
        let expected: Vec<PathBuf> = [
            "/Users/someone/.local/bin",
            "/Users/someone/bin",
            "/usr/local/bin",
            "/Users/someone/.cargo/bin",
            "/Users/someone/go/bin",
            "/Users/someone/.bun/bin",
            "/Users/someone/.deno/bin",
            "/Users/someone/.opencode/bin",
            // Raw: the duplicate is `scan_dirs`'s to drop, by canonical path.
            "/Users/someone/.local/bin",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        assert_eq!(dirs, expected);
    }

    #[test]
    fn test_candidate_dirs_add_cargo_home_bin_when_the_host_sets_it() {
        let dirs = candidate_dirs(&env("/Users/someone", &[], Some("/Volumes/Data/cargo")));
        assert!(
            dirs.contains(&PathBuf::from("/Volumes/Data/cargo/bin")),
            "{dirs:?}"
        );
        assert!(
            dirs.contains(&PathBuf::from("/Users/someone/.cargo/bin")),
            "{dirs:?}"
        );
    }

    #[test]
    fn test_display_path_abbreviates_home_and_only_home() {
        let home = Path::new("/Users/someone");
        assert_eq!(
            display_path(Path::new("/Users/someone/.local/bin/agy"), home),
            PathBuf::from("~/.local/bin/agy")
        );
        assert_eq!(display_path(home, home), PathBuf::from("~"));
        assert_eq!(
            display_path(Path::new("/usr/local/bin/helper"), home),
            PathBuf::from("/usr/local/bin/helper")
        );
        // A sibling that merely starts with the same characters is not under home.
        assert_eq!(
            display_path(Path::new("/Users/someone-else/bin/x"), home),
            PathBuf::from("/Users/someone-else/bin/x")
        );
    }

    #[test]
    fn test_app_bundle_takes_the_first_candidate_with_a_dot_app_component() {
        let none = app_bundle([Path::new("/Users/someone/.local/bin/agy")]);
        assert_eq!(none, None);
        let resolved = app_bundle([
            Path::new("/Applications/Helper.app/Contents/Helpers/helper-cli"),
            Path::new("/usr/local/bin/helper-cli"),
        ]);
        assert_eq!(resolved.as_deref(), Some("Helper"));
        // A broken link's own text, relative as the installer wrote it.
        let relative = app_bundle([Path::new("../../Removed.app/Contents/MacOS/x")]);
        assert_eq!(relative.as_deref(), Some("Removed"));
    }

    #[test]
    fn test_glob_matches_a_prefix_something_and_a_suffix_on_the_name_alone() {
        // agy's updater leaves `agy.<time>.old` beside the launcher
        // (spec §3.5). Something has to sit between prefix and suffix, so
        // `agy..old` and `agy.old` are not matches; the name is all that is
        // looked at here -- the kind of file is the caller's (`claimant`,
        // `removal::listed_items`).
        let glob = Glob {
            dir: "~/.local/bin",
            prefix: "agy.",
            suffix: ".old",
            what: RemovedWhat::Backups,
        };
        for name in [
            "agy.1727000000.old",
            "agy.2026-09-25T12-25-00.old",
            "agy.x.old",
        ] {
            assert!(glob.matches_name(name), "{name}");
        }
        for name in [
            "agy",
            "agy.old",
            "agy..old",
            "agy.1727000000.old.bak",
            "xagy.1.old",
            "agy.1.OLD",
        ] {
            assert!(!glob.matches_name(name), "{name}");
        }
    }

    #[test]
    fn test_glob_dir_under_joins_like_a_recipe_path() {
        // The scan compares raw spellings (F's rule 0, `route::expand`'s
        // doc), so the pattern's directory is spelled off the same home.
        let glob = Glob {
            dir: "~/.local/bin",
            prefix: "agy.",
            suffix: ".old",
            what: RemovedWhat::Backups,
        };
        assert_eq!(
            glob.dir_under(Path::new("/Users/someone")),
            PathBuf::from("/Users/someone/.local/bin")
        );
        assert_eq!(
            glob.dir_under(Path::new("/Volumes/Data/homes/someone")),
            PathBuf::from("/Volumes/Data/homes/someone/.local/bin")
        );
    }

    #[test]
    fn test_owned_roots_table() {
        let brew = ManagerInstance {
            prefix: PathBuf::from("/opt/homebrew"),
            ..crate::testing::manager_instance("brew", "brew:/opt/homebrew")
        };
        assert_eq!(
            owned_roots(&brew),
            vec![
                PathBuf::from("/opt/homebrew/Cellar"),
                PathBuf::from("/opt/homebrew/Caskroom"),
                PathBuf::from("/opt/homebrew/opt"),
            ]
        );
        let ollama = ManagerInstance {
            prefix: PathBuf::from("/Users/someone/.ollama"),
            ..crate::testing::manager_instance("ollama", "ollama:http://127.0.0.1:11434")
        };
        assert_eq!(
            owned_roots(&ollama),
            vec![PathBuf::from("/Users/someone/.ollama")]
        );
        // npm: where global packages unpack and every bin link points
        // (npm.rs:50). Not `<prefix>/bin`, which on `/usr/local` is where
        // third-party installers drop things.
        let npm = ManagerInstance {
            prefix: PathBuf::from("/usr/local"),
            ..crate::testing::manager_instance("npm", "npm:/usr/local")
        };
        assert_eq!(
            owned_roots(&npm),
            vec![PathBuf::from("/usr/local/lib/node_modules")]
        );
        // A standalone tool owns its root: the launcher is the instance's
        // `exe_path` (rules 0/1), the `versions/<v>` store under the root
        // is this row's (phase 4 step B; agy and grok join with step D).
        let claude = ManagerInstance {
            prefix: PathBuf::from("/Users/someone/.local/share/claude"),
            ..crate::testing::manager_instance("standalone-claude", "standalone-claude")
        };
        assert_eq!(
            owned_roots(&claude),
            vec![PathBuf::from("/Users/someone/.local/share/claude")]
        );
        // The two other path-list tools own their roots the same way
        // (phase 4 step D): agy's `~/.gemini/antigravity-cli`, grok's
        // `~/.grok`, each the instance's `prefix`.
        for (adapter, prefix) in [
            ("standalone-agy", "/Users/someone/.gemini/antigravity-cli"),
            ("standalone-grok", "/Users/someone/.grok"),
        ] {
            let inst = ManagerInstance {
                prefix: PathBuf::from(prefix),
                ..crate::testing::manager_instance(adapter, adapter)
            };
            assert_eq!(owned_roots(&inst), vec![PathBuf::from(prefix)], "{adapter}");
        }
        // A `parent()`-derived prefix, or `$CARGO_HOME`, is never a root.
        for (adapter, id, prefix) in [
            (
                "cargo",
                "cargo:/Users/someone/.cargo",
                "/Users/someone/.cargo",
            ),
            ("uv", "uv", "/Users/someone/.local/bin"),
            ("pipx", "pipx", "/Users/someone/.local/bin"),
            ("pip", "pip:/usr/bin/python3", "/usr/bin"),
            // rustup's root is the Cargo home, whose `bin/` is the very
            // directory being scanned: nothing of rustup's is placed by
            // its prefix. rustup itself and its thirteen proxies resolve
            // to the launcher (rule 1); `cargo install`ed programs carry
            // their path on cargo's artifacts (rule 2).
            (
                "standalone-rustup",
                "standalone-rustup",
                "/Users/someone/.cargo",
            ),
        ] {
            let inst = ManagerInstance {
                prefix: PathBuf::from(prefix),
                ..crate::testing::manager_instance(adapter, id)
            };
            assert_eq!(owned_roots(&inst), Vec::<PathBuf>::new(), "{adapter}");
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let raw = std::env::temp_dir().join(format!(
            "canager-scan-unit-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&raw).expect("create temp dir");
        std::fs::canonicalize(&raw).expect("canonical temp dir")
    }

    #[test]
    fn test_the_longest_owned_root_wins() {
        // Two Ollama instances (two hosts) whose roots nest: the one whose
        // root is the longer prefix of the entry is the closer owner.
        let tmp = temp_dir("longest-root");
        let outer = tmp.join("outer");
        let inner = outer.join("inner");
        let inner_bin = inner.join("bin");
        std::fs::create_dir_all(&inner_bin).expect("create dirs");
        let entry = inner_bin.join("x");
        std::fs::write(&entry, b"x").expect("write");
        let outer_inst = ManagerInstance {
            prefix: outer.clone(),
            ..crate::testing::manager_instance("ollama", "ollama:http://outer:11434")
        };
        let inner_inst = ManagerInstance {
            prefix: inner.clone(),
            ..crate::testing::manager_instance("ollama", "ollama:http://inner:11434")
        };
        let known = Known::index(&[outer_inst, inner_inst], &[], &[], &tmp);
        assert_eq!(
            known
                .claimant(&entry, &inner_bin, Some(&entry), EntryKind::File)
                .map(String::as_str),
            Some("ollama:http://inner:11434")
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
