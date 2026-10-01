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
//! Read-only in the strictest sense: the folder listing, `lstat` and
//! `readlink`, one level deep, over a fixed list of bin directories. No
//! `CommandRunner`, so nothing it finds is ever run; no write of any kind.
//!
//! It keeps the promise the command check (`commands`) and the disk-use
//! measurement (`size`) keep: nothing inside a protected place
//! (`protected`: `~/Documents`, `~/Desktop`, `~/Downloads`, the media
//! folders, iCloud Drive and other cloud folders, other apps' containers,
//! and `/Volumes`, also as spelled from `/System/Volumes/Data`) is ever
//! listed, `lstat`ed, its links read or its path resolved, through any
//! link. Every path is found one step at a time from `/` and every step
//! checked before it is taken (`protected::resolve`); each scanned folder
//! is listed, and its entries looked at, through a descriptor held open on
//! it (`dirfd`), so a folder replaced by a link meanwhile is never
//! followed. A scanned folder in a protected place is not read and is
//! named on the page (`UnknownScan::protected_dirs`); an entry of a scanned
//! folder that is itself one (`Documents` in a home folder on `PATH`) is
//! passed over unlooked-at; a link into one is listed by its own name and
//! not followed (`EntryKind::ProtectedSymlink`).
//!
//! Under `scan/`, not `adapters/unknown.rs` as the design spec's §3 drew
//! it: someone reading `adapters/` should not find a module there with no
//! `impl Adapter` (phase 4 spec §8.1, Q12; its appendix C records the
//! deviation).

use crate::dirfd::Dir;
use crate::model::{InstalledArtifact, InstanceId, ManagerInstance, RemovedWhat};
use crate::protected::{self, Protected, Resolution};
use crate::runner::HostEnv;
use serde::{Deserialize, Serialize};
use std::ffi::{OsStr, OsString};
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
    /// A symlink whose target could not be reached: nothing there, a loop,
    /// or a folder on the way that cannot be searched. `link_target`
    /// still carries what it says; the research machine had one pointing
    /// into an app that had since been deleted.
    BrokenSymlink,
    /// A symlink that leads into a protected place (`protected`):
    /// `~/Documents`, iCloud Drive, `/Volumes` and the rest. Listed by its
    /// own name, and its target never followed -- so no `resolved`, size
    /// or date. `link_target` is its text, read from the link itself, in
    /// the folder being scanned; the page says 「指向受保护的位置」 in place
    /// of a path.
    ProtectedSymlink,
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
    /// Where the entry leads: every link hop followed (`protected::resolve`,
    /// as `realpath` would but never into a protected place, each name as
    /// spelled), absolute, never abbreviated. `None` for a broken link and
    /// for one that leads into a protected place. Shown under technical
    /// details as "Links to …" for a `Symlink`.
    pub resolved: Option<PathBuf>,
    /// `readlink`'s text, verbatim, for links only -- relative or absolute
    /// as the installer wrote it. The `{{target}}` of the broken-link
    /// sentence.
    pub link_target: Option<String>,
    /// The target's size. `None` for a broken link: there is no target to
    /// measure; and for a link into a protected place, whose target is
    /// never looked at. Formatted by `formatBytes` into the size · date subtitle.
    pub size_bytes: Option<u64>,
    /// The target's modification time, unix seconds. `None` for a broken
    /// link, whose own `mtime` would only say when the link was made, and
    /// for a link into a protected place.
    /// Formatted with `Intl.DateTimeFormat`, as an absolute date: when the
    /// file last changed, not how long ago.
    pub modified_at: Option<i64>,
    /// Whether the entry itself belongs to the user Banager runs as
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
    /// Every directory that is, or leads into, a protected place
    /// (`protected`), and so was not read: `~`-abbreviated like
    /// `ScannedDir::path`, as it was looked for (a `PATH` entry in
    /// `~/Documents`, or `~/bin` as a link to the Desktop), each place once.
    /// The page's 「有N个文件夹在受保护的位置，没有读取」. Defaulted when
    /// absent, so a scan from before it still reads.
    #[serde(default)]
    pub protected_dirs: Vec<PathBuf>,
    /// The programs nobody claimed: the rows.
    pub entries: Vec<UnknownEntry>,
    /// How many examined programs a registered source accounted for and
    /// are therefore not listed. The page's "N more programs came from
    /// sources Banager knows" sentence.
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
/// not what this page is for. Which `PATH` that is depends on how Banager
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
/// `~/.gemini/antigravity-cli`, `standalone-grok` → `~/.grok`,
/// `standalone-codex` → `~/.codex/packages/standalone`,
/// `standalone-opencode` → `~/.opencode`, each the
/// instance's `prefix`; `standalone-rustup` nothing (its root is the
/// Cargo home, whose `bin/` is scanned; rule 1 has the launcher and its
/// proxies, rule 2 the `cargo install`ed programs). uv owns the folder
/// its managed Pythons are installed in, `~/.local/share/uv/python` under
/// `home` (`uv_python_dir`). A row here with no
/// adapter that can produce its instance would be a definition without a
/// producer (spec §十).
pub fn owned_roots(inst: &ManagerInstance, home: &Path) -> Vec<PathBuf> {
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
        // Codex's `~/.codex/packages/standalone`, never `~/.codex`, where
        // its settings and sessions live: its `releases/` hold the program
        // both of its links in `~/.local/bin` resolve to.
        // opencode's `~/.opencode`: its script puts the program in its
        // `bin/`, and opencode keeps its own plugin packages beside it.
        "standalone-claude"
        | "standalone-agy"
        | "standalone-grok"
        | "standalone-codex"
        | "standalone-opencode" => vec![inst.prefix.clone()],
        // uv: the Pythons `uv python install` puts in its data folder,
        // whose executables it links from `~/.local/bin` (`python3.12 ->
        // ~/.local/share/uv/python/cpython-3.12-macos-aarch64-none/bin/
        // python3.12`). Never the prefix, which is `exe_path.parent()`.
        // Its tools are rule 2's, through the tool venv their artifacts
        // carry.
        "uv" => vec![uv_python_dir(home)],
        // cargo: `$CARGO_HOME` holds `bin/`, the very directory being
        // scanned; rule 1 places the proxies and rule 2 places
        // `cargo install`ed binaries. pipx (from Task 3b): rule 2, through
        // the tool venv its artifacts carry. pip: a `parent()`-derived
        // prefix, never a root.
        _ => Vec::new(),
    }
}

/// Where uv installs the Pythons it manages, by default: the `python/`
/// folder of its data folder, `~/.local/share/uv/python` (uv's docs,
/// docs.astral.sh/uv/reference/storage, "Python versions"; read as text,
/// uv never run). `UV_PYTHON_INSTALL_DIR` or `XDG_DATA_HOME` would move it,
/// but Banager is handed only the shell's `PATH` (`HostEnv`), so a Python
/// installed elsewhere is not placed and its link is listed.
pub fn uv_python_dir(home: &Path) -> PathBuf {
    home.join(".local/share/uv/python")
}

/// What the registered sources have said is theirs, indexed once per scan
/// so the rules are lookups rather than a resolve per entry per instance.
/// Built from a clone of the snapshot (`Session::scan_unknown`): a refresh
/// committing meanwhile does not move it.
///
/// "Resolves to" below is `leads_to`: every link followed one step at a
/// time, never into a protected place. Where an entry, an `exe_path`, an
/// artifact's path or an owned root is, or leads into, a protected place,
/// it is compared by the path as far as that place and the rest as written
/// -- by name, nothing there looked at. So an owned root on `/Volumes` (a
/// Homebrew installed on an external disk) still claims what links into
/// it by name, and is never turned into unknown programs; and a link that
/// leads into `~/Documents`, under no source's root by name, is listed.
/// Paths compare as a Mac's disk names them, case aside and whichever
/// spelling of the data volume names them (`same_place`, `is_under`).
///
/// The rules, in order; the first that matches wins (spec §8.3):
///
/// 0. The entry *is* an instance's `exe_path`, byte for byte, no
///    `canonicalize`. This is what catches a launcher that is a dangling
///    symlink (step B's `InstanceNote::LauncherOnly`, the state a
///    stopped uninstall leaves): `canonicalize` fails on it, so rules 2
///    and 3 cannot see it, and rule 1 only when `dead_end` can tell where
///    it would lead: without rule 0, a launcher `dead_end` cannot place
///    would be listed as a broken link here while also being a source on
///    the Installed page.
/// 1. The entry resolves to the same file an instance's `exe_path`
///    resolves to. rustup's thirteen proxies in `~/.cargo/bin` are
///    relative links to `rustup`, and so is the cargo instance's own
///    `cargo`; grok's `agent` and `grok` links resolve to one download.
///    Or, both leading nowhere, the entry would lead to the same missing
///    file as an instance's `exe_path` would (`dead_end`): the launcher-only
///    state again, where grok's `~/.grok/bin/agent` would lead to the
///    download its launcher would, and so would a fallback link the
///    installer made in `~/.local/bin`, whether its text names one of the
///    two or the download itself -- links grok's uninstall takes for
///    grok's too (`route::leads_to_program`) and moves with the launcher.
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
    /// Where each instance's `exe_path` leads (`leads_to`): every link
    /// followed, or -- for one that leads into a protected place -- the
    /// path as far as that, the rest as written.
    exe_canonical: Vec<(PathBuf, InstanceId)>,
    /// Rule 1 for a link that leads nowhere: where each instance's
    /// `exe_path` that leads nowhere would lead (`dead_end`), with the
    /// instance. Empty unless some instance's `exe_path` leads nowhere;
    /// while it is empty, `claimant` looks up no broken link's `dead_end`.
    exe_dead_ends: Vec<(PathBuf, InstanceId)>,
    artifact_roots: Vec<(PathBuf, InstanceId)>,
    /// Rule 3: every `owned_roots` of every instance, where it leads
    /// (`leads_to`), with the instance that owns it. A root that does not
    /// exist (Homebrew with no casks has no `Caskroom`) is simply absent;
    /// one in a protected place -- a Homebrew prefix on `/Volumes` -- is
    /// kept by its name, never looked at.
    owned: Vec<(PathBuf, InstanceId)>,
    /// Rule 4: for every instance whose adapter declares backup-file
    /// patterns (`Recipe.backup_globs`, handed in by `Session::scan_unknown`
    /// keyed by adapter id), each pattern's directory, where it leads, with
    /// the pattern and the instance. A directory that does not exist is
    /// simply absent; a tool with no instance contributes nothing, so its
    /// leftover backup is listed.
    backups: Vec<(PathBuf, Glob, InstanceId)>,
}

/// Where `path` leads, for attribution: every link on the way followed
/// one step at a time, never into a protected place (`protected::resolve`).
/// For a path that is, or leads into, a protected place, the path as far
/// as the links outside it were followed and the rest as written
/// (`Resolution::Protected`, a `..` in it folded by name): nothing there is
/// looked at, so it is compared by name alone -- and not at all once a
/// `..` climbs back out of the place (`by_name`). That is how a Homebrew whose prefix is on
/// `/Volumes` keeps its programs: the link in `/usr/local/bin` leads, by
/// name, under `/Volumes/<disk>/homebrew/Cellar`, and so does that root,
/// and neither is entered. `None` when nothing is there, or a folder on
/// the way cannot be searched.
fn leads_to(path: &Path, protected: &Protected) -> Option<PathBuf> {
    match protected::resolve(path, protected, true) {
        Resolution::Found(real, _) => Some(real),
        Resolution::Protected(at) => by_name(at, protected),
        Resolution::Missing | Resolution::Refused => None,
    }
}

/// A path `resolve` answered `Resolution::Protected` with, for comparing
/// by name: kept while it is still in a protected place, `None` once a
/// `..` after the protected place took it back out. `resolve` folds such a
/// `..` by name, but the place it climbs out of may itself be a link
/// elsewhere (`~/Documents` moved to another disk), so
/// `~/Documents/../.local/share/claude/x` may really lead anywhere; by
/// name it would pass for Claude Code's, and an odd link could hide an
/// unknown program behind it. A `..` that stays inside the place is still
/// folded by name, as the rest is taken as written.
fn by_name(at: PathBuf, protected: &Protected) -> Option<PathBuf> {
    protected.contains(&at).then_some(at)
}

/// Whether `a` and `b` are one place: compared as a Mac's disk compares
/// names, without regard to ASCII case (`resolve` keeps each name as it is
/// spelled, where `realpath` would answer the disk's spelling), and
/// whichever spelling of the data volume names either
/// (`protected::without_data_volume`).
fn same_place(a: &Path, b: &Path) -> bool {
    protected::same_path(
        &protected::without_data_volume(a),
        &protected::without_data_volume(b),
    )
}

/// Whether `path` is `root` or under it, compared as `same_place` does.
fn is_under(path: &Path, root: &Path) -> bool {
    protected::starts_with_folded(
        &protected::without_data_volume(path),
        &protected::without_data_volume(root),
    )
}

impl Known {
    fn index(
        instances: &[ManagerInstance],
        artifacts: &[InstalledArtifact],
        globs: &[(String, &'static [Glob])],
        home: &Path,
        protected: &Protected,
    ) -> Known {
        let mut exe_raw = Vec::with_capacity(instances.len());
        let mut exe_canonical = Vec::with_capacity(instances.len());
        let mut exe_dead_ends = Vec::new();
        for inst in instances {
            exe_raw.push((inst.exe_path.clone(), inst.id.clone()));
            match leads_to(&inst.exe_path, protected) {
                Some(leads) => exe_canonical.push((leads, inst.id.clone())),
                None => {
                    if let Some(end) = dead_end(&inst.exe_path, protected) {
                        exe_dead_ends.push((end, inst.id.clone()));
                    }
                }
            }
        }
        let artifact_roots = artifacts
            .iter()
            .filter_map(|artifact| {
                let leads = leads_to(artifact.path.as_ref()?, protected)?;
                Some((leads, artifact.key.instance_id.clone()))
            })
            .collect();
        let owned = instances
            .iter()
            .flat_map(|inst| {
                owned_roots(inst, home)
                    .into_iter()
                    .filter_map(move |root| Some((leads_to(&root, protected)?, inst.id.clone())))
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
                            let dir = leads_to(&glob.dir_under(home), protected)?;
                            Some((dir, *glob, inst.id.clone()))
                        })
                    })
            })
            .collect();
        Known {
            exe_raw,
            exe_canonical,
            exe_dead_ends,
            artifact_roots,
            owned,
            backups,
        }
    }

    /// The source that put `raw` (in the folder `dir`, where it leads;
    /// leading to `leads` -- `leads_to`, `None` for a broken link; `kind`
    /// what it is) there, by the first rule that matches -- or `None`:
    /// unknown.
    fn claimant(
        &self,
        raw: &Path,
        dir: &Path,
        leads: Option<&Path>,
        kind: EntryKind,
        protected: &Protected,
    ) -> Option<&InstanceId> {
        if let Some((_, id)) = self.exe_raw.iter().find(|(exe, _)| exe == raw) {
            return Some(id);
        }
        if let Some(leads) = leads {
            if let Some((_, id)) = self
                .exe_canonical
                .iter()
                .find(|(exe, _)| same_place(exe, leads))
            {
                return Some(id);
            }
            if let Some((_, id)) = self
                .artifact_roots
                .iter()
                .find(|(root, _)| is_under(leads, root))
            {
                return Some(id);
            }
            // The longest matching root: the closest owner when roots nest.
            if let Some((_, id)) = self
                .owned
                .iter()
                .filter(|(root, _)| is_under(leads, root))
                .max_by_key(|(root, _)| root.as_os_str().len())
            {
                return Some(id);
            }
        }
        // Rule 1 for a link that leads nowhere: the same missing file an
        // instance's `exe_path`, leading nowhere too, would lead to.
        if kind == EntryKind::BrokenSymlink && !self.exe_dead_ends.is_empty() {
            if let Some(end) = dead_end(raw, protected) {
                if let Some((_, id)) = self
                    .exe_dead_ends
                    .iter()
                    .find(|(exe, _)| same_place(exe, &end))
                {
                    return Some(id);
                }
            }
        }
        // Rule 4: a backup the tool's own updater left, by name, in the
        // pattern's directory, a regular file -- a link of that name is
        // somebody's link, not the updater's copy.
        if kind == EntryKind::File {
            if let Some(name) = raw.file_name().and_then(|name| name.to_str()) {
                if let Some((_, _, id)) = self.backups.iter().find(|(glob_dir, glob, _)| {
                    same_place(glob_dir, dir) && glob.matches_name(name)
                }) {
                    return Some(id);
                }
            }
        }
        None
    }
}

/// The most links macOS follows in one lookup (`MAXSYMLINKS`); `dead_end`
/// gives up on a chain of more, as the system does on a loop.
const MOST_LINKS: usize = 32;

/// Where `link`, a symbolic link that leads nowhere, would lead: the place
/// its text names, read from the link's folder (`placed`), and -- while
/// that place is itself a link -- the place that link's text names in
/// turn, until one is not there. Once grok's `downloads/` is in the Trash,
/// that is `~/.grok/downloads/grok-<version>-macos-aarch64` for its
/// launcher and for `~/.grok/bin/agent` beside it, and for a fallback link
/// in `~/.local/bin` whose text names either. `None` when a place on the
/// way is there and is not a link (the link leads somewhere after all) or
/// cannot be placed or looked at, when it is in or leads into a protected
/// place (nothing there is looked at, so nobody knows whether it is
/// there), and when more than `MOST_LINKS` links stand in the way. Every
/// look is a guarded one (`protected::resolve`, `link_text`). Read by
/// `Known::index`, for an instance's `exe_path`, and by `Known::claimant`,
/// for a broken link (rule 1).
fn dead_end(link: &Path, protected: &Protected) -> Option<PathBuf> {
    let mut link = link.to_path_buf();
    for _ in 0..MOST_LINKS {
        let (folder, text) = link_text(&link, protected)?;
        // `join` with an absolute text is that text.
        let place = placed(&folder.join(text), protected)?;
        match protected::resolve(&place, protected, false) {
            Resolution::Found(_, stat) if stat.is_symlink() => link = place,
            Resolution::Missing => return Some(place),
            Resolution::Found(..) | Resolution::Protected(_) | Resolution::Refused => return None,
        }
    }
    None
}

/// The text of the link at `path`, and the folder it is in, every link on
/// the way to that folder followed (`protected::resolve`). The text is
/// read from that folder held open -- reached from `/` with no link
/// followed and checked to be the folder `resolve` found (`dirfd`) --
/// never by the link's own path. `None` when the folder is, or leads into,
/// a protected place, or the link itself is one (`~/Documents` as a link):
/// nothing there is looked at. Also `None` when it is not a link.
fn link_text(path: &Path, protected: &Protected) -> Option<(PathBuf, PathBuf)> {
    let name = path.file_name()?;
    let Resolution::Found(folder, stat) = protected::resolve(path.parent()?, protected, true)
    else {
        return None;
    };
    if !stat.is_dir() || protected.contains(&folder.join(name)) {
        return None;
    }
    let (held, opened) = Dir::open_path(&folder, false).ok()?;
    if !opened.same_as(&stat) || !held.stat_at(name).ok()?.is_symlink() {
        return None;
    }
    let text = held.read_link_at(name).ok()?;
    Some((folder, text))
}

/// The place `path` names, its last name not followed: every folder on the
/// way resolved while it is there -- `protected::resolve` on each in turn,
/// so a linked folder is followed and a `..` after it climbs from where it
/// led, as the system's lookup does -- and, from the first folder that is
/// not there, the rest folded without touching the disk. `None` for a
/// relative `path`, one ending in `..` or naming `/`, and one with a folder
/// on the way that is there but cannot be resolved (a link to nothing, a
/// loop, a file, a folder Banager may not look into) or that is, or leads
/// into, a protected place. Read by `dead_end`.
fn placed(path: &Path, protected: &Protected) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let name = path.file_name()?;
    let mut folder = PathBuf::new();
    let mut there = true;
    for component in path.parent()?.components() {
        match component {
            Component::Normal(part) => {
                folder.push(part);
                if there {
                    match protected::resolve(&folder, protected, true) {
                        Resolution::Found(real, _) => folder = real,
                        // Not there at all, rather than a link to
                        // nothing: the folder it is in was resolved just
                        // before, so this looks at that one name alone.
                        Resolution::Missing => {
                            match protected::resolve(&folder, protected, false) {
                                Resolution::Missing => there = false,
                                _ => return None,
                            }
                        }
                        Resolution::Protected(_) | Resolution::Refused => return None,
                    }
                }
            }
            // Safe while `folder` holds no link: what is there of it is
            // resolved, and the rest is not there.
            Component::ParentDir => {
                folder.pop();
            }
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) => folder.push(component.as_os_str()),
        }
    }
    Some(folder.join(name))
}

/// One entry `examine` describes: the row, and where it leads for the
/// rules (`leads_to`'s answer; `None` for a broken link).
struct Examined {
    entry: UnknownEntry,
    leads: Option<PathBuf>,
}

/// One directory entry as the page will describe it, or `None` for the
/// ones the scan does not list at all: a subdirectory (depth 1, never
/// recursed -- `~/Library/pnpm` on the research machine held `bin/` and
/// `store/`), a link to a directory, a file with no execute bit in any
/// position (checked on the target; a theoretical boundary, the
/// research machine had none), anything that is neither a file nor a
/// link, and an entry whose `lstat` failed -- one failed `stat` costs that
/// entry and nothing else.
///
/// `name` is looked at in `folder`, held open on the scanned folder
/// (`fstatat`, `readlinkat` from its descriptor); `dir` is that folder's
/// path as `resolve` found it, `raw` the entry as the page names it. Where
/// a link leads is found one step at a time and never into a protected
/// place (`protected::resolve`): a link that leads into one is listed by
/// its own name, as `EntryKind::ProtectedSymlink`, and nothing it leads to
/// is looked at -- not its size, its date, nor whether it can be run.
/// Every file-system read the walk makes is here, in `scan_dirs`'s
/// listing, or -- for a broken link, while some instance's launcher leads
/// nowhere too -- in `dead_end` (rule 1).
fn examine(
    folder: &Dir,
    dir: &Path,
    name: &OsStr,
    raw: &Path,
    env: &HostEnv,
    protected: &Protected,
) -> Option<Examined> {
    let lstat = folder.stat_at(name).ok()?;
    let at = dir.join(name);
    let (kind, resolved, link_target, target, leads) = if lstat.is_symlink() {
        let text = folder.read_link_at(name).ok();
        let link_target = text
            .as_ref()
            .map(|target| target.to_string_lossy().into_owned());
        match protected::resolve(&at, protected, true) {
            Resolution::Found(real, stat) => (
                EntryKind::Symlink,
                Some(real.clone()),
                link_target,
                Some(stat),
                Some(real),
            ),
            Resolution::Protected(leads) => (
                EntryKind::ProtectedSymlink,
                None,
                link_target,
                None,
                by_name(leads, protected),
            ),
            Resolution::Missing | Resolution::Refused => {
                (EntryKind::BrokenSymlink, None, link_target, None, None)
            }
        }
    } else if lstat.is_file() {
        (
            EntryKind::File,
            Some(at.clone()),
            None,
            Some(lstat),
            Some(at),
        )
    } else {
        return None;
    };
    // Size, date and the executable check are the target's: a link's own
    // say only when the installer made the link.
    if let Some(target) = &target {
        if target.is_dir() || (target.mode() & 0o111) == 0 {
            return None;
        }
    }
    let (size_bytes, modified_at) = match &target {
        Some(target) => (Some(target.size()), Some(target.mtime())),
        None => (None, None),
    };
    let mut bundle_candidates: Vec<&Path> = Vec::new();
    if let Some(leads) = &leads {
        bundle_candidates.push(leads);
    }
    if let Some(target) = &link_target {
        bundle_candidates.push(Path::new(target));
    }
    bundle_candidates.push(raw);
    let app_bundle = app_bundle(bundle_candidates);
    Some(Examined {
        entry: UnknownEntry {
            path: display_path(raw, &env.home),
            kind,
            resolved,
            link_target,
            size_bytes,
            modified_at,
            owned_by_me: lstat.uid() == env.euid,
            app_bundle,
        },
        leads,
    })
}

/// The scan over an explicit directory list. `scan_unknown` is what
/// production calls; this is what the synthetic-tree tests call, so a
/// test never reads the `/usr/local/bin` of the machine running it.
///
/// Each directory is found one step at a time from `/`, every step
/// checked against the protected places before it is taken
/// (`protected::resolve`, with the places of `env.home`): one that is, or
/// leads into, a protected place is not read, nor anything in it looked
/// at, and is named in `UnknownScan::protected_dirs` instead. Directories
/// that do not exist, or that cannot be reached or listed, are skipped
/// without a trace; each distinct directory (by where it leads) is read
/// once, listed from `/` with no link followed and only while it is still
/// the folder `resolve` found (`dirfd`); entries are taken in name order
/// so a stop at the budget is reproducible. The time budget is checked
/// before every listing, and both limits before every entry
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
    let protected = Protected::new(&env.home);
    let known = Known::index(instances, artifacts, globs, &env.home, &protected);
    // The clock starts here, after indexing: `ScanBudget::max_duration`
    // bounds the walk, not the resolving of each known path above.
    let started = Instant::now();
    let mut scanned = Vec::new();
    let mut protected_dirs = Vec::new();
    let mut entries = Vec::new();
    let mut attributed = 0u32;
    let mut stopped = None;
    let mut examined = 0usize;
    let mut seen: Vec<PathBuf> = Vec::new();
    'dirs: for dir in dirs {
        let (canonical, stat) = match protected::resolve(dir, &protected, true) {
            Resolution::Found(canonical, stat) if stat.is_dir() => (canonical, stat),
            Resolution::Found(..) | Resolution::Missing | Resolution::Refused => continue,
            Resolution::Protected(leads) => {
                // Named once, by the name it was looked for under, however
                // many entries lead there.
                if !seen.iter().any(|seen| same_place(seen, &leads)) {
                    seen.push(leads);
                    protected_dirs.push(display_path(dir, &env.home));
                }
                continue;
            }
        };
        if seen.iter().any(|seen| same_place(seen, &canonical)) {
            continue;
        }
        seen.push(canonical.clone());
        if started.elapsed() >= budget.max_duration {
            stopped = Some(time_stop.clone());
            break;
        }
        // Unreadable (permissions) is not "read": it is not reported
        // either. Nor is a folder that was replaced since `resolve` saw it.
        let listed = Dir::open_path(&canonical, true)
            .ok()
            .filter(|(_, opened)| opened.same_as(&stat))
            .and_then(|(folder, _)| {
                let names = folder.entries().ok()?;
                Some((folder, names))
            });
        let Some((folder, read)) = listed else {
            continue;
        };
        let mut names: Vec<OsString> = read.filter_map(Result::ok).collect();
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
            // An entry that is itself one of the protected places --
            // `Documents` in a home folder that is on `PATH`, `Containers`
            // in a scanned `~/Library` -- is not looked at at all, not even
            // its `lstat`: it is that place, or a link standing in its
            // name, and never a program anyway.
            if protected.contains(&canonical.join(&name)) {
                continue;
            }
            let raw = dir.join(&name);
            let Some(found) = examine(&folder, &canonical, &name, &raw, env, &protected) else {
                continue;
            };
            match known.claimant(
                &raw,
                &canonical,
                found.leads.as_deref(),
                found.entry.kind,
                &protected,
            ) {
                Some(_) => attributed += 1,
                None => entries.push(found.entry),
            }
        }
        scanned.push(ScannedDir {
            path: display_path(dir, &env.home),
            entries: count,
        });
    }
    UnknownScan {
        scanned,
        protected_dirs,
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

        assert_eq!(
            serde_json::to_string(&EntryKind::ProtectedSymlink).unwrap(),
            r#""ProtectedSymlink""#
        );

        let scan = UnknownScan {
            scanned: vec![ScannedDir {
                path: PathBuf::from("~/.local/bin"),
                entries: 5,
            }],
            protected_dirs: vec![PathBuf::from("~/Documents/scripts")],
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
        assert!(
            json.contains(r#""protected_dirs":["~/Documents/scripts"]"#),
            "{json}"
        );
        assert_eq!(
            serde_json::from_str::<UnknownScan>(&json).expect("deserialize"),
            scan
        );
        // A scan sent before `protected_dirs` existed reads as none.
        let older = r#"{"scanned":[],"entries":[],"attributed":0,"stopped":null}"#;
        assert_eq!(
            serde_json::from_str::<UnknownScan>(older)
                .expect("deserialize")
                .protected_dirs,
            Vec::<PathBuf>::new()
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
        let home = Path::new("/Users/someone");
        let brew = ManagerInstance {
            prefix: PathBuf::from("/opt/homebrew"),
            ..crate::testing::manager_instance("brew", "brew:/opt/homebrew")
        };
        assert_eq!(
            owned_roots(&brew, home),
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
            owned_roots(&ollama, home),
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
            owned_roots(&npm, home),
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
            owned_roots(&claude, home),
            vec![PathBuf::from("/Users/someone/.local/share/claude")]
        );
        // The two other path-list tools own their roots the same way
        // (phase 4 step D): agy's `~/.gemini/antigravity-cli`, grok's
        // `~/.grok`, each the instance's `prefix`.
        for (adapter, prefix) in [
            ("standalone-agy", "/Users/someone/.gemini/antigravity-cli"),
            ("standalone-grok", "/Users/someone/.grok"),
            // Codex, listed only: its package folder, not `~/.codex`.
            (
                "standalone-codex",
                "/Users/someone/.codex/packages/standalone",
            ),
            ("standalone-opencode", "/Users/someone/.opencode"),
        ] {
            let inst = ManagerInstance {
                prefix: PathBuf::from(prefix),
                ..crate::testing::manager_instance(adapter, adapter)
            };
            assert_eq!(
                owned_roots(&inst, home),
                vec![PathBuf::from(prefix)],
                "{adapter}"
            );
        }
        // uv: the folder its managed Pythons are in, under the home
        // folder -- never its `parent()`-derived prefix, `~/.local/bin`.
        let uv = ManagerInstance {
            prefix: PathBuf::from("/Users/someone/.local/bin"),
            ..crate::testing::manager_instance("uv", "uv")
        };
        assert_eq!(
            owned_roots(&uv, home),
            vec![PathBuf::from("/Users/someone/.local/share/uv/python")]
        );
        // A `parent()`-derived prefix, or `$CARGO_HOME`, is never a root.
        for (adapter, id, prefix) in [
            (
                "cargo",
                "cargo:/Users/someone/.cargo",
                "/Users/someone/.cargo",
            ),
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
            assert_eq!(owned_roots(&inst, home), Vec::<PathBuf>::new(), "{adapter}");
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let raw = std::env::temp_dir().join(format!(
            "banager-scan-unit-{}-{}-{}",
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
        let protected = Protected::new(&tmp);
        let known = Known::index(&[outer_inst, inner_inst], &[], &[], &tmp, &protected);
        assert_eq!(
            known
                .claimant(
                    &entry,
                    &inner_bin,
                    Some(&entry),
                    EntryKind::File,
                    &protected
                )
                .map(String::as_str),
            Some("ollama:http://inner:11434")
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
