//! The path-list uninstall: how a tool with no uninstall command is removed
//! (phase 4 spec §6.2-§6.3, D8). `plan_removal` turns a recipe's
//! `Uninstall::Paths` list into the absolute paths to move, what each one
//! is (`ItemIdentity`) and the warnings the dialog lists, under the checks
//! below; `execute_removal` runs the same checks again at the confirmation,
//! compares every identity with the one the preview recorded, and then
//! moves each path to the Trash in order -- the launcher last -- running
//! every check on that item once more immediately before its move and,
//! before the launcher's move, looking once more for every other listed
//! path, which must be gone by then unless it is an optional one this run
//! never moved that Canager cannot confirm is the tool's
//! (`listed_path_back`). Nothing here knows the `Adapter` contract
//! (`mod.rs` does) or how an item is moved (`crate::trash` does).
//!
//! What the checks guard against is change by accident: the tool's own
//! updater, the user, another app doing its ordinary work between the
//! preview and the click, or during the pauses between moves. A program
//! running as the user can do everything Canager can; one that swaps an
//! item in the instant between that item's last check and the system's
//! move could still race it (`take_turn`; docs/what-we-run.md says so).

use super::recipe::{Expect, KeepSpec, Recipe, RemoveSpec, SHARED_FOLDERS};
use super::route::{self, Probe};
use super::Detected;
use crate::adapters::AdapterError;
use crate::events::{EventSink, LogNote, OpId, OperationEvent};
use crate::model::{
    Fault, ItemIdentity, ItemKind, KeptWhat, Outcome, RemovedWhat, UninstallUnsafeReason, Warning,
};
use crate::scan::Glob;
use crate::trash::{TrashError, Trasher};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// `Plan.timeout_secs` for a path-list uninstall. There is no process for
/// the runner to time out; `execute_removal` keeps its own clock against
/// this (through `Pacing.budget`) and stops between items once it is spent
/// -- before an item, never during one: a move already handed to the
/// system is waited for. The moves take milliseconds; the budget is for the
/// pauses below -- a run's own, and the ones another path-list uninstall
/// running at the same time adds (`LastMove`) -- and for a Trash that is
/// slow to answer. Read by `StandaloneAdapter::plan`.
pub const TIMEOUT_SECS: u64 = 120;

/// The gap after each move to the Trash: before the next move -- this
/// run's next item, or an item of another path-list uninstall running at
/// the same time (`LastMove`) -- and after a run's last move before that
/// run is reported finished. From a process without Full Disk
/// Access -- a Finder-launched Canager -- Finder wrote its "Put Back"
/// record for only the first of a burst of `trashItemAtURL:` calls up to
/// 1.5 s apart, and for every item when they were 2 s or more apart
/// (2026-09-25, one Mac, macOS 27.0: 15/15 and 4/4 runs; the mechanism is
/// not known). Those 4 runs also stayed alive 3 s after their last call,
/// and the record is written after the call returns (with Full Disk
/// Access, a process that exited at once lost the later records): hence
/// the same gap after a run's last item. Three seconds is the largest gap
/// measured to work, a full second above the largest that failed: it makes
/// Put Back likely for every item, not certain -- an item without the
/// record can still be dragged back out of the Trash -- and
/// `docs/what-we-run.md` says so. Each wait is cut short by Cancel and by
/// what is left of `TIMEOUT_SECS`. Read by `StandaloneAdapter::new`;
/// `with_trash_gap` sets it to zero for tests.
pub const PUT_BACK_SETTLE: Duration = Duration::from_secs(3);

/// One tool's removal as the checks see it: the recipe (for its route),
/// what detect learned (home, euid) and the recipe's two lists and its
/// backup-file patterns. Owned --
/// a copy of `Detected`, the rest `'static` recipe data -- so each item's
/// last check and its move can run together on tokio's blocking pool
/// (`execute_removal`). Built by `StandaloneAdapter::plan` and `::execute`;
/// read by `plan_removal` and `execute_removal`.
#[derive(Clone, Debug)]
pub struct Job {
    pub recipe: &'static Recipe,
    pub detected: Detected,
    pub remove: &'static [RemoveSpec],
    pub keep: &'static [KeepSpec],
    /// The recipe's backup-file patterns (`Recipe.backup_globs`): check 5.
    pub globs: &'static [Glob],
}

/// What `plan_removal` found: the absolute paths to move, in order; what
/// each one was at that moment; and the dialog's warnings -- one
/// `WillTrash` per path in the same order, an `AlreadyGone` for a program
/// path an earlier stopped run already moved, then one `WillKeep` per kept
/// path that exists. Read by `StandaloneAdapter::plan` (all three, into
/// the `Plan`: `PlanAction::TrashPaths { paths, previewed: identities }`
/// and `warnings`) and by `execute_removal` (its fresh look).
#[derive(Debug, PartialEq, Eq)]
pub struct Removal {
    pub paths: Vec<PathBuf>,
    pub identities: Vec<ItemIdentity>,
    pub warnings: Vec<Warning>,
}

/// What the user confirmed: the plan's paths in order, and what the
/// preview saw at each (`PlanAction::TrashPaths`). Built by
/// `StandaloneAdapter::execute`; read by `execute_removal`.
#[derive(Clone, Copy, Debug)]
pub struct Confirmed<'a> {
    pub paths: &'a [PathBuf],
    pub previewed: &'a [ItemIdentity],
}

/// How `execute_removal` paces itself: the gap it keeps after each move
/// (`PUT_BACK_SETTLE` in production, zero in tests), the clock it keeps
/// that gap on -- shared with every other path-list uninstall that can run
/// at the same time (`LastMove`) -- and the budget it stops between items
/// once spent (`Plan.timeout_secs`). Built by `StandaloneAdapter::execute`.
#[derive(Clone, Debug)]
pub struct Pacing {
    pub settle: Duration,
    pub budget: Duration,
    pub last_move: Arc<LastMove>,
}

/// When this process last moved an item to the Trash (`None` before its
/// first move), behind the lock an item's turn holds from its wait to its
/// move. One for every path-list uninstall that can run at the same time:
/// `standalone::all` hands one to every adapter it builds, beside the one
/// trasher, and `Session::new` calls it once. The operation manager runs
/// up to three operations at once and a path-list uninstall locks only its
/// own instance, so two tools' uninstalls can run side by side; the Trash
/// spike's bursts were one process's calls (`PUT_BACK_SETTLE`), and to
/// macOS two uninstalls in Canager are one process too. So each item waits
/// for the lock, then until its run's `Pacing.settle` has passed since the
/// move recorded here, has its turn, and records its own move before it
/// lets go (`wait_for_the_trash`, `execute_removal`): no other uninstall's
/// move lands in between, and every move made through one `LastMove`
/// begins at least its run's `settle` after the move before it returned,
/// whichever uninstall made that one. Built by `StandaloneAdapter::new`
/// (one per adapter) and `standalone::all` (one for all it builds); read
/// by `execute_removal`.
#[derive(Debug, Default)]
pub struct LastMove(tokio::sync::Mutex<Option<Instant>>);

/// A path that fails a check, and which one. The path is absolute -- a
/// listed path, or for `OverlapsKept` the kept path a listed one would
/// disturb -- and abbreviated only on its way into a sentence.
#[derive(Debug)]
struct Refusal {
    path: PathBuf,
    reason: UninstallUnsafeReason,
}

impl Refusal {
    fn new(path: &Path, reason: UninstallUnsafeReason) -> Refusal {
        Refusal {
            path: path.to_path_buf(),
            reason,
        }
    }

    /// The preview's refusal (`UninstallUnsafe`), the path abbreviated.
    fn into_error(self, home: &Path) -> AdapterError {
        AdapterError::UninstallUnsafe {
            path: shown(home, &self.path),
            reason: self.reason,
        }
    }
}

/// `path` with the home folder abbreviated to `~`, for a sentence: F's one
/// rule (`scan::display_path`).
fn shown(home: &Path, path: &Path) -> String {
    crate::scan::display_path(path, home).display().to_string()
}

/// A run that stopped at `path` because it is not what was confirmed.
fn changed(home: &Path, path: &Path) -> Outcome {
    Outcome::CanagerFailed(Fault::PathChanged {
        path: shown(home, path),
    })
}

/// A recipe path as the recipe spells it under the home folder, `~/`
/// dropped: `.claude/downloads`.
fn spelled(recipe_path: &'static str) -> &'static Path {
    Path::new(recipe_path.strip_prefix("~/").unwrap_or(recipe_path))
}

/// What `lstat` said about a path: `(st_dev, st_ino)` and the kind -- a
/// link's own, never its target's.
fn identity_of(meta: &std::fs::Metadata) -> ItemIdentity {
    let file_type = meta.file_type();
    let kind = if file_type.is_symlink() {
        ItemKind::Symlink
    } else if file_type.is_dir() {
        ItemKind::Dir
    } else if file_type.is_file() {
        ItemKind::File
    } else {
        ItemKind::Other
    };
    ItemIdentity {
        dev: meta.dev(),
        ino: meta.ino(),
        kind,
    }
}

/// One thing the list may move, as the checks see it: a listed
/// `RemoveSpec`, or a backup file one of the recipe's `backup_globs`
/// matched (check 5). `rel` is how the recipe spells it under the home
/// folder -- for a match, the pattern's folder joined with the file's name
/// (`.local/bin/agy.1727000000.old`) -- which the ancestry rule compares
/// against; `path` where it is. Built by `listed_items`; read by
/// `plan_removal` and `take_turn`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Item {
    rel: PathBuf,
    path: PathBuf,
    expect: Expect,
    what: RemovedWhat,
    optional: bool,
}

/// The list as it stands on the disk now, in execution order: every listed
/// path, with the backup files the recipe's patterns match (`Job.globs`,
/// spec §6.3 check 5) placed before the last listed path -- the launcher
/// -- so it still goes last (spec §6.2). A
/// match is a regular file (never a link) directly in the pattern's folder
/// whose name is prefix + something + suffix (`Glob::matches_name`), in
/// name order so the preview is stable; a folder that cannot be read
/// matches nothing (the launcher's own check speaks for that folder).
/// Every match is optional: it may be gone by its turn, and one Canager
/// cannot confirm is the tool's is kept and said, like an optional listed
/// path.
fn listed_items(job: &Job) -> Vec<Item> {
    let home = job.detected.home.as_path();
    let listed = |spec: &RemoveSpec| Item {
        rel: spelled(spec.path).to_path_buf(),
        path: route::expand(home, spec.path),
        expect: spec.expect,
        what: spec.what,
        optional: spec.optional,
    };
    let Some((last, before)) = job.remove.split_last() else {
        return Vec::new();
    };
    let mut items: Vec<Item> = before.iter().map(listed).collect();
    for glob in job.globs {
        let dir = glob.dir_under(home);
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut names: Vec<String> = read
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| glob.matches_name(name))
            .filter(|name| {
                std::fs::symlink_metadata(dir.join(name))
                    .is_ok_and(|meta| meta.file_type().is_file())
            })
            .collect();
        names.sort();
        items.extend(names.into_iter().map(|name| Item {
            rel: spelled(glob.dir).join(&name),
            path: dir.join(&name),
            expect: Expect::File,
            what: glob.what,
            optional: true,
        }));
    }
    items.push(listed(last));
    items
}

/// Which of a check's refusals mean, for an *optional* path, "not this
/// install's, leave it" rather than "stop": the wrong shape, a link
/// elsewhere or a folder on the way that is a link
/// (`NotWhatInstructionsExpect`), and a folder that leads out of the home
/// folder or into a shared one (`OutsideHome`, `SharedFolder`) -- all
/// places Canager will not move from, none of them a reason to leave the
/// tool uninstallable (spec §十三 #27). `NotOwnedByYou` and `OverlapsKept`
/// still stop the whole list: they are about what the move would do, not
/// about whose the path is. Read by `plan_removal`.
fn keeps_instead(reason: UninstallUnsafeReason) -> bool {
    matches!(
        reason,
        UninstallUnsafeReason::NotWhatInstructionsExpect
            | UninstallUnsafeReason::OutsideHome
            | UninstallUnsafeReason::SharedFolder
    )
}

/// The kept paths outside the home folder that are this tool's links --
/// grok's installer may put `/usr/local/bin/grok` and `/usr/local/bin/agent`
/// there when `~/.grok/bin` is not on PATH (grok.md §2) -- each as a
/// `WillKeep` sentence: Canager never moves anything outside the home
/// folder (spec §6.3), so after the uninstall they are dead links the user
/// can delete. Only a link *into the root* (`points_into`) gets the
/// sentence: the same path may be Homebrew's live link into its Caskroom
/// (an Intel Mac's `/usr/local`), another CLI's `agent`, or a file, and
/// "a dead link you can delete" would then be false. Spelled absolute in
/// the recipe (`recipes::tests` hold them to `/`, never `~/`), so nothing
/// to expand; never protected (`kept_places` skips them: a link into the
/// program folder would otherwise refuse the uninstall). Read by
/// `plan_removal`.
fn outside_home_keeps(look: &Look<'_>) -> Vec<Warning> {
    look.job
        .keep
        .iter()
        .filter(|spec| spec.what == KeptWhat::OutsideHome)
        .filter(|spec| points_into(Path::new(spec.path), &look.root))
        .map(|spec| Warning::WillKeep {
            path: spec.path.to_string(),
            what: KeptWhat::OutsideHome,
        })
        .collect()
}

/// Whether `link` is a symbolic link whose target lies under `root` (the
/// recipe's root as expanded for this home): where it resolves when it
/// resolves, or -- dangling, as on the second run of a stopped uninstall
/// whose `downloads/` is already in the Trash -- where its own text points,
/// folded from the link's folder without touching the disk
/// (`route::lexical_join`), compared against the root as spelled and as
/// canonical. Anything that is not a symbolic link, or is not there, is
/// not the installer's fallback link: `false`. Read by `outside_home_keeps`.
fn points_into(link: &Path, root: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(link) else {
        return false;
    };
    if !meta.file_type().is_symlink() {
        return false;
    }
    let canonical_root = std::fs::canonicalize(root).ok();
    match std::fs::canonicalize(link) {
        Ok(real) => canonical_root.is_some_and(|root| real.starts_with(root)),
        Err(_) => {
            let Ok(text) = std::fs::read_link(link) else {
                return false;
            };
            let folder = link.parent().unwrap_or(Path::new("/"));
            let named = route::lexical_join(folder, &text);
            named.starts_with(root) || canonical_root.is_some_and(|root| named.starts_with(root))
        }
    }
}

/// One look at the disk: the home folder resolved, and the route's two
/// paths as the recipe expands them. Taken afresh by `plan_removal` and by
/// every item's turn (`take_turn`), never kept across a pause.
struct Look<'j> {
    job: &'j Job,
    canonical_home: PathBuf,
    launcher: PathBuf,
    root: PathBuf,
}

impl<'j> Look<'j> {
    fn new(job: &'j Job) -> std::io::Result<Look<'j>> {
        let home = job.detected.home.as_path();
        Ok(Look {
            job,
            canonical_home: std::fs::canonicalize(home)?,
            launcher: route::expand(home, job.recipe.route.launcher),
            root: route::expand(home, job.recipe.route.root),
        })
    }
}

/// A kept path that is there, and where it is: `entry` is the path itself
/// with its folder resolved (a link is not followed), `target` where it
/// leads with every link followed (`None` for a link to nothing), and
/// `on_the_way` every folder, link and file the system looks up to get
/// there (`the_way_to`) -- the links between the two ends among them.
struct Kept {
    spec: &'static KeepSpec,
    path: PathBuf,
    entry: PathBuf,
    target: Option<PathBuf>,
    on_the_way: Vec<PathBuf>,
}

/// The most links macOS follows in one lookup (`MAXSYMLINKS`): at one
/// more, the system gives up as it does on a loop (`ELOOP`), and so does
/// `the_way_to`.
const MOST_LINKS: usize = 32;

/// One step `the_way_to` has still to take: back to `/`, up one folder,
/// or into a name.
enum Step {
    Root,
    Up,
    Name(std::ffi::OsString),
}

/// Puts `path`'s steps on `left`, its first step on top.
fn push_steps(left: &mut Vec<Step>, path: &Path) {
    use std::path::Component;
    for component in path.components().rev() {
        left.push(match component {
            Component::Prefix(_) | Component::RootDir => Step::Root,
            Component::CurDir => continue,
            Component::ParentDir => Step::Up,
            Component::Normal(name) => Step::Name(name.to_os_string()),
        });
    }
}

/// Every entry the system looks up to reach what `path` leads to, in the
/// order it looks them up, each placed as the folder it is in, fully
/// resolved, joined with its own name: `path`'s own folders and name, and
/// for every link met on the way, the link and then each name its text
/// gives in turn, from the link's folder or from `/` -- a `..` climbs
/// from where the lookup has got to, as the system's does. So the links
/// between a kept path and what it leads to are on it (`~/.claude.json ->
/// ~/.local/share/claude/settings-link -> ~/settings/claude.json` needs
/// its middle link as much as its two ends), and so is a linked folder on
/// the way (`~/.local/share/claude/config -> ~/settings`). A relative
/// `path` is read from the current folder, as the system reads one. A
/// name that is not there ends the way where it is: a link to nothing
/// leads that far. Anything else that stops the lookup -- a name on the
/// way that is not a folder (a `..` after a file included, which macOS's
/// `realpath`, and so `canonicalize`, climbs past without looking), more
/// links than `MOST_LINKS`, a folder Canager may not look into -- is an
/// error, as the system's own lookup (`stat`) gives one. Read by
/// `kept_places`.
fn the_way_to(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut left = Vec::new();
    push_steps(&mut left, &std::path::absolute(path)?);
    // Where the lookup has got to: a real folder, never a link.
    let mut folder = PathBuf::from("/");
    let mut way = Vec::new();
    let mut links = 0;
    while let Some(step) = left.pop() {
        let name = match step {
            Step::Root => {
                folder = PathBuf::from("/");
                continue;
            }
            Step::Up => {
                folder.pop();
                continue;
            }
            Step::Name(name) => name,
        };
        let entry = folder.join(&name);
        let meta = match std::fs::symlink_metadata(&entry) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error),
        };
        way.push(std::fs::canonicalize(&folder)?.join(&name));
        if meta.file_type().is_symlink() {
            links += 1;
            if links > MOST_LINKS {
                return Err(std::io::Error::from_raw_os_error(libc::ELOOP));
            }
            push_steps(&mut left, &std::fs::read_link(&entry)?);
        } else if meta.is_dir() {
            folder = entry;
        } else if !left.is_empty() {
            return Err(std::io::Error::from_raw_os_error(libc::ENOTDIR));
        }
    }
    Ok(way)
}

/// The kept paths that exist -- a missing one is neither listed nor
/// protected (ruling 6) -- each placed, with the way to what it leads to
/// (`the_way_to`). One that is there but cannot be placed -- its folder,
/// what it leads to or the way there cannot be looked up -- refuses the
/// whole list: Canager could not confirm the moves leave it alone
/// (`OverlapsKept`).
fn kept_places(look: &Look<'_>) -> Result<Vec<Kept>, Refusal> {
    let home = look.job.detected.home.as_path();
    let mut kept = Vec::new();
    for spec in look.job.keep {
        // Reported, not protected (`outside_home_keeps`): absolute, outside
        // the home folder, and possibly a link *into* the program folder,
        // which `disturbed` would otherwise call `OverlapsKept`.
        if spec.what == KeptWhat::OutsideHome {
            continue;
        }
        let path = route::expand(home, spec.path);
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(Refusal::new(&path, UninstallUnsafeReason::OverlapsKept)),
        }
        let entry = match (path.parent().map(std::fs::canonicalize), path.file_name()) {
            (Some(Ok(folder)), Some(name)) => folder.join(name),
            _ => return Err(Refusal::new(&path, UninstallUnsafeReason::OverlapsKept)),
        };
        let target = match std::fs::canonicalize(&path) {
            Ok(target) => Some(target),
            // A link to nothing: nothing at its end to keep -- the link
            // itself, and the way as far as it goes (`on_the_way`).
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(Refusal::new(&path, UninstallUnsafeReason::OverlapsKept)),
        };
        let Ok(on_the_way) = the_way_to(&path) else {
            return Err(Refusal::new(&path, UninstallUnsafeReason::OverlapsKept));
        };
        kept.push(Kept {
            spec,
            path,
            entry,
            target,
            on_the_way,
        });
    }
    Ok(kept)
}

/// The kept path that moving the item at `location` would disturb, if
/// any (ruling 25). With every link resolved, the item may lie inside a
/// kept path only where the recipe lists it there -- `~/.claude/downloads`
/// inside `~/.claude`, a real folder -- and may never be a kept path, hold
/// one, hold what one leads to, or be or hold anything on the way from
/// one to what it leads to (`Kept.on_the_way`: moving
/// `~/.local/share/claude` would take the middle link of `~/.claude.json
/// -> ~/.local/share/claude/settings-link -> ~/settings/claude.json` along
/// and leave `~/.claude.json` dangling): the preview says those stay.
/// `location` is where the item itself is (its folders are real folders by
/// then, the ancestry rule), `rel` how the recipe spells it.
fn disturbed<'k>(kept: &'k [Kept], rel: &Path, location: &Path) -> Option<&'k Kept> {
    kept.iter().find(|kept| {
        let kept_rel = spelled(kept.spec.path);
        let target = kept.target.as_deref();
        let takes_it = kept.entry.starts_with(location)
            || target.is_some_and(|t| t.starts_with(location))
            || kept
                .on_the_way
                .iter()
                .any(|step| step.starts_with(location));
        let listed_inside =
            rel.starts_with(kept_rel) && rel != kept_rel && target == Some(kept.entry.as_path());
        let inside_it = target.is_some_and(|t| location.starts_with(t)) && !listed_inside;
        takes_it || inside_it
    })
}

/// Whether `folder` (fully resolved) is the home folder or one of
/// `SHARED_FOLDERS` -- each compared both as the resolved home spells it
/// and where it resolves, so a shared folder kept elsewhere in the home
/// folder through a link (dotfiles) still counts. Check 1's never-list.
fn is_shared_folder(folder: &Path, canonical_home: &Path) -> bool {
    folder == canonical_home
        || SHARED_FOLDERS.iter().any(|name| {
            let shared = canonical_home.join(name);
            folder == shared || std::fs::canonicalize(&shared).is_ok_and(|real| folder == real)
        })
}

/// Every check on one listed path that is there (spec §6.3 checks 1, 3
/// and 4, and rulings 24 and 25), returning what it is. `rel` is the
/// recipe's spelling of the path under the home folder (a glob match's is
/// its pattern's folder plus its name), `expect` what must be there. Its
/// last step is the `lstat` whose answer it returns, so a caller that
/// moves the item next has nothing on the disk between the check and the
/// move (`take_turn`). In order:
///
/// - check 1: the folder the path is in, fully resolved, is inside the
///   home folder (`OutsideHome`) and is neither the home folder itself nor
///   a folder many tools share (`SharedFolder`, ruling 5);
/// - the ancestry rule (ruling 24): every folder between the home folder
///   and the path is a real folder, not a link -- the resolved folder is
///   exactly the resolved home joined with the recipe's own spelling.
///   `~/.claude -> ~/Documents` would otherwise make `~/Documents/downloads`
///   Claude Code's cache, and an ancestor renamed away and replaced by a
///   link would carry the move outside the home folder while the item kept
///   its identity (`NotWhatInstructionsExpect`);
/// - kept paths stay kept (ruling 25, `disturbed`; `OverlapsKept`, naming
///   the kept path);
/// - check 4 for a launcher (`SymlinkIntoRoot`): the one-hop link into the
///   root, or its dangling launcher-only state (`route::probe`); for
///   another link to the program (`SymlinkToProgram`), that it leads to
///   the program or to the file the launcher runs, not merely into the
///   root, which may be the very folder the uninstall keeps
///   (`route::leads_to_program`; `NotWhatInstructionsExpect` either way);
/// - last, the item's own `lstat`: there (`Missing`), the user's own
///   (check 3, `NotOwnedByYou`), and the kind the list describes --
///   a real directory for `Dir`, a link for either link kind, a regular
///   file for `File`, so the item is a link only where the recipe says so
///   (check 4, `NotWhatInstructionsExpect`).
fn check_item(
    look: &Look<'_>,
    kept: &[Kept],
    rel: &Path,
    expect: Expect,
    path: &Path,
) -> Result<ItemIdentity, Refusal> {
    use UninstallUnsafeReason::{
        Missing, NotOwnedByYou, NotWhatInstructionsExpect, OutsideHome, OverlapsKept, SharedFolder,
    };
    let refuse = |reason| Refusal::new(path, reason);
    let (Some(folder), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(refuse(NotWhatInstructionsExpect));
    };
    let Ok(real_folder) = std::fs::canonicalize(folder) else {
        return Err(refuse(NotWhatInstructionsExpect));
    };
    if !real_folder.starts_with(&look.canonical_home) {
        return Err(refuse(OutsideHome));
    }
    if is_shared_folder(&real_folder, &look.canonical_home) {
        return Err(refuse(SharedFolder));
    }
    if real_folder
        != look
            .canonical_home
            .join(rel.parent().unwrap_or(Path::new("")))
    {
        return Err(refuse(NotWhatInstructionsExpect));
    }
    if let Some(kept) = disturbed(kept, rel, &real_folder.join(name)) {
        return Err(Refusal::new(&kept.path, OverlapsKept));
    }
    let kind = look.job.recipe.route.kind;
    let confirmed = match expect {
        Expect::SymlinkIntoRoot => matches!(
            route::probe(kind, path, &look.root),
            Probe::Present { .. } | Probe::LauncherOnly
        ),
        Expect::SymlinkToProgram { program, via } => {
            let home = look.job.detected.home.as_path();
            let runs = match route::probe(kind, &look.launcher, &look.root) {
                Probe::Present { real } => Some(real),
                Probe::Absent | Probe::LauncherOnly => None,
            };
            let via: Vec<PathBuf> = via.iter().map(|link| route::expand(home, link)).collect();
            route::leads_to_program(path, &route::expand(home, program), &via, runs.as_deref())
        }
        Expect::Dir | Expect::File => true,
    };
    if !confirmed {
        return Err(refuse(NotWhatInstructionsExpect));
    }
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Err(refuse(Missing)),
        Err(_) => return Err(refuse(NotWhatInstructionsExpect)),
    };
    if meta.uid() != look.job.detected.euid {
        return Err(refuse(NotOwnedByYou));
    }
    let identity = identity_of(&meta);
    let expected = match expect {
        Expect::Dir => ItemKind::Dir,
        Expect::SymlinkIntoRoot | Expect::SymlinkToProgram { .. } => ItemKind::Symlink,
        Expect::File => ItemKind::File,
    };
    if identity.kind != expected {
        return Err(refuse(NotWhatInstructionsExpect));
    }
    Ok(identity)
}

/// Spec §6.3 on every item of the list -- the recipe's paths and, before
/// the last of them, the backup files its patterns match (check 5,
/// `listed_items`) -- then the kept paths. Check 2 (is it there?) goes
/// first, because the others need something to look at: a missing optional
/// item is skipped, a missing program directory of a launcher-only install
/// is `AlreadyGone` (re-probed from the disk now, ruling 4 of the step C
/// plan), anything else missing refuses. Then `check_item`; an optional
/// item it cannot confirm is the tool's is kept and said (`WillKeep {
/// NotOurs }`, after the moves; `keeps_instead`). Any other failure refuses
/// the whole list with nothing moved (`AdapterError::UninstallUnsafe`, one
/// of six reasons); an empty list is a plain `Refused` (unreachable while
/// the launcher is listed and the row exists), and so is a home folder
/// that cannot be resolved. The kept paths come last: the recipe's under
/// the home folder, then the ones outside it (`outside_home_keeps`).
pub fn plan_removal(job: &Job) -> Result<Removal, AdapterError> {
    let home = job.detected.home.as_path();
    let look = Look::new(job).map_err(|e| {
        AdapterError::Refused(format!(
            "cannot resolve the home folder {}: {e}",
            home.display()
        ))
    })?;
    let kept = kept_places(&look).map_err(|refusal| refusal.into_error(home))?;
    let launcher_only =
        route::probe(job.recipe.route.kind, &look.launcher, &look.root) == Probe::LauncherOnly;

    let mut paths = Vec::new();
    let mut identities = Vec::new();
    let mut warnings = Vec::new();
    let mut not_ours = Vec::new();
    for item in listed_items(job) {
        // Check 2: is it there? `lstat`, so a dangling launcher counts.
        match std::fs::symlink_metadata(&item.path) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if item.optional {
                    continue;
                }
                if launcher_only && item.path != look.launcher {
                    warnings.push(Warning::AlreadyGone {
                        path: shown(home, &item.path),
                    });
                    continue;
                }
                return Err(
                    Refusal::new(&item.path, UninstallUnsafeReason::Missing).into_error(home)
                );
            }
            // Unreadable (a permission error, a loop): not confirmed to be
            // what the list describes, and not something to move blind -- an
            // optional one is left where it is and said.
            Err(_) if item.optional => {
                not_ours.push(Warning::WillKeep {
                    path: shown(home, &item.path),
                    what: KeptWhat::NotOurs,
                });
                continue;
            }
            Err(_) => {
                return Err(Refusal::new(
                    &item.path,
                    UninstallUnsafeReason::NotWhatInstructionsExpect,
                )
                .into_error(home))
            }
        }
        match check_item(&look, &kept, &item.rel, item.expect, &item.path) {
            Ok(identity) => {
                warnings.push(Warning::WillTrash {
                    path: shown(home, &item.path),
                    what: item.what,
                });
                identities.push(identity);
                paths.push(item.path);
            }
            // An optional path that is there but not, as far as Canager can
            // tell, this install's (spec §6.3 check 4, §十三 #27): kept, and
            // said after the moves. Never the launcher, which is never
            // optional.
            Err(refusal) if item.optional && keeps_instead(refusal.reason) => {
                not_ours.push(Warning::WillKeep {
                    path: shown(home, &item.path),
                    what: KeptWhat::NotOurs,
                });
            }
            Err(refusal) => return Err(refusal.into_error(home)),
        }
    }
    if paths.is_empty() {
        return Err(AdapterError::Refused(format!(
            "{}: nothing on the uninstall list is there to move",
            job.recipe.id
        )));
    }
    warnings.extend(not_ours);
    warnings.extend(kept.iter().map(|kept| Warning::WillKeep {
        path: shown(home, &kept.path),
        what: kept.spec.what,
    }));
    warnings.extend(outside_home_keeps(&look));
    Ok(Removal {
        paths,
        identities,
        warnings,
    })
}

/// The first path the two lists disagree on: one that appeared since the
/// preview, else one that disappeared, else the first out of order.
fn first_difference<'a>(planned: &'a [PathBuf], fresh: &'a [PathBuf]) -> Option<&'a PathBuf> {
    fresh
        .iter()
        .find(|path| !planned.contains(path))
        .or_else(|| planned.iter().find(|path| !fresh.contains(path)))
        .or_else(|| {
            planned
                .iter()
                .zip(fresh)
                .find(|(a, b)| a != b)
                .map(|(a, _)| a)
        })
}

/// Waits `gap` -- what is left of Finder's time to write the Put Back
/// record of the last item moved (`PUT_BACK_SETTLE`), already cut to what
/// is left of the budget -- or until `cancel` fires, whichever comes
/// first; `true` when it was the cancel. No wait at all for a zero gap
/// (tests, `StandaloneAdapter::with_trash_gap`, a gap already over, a
/// spent budget).
async fn pause(gap: Duration, cancel: &CancellationToken) -> bool {
    if gap.is_zero() {
        return false;
    }
    tokio::select! {
        biased;
        _ = cancel.cancelled() => true,
        _ = tokio::time::sleep(gap) => false,
    }
}

/// How the wait before an item ended (`wait_for_the_trash`).
enum Wait<'p> {
    /// The turn at the Trash is this run's until the guard drops; the
    /// item's move is recorded in it first.
    Ready(tokio::sync::MutexGuard<'p, Option<Instant>>),
    /// Cancel ended a wait.
    Cancelled,
    /// The budget was spent first.
    OutOfTime,
}

/// The wait before each item: for the turn at the Trash -- no other
/// path-list uninstall's item between its own wait and its move
/// (`LastMove`'s lock) -- and then until `pacing.settle` has passed since
/// the last move recorded in `pacing.last_move`, this run's previous
/// item's or another run's (Finder's time to write that item's Put Back
/// record, `PUT_BACK_SETTLE`). The turn is held through the second wait,
/// so no other uninstall's move can land between it and this item's.
/// Cancel ends either wait and is answered before a spent budget, as a run
/// the user stopped needs no line of Canager's; neither wait outlasts the
/// budget (`left`). Read by `execute_removal`.
async fn wait_for_the_trash<'p>(
    pacing: &'p Pacing,
    left: impl Fn() -> Duration,
    cancel: &CancellationToken,
) -> Wait<'p> {
    let turn = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Wait::Cancelled,
        turn = pacing.last_move.0.lock() => turn,
        _ = tokio::time::sleep(left()) => return Wait::OutOfTime,
    };
    let since = (*turn).map_or(Duration::MAX, |at| at.elapsed());
    if pause(pacing.settle.saturating_sub(since).min(left()), cancel).await || cancel.is_cancelled()
    {
        return Wait::Cancelled;
    }
    if left().is_zero() {
        return Wait::OutOfTime;
    }
    Wait::Ready(turn)
}

/// How one item's turn ended (`take_turn`).
enum Turn {
    /// Moved; where the system put it.
    Moved(PathBuf),
    /// Not what the preview saw: a check failed at `.0` (the item, or the
    /// kept path it would disturb), the item's identity differs, or -- at
    /// the launcher's turn -- another listed path, `.0`, is there again
    /// (`listed_path_back`). Not moved.
    Changed(PathBuf),
    /// The system refused; its own words. Not moved.
    Refused(String),
    /// Canager could not ask the system at all (`TrashError::Unsupported`).
    CannotAsk,
}

/// The listed path, other than the launcher, that is there when the
/// launcher's turn comes, if any. The launcher is last (`recipes::tests`)
/// because it is what keeps the row, and the row is the one place that
/// shows what else the list left behind: a program folder that came back
/// during a pause -- a Claude Code still running installs an update into
/// it -- would be invisible once the launcher is gone, and `route::probe`
/// alone cannot tell (`Present` if the launcher's target is among the new
/// files, `LauncherOnly` if not; the folder is there either way). So
/// before the launcher moves, every other path the recipe lists must be
/// gone from the disk: each was moved earlier in this run (`moved`), was
/// already gone at the preview (`Warning::AlreadyGone`), or was never
/// there (an optional path). One whose `lstat` answers anything but "no
/// such file" -- it is there again, or cannot be looked at -- is returned,
/// and the run stops before the launcher (`Turn::Changed`): the launcher
/// stays, the row with it, and a fresh preview lists what came back. The
/// one exception is an optional path this run never moved that Canager
/// cannot confirm is the tool's, by the preview's own rule
/// (`kept_as_not_ours`): not something the list moves -- a preview lists
/// it among what stays (`WillKeep { NotOurs }`) -- and stopping for it
/// would stop every run of an uninstall whose preview kept it, such as a
/// foreign `~/.local/bin/agent` beside Grok Build's (spec §十三 #27; ruling
/// 5 of the phase 4 step D plan). A path this run moved is never
/// excepted: back as anything, it is there again. Read by `take_turn`.
fn listed_path_back(look: &Look<'_>, moved: &[PathBuf]) -> Option<PathBuf> {
    let home = look.job.detected.home.as_path();
    look.job
        .remove
        .iter()
        .map(|spec| (spec, route::expand(home, spec.path)))
        .filter(|(_, path)| *path != look.launcher)
        .find(|(spec, path)| {
            let gone = matches!(
                std::fs::symlink_metadata(path),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound
            );
            let excepted =
                spec.optional && !moved.contains(path) && kept_as_not_ours(look, spec, path);
            !gone && !excepted
        })
        .map(|(_, path)| path)
}

/// Whether the preview's own rule keeps `path`, an optional listed path,
/// as one Canager cannot confirm is the tool's (`WillKeep { NotOurs }`) --
/// `plan_removal`'s two branches that say so, asked again from this look:
/// its `lstat` fails with anything but "no such file" (it cannot be looked
/// at), or `check_item` refuses it for a reason `keeps_instead` names.
/// Gone, it is not kept at all, and when a kept path cannot be placed
/// (`kept_places`) nothing is confirmed: `false` for both. Read by
/// `listed_path_back`.
fn kept_as_not_ours(look: &Look<'_>, spec: &'static RemoveSpec, path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(_) => kept_places(look).is_ok_and(|kept| {
            matches!(
                check_item(look, &kept, spelled(spec.path), spec.expect, path),
                Err(refusal) if keeps_instead(refusal.reason)
            )
        }),
        Err(error) => error.kind() != std::io::ErrorKind::NotFound,
    }
}

/// One item's turn, on tokio's blocking pool: every check again, from a
/// fresh look (the home folder, the kept paths, the item's folders, the
/// item itself), its identity compared with what the preview saw, and then
/// -- with nothing in between -- the move. At the launcher's turn, before
/// its own checks, every other listed path must be gone, save an optional
/// one this run never moved (`moved`: the paths before this one) that
/// Canager cannot confirm is the tool's (`listed_path_back`).
/// `check_item`'s last step is the `lstat` that
/// produced `seen`, and `Trasher::trash` is handed that answer's kind
/// rather than looking again: Canager checks each item immediately before
/// moving it; a program running as you that swaps the item in that instant
/// could still race it (docs/what-we-run.md, "Moving files to the Trash").
/// That is the documented edge of the design: the system's call takes a
/// path, and what it finds there is what it moves.
fn take_turn(
    job: &Job,
    path: &Path,
    previewed: ItemIdentity,
    moved: &[PathBuf],
    trasher: &dyn Trasher,
) -> Turn {
    // The item as the list stands now (`listed_items`: a listed path, or a
    // backup a pattern matches); one that is no longer listed -- a backup
    // that vanished by its turn -- is not what the preview saw.
    let Some(item) = listed_items(job).into_iter().find(|item| item.path == path) else {
        return Turn::Changed(path.to_path_buf());
    };
    let Ok(look) = Look::new(job) else {
        return Turn::Changed(path.to_path_buf());
    };
    if path == look.launcher {
        if let Some(back) = listed_path_back(&look, moved) {
            return Turn::Changed(back);
        }
    }
    let seen = match kept_places(&look)
        .and_then(|kept| check_item(&look, &kept, &item.rel, item.expect, path))
    {
        Ok(seen) => seen,
        Err(refusal) => return Turn::Changed(refusal.path),
    };
    if seen != previewed {
        return Turn::Changed(path.to_path_buf());
    }
    match trasher.trash(path, seen.kind) {
        Ok(trashed_to) => Turn::Moved(trashed_to),
        Err(TrashError::Refused { detail }) => Turn::Refused(detail),
        Err(TrashError::Unsupported) => Turn::CannotAsk,
    }
}

/// Spec §6.2's execution. First the fresh look at the confirmation: every
/// check again on the list rebuilt from the disk (`plan_removal`), the
/// list compared with the confirmed one, and every item's identity
/// compared with the one the preview recorded (spec §6.3) -- any
/// difference is `Fault::PathChanged` before anything moves, so a Claude
/// Code that updated itself between the preview and the click (its updater
/// re-points the launcher) sends the user back to a fresh preview. Then
/// each path in order: the wait for the turn at the Trash and for
/// `settle` since the last move made through `pacing.last_move`, this
/// run's or another path-list uninstall's running at the same time
/// (`wait_for_the_trash`: Cancel ends it, and it never outlasts the
/// budget); one turn on the blocking pool (`take_turn`: at the launcher's,
/// that every other listed path is gone or, optional and never moved by
/// this run, one Canager cannot confirm is the tool's; every check again,
/// the identity against the preview's, the move), awaited to its end even
/// if Cancel arrives meanwhile -- a move handed to the system finishes and
/// is reported -- and its move recorded before the turn is let go; one log
/// note. After the last move the same pause once more, before `Succeeded`
/// (a Cancel there only cuts it short: everything is moved).
///
/// `Succeeded` only when every path was moved; `Failed` with the system's
/// own words when it refused one (the launcher, last, is then still there,
/// and the row comes back as launcher-only); `CanagerFailed(Internal)`
/// when Canager could not ask the system at all (`TrashError::Unsupported`,
/// at the first item); `Unconfirmed` when cancelled or out of time between
/// items (the budget's stop, the one the user did not ask for, first
/// writes `LogNote::OutOfTime` naming the item it stopped before), or when
/// a turn panicked -- `run_operation` then reads the disk and reports what
/// it finds. Never an `Err` for a state of the Mac: the
/// `Err` arm is a bug's (a home folder that cannot be resolved, an empty
/// list, a plan that lost what its preview saw). Read by
/// `StandaloneAdapter::execute`.
pub async fn execute_removal(
    job: &Job,
    confirmed: Confirmed<'_>,
    trasher: &Arc<dyn Trasher>,
    pacing: Pacing,
    sink: Arc<dyn EventSink>,
    op_id: OpId,
    cancel: CancellationToken,
) -> Result<Outcome, AdapterError> {
    let home = job.detected.home.as_path();
    let started = Instant::now();
    let left = || pacing.budget.saturating_sub(started.elapsed());
    // What the preview saw travels with the plan, one identity per path;
    // a plan without it (built by hand, or read back from JSON, which
    // never carries it) has nothing to compare with, and moving anyway
    // would move what nobody looked at.
    if confirmed.previewed.len() != confirmed.paths.len() {
        return Err(AdapterError::Refused(format!(
            "{}: the plan does not carry what its preview saw",
            job.recipe.id
        )));
    }
    let fresh = match plan_removal(job) {
        Ok(fresh) => fresh,
        // A check that passed at preview time fails now: something at the
        // listed path changed. Not a refusal of a plan (that plan was
        // issued and confirmed) but a run that stopped before moving it.
        Err(AdapterError::UninstallUnsafe { path, .. }) => {
            return Ok(Outcome::CanagerFailed(Fault::PathChanged { path }));
        }
        Err(other) => return Err(other),
    };
    if let Some(path) = first_difference(confirmed.paths, &fresh.paths) {
        return Ok(changed(home, path));
    }
    if let Some((path, _)) = confirmed
        .paths
        .iter()
        .zip(confirmed.previewed.iter().zip(&fresh.identities))
        .find(|(_, (then, now))| then != now)
    {
        return Ok(changed(home, path));
    }
    for (index, (path, &previewed)) in confirmed.paths.iter().zip(confirmed.previewed).enumerate() {
        // The turn at the Trash, `settle` after the last move made through
        // `pacing.last_move` -- this run's previous item's or another
        // uninstall's (`wait_for_the_trash`). Cancel first: a run the user
        // stopped needs no line of Canager's (`run_operation` reports it as
        // `Cancelled` once it finds the launcher, last, still there). A
        // spent budget is the stop nobody asked for, so it says which item
        // it stopped before and how much time there was -- otherwise the
        // log ends at the last move and the outcome says only
        // "unconfirmed".
        let mut last_move = match wait_for_the_trash(&pacing, left, &cancel).await {
            Wait::Ready(last_move) => last_move,
            Wait::Cancelled => return Ok(Outcome::Unconfirmed),
            Wait::OutOfTime => {
                sink.emit(OperationEvent::Note {
                    op_id,
                    note: LogNote::OutOfTime {
                        path: shown(home, path),
                        seconds: pacing.budget.as_secs(),
                    },
                });
                return Ok(Outcome::Unconfirmed);
            }
        };
        let turn = {
            let (job, path, trasher) = (job.clone(), path.clone(), Arc::clone(trasher));
            // Every path before this one was moved by this run: a turn that
            // did not move its item ended the run (below).
            let moved = confirmed.paths[..index].to_vec();
            tokio::task::spawn_blocking(move || {
                take_turn(&job, &path, previewed, &moved, trasher.as_ref())
            })
            .await
        };
        // Recorded before the turn is let go, so the next move -- this
        // run's or another uninstall's -- waits `settle` from this one. A
        // turn that panicked may have moved its item: it counts as a move.
        if matches!(turn, Ok(Turn::Moved(_)) | Err(_)) {
            *last_move = Some(Instant::now());
        }
        drop(last_move);
        match turn {
            Ok(Turn::Moved(trashed_to)) => sink.emit(OperationEvent::Note {
                op_id,
                note: LogNote::MovedToTrash {
                    path: shown(home, path),
                    trashed_to: shown(home, &trashed_to),
                },
            }),
            Ok(Turn::Changed(at)) => return Ok(changed(home, &at)),
            Ok(Turn::Refused(detail)) => {
                sink.emit(OperationEvent::Note {
                    op_id,
                    note: LogNote::TrashFailed {
                        path: shown(home, path),
                        error: detail.clone(),
                    },
                });
                // macOS's own words, quoted by the front end as a tool's
                // stderr would be. Whatever was moved before is in the
                // Trash; the launcher (last) is not, and the row comes back.
                return Ok(Outcome::Failed {
                    exit_code: None,
                    summary: detail,
                });
            }
            // Canager could not ask the system at all (not macOS, or a path
            // `NSString` cannot carry): its own limitation, with no words of
            // the Mac's to quote (ruling 9). `RealTrasher` answers it for
            // every path alike, so it comes at the first item, before
            // anything moved.
            Ok(Turn::CannotAsk) => return Ok(Outcome::CanagerFailed(Fault::Internal)),
            // The turn panicked: whether this item moved is not known.
            Err(_) => return Ok(Outcome::Unconfirmed),
        }
    }
    // The same pause after the last move: Finder writes the Put Back record
    // after the call returns, so the run is not reported finished -- the cue
    // a user may quit Canager on -- before it had the time every measured
    // run gave it. Everything is in the Trash by now, so a Cancel or the
    // budget only cuts the wait short.
    pause(pacing.settle.min(left()), &cancel).await;
    Ok(Outcome::Succeeded)
}

#[cfg(test)]
mod tests {
    use super::super::recipe::{Expect, KeepSpec, RemoveSpec, Uninstall};
    use super::super::recipes::CLAUDE;
    use super::super::testing::{claude_layout, TempHome, TimedTrasher};
    use super::*;
    use crate::events::VecSink;
    use crate::model::{KeptWhat, RemovedWhat, UninstallUnsafeReason, Warning};
    use crate::scan::Glob;
    use crate::trash::{MockTrasher, TrashError};
    use std::sync::Mutex as StdMutex;

    fn claude_lists() -> (&'static [RemoveSpec], &'static [KeepSpec]) {
        match &CLAUDE.uninstall {
            Some(Uninstall::Paths { remove, keep }) => (remove, keep),
            // A `Command` uninstall (rustup's) has no list either.
            None | Some(Uninstall::Command(_)) => panic!("claude has a path list"),
        }
    }

    /// What `detect` would have written for `home`, as the user this test
    /// runs as (the files it makes are that user's), with the default
    /// Cargo and rustup homes and no `ZDOTDIR`.
    fn detected(home: &Path) -> Detected {
        Detected {
            home: home.to_path_buf(),
            euid: std::fs::metadata(home).expect("home metadata").uid(),
            cargo_home: Some(home.join(".cargo")),
            rustup_home: Some(home.join(".rustup")),
            zdotdir: None,
        }
    }

    fn claude_job(d: &Detected) -> Job {
        let (remove, keep) = claude_lists();
        Job {
            recipe: &CLAUDE,
            detected: d.clone(),
            remove,
            keep,
            globs: &[],
        }
    }

    /// A one-path list for a check's own test: `'static`, as a recipe's
    /// is (leaked; a test's lifetime is the process's).
    fn only(spec: RemoveSpec) -> &'static [RemoveSpec] {
        Box::leak(Box::new([spec]))
    }

    /// A one-pattern glob list, `'static` like a recipe's.
    fn only_glob(glob: Glob) -> &'static [Glob] {
        Box::leak(Box::new([glob]))
    }

    /// A one-path keep list, `'static` like a recipe's; `path` is leaked
    /// too, so a test can keep a path under its own temp directory.
    fn only_keep(path: String, what: KeptWhat) -> &'static [KeepSpec] {
        let path: &'static str = Box::leak(path.into_boxed_str());
        Box::leak(Box::new([KeepSpec { path, what }]))
    }

    fn trash(path: &str, what: RemovedWhat) -> Warning {
        Warning::WillTrash {
            path: path.to_string(),
            what,
        }
    }

    fn keep(path: &str, what: KeptWhat) -> Warning {
        Warning::WillKeep {
            path: path.to_string(),
            what,
        }
    }

    fn refused(result: Result<Removal, AdapterError>) -> (String, UninstallUnsafeReason) {
        match result {
            Err(AdapterError::UninstallUnsafe { path, reason }) => (path, reason),
            other => panic!("expected UninstallUnsafe, got {other:?}"),
        }
    }

    fn identity(path: &Path) -> ItemIdentity {
        identity_of(&std::fs::symlink_metadata(path).expect("lstat"))
    }

    #[test]
    fn test_plan_removal_lists_claude_codes_paths_in_execution_order_with_the_kept_ones_after() {
        // Spec §6.6's dialog, from a full native install: three moves in
        // the recipe's order (the launcher last), then the two kept paths
        // that exist. `~/.claude/downloads` lies inside the kept
        // `~/.claude` exactly as the recipe lists it, which ruling 25
        // allows. Identities are what `execute_removal` compares.
        let home = TempHome::new("removal-full");
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        home.file(".claude/projects/p/session.jsonl");
        home.file(".claude.json");
        let d = detected(home.path());

        let removal = plan_removal(&claude_job(&d)).expect("a plan");

        assert_eq!(
            removal.paths,
            vec![
                home.path().join(".local/share/claude"),
                home.path().join(".claude/downloads"),
                layout.launcher.clone(),
            ]
        );
        let launcher_meta = std::fs::symlink_metadata(&layout.launcher).unwrap();
        assert_eq!(
            removal.identities,
            vec![
                identity(&layout.root),
                identity(&home.path().join(".claude/downloads")),
                ItemIdentity {
                    dev: launcher_meta.dev(),
                    ino: launcher_meta.ino(),
                    kind: ItemKind::Symlink,
                },
            ],
            "the link's own identity, not its target's"
        );
        assert_eq!(removal.identities[0].kind, ItemKind::Dir);
        assert_eq!(
            removal.warnings,
            vec![
                trash("~/.local/share/claude", RemovedWhat::Program),
                trash("~/.claude/downloads", RemovedWhat::Cache),
                trash("~/.local/bin/claude", RemovedWhat::Launcher),
                keep("~/.claude", KeptWhat::SettingsAndHistory),
                keep("~/.claude.json", KeptWhat::Settings),
            ]
        );
    }

    #[test]
    fn test_plan_removal_skips_a_missing_optional_path_and_a_missing_kept_path_silently() {
        // A Mac without `~/.claude/downloads` or `~/.claude.json`: neither
        // is a sentence (ruling 6).
        let home = TempHome::new("removal-minimal");
        let layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());

        let removal = plan_removal(&claude_job(&d)).expect("a plan");

        assert_eq!(
            removal.paths,
            vec![
                home.path().join(".local/share/claude"),
                layout.launcher.clone()
            ]
        );
        assert_eq!(
            removal.warnings,
            vec![
                trash("~/.local/share/claude", RemovedWhat::Program),
                trash("~/.local/bin/claude", RemovedWhat::Launcher),
            ]
        );
    }

    #[test]
    fn test_plan_removal_refuses_a_required_path_that_is_missing() {
        // No launcher at all is not the launcher-only state: the row
        // should not exist, and a plan for it stops (check 2).
        let home = TempHome::new("removal-missing");
        let layout = claude_layout(&home, "2.1.281");
        std::fs::remove_file(&layout.launcher).unwrap();
        let d = detected(home.path());

        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            ("~/.local/bin/claude", UninstallUnsafeReason::Missing)
        );
    }

    #[test]
    fn test_plan_removal_lists_the_program_dir_as_already_gone_on_a_launcher_only_install() {
        // The state a stopped run leaves (spec §6.2): the program files are
        // in the Trash, the link dangles. Check 2 on the program directory
        // becomes `AlreadyGone`, read from the disk now rather than from the
        // row's note (ruling 4), and the list is the launcher.
        let home = TempHome::new("removal-launcher-only");
        let layout = claude_layout(&home, "2.1.281");
        std::fs::remove_dir_all(&layout.root).unwrap();
        let d = detected(home.path());

        let removal = plan_removal(&claude_job(&d)).expect("a plan");

        assert_eq!(removal.paths, vec![layout.launcher.clone()]);
        assert_eq!(
            removal.warnings,
            vec![
                Warning::AlreadyGone {
                    path: "~/.local/share/claude".to_string()
                },
                trash("~/.local/bin/claude", RemovedWhat::Launcher),
            ]
        );
    }

    #[test]
    fn test_plan_removal_refuses_a_parent_that_leads_outside_home() {
        // Check 1, for a directory and for a link alike (spec §十三 #26):
        // a `~/.local/share` or `~/.local/bin` that is itself a link to
        // another volume would carry the move out of the home folder.
        let outside = TempHome::new("removal-outside");

        // (a) The program directory's parent is a link out.
        let home = TempHome::new("removal-parent-dir");
        let real_share = outside.dir("share");
        let real = outside.executable("share/claude/versions/2.1.281");
        home.link(".local/share", &real_share);
        home.link(".local/bin/claude", &real);
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            ("~/.local/share/claude", UninstallUnsafeReason::OutsideHome)
        );

        // (b) The launcher's directory is a link out; the program directory
        // is fine, so it is the launcher that is refused.
        let home = TempHome::new("removal-parent-link");
        let real = home.executable(".local/share/claude/versions/2.1.281");
        let outside_bin = outside.dir("bin");
        std::os::unix::fs::symlink(real, outside_bin.join("claude")).unwrap();
        home.link(".local/bin", &outside_bin);
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            ("~/.local/bin/claude", UninstallUnsafeReason::OutsideHome)
        );
    }

    #[test]
    fn test_plan_removal_refuses_a_launcher_reached_through_a_linked_folder_inside_home() {
        // Ruling 24 of the step C plan: every folder between the home
        // folder and a listed path must be a real folder -- inside the home
        // folder or not -- so a launcher whose `~/.local/bin` is kept as a
        // link to a dotfiles folder is refused. (C's other half, an
        // optional `~/.claude/downloads` behind a linked `~/.claude`, is
        // kept and said since step D: see
        // `test_plan_removal_keeps_an_optional_path_whose_folder_leads_elsewhere`.)
        let home = TempHome::new("removal-linked-bin-inside");
        let real = home.executable(".local/share/claude/versions/2.1.281");
        let dotfiles_bin = home.dir("dotfiles/bin");
        std::os::unix::fs::symlink(real, dotfiles_bin.join("claude")).unwrap();
        home.link(".local/bin", &dotfiles_bin);
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            (
                "~/.local/bin/claude",
                UninstallUnsafeReason::NotWhatInstructionsExpect
            )
        );
    }

    #[test]
    fn test_plan_removal_refuses_when_a_kept_path_leads_into_what_it_would_move() {
        // Ruling 25, the review's first counterexample: with `~/.claude ->
        // ~/.local/share/claude`, moving the program folder would take the
        // settings and history the preview says it keeps. Refused, naming
        // the kept path -- and the same for a kept file that is a link into
        // the program folder.
        let home = TempHome::new("removal-settings-aliased");
        let layout = claude_layout(&home, "2.1.281");
        home.link(".claude", &layout.root);
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            ("~/.claude", UninstallUnsafeReason::OverlapsKept)
        );

        let home = TempHome::new("removal-settings-file-aliased");
        let layout = claude_layout(&home, "2.1.281");
        let inside = home.file(".local/share/claude/settings.json");
        home.link(".claude.json", &inside);
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            ("~/.claude.json", UninstallUnsafeReason::OverlapsKept)
        );
        assert!(layout.root.is_dir());
    }

    #[test]
    fn test_plan_removal_refuses_when_the_way_to_a_kept_path_runs_through_what_it_would_move() {
        // A kept path needs more than its two ends (the step C code review,
        // 2026-09-26): `~/.claude.json -> ~/.local/share/claude/settings-link
        // -> ~/settings/claude.json` ends outside the program folder, but
        // moving the folder would take the middle link along and leave
        // `~/.claude.json` dangling. Refused, naming the kept path -- with
        // the links' texts absolute or relative (`..` included), and for a
        // linked folder on the way (`~/.local/share/claude/config ->
        // ~/settings`) alike. The same for a kept link to nothing whose way
        // runs through the program folder: writing through it would create
        // `~/settings/claude.json` by way of the middle link.
        type Make = fn(&TempHome);
        let cases: [(&str, Make); 4] = [
            ("removal-kept-way-two-hops", |home| {
                let settings = home.file("settings/claude.json");
                let middle = home.link(".local/share/claude/settings-link", &settings);
                home.link(".claude.json", &middle);
            }),
            ("removal-kept-way-relative", |home| {
                home.file("settings/claude.json");
                home.link(
                    ".local/share/claude/settings-link",
                    Path::new("../../../settings/claude.json"),
                );
                home.link(
                    ".claude.json",
                    Path::new(".local/share/claude/settings-link"),
                );
            }),
            ("removal-kept-way-linked-folder", |home| {
                home.file("settings/claude.json");
                let settings = home.path().join("settings");
                home.link(".local/share/claude/config", &settings);
                home.link(
                    ".claude.json",
                    &home.path().join(".local/share/claude/config/claude.json"),
                );
            }),
            ("removal-kept-way-to-nothing", |home| {
                home.dir("settings");
                let middle = home.link(
                    ".local/share/claude/settings-link",
                    &home.path().join("settings/claude.json"),
                );
                home.link(".claude.json", &middle);
            }),
        ];
        for (tag, make) in cases {
            let home = TempHome::new(tag);
            let layout = claude_layout(&home, "2.1.281");
            make(&home);
            let d = detected(home.path());

            let (path, reason) = refused(plan_removal(&claude_job(&d)));

            assert_eq!(
                (path.as_str(), reason),
                ("~/.claude.json", UninstallUnsafeReason::OverlapsKept),
                "{tag}"
            );
            assert!(layout.root.is_dir(), "{tag}");
        }
    }

    #[test]
    fn test_plan_removal_goes_on_when_the_way_to_a_kept_path_runs_clear_of_what_it_moves() {
        // The other side of the same rule: a kept path whose way runs clear
        // of the listed paths -- its middle link in a dotfiles folder, or in
        // a folder beside the program folder whose name only begins the
        // same -- is no reason to refuse. The plan goes on and says the path
        // stays.
        for (tag, middle) in [
            ("removal-kept-way-dotfiles", "dotfiles/claude-link"),
            (
                "removal-kept-way-beside",
                ".local/share/claude-settings/link",
            ),
        ] {
            let home = TempHome::new(tag);
            let _layout = claude_layout(&home, "2.1.281");
            let settings = home.file("settings/claude.json");
            let middle = home.link(middle, &settings);
            home.link(".claude.json", &middle);
            let d = detected(home.path());

            let removal = plan_removal(&claude_job(&d)).expect("a plan");

            assert_eq!(
                removal.warnings,
                vec![
                    trash("~/.local/share/claude", RemovedWhat::Program),
                    trash("~/.local/bin/claude", RemovedWhat::Launcher),
                    keep("~/.claude.json", KeptWhat::Settings),
                ],
                "{tag}"
            );
        }
    }

    #[test]
    fn test_the_way_to_lists_each_entry_the_lookup_meets_and_fails_where_the_system_would() {
        // `the_way_to` alone. Each folder, link and file in the order the
        // lookup meets them, a link's text followed from `/` when absolute
        // and from the link's own folder when relative (`..` climbing from
        // there), ending where `canonicalize` ends; a name that is not
        // there ends the way with what was met; a loop, and a file used as
        // a folder, are errors, where the system's own lookup (`stat`,
        // `std::fs::metadata`) fails too -- `..` after a file included,
        // which `canonicalize` on macOS climbs past. The home's own
        // folders, met first and again after an absolute text, are left
        // out of the comparison: `TempHome` is canonical, so they are the
        // real folders above it.
        let home = TempHome::new("removal-the-way");
        let h = home.path();
        home.file("settings/claude.json");
        home.link(
            "share/claude/settings-link",
            Path::new("../../settings/claude.json"),
        );
        let kept = home.link(".claude.json", &h.join("share/claude/settings-link"));
        let below_home = |way: Vec<PathBuf>| -> Vec<PathBuf> {
            way.into_iter()
                .filter(|step| !h.starts_with(step))
                .collect()
        };

        let way = the_way_to(&kept).expect("a way");

        assert_eq!(
            below_home(way.clone()),
            vec![
                h.join(".claude.json"),
                h.join("share"),
                h.join("share/claude"),
                h.join("share/claude/settings-link"),
                h.join("settings"),
                h.join("settings/claude.json"),
            ]
        );
        assert_eq!(way.last(), Some(&std::fs::canonicalize(&kept).unwrap()));

        let dangling = home.link("gone-link", &h.join("share/claude/nothing/x"));
        assert_eq!(
            below_home(the_way_to(&dangling).expect("a way as far as it goes")),
            vec![h.join("gone-link"), h.join("share"), h.join("share/claude")]
        );

        home.link("loop-a", &h.join("loop-b"));
        let looping = home.link("loop-b", &h.join("loop-a"));
        assert!(the_way_to(&looping).is_err(), "a loop");
        assert!(std::fs::metadata(&looping).is_err(), "a loop");
        for through_a_file in [
            "settings/claude.json/x",
            "settings/claude.json/../claude.json",
        ] {
            let link = home.link("through-a-file", &h.join(through_a_file));
            assert!(the_way_to(&link).is_err(), "{through_a_file}");
            assert!(std::fs::metadata(&link).is_err(), "{through_a_file}");
            std::fs::remove_file(&link).unwrap();
        }
        // Pinned so the comments above and on `the_way_to` stay true:
        // macOS's `realpath` climbs past a `..` after a file, where the
        // system's lookup fails.
        #[cfg(target_os = "macos")]
        {
            let link = home.link(
                "up-from-a-file",
                &h.join("settings/claude.json/../claude.json"),
            );
            assert!(std::fs::canonicalize(&link).is_ok());
        }
    }

    #[test]
    fn test_plan_removal_accepts_a_home_reached_through_a_symlink() {
        // `HostEnv.home` may be a link to the real home; both sides are
        // resolved before check 1 compares them, and the plan's paths keep
        // the spelling the home was given.
        let tmp = TempHome::new("removal-linked-home");
        let real_home = tmp.dir("real-home");
        let real = tmp.executable("real-home/.local/share/claude/versions/2.1.281");
        tmp.link("real-home/.local/bin/claude", &real);
        let linked_home = tmp.link("linked-home", &real_home);
        let d = detected(&linked_home);

        let removal = plan_removal(&claude_job(&d)).expect("a plan");

        assert_eq!(
            removal.paths,
            vec![
                linked_home.join(".local/share/claude"),
                linked_home.join(".local/bin/claude"),
            ]
        );
        assert_eq!(
            removal.warnings,
            vec![
                trash("~/.local/share/claude", RemovedWhat::Program),
                trash("~/.local/bin/claude", RemovedWhat::Launcher),
            ]
        );
    }

    #[test]
    fn test_plan_removal_refuses_a_path_the_user_does_not_own() {
        // Check 3, with an injected euid: a test cannot make a file owned
        // by someone else, but the check compares two numbers.
        let home = TempHome::new("removal-owner");
        let _layout = claude_layout(&home, "2.1.281");
        let d = Detected {
            euid: detected(home.path()).euid + 1,
            ..detected(home.path())
        };

        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            (
                "~/.local/share/claude",
                UninstallUnsafeReason::NotOwnedByYou
            )
        );
    }

    #[test]
    fn test_plan_removal_refuses_what_the_instructions_do_not_describe() {
        // Check 4, each `Expect`: a launcher that links elsewhere, a
        // launcher that is a plain file, a program directory that is a link.
        let home = TempHome::new("removal-launcher-elsewhere");
        let layout = claude_layout(&home, "2.1.281");
        let elsewhere = home.executable("elsewhere/claude");
        std::fs::remove_file(&layout.launcher).unwrap();
        std::os::unix::fs::symlink(elsewhere, &layout.launcher).unwrap();
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            (
                "~/.local/bin/claude",
                UninstallUnsafeReason::NotWhatInstructionsExpect
            )
        );

        let home = TempHome::new("removal-launcher-file");
        let layout = claude_layout(&home, "2.1.281");
        std::fs::remove_file(&layout.launcher).unwrap();
        home.executable(".local/bin/claude");
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            (
                "~/.local/bin/claude",
                UninstallUnsafeReason::NotWhatInstructionsExpect
            )
        );

        let home = TempHome::new("removal-root-is-link");
        let real_root = home.dir("elsewhere/claude-root");
        home.executable("elsewhere/claude-root/versions/2.1.281");
        home.link(".local/share/claude", &real_root);
        home.link(
            ".local/bin/claude",
            &home.path().join(".local/share/claude/versions/2.1.281"),
        );
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            (
                "~/.local/share/claude",
                UninstallUnsafeReason::NotWhatInstructionsExpect
            )
        );
    }

    #[test]
    fn test_plan_removal_keeps_an_optional_path_it_cannot_confirm_is_the_tools_and_says_so() {
        // Spec §6.3 check 4 (§十三 #27): an optional path that is there but
        // not what the list describes -- `~/.claude/downloads` as a link
        // elsewhere, or as a file -- is not this install's to move. C
        // refused the whole uninstall for it; now it stays, the preview
        // says so after the moves, and the uninstall goes on. (The array
        // is typed as fn pointers: two closures never share a type, and
        // the coercion does not reach inside a tuple; the alias keeps
        // clippy's `type_complexity` quiet.)
        type Make = fn(&TempHome);
        let cases: [(&str, Make); 2] = [
            ("removal-optional-link", |home| {
                let elsewhere = home.dir("elsewhere/downloads");
                home.link(".claude/downloads", &elsewhere);
            }),
            ("removal-optional-file", |home| {
                home.file(".claude/downloads");
            }),
        ];
        for (tag, make) in cases {
            let home = TempHome::new(tag);
            let layout = claude_layout(&home, "2.1.281");
            make(&home);
            let d = detected(home.path());

            let removal = plan_removal(&claude_job(&d)).expect("a plan");

            assert_eq!(
                removal.paths,
                vec![
                    home.path().join(".local/share/claude"),
                    layout.launcher.clone()
                ],
                "{tag}"
            );
            assert_eq!(
                removal.warnings,
                vec![
                    trash("~/.local/share/claude", RemovedWhat::Program),
                    trash("~/.local/bin/claude", RemovedWhat::Launcher),
                    keep("~/.claude/downloads", KeptWhat::NotOurs),
                    keep("~/.claude", KeptWhat::SettingsAndHistory),
                ],
                "{tag}"
            );
        }
    }

    #[test]
    fn test_plan_removal_keeps_an_optional_path_whose_folder_leads_elsewhere() {
        // The other reasons the skip covers (ruling 5): an optional path
        // whose folder is a link inside the home folder (C's ruling 24 case,
        // `~/.claude -> ~/Documents`, which C refused as
        // `NotWhatInstructionsExpect` and now keeps), to another volume
        // (`OutsideHome`), or into a shared folder (`SharedFolder`). None is
        // the tool's to move; a grok whose `~/.config` is a dotfiles link
        // keeps its fish completion rather than becoming impossible to
        // uninstall.
        let home = TempHome::new("removal-optional-linked-inside");
        let _layout = claude_layout(&home, "2.1.281");
        let documents = home.dir("Documents");
        home.dir("Documents/downloads");
        home.link(".claude", &documents);
        let d = detected(home.path());
        let removal = plan_removal(&claude_job(&d)).expect("a plan, not C's refusal");
        assert_eq!(
            removal.warnings,
            vec![
                trash("~/.local/share/claude", RemovedWhat::Program),
                trash("~/.local/bin/claude", RemovedWhat::Launcher),
                keep("~/.claude/downloads", KeptWhat::NotOurs),
                keep("~/.claude", KeptWhat::SettingsAndHistory),
            ]
        );
        assert!(home.path().join("Documents/downloads").is_dir());

        let outside = TempHome::new("removal-optional-outside");
        let home = TempHome::new("removal-optional-folder-away");
        let _layout = claude_layout(&home, "2.1.281");
        let away = outside.dir("claude-state");
        outside.dir("claude-state/downloads");
        home.link(".claude", &away);
        let d = detected(home.path());
        let removal = plan_removal(&claude_job(&d)).expect("a plan");
        assert!(removal
            .warnings
            .contains(&keep("~/.claude/downloads", KeptWhat::NotOurs)));
        assert_eq!(removal.paths.len(), 2);

        let home = TempHome::new("removal-optional-shared");
        let _layout = claude_layout(&home, "2.1.281");
        home.dir(".cache/thing");
        let d = detected(home.path());
        let job = Job {
            recipe: &CLAUDE,
            detected: d.clone(),
            remove: Box::leak(Box::new([
                RemoveSpec {
                    path: "~/.cache/thing",
                    expect: Expect::Dir,
                    what: RemovedWhat::Cache,
                    optional: true,
                },
                RemoveSpec {
                    path: "~/.local/bin/claude",
                    expect: Expect::SymlinkIntoRoot,
                    what: RemovedWhat::Launcher,
                    optional: false,
                },
            ])),
            keep: &[],
            globs: &[],
        };
        let removal = plan_removal(&job).expect("a plan");
        assert_eq!(
            removal.warnings,
            vec![
                trash("~/.local/bin/claude", RemovedWhat::Launcher),
                keep("~/.cache/thing", KeptWhat::NotOurs),
            ]
        );
    }

    #[test]
    fn test_plan_removal_still_refuses_an_optional_path_that_is_not_yours_or_overlaps_a_kept_one() {
        // The skip is for "not ours", never for "not yours" or "would take
        // what stays": those two refuse for an optional path exactly as for
        // a required one.
        let home = TempHome::new("removal-optional-owner");
        let _layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        let d = Detected {
            euid: detected(home.path()).euid + 1,
            ..detected(home.path())
        };
        let job = Job {
            recipe: &CLAUDE,
            detected: d,
            remove: only(RemoveSpec {
                path: "~/.claude/downloads",
                expect: Expect::Dir,
                what: RemovedWhat::Cache,
                optional: true,
            }),
            keep: &[],
            globs: &[],
        };
        let (path, reason) = refused(plan_removal(&job));
        assert_eq!(
            (path.as_str(), reason),
            ("~/.claude/downloads", UninstallUnsafeReason::NotOwnedByYou)
        );

        let home = TempHome::new("removal-optional-overlap");
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        // The kept settings file is a link into the optional cache folder.
        home.link(".claude.json", &home.path().join(".claude/downloads"));
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            ("~/.claude.json", UninstallUnsafeReason::OverlapsKept)
        );
        assert!(layout.root.is_dir());
    }

    #[test]
    fn test_plan_removal_moves_backup_files_a_pattern_names_before_the_launcher() {
        // Check 5 (spec §6.3): a regular file in the pattern's folder named
        // prefix+something+suffix is moved, listed as a backup, before the
        // last listed path; a link of that name and a name without the
        // middle are not matches. Name order, so the preview is stable.
        let home = TempHome::new("removal-globs");
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        home.file(".local/bin/claude.1727000000.old");
        home.file(".local/bin/claude.1726000000.old");
        home.file(".local/bin/claude.old");
        home.link(".local/bin/claude.9.old", &layout.real);
        let d = detected(home.path());
        let (remove, keep) = claude_lists();
        let job = Job {
            recipe: &CLAUDE,
            detected: d,
            remove,
            keep,
            globs: only_glob(Glob {
                dir: "~/.local/bin",
                prefix: "claude.",
                suffix: ".old",
                what: RemovedWhat::Backups,
            }),
        };

        let removal = plan_removal(&job).expect("a plan");

        assert_eq!(
            removal.paths,
            vec![
                home.path().join(".local/share/claude"),
                home.path().join(".claude/downloads"),
                home.path().join(".local/bin/claude.1726000000.old"),
                home.path().join(".local/bin/claude.1727000000.old"),
                layout.launcher.clone(),
            ]
        );
        assert_eq!(removal.identities[2].kind, ItemKind::File);
        assert_eq!(
            removal.warnings[2],
            trash("~/.local/bin/claude.1726000000.old", RemovedWhat::Backups)
        );
        assert_eq!(
            removal.warnings[4],
            trash("~/.local/bin/claude", RemovedWhat::Launcher)
        );
        // With the launcher as the only listed path, the backups still come
        // before it.
        let job = Job {
            remove: only(RemoveSpec {
                path: "~/.local/bin/claude",
                expect: Expect::SymlinkIntoRoot,
                what: RemovedWhat::Launcher,
                optional: false,
            }),
            ..job
        };
        let removal = plan_removal(&job).expect("a plan");
        assert_eq!(removal.paths.last(), Some(&layout.launcher));
        assert_eq!(removal.paths.len(), 3);
    }

    #[test]
    fn test_plan_removal_lists_a_kept_path_outside_the_home_folder_as_a_sentence_only() {
        // Ruling 6: grok's installer may leave `/usr/local/bin/grok`, a link
        // into `~/.grok/downloads`. It is reported (it becomes a dead link),
        // never protected -- as a kept path it would refuse the very
        // uninstall it exists for (`OverlapsKept`) -- and reported only when
        // it is a link into this tool's root: the same path may be
        // Homebrew's live link into its Caskroom (an Intel Mac), another
        // CLI's `agent`, or a plain file, and "a dead link you can delete"
        // would then be a false sentence. Stood in for by paths under
        // another temp directory, since a test cannot write to
        // /usr/local/bin.
        let outside = TempHome::new("removal-outside-keep");
        let home = TempHome::new("removal-outside-keep-home");
        let layout = claude_layout(&home, "2.1.281");
        let fallback = outside.link("bin/claude", &layout.real);
        let caskroom = outside.executable("Caskroom/claude-code/2.1.281/claude");
        let brews = outside.link("bin/claude-brew", &caskroom);
        let plain = outside.file("bin/claude-file");
        let (remove, _) = claude_lists();
        let job_for = |kept: &Path| Job {
            recipe: &CLAUDE,
            detected: detected(home.path()),
            remove,
            keep: only_keep(kept.display().to_string(), KeptWhat::OutsideHome),
            globs: &[],
        };

        // A link into the root: reported, after the moves.
        let removal = plan_removal(&job_for(&fallback)).expect("a plan, not OverlapsKept");
        assert_eq!(
            removal.warnings,
            vec![
                trash("~/.local/share/claude", RemovedWhat::Program),
                trash("~/.local/bin/claude", RemovedWhat::Launcher),
                Warning::WillKeep {
                    path: fallback.display().to_string(),
                    what: KeptWhat::OutsideHome,
                },
            ]
        );
        // A link elsewhere (Homebrew's), a regular file, and nothing at all:
        // no sentence, and no refusal.
        for (what, kept) in [("a link elsewhere", &brews), ("a regular file", &plain)] {
            let removal = plan_removal(&job_for(kept)).expect("a plan");
            assert_eq!(removal.warnings.len(), 2, "{what}: {:?}", removal.warnings);
        }
        std::fs::remove_file(&fallback).unwrap();
        let removal = plan_removal(&job_for(&fallback)).expect("a plan");
        assert_eq!(removal.warnings.len(), 2);
    }

    #[test]
    fn test_points_into_reads_a_links_target_resolved_or_by_its_own_text() {
        // The one question `outside_home_keeps` asks: is this a symbolic
        // link into the root? Resolved when it resolves; by its own text,
        // folded from its folder, when it dangles (the second run of a
        // stopped uninstall, `downloads/` already in the Trash); never for
        // a file or a folder.
        let home = TempHome::new("removal-points-into");
        let root = home.dir(".grok");
        let real = home.executable(".grok/downloads/grok-1.0.41-macos-aarch64");
        let outside = TempHome::new("removal-points-into-outside");
        let resolving = outside.link("bin/grok", &real);
        let dangling = outside.link(
            "bin/grok-gone",
            &home
                .path()
                .join(".grok/downloads/grok-1.0.40-macos-aarch64"),
        );
        let relative_dangling = outside.link(
            "bin/grok-rel",
            Path::new(&format!(
                "../../{}/.grok/bin/grok",
                home.path().file_name().unwrap().to_str().unwrap()
            )),
        );
        let elsewhere = outside.link("bin/other", &outside.executable("Caskroom/x/grok"));
        let file = outside.file("bin/file");
        let folder = outside.dir("bin/folder");
        assert!(points_into(&resolving, &root));
        assert!(points_into(&dangling, &root));
        // `../../<home-name>/.grok/bin/grok` from `<outside>/bin`: the two
        // temp homes are siblings, so the text lands under the root.
        assert!(
            points_into(&relative_dangling, &root),
            "{relative_dangling:?}"
        );
        assert!(!points_into(&elsewhere, &root));
        assert!(!points_into(&file, &root));
        assert!(!points_into(&folder, &root));
        assert!(!points_into(&outside.path().join("bin/missing"), &root));
    }

    #[tokio::test]
    async fn test_execute_removal_moves_a_backup_the_preview_listed_and_stops_when_one_appears_after_it(
    ) {
        // Check 5 at run time: the backup the preview listed is moved in its
        // place; a second backup appearing between the preview and the
        // click makes the fresh list differ, and nothing moves.
        let home = TempHome::new("removal-exec-globs");
        let layout = claude_layout(&home, "2.1.281");
        home.file(".local/bin/claude.1727000000.old");
        let (remove, keep) = claude_lists();
        let job = Job {
            recipe: &CLAUDE,
            detected: detected(home.path()),
            remove,
            keep,
            globs: only_glob(Glob {
                dir: "~/.local/bin",
                prefix: "claude.",
                suffix: ".old",
                what: RemovedWhat::Backups,
            }),
        };
        let preview = plan_removal(&job).unwrap();
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();

        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(mock.calls(), preview.paths);
        assert_eq!(mock.kinds()[1], ItemKind::File, "the backup, as a file");
        assert!(std::fs::symlink_metadata(&layout.launcher).is_err());

        let home = TempHome::new("removal-exec-glob-appeared");
        let _layout = claude_layout(&home, "2.1.281");
        home.file(".local/bin/claude.1727000000.old");
        let job = Job {
            detected: detected(home.path()),
            ..job
        };
        let preview = plan_removal(&job).unwrap();
        home.file(".local/bin/claude.1727000600.old");
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();

        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.local/bin/claude.1727000600.old"));
        assert!(mock.calls().is_empty());
    }

    #[test]
    fn test_plan_removal_refuses_a_path_whose_folder_is_home_or_shared() {
        // Check 1's never-list (spec §6.3, ruling 5): a path directly in the
        // home folder, or directly in one of the folders many tools share,
        // is refused -- a recipe that lists `~/.local/bin` for
        // `~/.local/bin/claude` must not move every tool's launcher -- and
        // so is one whose folder leads into a shared folder through a link
        // (dotfiles kept elsewhere in the home folder). No shipped recipe
        // lists such a path (`recipes::tests`); this pins the check itself.
        let home = TempHome::new("removal-shared-folder");
        let _layout = claude_layout(&home, "2.1.281");
        home.dir("claude-thing");
        let dotfiles = home.dir("dotfiles/config");
        home.dir("dotfiles/config/fish");
        home.link(".config", &dotfiles);
        let d = detected(home.path());
        for listed in ["~/claude-thing", "~/.local/bin", "~/.config/fish"] {
            let job = Job {
                recipe: &CLAUDE,
                detected: d.clone(),
                remove: only(RemoveSpec {
                    path: listed,
                    expect: Expect::Dir,
                    what: RemovedWhat::Launcher,
                    optional: false,
                }),
                keep: &[],
                globs: &[],
            };
            let (path, reason) = refused(plan_removal(&job));
            assert_eq!(
                (path.as_str(), reason),
                (listed, UninstallUnsafeReason::SharedFolder)
            );
        }
    }

    #[test]
    fn test_plan_removal_refuses_an_empty_list_as_a_plain_refusal() {
        // Every listed path optional and absent: nothing to move. A plain
        // `Refused`, on purpose without copy of its own -- unreachable for a
        // shipped recipe, whose launcher is never optional and exists while
        // the row does (spec §6.3).
        let home = TempHome::new("removal-empty");
        let _layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = Job {
            recipe: &CLAUDE,
            detected: d,
            remove: only(RemoveSpec {
                path: "~/.claude/downloads",
                expect: Expect::Dir,
                what: RemovedWhat::Cache,
                optional: true,
            }),
            keep: &[],
            globs: &[],
        };

        assert!(matches!(plan_removal(&job), Err(AdapterError::Refused(_))));
    }

    /// A run of `execute_removal` over `job` with what the preview
    /// `removal` found, through a `VecSink`, returning the outcome and the
    /// notes written.
    async fn run(
        job: &Job,
        removal: &Removal,
        trasher: &Arc<dyn Trasher>,
        pacing: Pacing,
        cancel: CancellationToken,
    ) -> (Outcome, Vec<LogNote>) {
        let sink = Arc::new(VecSink::new());
        let confirmed = Confirmed {
            paths: &removal.paths,
            previewed: &removal.identities,
        };
        let outcome = execute_removal(job, confirmed, trasher, pacing, sink.clone(), 9, cancel)
            .await
            .expect("execute_removal");
        let notes = sink
            .snapshot()
            .into_iter()
            .filter_map(|event| match event {
                OperationEvent::Note { op_id: 9, note } => Some(note),
                _ => None,
            })
            .collect();
        (outcome, notes)
    }

    /// No gap after a move, the production budget, and a `LastMove` of
    /// this run's own.
    fn no_gap() -> Pacing {
        Pacing {
            settle: Duration::ZERO,
            budget: Duration::from_secs(TIMEOUT_SECS),
            last_move: Arc::default(),
        }
    }

    fn moved(path: &str, to: &Path) -> LogNote {
        LogNote::MovedToTrash {
            path: path.to_string(),
            trashed_to: to.display().to_string(),
        }
    }

    fn path_changed(path: &str) -> Outcome {
        Outcome::CanagerFailed(Fault::PathChanged {
            path: path.to_string(),
        })
    }

    #[tokio::test]
    async fn test_execute_removal_moves_each_path_in_order_and_notes_each() {
        let home = TempHome::new("removal-exec-full");
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        home.file(".claude.json");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();

        let (outcome, notes) =
            run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(mock.calls(), preview.paths);
        // Each item is handed over as what its last check saw: the launcher
        // as a link, never as a folder a URL's trailing slash could enter.
        assert_eq!(
            mock.kinds(),
            vec![ItemKind::Dir, ItemKind::Dir, ItemKind::Symlink]
        );
        // Both `claude`s are in the bin: the directory under its own name,
        // the link -- moved as a link -- under a suffixed one (the mock
        // suffixes with the call index; the system with the time).
        assert!(mock.bin().join("claude/versions/2.1.281").is_file());
        assert!(mock.bin().join("downloads").is_dir());
        let link = mock.bin().join("claude 2");
        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(std::fs::symlink_metadata(&layout.launcher).is_err());
        assert!(home.path().join(".claude.json").is_file(), "kept");
        assert!(home.path().join(".claude").is_dir(), "kept");
        assert_eq!(
            notes,
            vec![
                moved("~/.local/share/claude", &mock.bin().join("claude")),
                moved("~/.claude/downloads", &mock.bin().join("downloads")),
                moved("~/.local/bin/claude", &link),
            ]
        );
    }

    #[tokio::test]
    async fn test_execute_removal_refuses_when_the_fresh_list_differs_from_the_preview() {
        // The user confirmed two paths; by run time the updater has made
        // `~/.claude/downloads`. Not the list they saw: stop before the
        // first move, naming the path that appeared.
        let home = TempHome::new("removal-exec-list-grew");
        let _layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        home.dir(".claude/downloads");
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();

        let (outcome, notes) =
            run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.claude/downloads"));
        assert!(mock.calls().is_empty());
        assert!(notes.is_empty());

        // And the other way round: a path the user confirmed is gone.
        let preview = plan_removal(&job).unwrap();
        std::fs::remove_dir(home.path().join(".claude/downloads")).unwrap();
        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;
        assert_eq!(outcome, path_changed("~/.claude/downloads"));
        assert!(mock.calls().is_empty());
    }

    #[tokio::test]
    async fn test_execute_removal_refuses_when_a_check_now_fails() {
        // The launcher was re-pointed elsewhere after the preview: check 4
        // fails at run time, and that is a changed path, not a refused
        // plan. Nothing is moved -- the program directory is first on the
        // list and would have passed.
        let home = TempHome::new("removal-exec-check-fails");
        let layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let elsewhere = home.executable("elsewhere/claude");
        std::fs::remove_file(&layout.launcher).unwrap();
        std::os::unix::fs::symlink(elsewhere, &layout.launcher).unwrap();
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();

        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.local/bin/claude"));
        assert!(mock.calls().is_empty());
        assert!(
            layout.root.join("versions/2.1.281").is_file(),
            "nothing moved"
        );
    }

    #[tokio::test]
    async fn test_execute_removal_refuses_what_the_preview_did_not_see() {
        // Spec §6.3 (ruling 10): what the preview saw travels with the plan.
        // Claude Code's updater re-points the launcher at a new version
        // inside the root -- the same shape, every check still passes, and
        // the same path string -- but it is a new link, not the one the user
        // looked at: stop before anything moves, and the user previews
        // again. The same for a cache folder replaced by a new one.
        let home = TempHome::new("removal-exec-self-update");
        let layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let newer = home.executable(".local/share/claude/versions/2.1.282");
        std::fs::remove_file(&layout.launcher).unwrap();
        std::os::unix::fs::symlink(newer, &layout.launcher).unwrap();
        assert!(plan_removal(&job).is_ok(), "every check still passes");
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();

        let (outcome, notes) =
            run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.local/bin/claude"));
        assert!(mock.calls().is_empty());
        assert!(notes.is_empty());
        assert!(
            layout.root.join("versions/2.1.281").is_file(),
            "nothing moved"
        );

        let home = TempHome::new("removal-exec-cache-replaced");
        let _layout = claude_layout(&home, "2.1.281");
        let downloads = home.dir(".claude/downloads");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        // Made while the old one still exists, so it cannot get its inode.
        let old = home.path().join(".claude/downloads.old");
        std::fs::rename(&downloads, &old).unwrap();
        std::fs::create_dir(&downloads).unwrap();
        std::fs::remove_dir(&old).unwrap();

        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.claude/downloads"));
        assert!(mock.calls().is_empty());
    }

    #[tokio::test]
    async fn test_execute_removal_refuses_a_settings_folder_linked_to_the_program_folder_after_the_preview(
    ) {
        // Ruling 25 at run time: between the preview and the click,
        // `~/.claude` became a link to `~/.local/share/claude`. The fresh
        // look finds the kept folder inside what the first move would take,
        // and stops before anything moves, naming the kept path.
        let home = TempHome::new("removal-exec-settings-aliased");
        let layout = claude_layout(&home, "2.1.281");
        home.file(".claude/projects/p/session.jsonl");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        std::fs::rename(
            home.path().join(".claude"),
            layout.root.join("moved-settings"),
        )
        .unwrap();
        home.link(".claude", &layout.root);
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();

        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.claude"));
        assert!(mock.calls().is_empty());
    }

    /// A trasher that, right after moving its first item, rearranges
    /// `~/.claude.json` as a dotfiles tool might: the settings move to
    /// `~/settings/claude.json`, a link to them appears in the download
    /// cache (`~/.claude/downloads/settings-link`), and `~/.claude.json`
    /// becomes a link to that link -- a change before the next item's
    /// turn, as one during the pause that follows would be in production.
    struct RelinkingTrasher {
        inner: MockTrasher,
        home: PathBuf,
        done: StdMutex<bool>,
    }

    impl Trasher for RelinkingTrasher {
        fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError> {
            let result = self.inner.trash(path, kind);
            let mut done = self.done.lock().unwrap();
            if !*done {
                *done = true;
                let kept = self.home.join(".claude.json");
                let settings = self.home.join("settings/claude.json");
                let middle = self.home.join(".claude/downloads/settings-link");
                std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
                std::fs::rename(&kept, &settings).unwrap();
                std::os::unix::fs::symlink(&settings, &middle).unwrap();
                std::os::unix::fs::symlink(&middle, &kept).unwrap();
            }
            result
        }
    }

    #[tokio::test]
    async fn test_execute_removal_stops_when_the_way_to_a_kept_path_comes_to_run_through_the_next_item(
    ) {
        // The same finding at run time: after the program folder's move,
        // `~/.claude.json` becomes `-> ~/.claude/downloads/settings-link ->
        // ~/settings/claude.json`. The cache's turn places the kept paths
        // afresh, finds that the way to the settings now runs through the
        // cache (the middle link is in it), and stops before moving it,
        // naming the kept path: the program folder is in the Trash, the
        // cache and the launcher stay, and `~/.claude.json` still leads to
        // the settings.
        let home = TempHome::new("removal-exec-kept-way");
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        let kept = home.file(".claude.json");
        let written = std::fs::read(&kept).unwrap();
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let relinking = Arc::new(RelinkingTrasher {
            inner: MockTrasher::new(),
            home: home.path().to_path_buf(),
            done: StdMutex::new(false),
        });
        let trasher: Arc<dyn Trasher> = relinking.clone();

        let (outcome, notes) =
            run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.claude.json"));
        assert_eq!(relinking.inner.calls(), preview.paths[..1].to_vec());
        assert_eq!(notes.len(), 1, "the program folder's move was logged");
        let middle = home.path().join(".claude/downloads/settings-link");
        assert!(
            std::fs::symlink_metadata(&middle)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the middle link stays in the cache"
        );
        assert_eq!(
            std::fs::read(&kept).unwrap(),
            written,
            "the kept path still leads to the settings"
        );
        assert!(std::fs::symlink_metadata(&layout.launcher)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    /// A trasher that, while moving its first item, replaces the cache
    /// folder with a fresh one of the same name: a change during the pause,
    /// before the next item's check.
    struct ReplacingTrasher {
        inner: MockTrasher,
        replace: PathBuf,
        done: StdMutex<bool>,
    }

    impl Trasher for ReplacingTrasher {
        fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError> {
            let result = self.inner.trash(path, kind);
            let mut done = self.done.lock().unwrap();
            if !*done {
                *done = true;
                // The new directory is made while the old one still exists,
                // so it cannot be given the old inode number.
                let old = self.replace.with_extension("old");
                std::fs::rename(&self.replace, &old).unwrap();
                std::fs::create_dir(&self.replace).unwrap();
                std::fs::remove_dir_all(&old).unwrap();
            }
            result
        }
    }

    #[tokio::test]
    async fn test_execute_removal_catches_a_substitution_before_an_items_check() {
        // Ruling 26's boundary, first half: a same-name replacement that
        // happens before an item's turn is caught by that turn's check --
        // the cache folder passed every check at the confirmation, but the
        // one about to be moved is another inode. The trasher never gets
        // it; the program directory, moved first, stays in the Trash; the
        // launcher, last, stays in place.
        let home = TempHome::new("removal-exec-replaced");
        let layout = claude_layout(&home, "2.1.281");
        let downloads = home.dir(".claude/downloads");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let replacing = Arc::new(ReplacingTrasher {
            inner: MockTrasher::new(),
            replace: downloads,
            done: StdMutex::new(false),
        });
        let trasher: Arc<dyn Trasher> = replacing.clone();

        let (outcome, notes) =
            run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.claude/downloads"));
        assert_eq!(replacing.inner.calls(), preview.paths[..1].to_vec());
        assert_eq!(notes.len(), 1, "the program directory's move was logged");
        assert!(std::fs::symlink_metadata(&layout.launcher)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(std::fs::symlink_metadata(&layout.root).is_err(), "moved");
    }

    /// A trasher that swaps the item it is handed for another folder of the
    /// same name at the start of its second call, then moves what it finds:
    /// a substitution after that item's last check, inside the move itself.
    struct SwappingTrasher {
        inner: MockTrasher,
        calls: StdMutex<usize>,
    }

    impl Trasher for SwappingTrasher {
        fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError> {
            let nth = {
                let mut calls = self.calls.lock().unwrap();
                *calls += 1;
                *calls - 1
            };
            if nth == 1 {
                std::fs::rename(path, path.with_extension("checked")).unwrap();
                std::fs::create_dir(path).unwrap();
                std::fs::write(path.join("substitute"), b"not what was checked").unwrap();
            }
            self.inner.trash(path, kind)
        }
    }

    #[tokio::test]
    async fn test_a_substitution_inside_the_move_itself_is_beyond_the_last_check() {
        // Ruling 26's boundary, second half, pinned so the documentation
        // stays true: the last check is immediately before the call, and
        // the system's call takes a path, so an item swapped between the
        // two -- here, inside the call -- is what gets moved, and the run
        // cannot tell. Out of scope by design (the threat model is change by
        // accident, and a program running as the user can do all Canager
        // can); docs/what-we-run.md says so in one sentence. If this ever
        // fails because the gap was closed, change that sentence too.
        let home = TempHome::new("removal-exec-swapped-in-call");
        let _layout = claude_layout(&home, "2.1.281");
        let downloads = home.dir(".claude/downloads");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let swapping = Arc::new(SwappingTrasher {
            inner: MockTrasher::new(),
            calls: StdMutex::new(0),
        });
        let trasher: Arc<dyn Trasher> = swapping.clone();

        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, Outcome::Succeeded);
        assert!(swapping.inner.bin().join("downloads/substitute").is_file());
        assert!(
            downloads.with_extension("checked").is_dir(),
            "the checked one stayed"
        );
    }

    /// A trasher that, after moving its first item (the program folder),
    /// renames `~/.claude` to a folder outside the home folder on the same
    /// volume and leaves a link to it in its place: the cache inside keeps
    /// its inode, and only its folders changed.
    struct RelocatingTrasher {
        inner: MockTrasher,
        folder: PathBuf,
        away: PathBuf,
        done: StdMutex<bool>,
    }

    impl Trasher for RelocatingTrasher {
        fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError> {
            let result = self.inner.trash(path, kind);
            let mut done = self.done.lock().unwrap();
            if !*done {
                *done = true;
                std::fs::rename(&self.folder, &self.away).unwrap();
                std::os::unix::fs::symlink(&self.away, &self.folder).unwrap();
            }
            result
        }
    }

    #[tokio::test]
    async fn test_execute_removal_checks_an_items_folders_again_after_the_pause() {
        // Ruling 24 at run time, the review's third counterexample: during
        // the pause after the first move, `~/.claude` is renamed away out of
        // the home folder and replaced by a link to where it went. The
        // cache's own `(st_dev, st_ino)` did not change -- an identity check
        // alone would move a folder outside the home folder -- but its turn
        // runs every check again, and its folder now leads outside home.
        let home = TempHome::new("removal-exec-ancestor-moved");
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let elsewhere = TempHome::new("removal-exec-ancestor-away");
        let away = elsewhere.path().join("claude-settings");
        let relocating = Arc::new(RelocatingTrasher {
            inner: MockTrasher::new(),
            folder: home.path().join(".claude"),
            away: away.clone(),
            done: StdMutex::new(false),
        });
        let trasher: Arc<dyn Trasher> = relocating.clone();

        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.claude/downloads"));
        assert_eq!(
            identity(&away.join("downloads")),
            preview.identities[1],
            "the leaf kept its identity; only a folder above it changed"
        );
        assert_eq!(relocating.inner.calls(), preview.paths[..1].to_vec());
        assert!(away.join("downloads").is_dir(), "not moved");
        assert!(std::fs::symlink_metadata(&layout.launcher)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    /// A trasher that, right after its `after`-th call (0-based) has moved
    /// its item, writes the file `recreate` (folders made as needed): a
    /// listed path back before the next item's turn -- in production,
    /// during the pause that follows -- as when a Claude Code still running
    /// installs an update into a program folder that was just moved.
    struct RecreatingTrasher {
        inner: MockTrasher,
        after: usize,
        recreate: PathBuf,
        calls: StdMutex<usize>,
    }

    impl Trasher for RecreatingTrasher {
        fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError> {
            let result = self.inner.trash(path, kind);
            let nth = {
                let mut calls = self.calls.lock().unwrap();
                *calls += 1;
                *calls - 1
            };
            if nth == self.after {
                std::fs::create_dir_all(self.recreate.parent().unwrap()).unwrap();
                std::fs::write(&self.recreate, b"#!/bin/sh\n").unwrap();
            }
            result
        }
    }

    #[tokio::test]
    async fn test_execute_removal_stops_before_the_launcher_when_a_listed_path_is_back() {
        // The launcher is last so that a stopped run leaves a row; once it
        // is gone, nothing shows what the list left behind. A program
        // folder that came back during a pause -- a Claude Code still
        // running installed an update -- would be invisible after the
        // launcher's move, so the launcher's turn looks for every other
        // listed path once more and stops when one is there: `PathChanged`
        // naming it, the launcher in place, the row still there, and a
        // fresh preview lists what came back. The program folder twice --
        // holding the launcher's own target (`probe` would say `Present`)
        // and a newer version only (`probe` would say `LauncherOnly`, yet
        // the folder is there) -- and then the cache.
        for version in ["2.1.281", "2.1.282"] {
            let home = TempHome::new("removal-exec-root-back");
            let layout = claude_layout(&home, "2.1.281");
            home.dir(".claude/downloads");
            let d = detected(home.path());
            let job = claude_job(&d);
            let preview = plan_removal(&job).unwrap();
            let recreated = layout.root.join("versions").join(version);
            let recreating = Arc::new(RecreatingTrasher {
                inner: MockTrasher::new(),
                after: 0,
                recreate: recreated.clone(),
                calls: StdMutex::new(0),
            });
            let trasher: Arc<dyn Trasher> = recreating.clone();

            let (outcome, notes) =
                run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

            assert_eq!(outcome, path_changed("~/.local/share/claude"), "{version}");
            assert_eq!(
                recreating.inner.calls(),
                preview.paths[..2].to_vec(),
                "{version}: the cache, second, is checked as itself and moved; the launcher is never handed over"
            );
            assert_eq!(notes.len(), 2, "{version}");
            assert!(
                std::fs::symlink_metadata(&layout.launcher)
                    .unwrap()
                    .file_type()
                    .is_symlink(),
                "{version}: the launcher stays"
            );
            assert_ne!(
                route::probe(CLAUDE.route.kind, &layout.launcher, &layout.root),
                Probe::Absent,
                "{version}: the row stays"
            );
            assert!(
                recreated.is_file(),
                "{version}: what came back is left as it is"
            );
            let again = plan_removal(&job).expect("a fresh preview");
            assert_eq!(
                again.paths,
                vec![layout.root.clone(), layout.launcher.clone()],
                "{version}: the fresh preview lists what came back"
            );
        }

        // The cache, back after its own move (the program folder stays gone):
        // the same stop, naming the cache; the fresh preview lists the
        // program folder as already gone and the cache to move.
        let home = TempHome::new("removal-exec-cache-back");
        let layout = claude_layout(&home, "2.1.281");
        let downloads = home.dir(".claude/downloads");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let recreating = Arc::new(RecreatingTrasher {
            inner: MockTrasher::new(),
            after: 1,
            recreate: downloads.join("claude-2.1.282.tgz"),
            calls: StdMutex::new(0),
        });
        let trasher: Arc<dyn Trasher> = recreating.clone();

        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.claude/downloads"));
        assert_eq!(recreating.inner.calls(), preview.paths[..2].to_vec());
        assert!(std::fs::symlink_metadata(&layout.launcher)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(std::fs::symlink_metadata(&layout.root).is_err(), "moved");
        let again = plan_removal(&job).expect("a fresh preview");
        assert_eq!(
            again.paths,
            vec![downloads.clone(), layout.launcher.clone()]
        );
        assert_eq!(
            again.warnings[0],
            Warning::AlreadyGone {
                path: "~/.local/share/claude".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_execute_removal_moves_the_launcher_past_a_path_kept_as_not_ours_never_past_one_it_moved(
    ) {
        // Ruling 5 of the phase 4 step D plan: an optional path that is there
        // but that Canager cannot confirm is the tool's is kept and said, and
        // the uninstall goes on (spec §十三 #27). At the launcher's turn it is
        // still there -- nothing moved it -- and the preview showed it
        // staying, so the launcher moves. Claude Code's `~/.claude` as a link
        // to `~/Documents`, whose `downloads` the preview keeps.
        let home = TempHome::new("removal-exec-past-not-ours");
        let layout = claude_layout(&home, "2.1.281");
        let documents = home.dir("Documents");
        home.dir("Documents/downloads");
        home.link(".claude", &documents);
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        assert!(preview
            .warnings
            .contains(&keep("~/.claude/downloads", KeptWhat::NotOurs)));
        let trasher: Arc<dyn Trasher> = Arc::new(MockTrasher::new());

        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, Outcome::Succeeded);
        assert!(
            std::fs::symlink_metadata(&layout.launcher).is_err(),
            "the launcher moved"
        );
        assert!(
            home.path().join("Documents/downloads").is_dir(),
            "what the preview kept stays"
        );

        // A path this run moved, back as something that is not the tool's
        // (the cache folder, back as a file): it is there again, so it still
        // stops the run before the launcher, whatever it is now.
        let home = TempHome::new("removal-exec-moved-back-as-other");
        let layout = claude_layout(&home, "2.1.281");
        let downloads = home.dir(".claude/downloads");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let recreating = Arc::new(RecreatingTrasher {
            inner: MockTrasher::new(),
            after: 1,
            recreate: downloads.clone(),
            calls: StdMutex::new(0),
        });
        let trasher: Arc<dyn Trasher> = recreating.clone();

        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.claude/downloads"));
        assert!(downloads.is_file(), "what came back is left as it is");
        assert!(
            std::fs::symlink_metadata(&layout.launcher)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the launcher stays"
        );
    }

    #[tokio::test]
    async fn test_execute_removal_stops_at_a_refused_item_with_the_systems_words() {
        // macOS refused the second item: the outcome is `Failed` with its
        // words as the summary (quoted by the front end like a tool's
        // stderr), the first item is in the Trash, the launcher is not.
        let home = TempHome::new("removal-exec-refused");
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let mock = Arc::new(MockTrasher::new());
        mock.refuse_call(1, "Operation not permitted");
        let trasher: Arc<dyn Trasher> = mock.clone();

        let (outcome, notes) =
            run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(
            outcome,
            Outcome::Failed {
                exit_code: None,
                summary: "Operation not permitted".to_string()
            }
        );
        assert_eq!(mock.calls(), preview.paths[..2].to_vec());
        assert!(std::fs::symlink_metadata(&layout.launcher)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            notes,
            vec![
                moved("~/.local/share/claude", &mock.bin().join("claude")),
                LogNote::TrashFailed {
                    path: "~/.claude/downloads".to_string(),
                    error: "Operation not permitted".to_string(),
                },
            ]
        );
    }

    #[tokio::test]
    async fn test_execute_removal_stops_between_items_when_cancelled() {
        // Cancel while the first item's move is being reported: that move
        // is logged, and the run stops before the next item --
        // `Unconfirmed`, for `run_operation` to reconcile. The program
        // directory is in the Trash, the launcher still there, which is the
        // launcher-only state.
        let home = TempHome::new("removal-exec-cancel");
        let layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let token = CancellationToken::new();
        let mock = Arc::new(MockTrasher::new());
        mock.cancel_after_call(0, token.clone());
        let trasher: Arc<dyn Trasher> = mock.clone();

        let (outcome, notes) = run(&job, &preview, &trasher, no_gap(), token).await;

        assert_eq!(outcome, Outcome::Unconfirmed);
        assert_eq!(mock.calls(), preview.paths[..1].to_vec());
        assert_eq!(notes.len(), 1);
        assert!(std::fs::symlink_metadata(&layout.launcher)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    /// A trasher whose moves take `delay` (a slow Trash), for the test that
    /// presses Cancel in the middle of one.
    struct SlowTrasher {
        inner: MockTrasher,
        delay: Duration,
    }

    impl Trasher for SlowTrasher {
        fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError> {
            std::thread::sleep(self.delay);
            self.inner.trash(path, kind)
        }
    }

    #[tokio::test]
    async fn test_execute_removal_finishes_a_move_under_way_when_cancel_arrives() {
        // Ruling 28: a move already handed to the system is not abandoned.
        // Cancel arrives 50 ms into a 300 ms move; the run waits for it,
        // logs it, and only then stops -- nothing is still moving on another
        // thread when `execute_removal` reports.
        let home = TempHome::new("removal-exec-cancel-mid-move");
        let _layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let slow = Arc::new(SlowTrasher {
            inner: MockTrasher::new(),
            delay: Duration::from_millis(300),
        });
        let trasher: Arc<dyn Trasher> = slow.clone();
        let token = CancellationToken::new();
        let pressed = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            pressed.cancel();
        });
        let started = Instant::now();

        let (outcome, notes) = run(&job, &preview, &trasher, no_gap(), token).await;

        assert_eq!(outcome, Outcome::Unconfirmed);
        assert!(
            started.elapsed() >= Duration::from_millis(300),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(slow.inner.calls(), preview.paths[..1].to_vec());
        assert_eq!(
            notes,
            vec![moved(
                "~/.local/share/claude",
                &slow.inner.bin().join("claude")
            )]
        );
    }

    #[tokio::test]
    async fn test_execute_removal_stops_before_an_item_when_the_budget_is_spent_and_says_so() {
        // The one stop the user did not ask for gets a line of Canager's
        // own -- the item it stopped before and the budget it ran out of --
        // so the log does not simply end before a "Result unconfirmed".
        let home = TempHome::new("removal-exec-budget");
        let _layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();
        let spent = Pacing {
            settle: Duration::ZERO,
            budget: Duration::ZERO,
            ..no_gap()
        };

        let (outcome, notes) = run(&job, &preview, &trasher, spent, CancellationToken::new()).await;

        assert_eq!(outcome, Outcome::Unconfirmed);
        assert!(mock.calls().is_empty());
        assert_eq!(
            notes,
            vec![LogNote::OutOfTime {
                path: "~/.local/share/claude".to_string(),
                seconds: 0,
            }]
        );
    }

    #[tokio::test]
    async fn test_execute_removal_cuts_a_pause_to_what_is_left_of_the_budget() {
        // Ruling 28: no pause outlasts the budget. A 5 s pause with 1 s of
        // budget left ends when the budget does, and the run stops before
        // the next item.
        let home = TempHome::new("removal-exec-pause-budget");
        let _layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();
        let tight = Pacing {
            settle: Duration::from_secs(5),
            budget: Duration::from_secs(1),
            ..no_gap()
        };
        let started = Instant::now();

        let (outcome, notes) = run(&job, &preview, &trasher, tight, CancellationToken::new()).await;

        assert_eq!(outcome, Outcome::Unconfirmed);
        assert_eq!(mock.calls().len(), 1);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
        // The log: the one move, then why nothing followed it -- the item
        // the run stopped before (the launcher, second of two here) and
        // the budget that ended it.
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!(
            matches!(notes[0], LogNote::MovedToTrash { .. }),
            "{notes:?}"
        );
        assert_eq!(
            notes[1],
            LogNote::OutOfTime {
                path: "~/.local/bin/claude".to_string(),
                seconds: 1,
            }
        );
    }

    #[tokio::test]
    async fn test_execute_removal_waits_the_settle_gap_after_each_item() {
        // Author decision 1: a pause after each move (Finder's Put Back
        // record) -- before the next one, and after the last before the run
        // is reported finished -- and none before the first.
        let home = TempHome::new("removal-exec-paced");
        let _layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let trasher: Arc<dyn Trasher> = Arc::new(MockTrasher::new());
        let paced = Pacing {
            settle: Duration::from_millis(100),
            ..no_gap()
        };
        let started = Instant::now();

        let (outcome, _) = run(&job, &preview, &trasher, paced, CancellationToken::new()).await;

        assert_eq!(outcome, Outcome::Succeeded);
        assert!(
            started.elapsed() >= Duration::from_millis(300),
            "three pauses for three items -- two between them, one after the last: {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn test_execute_removal_ends_the_settle_wait_at_a_cancel() {
        // The pause is watched for Cancel. It is long enough here that
        // finishing quickly proves the cancel ended it, not the timer.
        let home = TempHome::new("removal-exec-gap-cancel");
        let _layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let token = CancellationToken::new();
        let mock = Arc::new(MockTrasher::new());
        mock.cancel_after_call(0, token.clone());
        let trasher: Arc<dyn Trasher> = mock.clone();
        let gap = Pacing {
            settle: Duration::from_secs(5),
            ..no_gap()
        };
        let started = Instant::now();

        let (outcome, _) = run(&job, &preview, &trasher, gap, token).await;

        assert_eq!(outcome, Outcome::Unconfirmed);
        assert_eq!(mock.calls().len(), 1);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "the cancel ended the 5 s wait: {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn test_execute_removal_waits_the_gap_after_another_uninstalls_move_before_its_first_item(
    ) {
        // Another tool's uninstall has just moved an item through the same
        // `LastMove` -- two can run at once: up to three operations, each
        // locking only its own instance. Finder's Put Back record needs the
        // gap between one process's moves whichever uninstall made them, so
        // this run's first move waits it out too, not only its later ones.
        let home = TempHome::new("removal-exec-after-another");
        let _layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let timed = Arc::new(TimedTrasher::default());
        let trasher: Arc<dyn Trasher> = timed.clone();
        let paced = Pacing {
            settle: Duration::from_millis(300),
            ..no_gap()
        };
        let other_move = Instant::now();
        *paced.last_move.0.lock().await = Some(other_move);

        let (outcome, _) = run(&job, &preview, &trasher, paced, CancellationToken::new()).await;

        assert_eq!(outcome, Outcome::Succeeded);
        let began = timed.began();
        assert_eq!(began.len(), 2, "the program folder and the launcher");
        assert!(
            began[0].duration_since(other_move) >= Duration::from_millis(300),
            "the first move began {:?} after the other uninstall's",
            began[0].duration_since(other_move)
        );
    }

    #[tokio::test]
    async fn test_execute_removal_waits_out_another_uninstalls_turn_unless_cancelled_or_out_of_time(
    ) {
        // Another tool's uninstall holds the turn at the Trash
        // (`LastMove`): one of its items is between its wait and its move.
        // No item of this run moves until it lets go. Cancel ends that
        // wait at once, with no line of Canager's; a spent budget ends it
        // too, with the note naming the item the run stopped before.
        // Nothing moves either time; once the other turn is over, the same
        // list goes ahead.
        let home = TempHome::new("removal-exec-other-turn");
        let _layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();
        let shared = no_gap();
        let other_turn = shared.last_move.0.lock().await;

        let token = CancellationToken::new();
        let pressed = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            pressed.cancel();
        });
        let cancelled = tokio::time::timeout(
            Duration::from_secs(10),
            run(&job, &preview, &trasher, shared.clone(), token),
        )
        .await
        .expect("the cancel ended the wait");
        assert_eq!(cancelled, (Outcome::Unconfirmed, vec![]));
        assert!(mock.calls().is_empty(), "{:?}", mock.calls());

        let spent = Pacing {
            budget: Duration::from_millis(100),
            ..shared.clone()
        };
        let out_of_time = tokio::time::timeout(
            Duration::from_secs(10),
            run(&job, &preview, &trasher, spent, CancellationToken::new()),
        )
        .await
        .expect("the budget ended the wait");
        assert_eq!(
            out_of_time,
            (
                Outcome::Unconfirmed,
                vec![LogNote::OutOfTime {
                    path: "~/.local/share/claude".to_string(),
                    seconds: 0,
                }]
            )
        );
        assert!(mock.calls().is_empty(), "{:?}", mock.calls());

        drop(other_turn);
        let (outcome, _) = run(&job, &preview, &trasher, shared, CancellationToken::new()).await;
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(mock.calls(), preview.paths);
    }

    /// A trasher that cannot ask the system at all: what `RealTrasher`
    /// answers off macOS, or for a path that is not UTF-8.
    struct UnsupportedTrasher;

    impl Trasher for UnsupportedTrasher {
        fn trash(&self, _path: &Path, _kind: ItemKind) -> Result<PathBuf, TrashError> {
            Err(TrashError::Unsupported)
        }
    }

    #[tokio::test]
    async fn test_execute_removal_reports_a_trasher_that_cannot_ask_as_canagers_own_failure() {
        // Ruling 9: Canager's own limitation is `Fault::Internal` -- never a
        // `Failed` whose summary the front end would quote as the Mac's
        // words, and never a `TrashFailed` note. Nothing was moved.
        let home = TempHome::new("removal-exec-unsupported");
        let layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let trasher: Arc<dyn Trasher> = Arc::new(UnsupportedTrasher);

        let (outcome, notes) =
            run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, Outcome::CanagerFailed(Fault::Internal));
        assert!(notes.is_empty());
        assert!(
            layout.root.join("versions/2.1.281").is_file(),
            "nothing moved"
        );
    }

    #[tokio::test]
    async fn test_execute_removal_refuses_a_plan_that_lost_what_its_preview_saw() {
        // A `TrashPaths` plan read back from JSON carries no identities
        // (`previewed` is skipped by serde); one without them is a bug in
        // Canager, refused before anything moves.
        let home = TempHome::new("removal-exec-no-preview");
        let layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();

        let result = execute_removal(
            &job,
            Confirmed {
                paths: &preview.paths,
                previewed: &[],
            },
            &trasher,
            no_gap(),
            Arc::new(VecSink::new()),
            9,
            CancellationToken::new(),
        )
        .await;

        assert!(
            matches!(result, Err(AdapterError::Refused(_))),
            "{result:?}"
        );
        assert!(mock.calls().is_empty());
        assert!(layout.root.is_dir());
    }
}
