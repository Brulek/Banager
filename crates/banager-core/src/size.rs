//! How much disk each installed tool takes: measured read-only, after a
//! refresh has committed, on a thread of its own (`SizeMeter`). Nothing in
//! the snapshot changes, so a measurement can never move its `generation`
//! (`Snapshot::same_content` never sees a size): the window reads the
//! result with its own command (`get_sizes`, through `Session::sizes`) and
//! hears that it moved through `EventSink::sizes_changed`.
//!
//! What is measured, per source (`roots_of`):
//! - a Homebrew formula: its keg, `<prefix>/Cellar/<name>/<version>`; and,
//!   apart, the other kegs in `<prefix>/Cellar/<name>`, its old versions;
//! - a Homebrew cask with an app: the `.app` Homebrew names for it and
//!   `<prefix>/Caskroom/<token>`. A cask with no app (a font, a `pkg`) is
//!   not measured: what it installed is elsewhere;
//! - an npm package: `<prefix>/lib/node_modules/<name>`;
//! - a pipx or uv tool: its environment (`InstalledArtifact.path`);
//! - a Cargo crate: the programs it installed in `<CARGO_HOME>/bin`, as
//!   `.crates2.json` lists them -- not Cargo's download or build caches;
//! - a tool with its own installer: the program file its launcher leads to
//!   (`InstalledArtifact.path`);
//! - Ollama's models, all together: the folder their layers are stored in,
//!   `<prefix>/models/blobs`, walked once. Two models that share a layer
//!   share its file, so adding up each model's own size would count that
//!   file twice. Each model keeps the size Ollama reports for it.
//!
//! A pip package and anything else is not measured.
//!
//! How (`walk`): `lstat` and `readdir`, nothing else -- no file is opened.
//! A symbolic link is never followed: the link itself counts, not what it
//! points at. A folder on another volume (another `st_dev`) is never
//! entered. A file counts the blocks the disk holds for it, `st_blocks`
//! × 512, so a sparse file counts what it really takes; a file with
//! several hard links counts once in a measurement and once in the total,
//! whichever of its names is reached first. A folder that cannot be read
//! is skipped and the result marked `partial`, and so is anything that
//! goes away while it is walked. Each round has a budget (`SizeBudget`); a
//! measurement it cut short is marked `at_least`.
//!
//! Where it never looks (`Protected`): the folders macOS asks the user
//! about before an app may read them -- Desktop, Documents, Downloads,
//! Pictures, Movies, Music, iCloud Drive (`~/Library/Mobile Documents`),
//! other apps' cloud folders (`~/Library/CloudStorage`) and other apps'
//! data (`~/Library/Containers`, `~/Library/Group Containers`) -- and
//! other volumes (`/Volumes`), the list `crate::protected` keeps for
//! `commands` too. A tool whose folder is in one of them, or reached
//! through a link into one, gets no size, so measuring never makes macOS
//! ask anything. Each folder on the way to a tool's folder is `lstat`ed,
//! and a link among them read (`readlink`) and followed only after where
//! it leads has been checked against that list.
//!
//! Numbers are rough by nature, and the window says so ("约", "about"): an
//! APFS clone shares its blocks with the file it was cloned from and still
//! counts them in full (uv makes its tools' environments that way from its
//! cache), and a folder that changes while it is walked is counted as it
//! was when each part of it was read.

use crate::model::{ArtifactKey, ArtifactKind, InstalledArtifact, InstanceId, ManagerInstance};
use crate::protected;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::ffi::OsString;
use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub use crate::protected::{OTHER_VOLUMES, PROTECTED_IN_HOME};

/// The most links followed on the way to one folder, as the kernel's own
/// limit for a path (`MAXSYMLINKS`).
const MAX_LINKS: u32 = 32;

/// How often a round shows the window what it has so far, at most.
const PUBLISH_EVERY: Duration = Duration::from_millis(500);

/// What one round of measuring may spend, across every folder it walks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SizeBudget {
    /// Entries looked at (`lstat`), folders and files alike.
    pub max_entries: u64,
    /// Wall-clock time from the first entry of the round.
    pub max_duration: Duration,
}

impl Default for SizeBudget {
    /// Enough for a Mac with several hundred tools and a few large apps:
    /// a Homebrew keg holds tens to a few thousand entries, an app bundle
    /// up to tens of thousands. Apps are walked last, so what the budget
    /// cuts short first is an app, marked `at_least`; what it did not
    /// reach gets no size that round, and the next round measures it
    /// before it measures again anything cut short.
    fn default() -> SizeBudget {
        SizeBudget {
            max_entries: 300_000,
            max_duration: Duration::from_secs(30),
        }
    }
}

/// How much one thing takes on disk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Measured {
    /// Bytes the disk holds for it: `st_blocks` × 512, every hard-linked
    /// file once.
    pub bytes: u64,
    /// Something in it could not be read, or went away while it was
    /// walked: it takes more than `bytes`, by an amount not known.
    pub partial: bool,
    /// The round's budget ran out before its walk finished: it takes at
    /// least `bytes`.
    pub at_least: bool,
}

/// One installed thing's size, by its key, as the window shows it in the
/// Installed page's details.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactSize {
    pub key: ArtifactKey,
    /// The version it was measured at: the artifact's `version` in the
    /// snapshot the round started from. The window shows a size only for
    /// the version it lists, so a size measured before an update is never
    /// shown beside the new version.
    pub version: String,
    /// `None` while it is still being measured.
    pub measured: Option<Measured>,
    /// A Homebrew formula's other kegs -- every `<prefix>/Cellar/<name>/*`
    /// besides the one of `version` -- together; `None` when it has none,
    /// for anything else, and while `measured` is `None`.
    pub old_versions: Option<Measured>,
}

/// One Ollama's models, measured as the folder their layers are in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelsSize {
    pub instance_id: InstanceId,
    /// `None` while it is still being measured.
    pub measured: Option<Measured>,
}

/// Everything the measurements of the last round say, for the window
/// (`get_sizes`). Not part of the `Snapshot`. Mirrored by `Sizes` in
/// src/lib/types.ts.
///
/// An artifact that is not in `artifacts` has no size to show: nothing is
/// measured for its kind, its folder is not there, or it is in a place
/// that is never measured (`Protected`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sizes {
    /// The `Snapshot::round` this was measured for; 0 before any.
    pub round: u64,
    /// Whether the round has finished: every measurement is in, or the
    /// budget ran out.
    pub done: bool,
    pub artifacts: Vec<ArtifactSize>,
    pub models: Vec<ModelsSize>,
    /// Everything in `artifacts` (old versions included) and `models`
    /// together, a file with several hard links counted once, `at_least`
    /// when the budget ran out before something was reached; `None` until
    /// `done`, and when nothing was measured.
    pub total: Option<Measured>,
    /// The same, one source at a time (`SourceSize`), for the Installed
    /// page's headings; empty until `done`.
    #[serde(default)]
    pub sources: Vec<SourceSize>,
}

/// Everything measured of one source -- its tools, a formula's old
/// versions, an Ollama's models folder -- together, a file with several
/// hard links counted once; `at_least` when the budget ran out before
/// something of it was reached. A source with nothing measured has none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSize {
    pub instance_id: InstanceId,
    pub measured: Measured,
}

/// The places a measurement never enters, for one home folder: each of
/// `PROTECTED_IN_HOME` under it -- spelled as given and with its own links
/// followed -- and `OTHER_VOLUMES`.
#[derive(Clone, Debug, Default)]
pub struct Protected {
    places: Vec<PathBuf>,
}

impl Protected {
    pub fn new(home: &Path) -> Protected {
        let mut homes = vec![home.to_path_buf()];
        if let Resolution::Found(real, _) = resolve(home, &Protected::default(), true) {
            if real != home {
                homes.push(real);
            }
        }
        Protected {
            places: protected::places(&homes),
        }
    }

    /// Whether `path` is one of the places or inside one.
    pub fn contains(&self, path: &Path) -> bool {
        protected::is_within(path, &self.places)
    }

    /// Whether one of the places is inside `path` (or is it): walking
    /// `path` would reach it.
    fn under(&self, path: &Path) -> bool {
        self.places
            .iter()
            .any(|place| protected::starts_with_folded(place, path))
    }
}

/// What `resolve` found at a path.
enum Resolution {
    /// The path with every link on the way followed, and what is there
    /// (`lstat`: a link at the end, when not followed, is the link).
    Found(PathBuf, Metadata),
    Missing,
    /// It is, or leads, into a protected place; or a folder on the way
    /// could not be read; or it is not absolute; or too many links.
    Refused,
}

/// `path`, with each link among its folders followed, one component at a
/// time, so that no step is ever taken into a protected place: each next
/// component is checked against `protected` before it is `lstat`ed, and a
/// link's text is read (`readlink`) and spliced in before anything it
/// names is looked at. The last component is followed only with
/// `follow_last`. Reads nothing but `lstat` and `readlink` of the folders
/// and links on the way.
fn resolve(path: &Path, protected: &Protected, follow_last: bool) -> Resolution {
    if !path.is_absolute() {
        return Resolution::Refused;
    }
    let mut pending: VecDeque<OsString> = names(path).collect();
    let mut resolved = PathBuf::from("/");
    let mut found: Option<Metadata> = None;
    let mut links = 0;
    while let Some(name) = pending.pop_front() {
        if name == ".." {
            resolved.pop();
            found = None;
            continue;
        }
        let candidate = resolved.join(&name);
        if protected.contains(&candidate) {
            return Resolution::Refused;
        }
        let meta = match std::fs::symlink_metadata(&candidate) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Resolution::Missing,
            Err(_) => return Resolution::Refused,
        };
        if meta.file_type().is_symlink() && (!pending.is_empty() || follow_last) {
            links += 1;
            if links > MAX_LINKS {
                return Resolution::Refused;
            }
            let Ok(target) = std::fs::read_link(&candidate) else {
                return Resolution::Refused;
            };
            if target.is_absolute() {
                resolved = PathBuf::from("/");
            }
            let spliced: Vec<OsString> = names(&target).collect();
            for name in spliced.into_iter().rev() {
                pending.push_front(name);
            }
            found = None;
            continue;
        }
        resolved = candidate;
        found = Some(meta);
    }
    match found {
        Some(meta) => Resolution::Found(resolved, meta),
        None => Resolution::Refused,
    }
}

/// `path`'s names, `..` kept as a name and `.` and the root dropped.
fn names(path: &Path) -> impl Iterator<Item = OsString> + '_ {
    path.components().filter_map(|component| match component {
        Component::Normal(name) => Some(name.to_os_string()),
        Component::ParentDir => Some(OsString::from("..")),
        Component::RootDir | Component::CurDir | Component::Prefix(_) => None,
    })
}

/// Whether `name` is one plain path component: not empty, no `/`, not `.`
/// or `..`. Every name that becomes part of a path here came from a
/// tool's own output or a file on disk, and is checked with this first.
fn plain(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\0')
}

/// What one round may still spend.
struct Budget {
    entries_left: u64,
    deadline: Instant,
}

impl Budget {
    fn new(budget: SizeBudget) -> Budget {
        let now = Instant::now();
        Budget {
            entries_left: budget.max_entries,
            deadline: now
                .checked_add(budget.max_duration)
                .unwrap_or(now + Duration::from_secs(86_400 * 365)),
        }
    }

    /// One more entry: false once the entries or the time have run out,
    /// and from then on.
    fn take(&mut self) -> bool {
        if self.entries_left == 0 || Instant::now() >= self.deadline {
            self.entries_left = 0;
            return false;
        }
        self.entries_left -= 1;
        true
    }
}

/// One measurement's result, with what the round's total needs to count
/// a file with several hard links once across measurements.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Walked {
    measured: Measured,
    /// Bytes of what counts by its path alone: folders, links, and files
    /// with one link.
    single: u64,
    /// Files with more than one link, once each: `(st_dev, st_ino,
    /// bytes)`, sorted.
    shared: Vec<(u64, u64, u64)>,
}

impl Walked {
    fn complete(&self) -> bool {
        !self.measured.partial && !self.measured.at_least
    }
}

/// What a walk adds up as it goes.
#[derive(Default)]
struct Tally {
    single: u64,
    shared: HashMap<(u64, u64), u64>,
    partial: bool,
    at_least: bool,
    /// Something was there to measure: a file, or a folder whose entries
    /// could be listed.
    reached: bool,
}

impl Tally {
    fn count(&mut self, meta: &Metadata) {
        let bytes = meta.blocks().saturating_mul(512);
        if !meta.is_dir() && meta.nlink() > 1 {
            self.shared.entry((meta.dev(), meta.ino())).or_insert(bytes);
        } else {
            self.single = self.single.saturating_add(bytes);
        }
    }

    fn finish(self) -> Walked {
        let mut shared: Vec<(u64, u64, u64)> = self
            .shared
            .into_iter()
            .map(|((dev, ino), bytes)| (dev, ino, bytes))
            .collect();
        shared.sort_unstable();
        let shared_bytes = shared
            .iter()
            .fold(0u64, |sum, (_, _, bytes)| sum.saturating_add(*bytes));
        Walked {
            measured: Measured {
                bytes: self.single.saturating_add(shared_bytes),
                partial: self.partial,
                at_least: self.at_least,
            },
            single: self.single,
            shared,
        }
    }
}

/// How a walk ended.
#[derive(Debug, PartialEq, Eq)]
enum WalkEnd {
    Walked(Walked),
    /// Nothing there could be measured: every root was gone, or could not
    /// be listed.
    Nothing,
    /// The round's budget ran out before anything there was reached: not
    /// measured this round.
    OutOfBudget,
    /// A newer round started: this one stops where it is.
    Superseded,
}

/// Measures `roots` together (`lstat` and `readdir` only; see the module
/// doc), spending `budget`, and asking `wanted` before each entry whether
/// to go on.
fn walk(
    roots: &[PathBuf],
    budget: &mut Budget,
    protected: &Protected,
    wanted: &mut dyn FnMut() -> bool,
) -> WalkEnd {
    let mut tally = Tally::default();
    for root in roots {
        if !wanted() {
            return WalkEnd::Superseded;
        }
        if !budget.take() {
            tally.at_least = true;
            break;
        }
        let meta = match std::fs::symlink_metadata(root) {
            Ok(meta) => meta,
            // Gone since it was planned, or no longer readable: whatever
            // else this measures is not all of it.
            Err(_) => {
                tally.partial = true;
                continue;
            }
        };
        tally.count(&meta);
        if !meta.is_dir() {
            tally.reached = true;
            continue;
        }
        match walk_folder(root, meta.dev(), &mut tally, budget, protected, wanted) {
            Flow::Finished => {}
            Flow::OutOfBudget => break,
            Flow::Superseded => return WalkEnd::Superseded,
        }
    }
    if !tally.reached {
        return if tally.at_least {
            WalkEnd::OutOfBudget
        } else {
            WalkEnd::Nothing
        };
    }
    WalkEnd::Walked(tally.finish())
}

/// How `walk_folder` stopped.
enum Flow {
    Finished,
    OutOfBudget,
    Superseded,
}

/// Everything under `root`, a folder on the volume `device`, into `tally`:
/// a folder on any other volume is neither counted nor entered.
fn walk_folder(
    root: &Path,
    device: u64,
    tally: &mut Tally,
    budget: &mut Budget,
    protected: &Protected,
    wanted: &mut dyn FnMut() -> bool,
) -> Flow {
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        if !wanted() {
            return Flow::Superseded;
        }
        // Never reached from a root `plan_round` let through; checked all
        // the same, folder by folder, so that no walk can ever list one.
        if protected.contains(&folder) {
            tally.partial = true;
            continue;
        }
        let entries = match std::fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(_) => {
                tally.partial = true;
                continue;
            }
        };
        tally.reached = true;
        for entry in entries {
            if !wanted() {
                return Flow::Superseded;
            }
            let Ok(entry) = entry else {
                tally.partial = true;
                continue;
            };
            // `lstat`: a link is the link.
            let meta = match entry.metadata() {
                Ok(meta) => meta,
                Err(_) => {
                    tally.partial = true;
                    continue;
                }
            };
            if meta.is_dir() && meta.dev() != device {
                // Another volume mounted here: not this tool's.
                continue;
            }
            if !budget.take() {
                tally.at_least = true;
                return Flow::OutOfBudget;
            }
            tally.count(&meta);
            if meta.is_dir() {
                folders.push(entry.path());
            }
        }
    }
    Flow::Finished
}

/// The total of several measurements, each file with several hard links
/// once however many of them reached it. Independent of their order.
fn total<'a>(walks: impl IntoIterator<Item = &'a Walked>) -> Option<Measured> {
    let mut any = false;
    let mut single = 0u64;
    let mut shared: HashMap<(u64, u64), u64> = HashMap::new();
    let mut partial = false;
    let mut at_least = false;
    for walked in walks {
        any = true;
        single = single.saturating_add(walked.single);
        for (dev, ino, bytes) in &walked.shared {
            shared.entry((*dev, *ino)).or_insert(*bytes);
        }
        partial |= walked.measured.partial;
        at_least |= walked.measured.at_least;
    }
    any.then(|| Measured {
        bytes: shared
            .values()
            .fold(single, |sum, bytes| sum.saturating_add(*bytes)),
        partial,
        at_least,
    })
}

/// What `look_at` found at one path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Looked {
    /// Nothing is there, or the path is in a protected place itself and
    /// was not looked at.
    Missing,
    /// Something is there; how much it takes, or `None` when that is not
    /// known: it leads into a protected place (never entered), it could
    /// not be read, or the budget ran out before any of it was reached.
    There(Option<Measured>),
}

/// A budget several `look_at`s share: what one uninstall preview may
/// spend measuring what the uninstall leaves behind (`kept_data`).
pub(crate) struct LookBudget(Budget);

impl LookBudget {
    pub(crate) fn new(budget: SizeBudget) -> LookBudget {
        LookBudget(Budget::new(budget))
    }
}

/// Whether anything is at `path` -- an absolute path, links on the way
/// and at the end followed (`resolve`), each step checked against
/// `protected` before it is looked at -- and how much it takes, measured
/// as a tool's folder is (`walk`: `lstat`, `readdir`, `readlink`, nothing
/// opened), spending `budget`. A path that is itself in a protected place
/// is not looked at; one that leads into one is there, with no size: its
/// own `lstat` is all that is read of it.
pub(crate) fn look_at(path: &Path, protected: &Protected, budget: &mut LookBudget) -> Looked {
    if protected.contains(path) {
        return Looked::Missing;
    }
    match resolve(path, protected, true) {
        Resolution::Missing => Looked::Missing,
        Resolution::Refused => match std::fs::symlink_metadata(path) {
            Ok(_) => Looked::There(None),
            Err(_) => Looked::Missing,
        },
        Resolution::Found(real, _) => match walk(&[real], &mut budget.0, protected, &mut || true) {
            WalkEnd::Walked(walked) => Looked::There(Some(walked.measured)),
            WalkEnd::Nothing | WalkEnd::OutOfBudget | WalkEnd::Superseded => Looked::There(None),
        },
    }
}

/// What a measurement is remembered by between rounds: the folder (or
/// file) it starts from and the version it was measured at -- not a
/// modified time, which an upgrade that rewrites a tool's files in place
/// (pipx) leaves as it was on the folder itself.
type CacheKey = (PathBuf, String);

/// One set of roots, measured together.
struct Job {
    key: CacheKey,
    roots: Vec<PathBuf>,
    /// A root of it was refused: what it measures is not all of it.
    partial: bool,
    result: Option<JobResult>,
    /// What an earlier round measured of the same folder at the same
    /// version and could not finish (`partial` or `at_least`): shown,
    /// marked as it was, until this round has measured it again.
    previous: Option<Walked>,
}

enum JobResult {
    Walked(Walked),
    Nothing,
    /// The round's budget ran out before it was reached, and no earlier
    /// round measured it: no size this round; the next round measures it
    /// before anything an earlier round already measured in part.
    NotReached,
}

impl Job {
    fn new(key: CacheKey, roots: Vec<PathBuf>, partial: bool) -> Job {
        Job {
            key,
            roots,
            partial,
            result: None,
            previous: None,
        }
    }

    /// What is shown for it: what this round measured, or while it has
    /// not yet, what an earlier round measured in part.
    fn walked(&self) -> Option<&Walked> {
        match &self.result {
            Some(JobResult::Walked(walked)) => Some(walked),
            Some(_) => None,
            None => self.previous.as_ref(),
        }
    }

    /// Measured by this round, or shown from an earlier one meanwhile.
    fn has_something_to_show(&self) -> bool {
        self.result.is_some() || self.previous.is_some()
    }

    /// `walked`'s measurement, with what planning knew added.
    fn measured(&self) -> Option<Measured> {
        self.walked().map(|walked| Measured {
            partial: walked.measured.partial || self.partial,
            ..walked.measured
        })
    }
}

/// What a unit's size is shown as.
enum Target {
    /// The artifact at this index of the round's list.
    Artifact { index: usize },
    /// One Ollama's models; `floor` is its largest model's own size: a
    /// folder holding less than that is not where its models are (they
    /// are elsewhere, `OLLAMA_MODELS`), and gets no line.
    Models { instance_id: InstanceId, floor: u64 },
}

/// What one line of the window's sizes is made of.
struct Unit {
    target: Target,
    /// Walked in this order: models first (a few large files), then
    /// packages and tools, then apps, the heaviest walks.
    order: (u8, usize),
    main: Job,
    old: Option<Job>,
}

impl Unit {
    fn finished(&self) -> bool {
        self.main.result.is_some() && self.old.as_ref().is_none_or(|old| old.result.is_some())
    }

    /// Every job of it has a result, or one an earlier round left to show
    /// until this round has its own.
    fn showable(&self) -> bool {
        self.main.has_something_to_show()
            && self
                .old
                .as_ref()
                .is_none_or(|old| old.has_something_to_show())
    }

    /// The budget ran out before some job of it was reached.
    fn not_reached(&self) -> bool {
        std::iter::once(&self.main)
            .chain(self.old.as_ref())
            .any(|job| matches!(job.result, Some(JobResult::NotReached)))
    }
}

/// Every `.crates2.json` read for one round, by the Cargo home it is in:
/// crate name to the programs it installed. `None` for one that could not
/// be read; the artifact's own `path` stands in.
type CratesBins = HashMap<PathBuf, Option<HashMap<String, Vec<String>>>>;

/// Where an artifact's size is measured from (see the module doc), not yet
/// resolved, and its walking order: `None` for anything not measured.
fn roots_of(
    inst: &ManagerInstance,
    artifact: &InstalledArtifact,
    protected: &Protected,
    crates: &mut CratesBins,
) -> Option<(Vec<PathBuf>, u8)> {
    let key = &artifact.key;
    let absolute_path = || {
        artifact
            .path
            .clone()
            .filter(|path| path.is_absolute())
            .map(|path| vec![path])
    };
    match inst.adapter_id.as_str() {
        "brew" => {
            // `user/tap/name` is `name` in the Cellar and the Caskroom.
            let short = key.name.rsplit('/').next().filter(|name| plain(name))?;
            match key.kind {
                ArtifactKind::Formula => {
                    if !plain(&artifact.version) {
                        return None;
                    }
                    let keg = inst
                        .prefix
                        .join("Cellar")
                        .join(short)
                        .join(&artifact.version);
                    Some((vec![keg], 1))
                }
                ArtifactKind::Cask => {
                    let app = artifact.path.clone().filter(|path| path.is_absolute())?;
                    let caskroom = inst.prefix.join("Caskroom").join(short);
                    Some((vec![app, caskroom], 2))
                }
                _ => None,
            }
        }
        "npm" => {
            // `name` or `@scope/name`, nothing else.
            let parts: Vec<&str> = key.name.split('/').collect();
            let shaped = match parts.as_slice() {
                [name] => plain(name) && !name.starts_with('@'),
                [scope, name] => scope.starts_with('@') && plain(scope) && plain(name),
                _ => false,
            };
            if !shaped {
                return None;
            }
            let folder = inst.prefix.join("lib").join("node_modules").join(&key.name);
            Some((vec![folder], 1))
        }
        "pipx" | "uv" => absolute_path().map(|roots| (roots, 1)),
        "cargo" => {
            let bins = crates
                .entry(inst.prefix.clone())
                .or_insert_with(|| read_crates_bins(&inst.prefix, protected))
                .as_ref()
                .and_then(|crates| crates.get(&key.name));
            match bins {
                Some(bins) if !bins.is_empty() => Some((
                    bins.iter()
                        .map(|bin| inst.prefix.join("bin").join(bin))
                        .collect(),
                    1,
                )),
                _ => absolute_path().map(|roots| (roots, 1)),
            }
        }
        id if id.starts_with("standalone-") => absolute_path().map(|roots| (roots, 1)),
        _ => None,
    }
}

/// `<cargo_home>/.crates2.json`'s programs per crate, every name a plain
/// file name; `None` when it is not a regular file in an allowed place or
/// does not parse.
fn read_crates_bins(
    cargo_home: &Path,
    protected: &Protected,
) -> Option<HashMap<String, Vec<String>>> {
    let Resolution::Found(path, meta) =
        resolve(&cargo_home.join(".crates2.json"), protected, false)
    else {
        return None;
    };
    if !meta.is_file() {
        return None;
    }
    let json = std::fs::read_to_string(path).ok()?;
    let parsed = crate::adapters::cargo::parse_crates2_bins(&json).ok()?;
    Some(
        parsed
            .into_iter()
            .map(|(name, bins)| (name, bins.into_iter().filter(|bin| plain(bin)).collect()))
            .collect(),
    )
}

/// `roots`, each resolved (`resolve`): `None` when the first -- the
/// artifact's own folder or file -- is missing, refused, a link, or would
/// reach a protected place; any other such root is left out, a refused one
/// marking the measurement partial. A root inside another is left out too,
/// so nothing is counted twice.
fn resolve_roots(roots: &[PathBuf], protected: &Protected) -> Option<(Vec<PathBuf>, bool)> {
    let mut resolved = Vec::new();
    let mut partial = false;
    for (index, root) in roots.iter().enumerate() {
        let usable = match resolve(root, protected, false) {
            Resolution::Found(path, meta) if !meta.file_type().is_symlink() => {
                if protected.under(&path) {
                    partial = true;
                    None
                } else {
                    Some(path)
                }
            }
            Resolution::Found(..) | Resolution::Missing => None,
            Resolution::Refused => {
                partial = true;
                None
            }
        };
        match usable {
            Some(path) => resolved.push(path),
            None if index == 0 => return None,
            None => {}
        }
    }
    let mut kept: Vec<PathBuf> = Vec::new();
    for path in &resolved {
        let inside_another = resolved
            .iter()
            .any(|other| other != path && path.starts_with(other));
        if !inside_another && !kept.contains(path) {
            kept.push(path.clone());
        }
    }
    Some((kept, partial))
}

/// A Homebrew formula's other kegs: every real folder in
/// `<prefix>/Cellar/<name>` but `current`'s, as one job.
fn old_versions_job(cellar_name: &Path, current: &str, protected: &Protected) -> Option<Job> {
    let Resolution::Found(folder, meta) = resolve(cellar_name, protected, false) else {
        return None;
    };
    if !meta.is_dir() || protected.under(&folder) {
        return None;
    }
    let mut versions: Vec<String> = std::fs::read_dir(&folder)
        .ok()?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_str()?.to_string();
            let meta = entry.metadata().ok()?;
            (plain(&name) && name != current && meta.is_dir()).then_some(name)
        })
        .collect();
    if versions.is_empty() {
        return None;
    }
    versions.sort();
    let roots = versions
        .iter()
        .map(|version| folder.join(version))
        .collect();
    let key = (folder, format!("old:{}", versions.join("\n")));
    Some(Job::new(key, roots, false))
}

/// The round's units, from the snapshot it started from. Reads, besides
/// `lstat` and `readlink` on the way to each folder (`resolve`), the
/// names in each formula's `<prefix>/Cellar/<name>` and each Cargo home's
/// `.crates2.json`.
fn plan_round(
    instances: &[ManagerInstance],
    artifacts: &[InstalledArtifact],
    protected: &Protected,
) -> Vec<Unit> {
    let by_id: HashMap<&str, &ManagerInstance> = instances
        .iter()
        .map(|inst| (inst.id.as_str(), inst))
        .collect();
    let mut crates: CratesBins = HashMap::new();
    let mut units = Vec::new();
    for (index, artifact) in artifacts.iter().enumerate() {
        let Some(inst) = by_id.get(artifact.key.instance_id.as_str()) else {
            continue;
        };
        let Some((roots, order)) = roots_of(inst, artifact, protected, &mut crates) else {
            continue;
        };
        let Some((roots, partial)) = resolve_roots(&roots, protected) else {
            continue;
        };
        let key = (roots[0].clone(), artifact.version.clone());
        let old = if inst.adapter_id == "brew" && artifact.key.kind == ArtifactKind::Formula {
            roots[0]
                .parent()
                .and_then(|cellar_name| old_versions_job(cellar_name, &artifact.version, protected))
        } else {
            None
        };
        units.push(Unit {
            target: Target::Artifact { index },
            order: (order, index),
            main: Job::new(key, roots, partial),
            old,
        });
    }
    for inst in instances {
        if inst.adapter_id != "ollama" || !crate::adapters::ollama::models_on_this_mac(inst) {
            continue;
        }
        let models: Vec<&InstalledArtifact> = artifacts
            .iter()
            .filter(|a| a.key.instance_id == inst.id && a.key.kind == ArtifactKind::Model)
            .collect();
        if models.is_empty() {
            continue;
        }
        let blobs = inst.prefix.join("models").join("blobs");
        let Some((roots, partial)) = resolve_roots(&[blobs], protected) else {
            continue;
        };
        let mut which: Vec<String> = models
            .iter()
            .map(|model| format!("{}@{}", model.key.name, model.version))
            .collect();
        which.sort();
        let floor = models
            .iter()
            .filter_map(|model| model.size_bytes)
            .max()
            .unwrap_or(0);
        units.push(Unit {
            target: Target::Models {
                instance_id: inst.id.clone(),
                floor,
            },
            order: (0, units.len()),
            main: Job::new((roots[0].clone(), which.join("\n")), roots, partial),
            old: None,
        });
    }
    units.sort_by_key(|unit| unit.order);
    units
}

/// `mutex`'s guard, even after a thread panicked holding it: what it
/// guards is whole at every point a panic could leave it -- one
/// assignment, one insert -- and `get_sizes` must keep answering.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Measures, read-only and off the refresh's path, how much disk each
/// installed thing takes; one per `Session` (`Session::new` makes it).
///
/// `measure` starts a round on a thread of its own for the snapshot a
/// refresh has just committed, and a newer round stops an older one at its
/// next entry. A round first plans (`plan_round`), then shows the window
/// everything it will measure as "measuring" -- filled in at once where a
/// previous round already measured the same folder at the same version --
/// then walks the rest, showing what it has at most every
/// `PUBLISH_EVERY`, and last everything, `done`. Each time, it calls
/// `on_change` with its round. It holds no lock but its own two, never
/// across a walk, and takes no adapter's lock.
pub struct SizeMeter {
    budget: SizeBudget,
    on_change: Box<dyn Fn(u64) + Send + Sync>,
    /// The round whose results may still be shown: the newest started.
    current: AtomicU64,
    published: Mutex<Sizes>,
    /// The last measurement of each folder, by `CacheKey`: a complete one
    /// -- not `partial`, not `at_least` -- so that an unchanged folder is
    /// not walked again at every refresh, and one cut short to show until
    /// it is measured again. Only what the newest round planned is kept.
    cache: Mutex<HashMap<CacheKey, Walked>>,
}

impl SizeMeter {
    pub fn new(
        budget: SizeBudget,
        on_change: impl Fn(u64) + Send + Sync + 'static,
    ) -> Arc<SizeMeter> {
        Arc::new(SizeMeter {
            budget,
            on_change: Box::new(on_change),
            current: AtomicU64::new(0),
            published: Mutex::new(Sizes::default()),
            cache: Mutex::new(HashMap::new()),
        })
    }

    /// What the last round has said so far.
    pub fn sizes(&self) -> Sizes {
        lock(&self.published).clone()
    }

    /// Starts measuring what the snapshot of `round` lists, on a thread of
    /// its own, and returns at once; a round no newer than the newest
    /// started is ignored (two refresh calls that shared a round). `home`
    /// is whose protected folders are never entered (`Protected`).
    pub fn measure(
        self: &Arc<Self>,
        round: u64,
        instances: &[ManagerInstance],
        artifacts: &[InstalledArtifact],
        home: &Path,
    ) {
        if self.current.fetch_max(round, Ordering::SeqCst) >= round {
            return;
        }
        let meter = Arc::clone(self);
        let instances = instances.to_vec();
        let artifacts = artifacts.to_vec();
        let home = home.to_path_buf();
        // A thread that could not start leaves the last round's sizes up,
        // and the next round tries again.
        let _ = std::thread::Builder::new()
            .name("banager-sizes".to_string())
            .spawn(move || {
                let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    meter.run_round(round, &instances, &artifacts, &home)
                }));
                // A round that panicked would leave its sizes "measuring"
                // for as long as Banager runs: it ends instead, with
                // nothing to show.
                if ran.is_err() {
                    meter.publish(
                        round,
                        Sizes {
                            round,
                            done: true,
                            ..Sizes::default()
                        },
                    );
                }
            });
    }

    fn wanted(&self, round: u64) -> bool {
        self.current.load(Ordering::SeqCst) == round
    }

    /// Shows `sizes` to the window, unless a newer round has started: the
    /// check and the write under one lock, so an older round can never
    /// write over a newer one's.
    fn publish(&self, round: u64, sizes: Sizes) -> bool {
        {
            let mut published = lock(&self.published);
            if !self.wanted(round) {
                return false;
            }
            *published = sizes;
        }
        (self.on_change)(round);
        true
    }

    /// One round, start to end, on the calling thread (`measure`'s).
    fn run_round(
        &self,
        round: u64,
        instances: &[ManagerInstance],
        artifacts: &[InstalledArtifact],
        home: &Path,
    ) {
        let protected = Protected::new(home);
        let mut units = plan_round(instances, artifacts, &protected);
        if !self.wanted(round) {
            return;
        }
        {
            let mut cache = lock(&self.cache);
            let planned: HashSet<&CacheKey> = units
                .iter()
                .flat_map(|unit| std::iter::once(&unit.main).chain(unit.old.as_ref()))
                .map(|job| &job.key)
                .collect();
            cache.retain(|key, _| planned.contains(key));
            for unit in &mut units {
                for job in std::iter::once(&mut unit.main).chain(unit.old.as_mut()) {
                    if let Some(walked) = cache.get(&job.key) {
                        if walked.complete() {
                            job.result = Some(JobResult::Walked(walked.clone()));
                        } else {
                            job.previous = Some(walked.clone());
                        }
                    }
                }
            }
        }
        if !self.publish(round, sizes_of(round, artifacts, &units, false)) {
            return;
        }
        let mut budget = Budget::new(self.budget);
        let mut shown = Instant::now();
        // First what no round has measured yet, then again what an earlier
        // round could only measure in part: a folder bigger than the whole
        // budget never keeps the ones after it from being measured.
        for again in [false, true] {
            for index in 0..units.len() {
                let unit = &mut units[index];
                for job in std::iter::once(&mut unit.main).chain(unit.old.as_mut()) {
                    if job.result.is_some() || job.previous.is_some() != again {
                        continue;
                    }
                    let mut wanted = || self.wanted(round);
                    job.result = Some(
                        match walk(&job.roots, &mut budget, &protected, &mut wanted) {
                            WalkEnd::Superseded => return,
                            WalkEnd::Nothing => {
                                lock(&self.cache).remove(&job.key);
                                JobResult::Nothing
                            }
                            WalkEnd::OutOfBudget => match job.previous.take() {
                                Some(previous) => JobResult::Walked(previous),
                                None => JobResult::NotReached,
                            },
                            WalkEnd::Walked(walked) => {
                                lock(&self.cache).insert(job.key.clone(), walked.clone());
                                JobResult::Walked(walked)
                            }
                        },
                    );
                }
                if shown.elapsed() >= PUBLISH_EVERY {
                    if !self.publish(round, sizes_of(round, artifacts, &units, false)) {
                        return;
                    }
                    shown = Instant::now();
                }
            }
        }
        self.publish(round, sizes_of(round, artifacts, &units, true));
    }
}

/// The window's view of a round's `units` so far: every unit still being
/// measured as `measured: None` (or, while it is measured again, with what
/// an earlier round measured in part), every finished one with its
/// numbers, and none that turned out to have nothing to measure or that
/// the budget did not reach -- the total then says `at_least`.
fn sizes_of(round: u64, artifacts: &[InstalledArtifact], units: &[Unit], done: bool) -> Sizes {
    let mut listed: Vec<(usize, ArtifactSize)> = Vec::new();
    let mut models = Vec::new();
    let mut counted: Vec<&Walked> = Vec::new();
    let mut not_reached = false;
    // By source: what it counted, and whether the budget missed some of it.
    let mut by_source: BTreeMap<&str, (Vec<&Walked>, bool)> = BTreeMap::new();
    for unit in units {
        let finished = unit.finished();
        not_reached |= unit.not_reached();
        let instance_id = match &unit.target {
            Target::Artifact { index } => artifacts[*index].key.instance_id.as_str(),
            Target::Models { instance_id, .. } => instance_id.as_str(),
        };
        if unit.not_reached() {
            by_source.entry(instance_id).or_default().1 = true;
        }
        if finished && unit.main.walked().is_none() {
            continue;
        }
        let showable = unit.showable();
        match &unit.target {
            Target::Artifact { index } => {
                let artifact = &artifacts[*index];
                if finished {
                    let walks = unit
                        .main
                        .walked()
                        .into_iter()
                        .chain(unit.old.as_ref().and_then(Job::walked));
                    let source = &mut by_source.entry(instance_id).or_default().0;
                    for walked in walks {
                        counted.push(walked);
                        source.push(walked);
                    }
                }
                let (measured, old_versions) = if showable {
                    (
                        unit.main.measured(),
                        unit.old.as_ref().and_then(Job::measured),
                    )
                } else {
                    (None, None)
                };
                listed.push((
                    *index,
                    ArtifactSize {
                        key: artifact.key.clone(),
                        version: artifact.version.clone(),
                        measured,
                        old_versions,
                    },
                ));
            }
            Target::Models { instance_id, floor } => {
                let measured = if showable { unit.main.measured() } else { None };
                if measured.is_some_and(|m| m.bytes.saturating_mul(10) < floor.saturating_mul(9)) {
                    continue;
                }
                if finished {
                    counted.extend(unit.main.walked());
                    by_source
                        .entry(instance_id)
                        .or_default()
                        .0
                        .extend(unit.main.walked());
                }
                models.push(ModelsSize {
                    instance_id: instance_id.clone(),
                    measured,
                });
            }
        }
    }
    listed.sort_by_key(|(index, _)| *index);
    Sizes {
        round,
        done,
        artifacts: listed.into_iter().map(|(_, size)| size).collect(),
        models,
        total: if done {
            reached_total(counted, not_reached)
        } else {
            None
        },
        sources: if done {
            by_source
                .into_iter()
                .filter_map(|(instance_id, (walks, not_reached))| {
                    reached_total(walks, not_reached).map(|measured| SourceSize {
                        instance_id: instance_id.to_string(),
                        measured,
                    })
                })
                .collect()
        } else {
            Vec::new()
        },
    }
}

/// `total` of `walks`, `at_least` when the budget did not reach something
/// that belongs with them: that takes something too, by an amount not
/// known. `None` when there is nothing to say.
fn reached_total(walks: Vec<&Walked>, not_reached: bool) -> Option<Measured> {
    match total(walks) {
        Some(sum) => Some(Measured {
            at_least: sum.at_least || not_reached,
            ..sum
        }),
        None if not_reached => Some(Measured {
            at_least: true,
            ..Measured::default()
        }),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::InstallReason;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::sync::{OnceLock, Weak};

    /// A folder of a test's own under the system's temporary folder,
    /// removed when dropped (permissions restored first, for a test that
    /// made one unreadable).
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!(
                "banager-size-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }

        fn path(&self, relative: &str) -> PathBuf {
            self.0.join(relative)
        }

        /// A file of `len` bytes of data at `relative`, its folders made.
        fn file(&self, relative: &str, len: usize) -> PathBuf {
            let path = self.path(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, vec![7u8; len]).unwrap();
            path
        }

        fn dir(&self, relative: &str) -> PathBuf {
            let path = self.path(relative);
            std::fs::create_dir_all(&path).unwrap();
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let mut stack = vec![self.0.clone()];
            while let Some(dir) = stack.pop() {
                let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755));
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        if entry.file_type().is_ok_and(|t| t.is_dir()) {
                            stack.push(entry.path());
                        }
                    }
                }
            }
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn blocks_of(path: &Path) -> u64 {
        std::fs::symlink_metadata(path).unwrap().blocks() * 512
    }

    /// What `walk` says of `roots` with no budget to speak of.
    fn measure_roots(roots: &[PathBuf]) -> WalkEnd {
        let mut budget = Budget::new(SizeBudget::default());
        walk(roots, &mut budget, &Protected::default(), &mut || true)
    }

    fn walked(end: WalkEnd) -> Walked {
        match end {
            WalkEnd::Walked(walked) => walked,
            other => panic!("expected a measurement, got {other:?}"),
        }
    }

    /// The blocks of a folder and everything in it, each hard link once,
    /// as `du` would count them.
    fn du(path: &Path) -> u64 {
        let mut seen = HashSet::new();
        let mut sum = 0;
        let mut stack = vec![path.to_path_buf()];
        while let Some(p) = stack.pop() {
            let meta = std::fs::symlink_metadata(&p).unwrap();
            if meta.is_dir() || meta.nlink() == 1 || seen.insert((meta.dev(), meta.ino())) {
                sum += meta.blocks() * 512;
            }
            if meta.is_dir() {
                for entry in std::fs::read_dir(&p).unwrap() {
                    stack.push(entry.unwrap().path());
                }
            }
        }
        sum
    }

    fn instance(adapter_id: &str, id: &str, prefix: &Path) -> ManagerInstance {
        ManagerInstance {
            prefix: prefix.to_path_buf(),
            ..crate::testing::manager_instance(adapter_id, id)
        }
    }

    fn artifact(
        instance_id: &str,
        kind: ArtifactKind,
        name: &str,
        version: &str,
        path: Option<PathBuf>,
    ) -> InstalledArtifact {
        InstalledArtifact {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind,
                name: name.to_string(),
            },
            display_name: name.to_string(),
            version: version.to_string(),
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path,
            auto_updates: false,
            uninstall_blocked: None,
            facts: Default::default(),
        }
    }

    /// A meter that keeps every `Sizes` it published, in order.
    fn recording_meter(budget: SizeBudget) -> (Arc<SizeMeter>, Arc<Mutex<Vec<Sizes>>>) {
        let slot: Arc<OnceLock<Weak<SizeMeter>>> = Arc::new(OnceLock::new());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let meter = SizeMeter::new(budget, {
            let slot = slot.clone();
            let seen = seen.clone();
            move |_round| {
                if let Some(meter) = slot.get().and_then(Weak::upgrade) {
                    seen.lock().unwrap().push(meter.sizes());
                }
            }
        });
        slot.set(Arc::downgrade(&meter)).unwrap();
        (meter, seen)
    }

    /// One round on this thread, as `measure` runs it on its own.
    fn run(
        meter: &SizeMeter,
        round: u64,
        instances: &[ManagerInstance],
        artifacts: &[InstalledArtifact],
        home: &Path,
    ) -> Sizes {
        meter.current.fetch_max(round, Ordering::SeqCst);
        meter.run_round(round, instances, artifacts, home);
        meter.sizes()
    }

    fn size_of<'a>(sizes: &'a Sizes, name: &str) -> Option<&'a ArtifactSize> {
        sizes.artifacts.iter().find(|size| size.key.name == name)
    }

    #[test]
    fn test_a_hard_linked_file_counts_once_in_a_measurement() {
        let scratch = Scratch::new("hardlink");
        let tool = scratch.dir("tool");
        let file = scratch.file("tool/lib.dylib", 200_000);
        std::fs::hard_link(&file, tool.join("again.dylib")).unwrap();
        let walked = walked(measure_roots(std::slice::from_ref(&tool)));
        assert_eq!(walked.measured.bytes, blocks_of(&tool) + blocks_of(&file));
        assert_eq!(walked.measured.bytes, du(&tool));
        assert_eq!(walked.shared.len(), 1, "one file, two names");
    }

    #[test]
    fn test_a_file_two_tools_share_counts_in_each_and_once_in_the_total() {
        let scratch = Scratch::new("shared");
        let file = scratch.file("one/shared.bin", 300_000);
        scratch.file("one/own.bin", 10_000);
        std::fs::create_dir_all(scratch.path("two")).unwrap();
        std::fs::hard_link(&file, scratch.path("two/shared.bin")).unwrap();
        let one = walked(measure_roots(&[scratch.path("one")]));
        let two = walked(measure_roots(&[scratch.path("two")]));
        assert!(one.measured.bytes >= blocks_of(&file));
        assert!(two.measured.bytes >= blocks_of(&file));
        let all = total([&one, &two]).unwrap();
        assert_eq!(
            all.bytes,
            one.measured.bytes + two.measured.bytes - blocks_of(&file),
            "the shared file is in each tool's size and once in the total"
        );
        assert_eq!(total([&two, &one]), Some(all), "whatever the order");
    }

    #[test]
    fn test_the_result_does_not_depend_on_the_order_the_roots_are_walked() {
        let scratch = Scratch::new("order");
        let file = scratch.file("a/x.bin", 100_000);
        scratch.file("a/sub/y.bin", 50_000);
        std::fs::create_dir_all(scratch.path("b")).unwrap();
        std::fs::hard_link(&file, scratch.path("b/x.bin")).unwrap();
        scratch.file("b/z.bin", 20_000);
        let ab = walked(measure_roots(&[scratch.path("a"), scratch.path("b")]));
        let ba = walked(measure_roots(&[scratch.path("b"), scratch.path("a")]));
        assert_eq!(ab, ba);
        assert_eq!(
            ab.measured.bytes,
            du(&scratch.path("a")) + du(&scratch.path("b")) - blocks_of(&file)
        );
    }

    #[test]
    fn test_a_symbolic_link_is_counted_as_itself_and_never_followed() {
        let scratch = Scratch::new("symlink");
        let big = scratch.file("elsewhere/big.bin", 2_000_000);
        let tool = scratch.dir("tool");
        scratch.file("tool/small.bin", 1_000);
        symlink(&big, tool.join("link-to-big")).unwrap();
        symlink(scratch.path("elsewhere"), tool.join("link-to-folder")).unwrap();
        let walked = walked(measure_roots(std::slice::from_ref(&tool)));
        assert_eq!(walked.measured.bytes, du(&tool));
        assert!(walked.measured.bytes < blocks_of(&big));
    }

    #[test]
    fn test_a_sparse_file_counts_the_blocks_it_takes_not_its_length() {
        let scratch = Scratch::new("sparse");
        let path = scratch.path("sparse.img");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(512 * 1024 * 1024).unwrap();
        drop(file);
        let walked = walked(measure_roots(std::slice::from_ref(&path)));
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 512 * 1024 * 1024);
        assert_eq!(walked.measured.bytes, blocks_of(&path));
        assert!(
            walked.measured.bytes < 1024 * 1024,
            "half a gigabyte long, next to nothing on disk: {}",
            walked.measured.bytes
        );
    }

    #[test]
    fn test_an_unreadable_folder_is_skipped_and_marks_the_result_partial() {
        let scratch = Scratch::new("unreadable");
        let tool = scratch.dir("tool");
        scratch.file("tool/readable.bin", 40_000);
        let locked = scratch.dir("tool/locked");
        scratch.file("tool/locked/hidden.bin", 400_000);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let end = measure_roots(std::slice::from_ref(&tool));
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        let walked = walked(end);
        assert!(walked.measured.partial);
        assert!(!walked.measured.at_least);
        assert!(walked.measured.bytes >= blocks_of(&scratch.path("tool/readable.bin")));
        assert!(
            walked.measured.bytes < du(&tool),
            "what it could not read is not in it"
        );
    }

    #[test]
    fn test_something_that_goes_away_mid_walk_makes_it_partial_not_an_error() {
        let scratch = Scratch::new("vanish");
        let tool = scratch.dir("tool");
        for name in ["a", "b", "c", "d"] {
            scratch.file(&format!("tool/{name}.bin"), 30_000);
        }
        let folder_blocks = blocks_of(&tool);
        let mut asked = 0;
        let mut budget = Budget::new(SizeBudget::default());
        // Asked before the root, before the folder is listed, then before
        // each of its entries: as the first entry is about to be looked
        // at, every file goes.
        let end = walk(
            std::slice::from_ref(&tool),
            &mut budget,
            &Protected::default(),
            &mut || {
                asked += 1;
                if asked == 3 {
                    for name in ["a", "b", "c", "d"] {
                        let _ = std::fs::remove_file(tool.join(format!("{name}.bin")));
                    }
                }
                true
            },
        );
        let mid_walk = walked(end);
        assert!(mid_walk.measured.partial);
        assert_eq!(mid_walk.measured.bytes, folder_blocks);
        // A root gone since it was planned, beside one that is there.
        let one_gone = walked(measure_roots(&[tool.clone(), scratch.path("gone")]));
        assert!(one_gone.measured.partial);
        // Every root gone: nothing to say, rather than "about 0 KB".
        assert_eq!(measure_roots(&[scratch.path("gone")]), WalkEnd::Nothing);
    }

    #[test]
    fn test_a_spent_budget_marks_the_result_at_least() {
        let scratch = Scratch::new("budget");
        let tool = scratch.dir("tool");
        for index in 0..10 {
            scratch.file(&format!("tool/{index}.bin"), 20_000);
        }
        let mut budget = Budget::new(SizeBudget {
            max_entries: 4,
            max_duration: Duration::from_secs(30),
        });
        let walked = walked(walk(
            std::slice::from_ref(&tool),
            &mut budget,
            &Protected::default(),
            &mut || true,
        ));
        assert!(walked.measured.at_least);
        assert!(walked.measured.bytes < du(&tool));
        // And no time left is no budget left: not reached, which is not
        // "nothing there".
        let mut budget = Budget::new(SizeBudget {
            max_entries: 1_000,
            max_duration: Duration::ZERO,
        });
        assert_eq!(
            walk(&[tool], &mut budget, &Protected::default(), &mut || true),
            WalkEnd::OutOfBudget
        );
    }

    #[test]
    fn test_a_newer_round_stops_a_walk_at_its_next_entry() {
        let scratch = Scratch::new("superseded");
        let tool = scratch.dir("tool");
        scratch.file("tool/a.bin", 1_000);
        let mut budget = Budget::new(SizeBudget::default());
        assert_eq!(
            walk(&[tool], &mut budget, &Protected::default(), &mut || false),
            WalkEnd::Superseded
        );
    }

    #[test]
    fn test_a_folder_on_another_volume_is_neither_counted_nor_entered() {
        // No test can mount a volume inside a folder of its own, so the
        // folder is walked as if it were on another volume than its
        // subfolders: they are skipped whole, and its own files still
        // count.
        let scratch = Scratch::new("volume");
        let tool = scratch.dir("tool");
        let top = scratch.file("tool/top.bin", 30_000);
        scratch.file("tool/mounted/inside.bin", 500_000);
        let device = std::fs::symlink_metadata(&tool).unwrap().dev();
        let mut tally = Tally::default();
        let mut budget = Budget::new(SizeBudget::default());
        let flow = walk_folder(
            &tool,
            device.wrapping_add(1),
            &mut tally,
            &mut budget,
            &Protected::default(),
            &mut || true,
        );
        assert!(matches!(flow, Flow::Finished));
        assert_eq!(tally.finish().measured.bytes, blocks_of(&top));
        // On its own volume, the same folder counts everything.
        let mut tally = Tally::default();
        walk_folder(
            &tool,
            device,
            &mut tally,
            &mut budget,
            &Protected::default(),
            &mut || true,
        );
        assert_eq!(tally.finish().measured.bytes, du(&tool) - blocks_of(&tool));
    }

    #[test]
    fn test_protected_places_are_named_from_the_home_folder_and_volumes() {
        let protected = Protected::new(Path::new("/Users/you"));
        for inside in [
            "/Users/you/Documents",
            "/Users/you/Documents/npm/lib",
            "/Users/you/desktop/x",
            "/Users/you/Downloads/tool",
            "/Users/you/Library/Mobile Documents/com~apple~CloudDocs/x",
            "/Users/you/Library/CloudStorage/Dropbox/x",
            "/Users/you/Pictures/a",
            "/Users/you/Movies/a",
            "/Users/you/Music/a",
            "/Volumes/External/ollama/models",
        ] {
            assert!(
                protected.contains(Path::new(inside)),
                "{inside} is protected"
            );
        }
        for outside in [
            "/Users/you/.ollama/models/blobs",
            "/Users/you/Library/Application Support/pipx/venvs/x",
            "/Users/you/Documents-old/x",
            "/opt/homebrew/Cellar/jq/1.8.2",
            "/Applications/iTerm.app",
        ] {
            assert!(
                !protected.contains(Path::new(outside)),
                "{outside} is not protected"
            );
        }
        assert!(
            protected.under(Path::new("/Users/you")),
            "the home folder holds them"
        );
        assert!(protected.under(Path::new("/Users/you/Library")));
        assert!(!protected.under(Path::new("/Users/you/.local")));
    }

    #[test]
    fn test_a_tool_in_a_protected_place_or_on_another_volume_is_never_measured() {
        let scratch = Scratch::new("protected");
        let home = scratch.dir("home");
        // npm's prefix in Documents, spelled outright.
        scratch.file(
            "home/Documents/npm/lib/node_modules/prettier/index.js",
            5_000,
        );
        // npm's prefix reached through a link into Documents.
        scratch.file(
            "home/Documents/other/lib/node_modules/typescript/tsc.js",
            5_000,
        );
        symlink(home.join("Documents/other"), home.join(".npm-global")).unwrap();
        // A uv tool on another volume, and one reached by a link to one.
        symlink("/Volumes/Somewhere/uv", home.join("uv-tools")).unwrap();
        // One that is fine, to show the round ran.
        scratch.file("home/.local/share/uv/tools/ruff/bin/ruff", 8_000);
        let instances = [
            instance("npm", "npm:docs", &home.join("Documents/npm")),
            instance("npm", "npm:linked", &home.join(".npm-global")),
            instance("uv", "uv", &home),
        ];
        let artifacts = [
            artifact("npm:docs", ArtifactKind::Package, "prettier", "3.8.1", None),
            artifact(
                "npm:linked",
                ArtifactKind::Package,
                "typescript",
                "6.0.2",
                None,
            ),
            artifact(
                "uv",
                ArtifactKind::Tool,
                "black",
                "25.1.0",
                Some(PathBuf::from("/Volumes/Somewhere/uv/black")),
            ),
            artifact(
                "uv",
                ArtifactKind::Tool,
                "mypy",
                "1.18.0",
                Some(home.join("uv-tools/mypy")),
            ),
            artifact(
                "uv",
                ArtifactKind::Tool,
                "ruff",
                "0.14.3",
                Some(home.join(".local/share/uv/tools/ruff")),
            ),
        ];
        let (meter, _) = recording_meter(SizeBudget::default());
        let sizes = run(&meter, 1, &instances, &artifacts, &home);
        assert!(sizes.done);
        let names: Vec<&str> = sizes
            .artifacts
            .iter()
            .map(|s| s.key.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec!["ruff"],
            "nothing in Documents or on another volume"
        );
    }

    #[test]
    fn test_ollama_models_are_measured_once_as_their_folder_and_no_more_than_their_sum() {
        let scratch = Scratch::new("ollama");
        let home = scratch.dir("home");
        // Two models sharing one layer: each model's own size counts it,
        // the folder holds it once.
        let shared = scratch.file("home/.ollama/models/blobs/sha256-shared", 400_000);
        let a = scratch.file("home/.ollama/models/blobs/sha256-a", 300_000);
        let b = scratch.file("home/.ollama/models/blobs/sha256-b", 200_000);
        scratch.file(
            "home/.ollama/models/manifests/registry.ollama.ai/library/a/latest",
            500,
        );
        let id = "ollama:http://127.0.0.1:11434";
        let mut model_a = artifact(id, ArtifactKind::Model, "a:latest", "aaaa", None);
        model_a.size_bytes = Some(700_000);
        let mut model_b = artifact(id, ArtifactKind::Model, "b:latest", "bbbb", None);
        model_b.size_bytes = Some(600_000);
        let instances = [instance("ollama", id, &home.join(".ollama"))];
        let artifacts = [model_a, model_b];
        let (meter, _) = recording_meter(SizeBudget::default());
        let sizes = run(&meter, 1, &instances, &artifacts, &home);
        assert!(sizes.artifacts.is_empty(), "each model keeps its own size");
        let models = sizes
            .models
            .first()
            .expect("the models' folder is measured");
        assert_eq!(models.instance_id, id);
        let measured = models.measured.unwrap();
        assert_eq!(
            measured.bytes,
            du(&home.join(".ollama/models/blobs")),
            "the folder, each layer once"
        );
        assert!(measured.bytes >= blocks_of(&shared) + blocks_of(&a) + blocks_of(&b));
        assert!(
            measured.bytes <= 700_000 + 600_000,
            "never more than the models' own sizes added up"
        );
        assert_eq!(
            sizes.sources,
            vec![SourceSize {
                instance_id: id.to_string(),
                measured,
            }],
            "Ollama's total is its models' folder, as its own line says"
        );
    }

    #[test]
    fn test_a_models_folder_holding_less_than_one_model_is_not_where_they_are() {
        // `OLLAMA_MODELS` elsewhere: `~/.ollama/models/blobs` holds a
        // leftover, smaller than the largest model Ollama lists.
        let scratch = Scratch::new("ollama-elsewhere");
        let home = scratch.dir("home");
        scratch.file("home/.ollama/models/blobs/sha256-left", 4_000);
        let id = "ollama:http://127.0.0.1:11434";
        let mut model = artifact(id, ArtifactKind::Model, "big:latest", "cccc", None);
        model.size_bytes = Some(4_000_000_000);
        let (meter, _) = recording_meter(SizeBudget::default());
        let sizes = run(
            &meter,
            1,
            &[instance("ollama", id, &home.join(".ollama"))],
            &[model.clone()],
            &home,
        );
        assert!(sizes.done);
        assert!(sizes.models.is_empty());
        // And another Mac's Ollama is never measured here at all.
        let remote = "ollama:http://10.0.0.5:11434";
        model.key.instance_id = remote.to_string();
        model.size_bytes = Some(1_000);
        let sizes = run(
            &meter,
            2,
            &[instance("ollama", remote, &home.join(".ollama"))],
            &[model],
            &home,
        );
        assert!(sizes.models.is_empty());
    }

    #[test]
    fn test_a_formula_is_its_keg_and_its_other_kegs_are_its_old_versions() {
        let scratch = Scratch::new("brew");
        let home = scratch.dir("home");
        let prefix = scratch.dir("homebrew");
        scratch.file("homebrew/Cellar/node@22/22.23.3/bin/node", 90_000);
        scratch.file("homebrew/Cellar/node@22/22.23.2_2/bin/node", 80_000);
        scratch.file("homebrew/Cellar/node@22/22.20.0/bin/node", 70_000);
        scratch.file("homebrew/Cellar/jq/1.8.2/bin/jq", 5_000);
        scratch.file("homebrew/Caskroom/iterm2/3.6.4/.metadata/x.json", 1_000);
        scratch.file("Applications/iTerm.app/Contents/MacOS/iTerm2", 60_000);
        let id = "brew:homebrew";
        let instances = [instance("brew", id, &prefix)];
        let artifacts = [
            artifact(id, ArtifactKind::Formula, "node@22", "22.23.3", None),
            artifact(id, ArtifactKind::Formula, "jq", "1.8.2", None),
            artifact(
                id,
                ArtifactKind::Cask,
                "iterm2",
                "3.6.4",
                Some(scratch.path("Applications/iTerm.app")),
            ),
            // A font: no app, nothing measured.
            artifact(id, ArtifactKind::Cask, "font-jetbrains-mono", "2.304", None),
        ];
        let (meter, _) = recording_meter(SizeBudget::default());
        let sizes = run(&meter, 1, &instances, &artifacts, &home);
        let node = size_of(&sizes, "node@22").unwrap();
        assert_eq!(node.version, "22.23.3");
        assert_eq!(
            node.measured.unwrap().bytes,
            du(&prefix.join("Cellar/node@22/22.23.3"))
        );
        assert_eq!(
            node.old_versions.unwrap().bytes,
            du(&prefix.join("Cellar/node@22/22.23.2_2"))
                + du(&prefix.join("Cellar/node@22/22.20.0"))
        );
        let jq = size_of(&sizes, "jq").unwrap();
        assert_eq!(jq.old_versions, None, "only one keg");
        let iterm = size_of(&sizes, "iterm2").unwrap();
        assert_eq!(
            iterm.measured.unwrap().bytes,
            du(&scratch.path("Applications/iTerm.app")) + du(&prefix.join("Caskroom/iterm2"))
        );
        assert!(size_of(&sizes, "font-jetbrains-mono").is_none());
        let names: Vec<&str> = sizes
            .artifacts
            .iter()
            .map(|s| s.key.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec!["node@22", "jq", "iterm2"],
            "in the snapshot's order"
        );
        // Everything above, old versions included, once.
        assert_eq!(
            sizes.total.unwrap().bytes,
            du(&prefix.join("Cellar/node@22/22.23.3"))
                + du(&prefix.join("Cellar/node@22/22.23.2_2"))
                + du(&prefix.join("Cellar/node@22/22.20.0"))
                + du(&prefix.join("Cellar/jq/1.8.2"))
                + du(&scratch.path("Applications/iTerm.app"))
                + du(&prefix.join("Caskroom/iterm2"))
        );
    }

    #[test]
    fn test_each_source_has_its_own_total_with_a_shared_file_once() {
        let scratch = Scratch::new("per-source");
        let home = scratch.dir("home");
        let prefix = scratch.dir("homebrew");
        scratch.file("homebrew/Cellar/jq/1.8.2/bin/jq", 5_000);
        scratch.file("homebrew/Cellar/jq/1.7.1/bin/jq", 4_000);
        // Two uv tools sharing one file through a hard link: in each tool's
        // size, once in uv's total.
        let shared = scratch.file("home/tools/aa/lib/shared.so", 64_000);
        scratch.file("home/tools/bb/bin/bb", 8_000);
        std::fs::hard_link(&shared, scratch.path("home/tools/bb/lib-shared.so")).unwrap();
        let instances = [
            instance("brew", "brew:homebrew", &prefix),
            instance("uv", "uv", &home),
            // A source with nothing measured has no total.
            instance("pip", "pip", &home),
        ];
        let tool = |name: &str| {
            artifact(
                "uv",
                ArtifactKind::Tool,
                name,
                "1.0.0",
                Some(home.join("tools").join(name)),
            )
        };
        let artifacts = [
            artifact("brew:homebrew", ArtifactKind::Formula, "jq", "1.8.2", None),
            tool("aa"),
            tool("bb"),
            artifact("pip", ArtifactKind::Package, "requests", "2.32.5", None),
        ];
        let (meter, _) = recording_meter(SizeBudget::default());
        let sizes = run(&meter, 1, &instances, &artifacts, &home);
        let aa = size_of(&sizes, "aa").unwrap().measured.unwrap().bytes;
        let bb = size_of(&sizes, "bb").unwrap().measured.unwrap().bytes;
        assert_eq!(
            (aa, bb),
            (du(&home.join("tools/aa")), du(&home.join("tools/bb"))),
            "each tool counts it"
        );
        let uv_total = aa + bb - blocks_of(&shared);
        let exact = |bytes| Measured {
            bytes,
            partial: false,
            at_least: false,
        };
        assert_eq!(
            sizes.sources,
            vec![
                SourceSize {
                    instance_id: "brew:homebrew".to_string(),
                    // Its old versions too, as the grand total has them.
                    measured: exact(du(&prefix.join("Cellar/jq"))),
                },
                SourceSize {
                    instance_id: "uv".to_string(),
                    measured: exact(uv_total),
                },
            ]
        );
        assert_eq!(
            sizes.total.unwrap().bytes,
            du(&prefix.join("Cellar/jq")) + uv_total,
            "the sources' totals add up to the grand total when they share nothing"
        );
    }

    #[test]
    fn test_a_source_the_budget_did_not_reach_says_at_least_and_no_other_does() {
        let scratch = Scratch::new("per-source-unreached");
        let home = scratch.dir("home");
        scratch.file("home/one/aa/bin/aa", 8_000);
        for index in 0..30 {
            scratch.file(&format!("home/one/big/lib/{index}.so"), 4_000);
        }
        scratch.file("home/two/cc/bin/cc", 8_000);
        let instances = [
            instance("uv", "uv-one", &home),
            instance("uv", "uv-two", &home),
        ];
        let tool = |id: &str, folder: &str, name: &str| {
            artifact(
                id,
                ArtifactKind::Tool,
                name,
                "1.0.0",
                Some(home.join(folder).join(name)),
            )
        };
        let artifacts = [
            tool("uv-one", "one", "aa"),
            tool("uv-one", "one", "big"),
            tool("uv-two", "two", "cc"),
        ];
        // Enough for "aa" whole and part of "big"; "cc" is never reached.
        let (meter, _) = recording_meter(SizeBudget {
            max_entries: 20,
            max_duration: Duration::from_secs(30),
        });
        let sizes = run(&meter, 1, &instances, &artifacts, &home);
        assert!(sizes.done);
        let one = &sizes.sources[0];
        assert_eq!(one.instance_id, "uv-one");
        assert!(one.measured.at_least && one.measured.bytes > 0);
        assert_eq!(
            sizes.sources[1],
            SourceSize {
                instance_id: "uv-two".to_string(),
                measured: Measured {
                    bytes: 0,
                    partial: false,
                    at_least: true,
                },
            },
            "nothing of it reached: at least, by an amount not known"
        );
        // A complete round says each source's exactly.
        let (meter, _) = recording_meter(SizeBudget::default());
        let sizes = run(&meter, 1, &instances, &artifacts, &home);
        assert!(sizes.sources.iter().all(|source| !source.measured.at_least));
    }

    #[test]
    fn test_sources_are_empty_until_the_round_is_done() {
        let scratch = Scratch::new("per-source-pending");
        let home = scratch.dir("home");
        scratch.file("home/tools/aa/bin/aa", 8_000);
        let instances = [instance("uv", "uv", &home)];
        let artifacts = [artifact(
            "uv",
            ArtifactKind::Tool,
            "aa",
            "1.0.0",
            Some(home.join("tools/aa")),
        )];
        let (meter, seen) = recording_meter(SizeBudget::default());
        run(&meter, 1, &instances, &artifacts, &home);
        let seen = seen.lock().unwrap().clone();
        assert!(seen
            .iter()
            .filter(|sizes| !sizes.done)
            .all(|sizes| sizes.sources.is_empty()));
        assert_eq!(seen.last().unwrap().sources.len(), 1);
    }

    #[test]
    fn test_each_source_is_measured_where_it_keeps_a_tool_and_nowhere_else() {
        let scratch = Scratch::new("sources");
        let home = scratch.dir("home");
        let npm = scratch.dir("npm-prefix");
        scratch.file(
            "npm-prefix/lib/node_modules/@anthropic-ai/claude-code/cli.js",
            70_000,
        );
        scratch.file(
            "npm-prefix/lib/node_modules/corepack/dist/corepack.js",
            9_000,
        );
        scratch.file("npm-prefix/lib/node_modules/evil/x.js", 9_000);
        let cargo = scratch.dir("home/.cargo");
        scratch.file("home/.cargo/bin/cargo-binstall", 30_000);
        scratch.file("home/.cargo/bin/detect-targets", 20_000);
        scratch.file("home/.cargo/registry/cache/huge.crate", 900_000);
        std::fs::write(
            cargo.join(".crates2.json"),
            r#"{"installs":{"cargo-binstall 1.15.0 (registry+https://github.com/rust-lang/crates.io-index)":{"bins":["cargo-binstall","detect-targets","../escape"]}}}"#,
        )
        .unwrap();
        scratch.file("home/.local/pipx/venvs/httpie/bin/http", 12_000);
        scratch.file("home/.local/share/claude/versions/2.1.282", 150_000);
        let instances = [
            instance("npm", "npm:p", &npm),
            instance("cargo", "cargo:c", &cargo),
            instance("pipx", "pipx", &home),
            instance(
                "standalone-claude",
                "standalone-claude",
                &home.join(".local/share/claude"),
            ),
            instance("pip", "pip:x", &home),
        ];
        let artifacts = [
            artifact(
                "npm:p",
                ArtifactKind::Package,
                "@anthropic-ai/claude-code",
                "2.1.269",
                None,
            ),
            artifact("npm:p", ArtifactKind::Package, "corepack", "0.36.0", None),
            artifact("npm:p", ArtifactKind::Package, "../evil", "1.0.0", None),
            artifact(
                "cargo:c",
                ArtifactKind::Binary,
                "cargo-binstall",
                "1.15.0",
                Some(cargo.join("bin/cargo-binstall")),
            ),
            artifact(
                "pipx",
                ArtifactKind::Tool,
                "httpie",
                "3.2.4",
                Some(home.join(".local/pipx/venvs/httpie")),
            ),
            artifact(
                "standalone-claude",
                ArtifactKind::Binary,
                "claude",
                "2.1.282",
                Some(home.join(".local/share/claude/versions/2.1.282")),
            ),
            artifact("pip:x", ArtifactKind::Package, "requests", "2.32.4", None),
        ];
        let (meter, _) = recording_meter(SizeBudget::default());
        let sizes = run(&meter, 1, &instances, &artifacts, &home);
        let bytes = |name: &str| {
            size_of(&sizes, name)
                .and_then(|s| s.measured)
                .map(|m| m.bytes)
        };
        assert_eq!(
            bytes("@anthropic-ai/claude-code"),
            Some(du(&npm.join("lib/node_modules/@anthropic-ai/claude-code")))
        );
        assert_eq!(
            bytes("corepack"),
            Some(du(&npm.join("lib/node_modules/corepack")))
        );
        assert_eq!(
            bytes("../evil"),
            None,
            "a name that is not a package name is no path"
        );
        assert_eq!(
            bytes("cargo-binstall"),
            Some(
                blocks_of(&cargo.join("bin/cargo-binstall"))
                    + blocks_of(&cargo.join("bin/detect-targets"))
            ),
            "every program the crate installed, not Cargo's caches"
        );
        assert_eq!(
            bytes("httpie"),
            Some(du(&home.join(".local/pipx/venvs/httpie")))
        );
        assert_eq!(
            bytes("claude"),
            Some(blocks_of(
                &home.join(".local/share/claude/versions/2.1.282")
            ))
        );
        assert_eq!(bytes("requests"), None, "pip is not measured");
    }

    #[test]
    fn test_a_tool_whose_folder_is_gone_or_is_a_link_gets_no_size() {
        let scratch = Scratch::new("gone");
        let home = scratch.dir("home");
        let real = scratch.dir("elsewhere/ruff");
        scratch.file("elsewhere/ruff/bin/ruff", 8_000);
        symlink(&real, home.join("ruff-link")).unwrap();
        let artifacts = [
            artifact(
                "uv",
                ArtifactKind::Tool,
                "black",
                "25.1.0",
                Some(home.join("missing/black")),
            ),
            artifact(
                "uv",
                ArtifactKind::Tool,
                "ruff",
                "0.14.3",
                Some(home.join("ruff-link")),
            ),
        ];
        let (meter, _) = recording_meter(SizeBudget::default());
        let sizes = run(&meter, 1, &[instance("uv", "uv", &home)], &artifacts, &home);
        assert!(sizes.done);
        assert!(sizes.artifacts.is_empty());
        assert_eq!(sizes.total, None, "nothing measured, no total");
    }

    #[test]
    fn test_a_round_shows_what_it_will_measure_first_then_each_result_then_done() {
        let scratch = Scratch::new("publish");
        let home = scratch.dir("home");
        scratch.file("home/tools/ruff/bin/ruff", 8_000);
        let artifacts = [artifact(
            "uv",
            ArtifactKind::Tool,
            "ruff",
            "0.14.3",
            Some(home.join("tools/ruff")),
        )];
        let (meter, seen) = recording_meter(SizeBudget::default());
        run(&meter, 7, &[instance("uv", "uv", &home)], &artifacts, &home);
        let seen = seen.lock().unwrap().clone();
        let first = seen.first().unwrap();
        assert_eq!(first.round, 7);
        assert!(!first.done);
        assert_eq!(first.artifacts[0].measured, None, "measuring");
        let last = seen.last().unwrap();
        assert!(last.done);
        assert!(last.artifacts[0].measured.is_some());
        assert!(last.total.is_some());
    }

    #[test]
    fn test_a_folder_measured_at_the_same_version_is_not_walked_again() {
        let scratch = Scratch::new("cache");
        let home = scratch.dir("home");
        let tool = scratch.dir("home/tools/ruff");
        scratch.file("home/tools/ruff/bin/ruff", 8_000);
        let instances = [instance("uv", "uv", &home)];
        let at = |version: &str| {
            [artifact(
                "uv",
                ArtifactKind::Tool,
                "ruff",
                version,
                Some(tool.clone()),
            )]
        };
        let (meter, seen) = recording_meter(SizeBudget::default());
        let first = run(&meter, 1, &instances, &at("0.14.3"), &home);
        let before = first.artifacts[0].measured.unwrap().bytes;
        scratch.file("home/tools/ruff/lib/new.so", 300_000);
        let second = run(&meter, 2, &instances, &at("0.14.3"), &home);
        assert_eq!(
            second.artifacts[0].measured.unwrap().bytes,
            before,
            "remembered"
        );
        // Shown at once, with no "measuring" first.
        let second_round: Vec<Sizes> = seen
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.round == 2)
            .cloned()
            .collect();
        assert!(second_round[0].artifacts[0].measured.is_some());
        let third = run(&meter, 3, &instances, &at("0.14.5"), &home);
        assert!(
            third.artifacts[0].measured.unwrap().bytes > before,
            "a new version is walked again"
        );
    }

    #[test]
    fn test_what_the_budget_did_not_reach_is_measured_first_next_round() {
        let scratch = Scratch::new("unreached");
        let home = scratch.dir("home");
        scratch.file("home/tools/aa/bin/aa", 8_000);
        for index in 0..30 {
            scratch.file(&format!("home/tools/big/lib/{index}.so"), 4_000);
        }
        scratch.file("home/tools/cc/bin/cc", 8_000);
        let instances = [instance("uv", "uv", &home)];
        let artifacts: Vec<InstalledArtifact> = ["aa", "big", "cc"]
            .into_iter()
            .map(|name| {
                artifact(
                    "uv",
                    ArtifactKind::Tool,
                    name,
                    "1.0.0",
                    Some(home.join("tools").join(name)),
                )
            })
            .collect();
        // Enough for "aa" whole and part of "big".
        let (meter, seen) = recording_meter(SizeBudget {
            max_entries: 20,
            max_duration: Duration::from_secs(30),
        });
        let first = run(&meter, 1, &instances, &artifacts, &home);
        assert!(first.done);
        let aa = size_of(&first, "aa").unwrap().measured.unwrap();
        assert!(!aa.at_least && !aa.partial);
        assert!(size_of(&first, "big").unwrap().measured.unwrap().at_least);
        assert!(
            size_of(&first, "cc").is_none(),
            "not reached: no size this round, rather than \"about 0 KB\""
        );
        assert!(
            first.total.unwrap().at_least,
            "the total says what it did not reach"
        );
        // The next round: "aa" is remembered, "cc" is measured before "big"
        // is walked again, and "big" keeps its "at least" meanwhile.
        let second = run(&meter, 2, &instances, &artifacts, &home);
        let cc = size_of(&second, "cc").unwrap().measured.unwrap();
        assert!(!cc.at_least && !cc.partial);
        assert!(size_of(&second, "big").unwrap().measured.unwrap().at_least);
        assert!(!second.total.unwrap().partial);
        let shown_first = seen
            .lock()
            .unwrap()
            .iter()
            .find(|sizes| sizes.round == 2)
            .cloned()
            .unwrap();
        assert!(
            size_of(&shown_first, "big")
                .unwrap()
                .measured
                .is_some_and(|m| m.at_least),
            "shown from the round before, never \"Calculating\" again"
        );
        assert_eq!(size_of(&shown_first, "cc").unwrap().measured, None);
    }

    #[test]
    fn test_an_older_round_never_shows_over_a_newer_one() {
        let scratch = Scratch::new("rounds");
        let home = scratch.dir("home");
        scratch.file("home/tools/ruff/bin/ruff", 8_000);
        let artifacts = [artifact(
            "uv",
            ArtifactKind::Tool,
            "ruff",
            "0.14.3",
            Some(home.join("tools/ruff")),
        )];
        let (meter, seen) = recording_meter(SizeBudget::default());
        meter.current.store(5, Ordering::SeqCst);
        meter.run_round(4, &[instance("uv", "uv", &home)], &artifacts, &home);
        assert!(
            seen.lock().unwrap().is_empty(),
            "round 4 started after 5: it shows nothing"
        );
        assert_eq!(meter.sizes(), Sizes::default());
        // And `measure` does not start a round no newer than the newest.
        meter.measure(5, &[], &artifacts, &home);
        meter.measure(3, &[], &artifacts, &home);
        assert_eq!(meter.current.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn test_measure_runs_on_a_thread_of_its_own_and_calls_back_with_its_round() {
        let scratch = Scratch::new("thread");
        let home = scratch.dir("home");
        scratch.file("home/tools/ruff/bin/ruff", 8_000);
        let artifacts = [artifact(
            "uv",
            ArtifactKind::Tool,
            "ruff",
            "0.14.3",
            Some(home.join("tools/ruff")),
        )];
        let rounds = Arc::new(Mutex::new(Vec::new()));
        let meter = SizeMeter::new(SizeBudget::default(), {
            let rounds = rounds.clone();
            move |round| rounds.lock().unwrap().push(round)
        });
        meter.measure(3, &[instance("uv", "uv", &home)], &artifacts, &home);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !meter.sizes().done {
            assert!(Instant::now() < deadline, "the round never finished");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(meter.sizes().round, 3);
        assert!(rounds.lock().unwrap().iter().all(|round| *round == 3));
        assert!(!rounds.lock().unwrap().is_empty());
    }

    #[test]
    fn test_the_window_still_gets_sizes_after_a_measuring_thread_panicked_holding_them() {
        let meter = SizeMeter::new(SizeBudget::default(), |_| {});
        let poisoner = meter.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.published.lock().unwrap();
            panic!("a measuring thread that died holding the sizes");
        })
        .join();
        assert!(meter.published.is_poisoned());
        assert_eq!(meter.sizes(), Sizes::default());
        meter.current.store(1, Ordering::SeqCst);
        assert!(meter.publish(
            1,
            Sizes {
                round: 1,
                done: true,
                ..Sizes::default()
            }
        ));
        assert!(meter.sizes().done);
    }

    #[test]
    fn test_sizes_are_the_json_the_typescript_mirror_reads() {
        // `src/lib/types.test.ts` spells this exact string.
        let sizes = Sizes {
            round: 3,
            done: true,
            artifacts: vec![ArtifactSize {
                key: ArtifactKey {
                    instance_id: "brew:/opt/homebrew".to_string(),
                    kind: ArtifactKind::Formula,
                    name: "node@22".to_string(),
                },
                version: "22.23.3".to_string(),
                measured: Some(Measured {
                    bytes: 312_000_000,
                    partial: false,
                    at_least: false,
                }),
                old_versions: Some(Measured {
                    bytes: 1_200_000_000,
                    partial: true,
                    at_least: false,
                }),
            }],
            models: vec![ModelsSize {
                instance_id: "ollama:http://127.0.0.1:11434".to_string(),
                measured: None,
            }],
            total: None,
            sources: vec![SourceSize {
                instance_id: "brew:/opt/homebrew".to_string(),
                measured: Measured {
                    bytes: 1_512_000_000,
                    partial: true,
                    at_least: false,
                },
            }],
        };
        let json = serde_json::to_string(&sizes).unwrap();
        assert_eq!(
            json,
            r#"{"round":3,"done":true,"artifacts":[{"key":{"instance_id":"brew:/opt/homebrew","kind":"Formula","name":"node@22"},"version":"22.23.3","measured":{"bytes":312000000,"partial":false,"at_least":false},"old_versions":{"bytes":1200000000,"partial":true,"at_least":false}}],"models":[{"instance_id":"ollama:http://127.0.0.1:11434","measured":null}],"total":null,"sources":[{"instance_id":"brew:/opt/homebrew","measured":{"bytes":1512000000,"partial":true,"at_least":false}}]}"#
        );
        assert_eq!(serde_json::from_str::<Sizes>(&json).unwrap(), sizes);
        assert_eq!(
            serde_json::to_string(&Sizes::default()).unwrap(),
            r#"{"round":0,"done":false,"artifacts":[],"models":[],"total":null,"sources":[]}"#
        );
    }
}
