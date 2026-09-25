//! The path-list uninstall: how a tool with no uninstall command is removed
//! (phase 4 spec §6.2-§6.3, D8). `plan_removal` turns a recipe's
//! `Uninstall::Paths` list into the absolute paths to move, what each one
//! is (`ItemIdentity`) and the warnings the dialog lists, under the checks
//! below; `execute_removal` runs the same checks again at the confirmation,
//! compares every identity with the one the preview recorded, and then
//! moves each path to the Trash in order -- the launcher last -- running
//! every check on that item once more immediately before its move and,
//! before the launcher's move, looking once more for every other listed
//! path, which must be gone by then (`listed_path_back`). Nothing here
//! knows the `Adapter` contract (`mod.rs` does) or how an item is moved
//! (`crate::trash` does).
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
use crate::model::{Fault, ItemIdentity, ItemKind, Outcome, UninstallUnsafeReason, Warning};
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
/// system is waited for. The moves take milliseconds; the budget covers the
/// pauses below and a Trash that is slow to answer. Read by
/// `StandaloneAdapter::plan`.
pub const TIMEOUT_SECS: u64 = 120;

/// The pause after each item: before the next one, and after the last
/// before the run is reported finished. From a process without Full Disk
/// Access -- a Finder-launched Canager -- Finder wrote its "Put Back"
/// record for only the first of a burst of `trashItemAtURL:` calls up to
/// 1.5 s apart, and for every item when they were 2 s or more apart
/// (2026-09-25, one Mac, macOS 27.0: 15/15 and 4/4 runs; the mechanism is
/// not known). Those 4 runs also stayed alive 3 s after their last call,
/// and the record is written after the call returns (with Full Disk
/// Access, a process that exited at once lost the later records): hence
/// the same pause after the last item. Three seconds is the largest gap
/// measured to work, a full second above the largest that failed: it makes
/// Put Back likely for every item, not certain -- an item without the
/// record can still be dragged back out of the Trash -- and
/// `docs/what-we-run.md` says so. Each pause is cut short by Cancel and by
/// what is left of `TIMEOUT_SECS`. Read by `StandaloneAdapter::new`;
/// `with_trash_gap` sets it to zero for tests.
pub const PUT_BACK_SETTLE: Duration = Duration::from_secs(3);

/// One tool's removal as the checks see it: the recipe (for its route),
/// what detect learned (home, euid) and the recipe's two lists. Owned --
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

/// How `execute_removal` paces itself: the pause after each item
/// (`PUT_BACK_SETTLE` in production, zero in tests) and the budget it
/// stops between items once spent (`Plan.timeout_secs`). Built by
/// `StandaloneAdapter::execute`.
#[derive(Clone, Copy, Debug)]
pub struct Pacing {
    pub settle: Duration,
    pub budget: Duration,
}

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
/// leads with every link followed (`None` for a link to nothing).
struct Kept {
    spec: &'static KeepSpec,
    path: PathBuf,
    entry: PathBuf,
    target: Option<PathBuf>,
}

/// The kept paths that exist -- a missing one is neither listed nor
/// protected (ruling 6) -- each placed. One that is there but cannot be
/// placed refuses the whole list: Canager could not confirm the moves
/// leave it alone (`OverlapsKept`).
fn kept_places(look: &Look<'_>) -> Result<Vec<Kept>, Refusal> {
    let home = look.job.detected.home.as_path();
    let mut kept = Vec::new();
    for spec in look.job.keep {
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
            // A link to nothing: only the link itself is here to keep.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(Refusal::new(&path, UninstallUnsafeReason::OverlapsKept)),
        };
        kept.push(Kept {
            spec,
            path,
            entry,
            target,
        });
    }
    Ok(kept)
}

/// The kept path that moving the item at `location` would disturb, if
/// any (ruling 25). With every link resolved, the item may lie inside a
/// kept path only where the recipe lists it there -- `~/.claude/downloads`
/// inside `~/.claude`, a real folder -- and may never be a kept path, hold
/// one, or hold what one leads to: the preview says those stay. `location`
/// is where the item itself is (its folders are real folders by then, the
/// ancestry rule), `rel` how the recipe spells it.
fn disturbed<'k>(kept: &'k [Kept], rel: &Path, location: &Path) -> Option<&'k Kept> {
    kept.iter().find(|kept| {
        let kept_rel = spelled(kept.spec.path);
        let target = kept.target.as_deref();
        let takes_it =
            kept.entry.starts_with(location) || target.is_some_and(|t| t.starts_with(location));
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
/// and 4, and rulings 24 and 25), returning what it is. Its last step is
/// the `lstat` whose answer it returns, so a caller that moves the item
/// next has nothing on the disk between the check and the move
/// (`take_turn`). In order:
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
/// - check 4 for a launcher: the one-hop link into the root, or its
///   dangling launcher-only state (`route::probe`);
/// - last, the item's own `lstat`: there (`Missing`), the user's own
///   (check 3, `NotOwnedByYou`), and the kind the instructions describe --
///   a real directory for `Dir`, a link for `SymlinkIntoRoot`, so the item
///   is a link only where the recipe says so (check 4,
///   `NotWhatInstructionsExpect`).
fn check_item(
    look: &Look<'_>,
    kept: &[Kept],
    spec: &'static RemoveSpec,
    path: &Path,
) -> Result<ItemIdentity, Refusal> {
    use UninstallUnsafeReason::{
        Missing, NotOwnedByYou, NotWhatInstructionsExpect, OutsideHome, OverlapsKept, SharedFolder,
    };
    let refuse = |reason| Refusal::new(path, reason);
    let rel = spelled(spec.path);
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
    if spec.expect == Expect::SymlinkIntoRoot
        && !matches!(
            route::probe(look.job.recipe.route.kind, path, &look.root),
            Probe::Present { .. } | Probe::LauncherOnly
        )
    {
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
    let expected = match spec.expect {
        Expect::Dir => ItemKind::Dir,
        Expect::SymlinkIntoRoot => ItemKind::Symlink,
    };
    if identity.kind != expected {
        return Err(refuse(NotWhatInstructionsExpect));
    }
    Ok(identity)
}

/// Spec §6.3 on every path the recipe lists (check 5, the backup-file
/// patterns, arrives with step D's `backup_globs`), then the kept paths.
/// Check 2 (is it there?) goes first, because the others need something
/// to look at: a missing optional path is skipped, a missing program
/// directory of a launcher-only install is `AlreadyGone` (re-probed from
/// the disk now, ruling 4), anything else missing refuses. Then
/// `check_item`. Any failure refuses the whole list with nothing moved
/// (`AdapterError::UninstallUnsafe`, one of six reasons); an empty list is
/// a plain `Refused` (unreachable while the launcher is listed and the row
/// exists), and so is a home folder that cannot be resolved.
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
    for spec in job.remove {
        let path = route::expand(home, spec.path);
        // Check 2: is it there? `lstat`, so a dangling launcher counts.
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if spec.optional {
                    continue;
                }
                if launcher_only && path != look.launcher {
                    warnings.push(Warning::AlreadyGone {
                        path: shown(home, &path),
                    });
                    continue;
                }
                return Err(Refusal::new(&path, UninstallUnsafeReason::Missing).into_error(home));
            }
            // Unreadable (a permission error, a loop): not something the
            // instructions describe, and not something to move blind.
            Err(_) => {
                return Err(
                    Refusal::new(&path, UninstallUnsafeReason::NotWhatInstructionsExpect)
                        .into_error(home),
                )
            }
        }
        let identity =
            check_item(&look, &kept, spec, &path).map_err(|refusal| refusal.into_error(home))?;
        warnings.push(Warning::WillTrash {
            path: shown(home, &path),
            what: spec.what,
        });
        identities.push(identity);
        paths.push(path);
    }
    if paths.is_empty() {
        return Err(AdapterError::Refused(format!(
            "{}: nothing on the uninstall list is there to move",
            job.recipe.id
        )));
    }
    warnings.extend(kept.iter().map(|kept| Warning::WillKeep {
        path: shown(home, &kept.path),
        what: kept.spec.what,
    }));
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

/// Waits `gap` -- Finder's time to write the Put Back record of the item
/// just moved (`PUT_BACK_SETTLE`), already cut to what is left of the
/// budget -- or until `cancel` fires, whichever comes first; `true` when it
/// was the cancel. No wait at all for a zero gap (tests,
/// `StandaloneAdapter::with_trash_gap`, a spent budget).
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
/// gone from the disk: each was moved earlier in this run, was already
/// gone at the preview (`Warning::AlreadyGone`), or was never there (an
/// optional path). One whose `lstat` answers anything but "no such file"
/// -- it is there again, or cannot be looked at -- is returned, and the
/// run stops before the launcher (`Turn::Changed`): the launcher stays,
/// the row with it, and a fresh preview lists what came back. Read by
/// `take_turn`.
fn listed_path_back(look: &Look<'_>) -> Option<PathBuf> {
    let home = look.job.detected.home.as_path();
    look.job
        .remove
        .iter()
        .map(|spec| route::expand(home, spec.path))
        .filter(|path| *path != look.launcher)
        .find(|path| {
            !matches!(
                std::fs::symlink_metadata(path),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound
            )
        })
}

/// One item's turn, on tokio's blocking pool: every check again, from a
/// fresh look (the home folder, the kept paths, the item's folders, the
/// item itself), its identity compared with what the preview saw, and then
/// -- with nothing in between -- the move. At the launcher's turn, before
/// its own checks, every other listed path must be gone
/// (`listed_path_back`). `check_item`'s last step is the `lstat` that
/// produced `seen`, and `Trasher::trash` is handed that answer's kind
/// rather than looking again: Canager checks each item immediately before
/// moving it; a program running as you that swaps the item in that instant
/// could still race it (docs/what-we-run.md, "Moving files to the Trash").
/// That is the documented edge of the design: the system's call takes a
/// path, and what it finds there is what it moves.
fn take_turn(job: &Job, path: &Path, previewed: ItemIdentity, trasher: &dyn Trasher) -> Turn {
    let home = job.detected.home.as_path();
    let Some(spec) = job
        .remove
        .iter()
        .find(|spec| route::expand(home, spec.path) == path)
    else {
        return Turn::Changed(path.to_path_buf());
    };
    let Ok(look) = Look::new(job) else {
        return Turn::Changed(path.to_path_buf());
    };
    if path == look.launcher {
        if let Some(back) = listed_path_back(&look) {
            return Turn::Changed(back);
        }
    }
    let seen = match kept_places(&look).and_then(|kept| check_item(&look, &kept, spec, path)) {
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
/// each path in order: after the first, the pause (Cancel ends it, and it
/// never outlasts the budget); Cancel and the budget checked; one turn on
/// the blocking pool (`take_turn`: at the launcher's, that every other
/// listed path is gone; every check again, the identity against the
/// preview's, the move), awaited to its end even if Cancel arrives
/// meanwhile -- a move handed to the system finishes and is reported; one
/// log note. After the last move the same pause once more, before
/// `Succeeded` (a Cancel there only cuts it short: everything is moved).
///
/// `Succeeded` only when every path was moved; `Failed` with the system's
/// own words when it refused one (the launcher, last, is then still there,
/// and the row comes back as launcher-only); `CanagerFailed(Internal)`
/// when Canager could not ask the system at all (`TrashError::Unsupported`,
/// at the first item); `Unconfirmed` when cancelled or out of time between
/// items, or when a turn panicked -- `run_operation` then reads the disk
/// and reports what it finds. Never an `Err` for a state of the Mac: the
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
        if index > 0 && pause(pacing.settle.min(left()), &cancel).await {
            return Ok(Outcome::Unconfirmed);
        }
        if cancel.is_cancelled() || left().is_zero() {
            return Ok(Outcome::Unconfirmed);
        }
        let turn = {
            let (job, path, trasher) = (job.clone(), path.clone(), Arc::clone(trasher));
            tokio::task::spawn_blocking(move || take_turn(&job, &path, previewed, trasher.as_ref()))
                .await
        };
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
    use super::super::testing::{claude_layout, TempHome};
    use super::*;
    use crate::events::VecSink;
    use crate::model::{KeptWhat, RemovedWhat, UninstallUnsafeReason, Warning};
    use crate::trash::{MockTrasher, TrashError};
    use std::sync::Mutex as StdMutex;

    fn claude_lists() -> (&'static [RemoveSpec], &'static [KeepSpec]) {
        match &CLAUDE.uninstall {
            Some(Uninstall::Paths { remove, keep }) => (remove, keep),
            None => panic!("claude has a path list"),
        }
    }

    /// What `detect` would have written for `home`, as the user this test
    /// runs as (the files it makes are that user's).
    fn detected(home: &Path) -> Detected {
        Detected {
            home: home.to_path_buf(),
            euid: std::fs::metadata(home).expect("home metadata").uid(),
        }
    }

    fn claude_job(d: &Detected) -> Job {
        let (remove, keep) = claude_lists();
        Job {
            recipe: &CLAUDE,
            detected: d.clone(),
            remove,
            keep,
        }
    }

    /// A one-path list for a check's own test: `'static`, as a recipe's
    /// is (leaked; a test's lifetime is the process's).
    fn only(spec: RemoveSpec) -> &'static [RemoveSpec] {
        Box::leak(Box::new([spec]))
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
    fn test_plan_removal_refuses_a_path_reached_through_a_linked_folder_inside_home() {
        // Ruling 24, the review's second counterexample: with `~/.claude ->
        // ~/Documents`, an unrelated `~/Documents/downloads` would pass
        // every other check as Claude Code's cache. Every folder between the
        // home folder and a listed path must be a real folder -- inside the
        // home folder or not -- so it is refused, and so is a launcher whose
        // `~/.local/bin` is kept as a link to a dotfiles folder.
        let home = TempHome::new("removal-linked-cache-parent");
        let _layout = claude_layout(&home, "2.1.281");
        let documents = home.dir("Documents");
        home.dir("Documents/downloads");
        home.link(".claude", &documents);
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            (
                "~/.claude/downloads",
                UninstallUnsafeReason::NotWhatInstructionsExpect
            )
        );

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
    fn test_plan_removal_refuses_an_optional_path_of_the_wrong_shape() {
        // Ruling 1: in this step an `optional` path that exists but is not
        // what the list expects -- `~/.claude/downloads` as a link, or as a
        // file -- refuses the whole uninstall. Step D replaces this branch
        // with a skip and `WillKeep { NotOurs }`, and this test with its own.
        let home = TempHome::new("removal-optional-link");
        let _layout = claude_layout(&home, "2.1.281");
        let elsewhere = home.dir("elsewhere/downloads");
        home.link(".claude/downloads", &elsewhere);
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            (
                "~/.claude/downloads",
                UninstallUnsafeReason::NotWhatInstructionsExpect
            )
        );

        let home = TempHome::new("removal-optional-file");
        let _layout = claude_layout(&home, "2.1.281");
        home.file(".claude/downloads");
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            (
                "~/.claude/downloads",
                UninstallUnsafeReason::NotWhatInstructionsExpect
            )
        );
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

    fn no_gap() -> Pacing {
        Pacing {
            settle: Duration::ZERO,
            budget: Duration::from_secs(TIMEOUT_SECS),
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
    async fn test_execute_removal_stops_before_an_item_when_the_budget_is_spent() {
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
        };

        let (outcome, notes) = run(&job, &preview, &trasher, spent, CancellationToken::new()).await;

        assert_eq!(outcome, Outcome::Unconfirmed);
        assert!(mock.calls().is_empty());
        assert!(notes.is_empty());
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
        };
        let started = Instant::now();

        let (outcome, _) = run(&job, &preview, &trasher, tight, CancellationToken::new()).await;

        assert_eq!(outcome, Outcome::Unconfirmed);
        assert_eq!(mock.calls().len(), 1);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
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
            budget: Duration::from_secs(TIMEOUT_SECS),
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
            budget: Duration::from_secs(TIMEOUT_SECS),
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
