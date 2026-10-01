//! `Session::refresh`: detect every registered adapter's instances, then
//! fetch inventory + updates per instance, merging failures into `errors`
//! and `stale` without ever aborting the whole refresh. Split out of
//! `session/mod.rs` (Task 14); no behaviour change from what shipped there.

use super::{DetectOutcome, InventoryPreview, Session, Snapshot, SourceError};
use crate::adapters::{AdapterError, CheckOptions};
use crate::model::{
    InstalledArtifact, InstanceId, InstanceNote, InstanceStatus, ManagerInstance, ResourceLock,
    Unavailable,
};
use crate::runner::HostEnv;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio_util::task::AbortOnDropHandle;

/// One adapter's part in a round's detection: spawned, or skipped with
/// last round's instances because an operation holds one of them -- the
/// held ones unchanged, the others without last round's check notes
/// (`refresh_round`).
enum Detection {
    Spawned(AbortOnDropHandle<Vec<ManagerInstance>>),
    Skipped(Vec<ManagerInstance>),
}

/// One `refresh` call counted in `Session::refreshes_under_way` for as long
/// as it lives: from `enter`, as the call arrives, until the call returns
/// or is dropped. A guard rather than a decrement at the end, so a refresh
/// dropped mid-round -- which cancels its workers (`refresh_round`) --
/// does not leave `Session::busy` answering yes for the rest of the run.
struct UnderWay<'a>(&'a AtomicUsize);

impl<'a> UnderWay<'a> {
    fn enter(count: &'a AtomicUsize) -> UnderWay<'a> {
        count.fetch_add(1, Ordering::SeqCst);
        UnderWay(count)
    }
}

impl Drop for UnderWay<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Session {
    /// Detect every registered adapter's instances concurrently (Task 11: a
    /// slow or failing adapter's `detect()` must not delay any other
    /// adapter's), then inventory + check updates for each resulting
    /// instance concurrently (each under that instance's own resource lock
    /// -- see below). Bumps `generation` only when the resulting data
    /// actually differs from the previous snapshot. Per-instance and
    /// per-adapter failures land in `errors` and set `stale`; they never
    /// abort the whole refresh, and a failing instance's *previous*
    /// artifacts/updates are kept rather than dropped, so a transient
    /// failure never makes something the user installed appear to vanish.
    /// The one exception is a carried-forward update candidate that this
    /// round's own inventory positively disproves -- the package is no
    /// longer installed, or is already at the version the candidate
    /// targets. That is not old data, it is data this snapshot knows to
    /// be wrong, and it is dropped; see the `check_updates` error branch
    /// below. A mere disagreement over which version is installed is not
    /// disproof and never drops a row.
    /// Concurrent calls are serialised, and a call is only ever answered
    /// with a round that *started after the call arrived*: the snapshot it
    /// gets back never predates its own request. A call that arrives while
    /// an older round is running waits for that round to finish. Then, if a
    /// round that started after it arrived has committed since -- another
    /// caller waiting behind the same round ran it -- it returns that
    /// round's snapshot; otherwise it runs the next round itself. So every
    /// caller that arrives during one round shares a single follow-up round
    /// (the `last_committed_round` check in the body). See
    /// `rounds_started` / `last_committed_round` on `Session` for the two
    /// counters, and why neither can be `generation`.
    ///
    /// Merging into the round in flight regardless of when it started,
    /// which this used to do, handed a caller a snapshot read before
    /// whatever made it call: the refresh after an uninstall got a round
    /// whose brew worker had already listed the package, and the refresh
    /// after a background `brew update` ended got the round that reported
    /// it still running. An instance whose `status.unavailable` is set is
    /// skipped by the per-instance fetch -- that is a *reported state*, not
    /// a failed refresh (Task 11) -- but it keeps the previous round's
    /// artifacts and updates, so the "here is what Banager saw last time"
    /// copy its notice carries is true rather than a promise over an empty
    /// group. An adapter whose `detect()` itself panics or is cancelled is
    /// the same story a level up: the instances it reported last time are
    /// kept, marked unavailable, rather than being taken off the screen as
    /// if the manager had been uninstalled. A source that declines to read
    /// because its catalogue is being rewritten (`AdapterError::
    /// IndexUpdating`) keeps its previous rows through the same branches
    /// as a failed read, but is neither an error nor `stale`: it gets
    /// `InstanceNote::IndexUpdating` instead, and when the inventory is
    /// what declined, `check_updates` is not called at all.
    pub async fn refresh(
        self: &std::sync::Arc<Self>,
        env: &HostEnv,
        opts: &CheckOptions,
    ) -> Snapshot {
        self.refresh_with_round(env, opts).await.1
    }

    /// `refresh`, with the number of the round whose snapshot it hands
    /// back: the round this call ran, or the one it shared. Two calls
    /// answered with one round get the same number, and a later round a
    /// higher one. The shell records who asked for each round by this
    /// number (`auto_check::RoundLog`), through `refresh_recording`.
    ///
    /// Counted in `busy` from the moment it arrives until it returns or is
    /// dropped, the wait for the gate included.
    pub async fn refresh_with_round(
        self: &std::sync::Arc<Self>,
        env: &HostEnv,
        opts: &CheckOptions,
    ) -> (u64, Snapshot) {
        self.refresh_recording(env, opts, |_, _| {}, |_| {}).await
    }

    /// `refresh_with_round`, calling `record` with the number of the round
    /// that answers this call and its snapshot, while this call holds the
    /// refresh gate: when this call runs the round, before the round's
    /// snapshot is committed -- so before `snapshot()`, or any other call,
    /// can hand that round to anyone -- and when it shares a round another
    /// call ran, as it takes that round. The shell records who asked for
    /// each round this way (`ipc::refresh_for`), so that a reader of the
    /// snapshot never finds a round not yet recorded. Not called when this
    /// call is dropped before a round answers it.
    ///
    /// `preview` is called at most once, and only when this call runs a
    /// round before any round has committed -- the first since launch, or
    /// one after it that was dropped before it could commit: with what that
    /// round's sources listed (`InventoryPreview`), as soon as every source
    /// it asked has listed its packages or failed to, while their update
    /// checks still run, and before the round commits. Not when nothing was
    /// listed at all, and never with anything committed: `snapshot()` and
    /// every caller waiting on this round see the round's snapshot only
    /// once it has committed, whole. The shell sends it to the window
    /// (`UiEvent::InventoryPreview`); a call that shares a round another
    /// call ran never calls it.
    pub async fn refresh_recording(
        self: &std::sync::Arc<Self>,
        env: &HostEnv,
        opts: &CheckOptions,
        record: impl FnOnce(u64, &Snapshot) + Send,
        preview: impl FnOnce(InventoryPreview) + Send,
    ) -> (u64, Snapshot) {
        let _under_way = UnderWay::enter(&self.refreshes_under_way);
        // Read before queueing on the gate: any round numbered above this
        // began after this call arrived (`refresh_round` bumps the counter
        // under the gate, before it reads anything).
        let arrived_after = self.rounds_started.load(Ordering::SeqCst);
        let gate = self.refresh_gate.lock().await;
        // Holding the gate, so no round can commit between this check and
        // the clone. `last_committed_round`, not `rounds_started`: a round
        // that began after this call but was dropped before committing
        // left no snapshot behind to share.
        let committed = self.last_committed_round.load(Ordering::SeqCst);
        if committed > arrived_after {
            let snapshot = self.snapshot.lock().unwrap().clone();
            record(committed, &snapshot);
            return (committed, snapshot);
        }
        let (round, snapshot) = self.refresh_round(gate, env, opts, record, preview).await;
        // Committed, and the gate released: sizes are measured on a thread
        // of their own from here, outside the snapshot (`sizes.rs`).
        self.measure_sizes(round, &snapshot, env);
        self.note_kept_data_home(&env.home);
        self.note_needed_by_env(env);
        (round, snapshot)
    }

    /// One real refresh round, the body of `refresh`, handing back its own
    /// number with its snapshot. Takes the `refresh_gate` guard by value so
    /// no round can run without holding it, and holds it until the round
    /// has committed. `record` is `commit`'s to call; `preview` is called,
    /// if at all, before it (`refresh_recording`).
    async fn refresh_round(
        self: &std::sync::Arc<Self>,
        _gate: tokio::sync::MutexGuard<'_, ()>,
        env: &HostEnv,
        opts: &CheckOptions,
        record: impl FnOnce(u64, &Snapshot) + Send,
        preview: impl FnOnce(InventoryPreview) + Send,
    ) -> (u64, Snapshot) {
        // Numbered before anything is read, under the gate: a caller that
        // read `rounds_started` below this number arrived before this round
        // read a thing, and may share it (`refresh`'s check).
        let round = self.rounds_started.fetch_add(1, Ordering::SeqCst) + 1;
        let previous = self.snapshot.lock().unwrap().clone();
        // Owned copy (CheckOptions is Copy): each per-instance spawned task
        // below needs its own 'static value, and the caller's `&opts`
        // reference cannot outlive this function. Stamped with this
        // round's start, which brew compares with when an update failed
        // (`UpdateRecord::unreported_failure`).
        let opts = CheckOptions {
            round_started: Some(std::time::Instant::now()),
            ..*opts
        };

        // No root check here: it used to short-circuit the entire refresh
        // to `RefusedAsRoot` behind `BrewAdapter::refuses_as_root(env)`, but
        // that predicate is nothing but `env.euid == 0` -- a fact about
        // *brew*, not about npm, pipx, uv, pip, cargo or ollama, none of
        // which object to root. A root user with no Homebrew objection saw
        // every one of the other six adapters disabled and was told
        // Homebrew refused to run, whether or not Homebrew was even
        // installed. The decision now lives solely in
        // `BrewAdapter::detect`, which under root contributes an instance
        // marked `Unavailable::RefusesAsRoot` -- so the refusal is reported
        // against brew alone, with the action that resolves it, instead of
        // silently disabling the other six. Every adapter, brew included,
        // is simply fanned out to below like any other refresh.
        // Fanned out in adapter-id order, not `HashMap` order, and joined in
        // the same order below: `dedupe_instance_ids` keeps the *first*
        // instance claiming an id, and "first" has to mean the same thing
        // on every refresh and in every process, or which duplicate wins
        // (and so whose packages the user sees) would flip at random.
        //
        // Every task this function spawns -- here and in the per-instance
        // fan-out below -- is held in an `AbortOnDropHandle`, never a bare
        // `JoinHandle`. Dropping a bare handle *detaches* its task, so a
        // caller that dropped this future mid-flight (a `select!`, a
        // `timeout`, an aborted parent task) used to leave every worker
        // running on its own, still holding its instance's resource lock
        // for as long as its commands took -- minutes for brew, 30 s per
        // installed crate for cargo -- and blocking the next refresh or a
        // user's operation on that instance for all of it. Aborting on
        // drop makes that impossible rather than merely unreachable: the
        // worker's future is dropped at its next await, which drops its
        // `ResourceLockGuard` (releasing the lock) and the in-flight
        // `CommandRunner::run` future (which kills the command's process
        // group -- see `runner::real`'s `GroupedChild`).
        //
        // That kill lands wherever the command happens to be, so an adapter
        // whose command must not be stopped halfway has to keep it out of
        // this future. `brew update` is the one today: `BrewAdapter::
        // maybe_update` runs it in a detached task of its own, so dropping
        // a refresh stops the refresh waiting for it but never kills it.
        //
        // Deliberately not a `JoinSet`: that yields in completion order,
        // and both joins below depend on *fan-out* order -- detection for
        // `dedupe_instance_ids`' "first wins", the fan-out for pairing a
        // panicked task's `JoinError` with the instance whose rows it must
        // carry forward. An abort-on-drop handle awaits exactly like the
        // `JoinHandle` it wraps, so a refresh that runs to completion sees
        // identical join results; the only new `Err` it can produce is a
        // cancellation, and nothing but dropping this future cancels.
        //
        // An adapter one of whose instances an operation is holding right
        // now is not asked anything this round (phase 4 step E). Its
        // `detect` runs the tool's own binary -- and on a Mac with rustup,
        // `rustup self update` replaces that very binary while it holds
        // both `standalone-rustup` and the cargo instance's lock, and the
        // `cargo` the cargo adapter would run is the same binary in proxy
        // mode, which begins by deleting the updater the operation is
        // about to run. So: the held set is read once here; every adapter
        // with a previous-round instance whose id is a held lock keeps
        // last round's instances (no notice, and the skip itself adds
        // nothing to `stale`: nothing failed) -- the held ones unchanged,
        // and the per-instance loop below carries the held instances' rows
        // forward instead of waiting on their lock -- a refresh used to
        // wait out a `brew install` for minutes -- together with any error
        // last round recorded against them, which this round has not
        // retried. Instances of a skipped adapter that are not themselves
        // held are still inventoried and checked under their own lock, so
        // they are carried with the notes last round's *check* left on
        // them stripped (`InstanceNote::is_from_update_check`): this round's
        // check answers for them again, and `merge_instance_notes` appends
        // its notes to whatever the instance already carries, which for a
        // freshly detected instance is detect's notes alone. Carried whole,
        // such a sibling gained one more copy of the same note per refresh
        // for as long as the operation ran, and kept a note the check had
        // stopped reporting. Its detect-time notes stay: detect did not run
        // to write them again. The operation's own reading afterwards
        // (`run_operation`'s reconcile, under its locks) and the refresh
        // the front end runs when it finishes replace these rows. Read
        // once, so an operation submitted after this line may start while
        // a detect it would have skipped is running its one `--version`:
        // that window is the command's duration.
        let held = self.ops.locks_held();
        let mut under_operation: HashSet<InstanceId> = HashSet::new();
        let mut adapters: Vec<_> = self.adapters.values().collect();
        adapters.sort_by(|a, b| a.meta().id.cmp(&b.meta().id));
        let mut detections = Vec::with_capacity(adapters.len());
        for adapter in adapters {
            let adapter_id = adapter.meta().id.clone();
            let carried: Vec<ManagerInstance> = previous
                .instances
                .iter()
                .filter(|i| i.adapter_id == adapter_id)
                .cloned()
                .collect();
            let held_here: Vec<InstanceId> = carried
                .iter()
                .filter(|i| held.contains(&ResourceLock(i.id.clone())))
                .map(|i| i.id.clone())
                .collect();
            if !held_here.is_empty() {
                let carried = carried
                    .into_iter()
                    .map(|mut inst| {
                        if !held_here.contains(&inst.id) {
                            inst.status.notes.retain(|n| !n.is_from_update_check());
                        }
                        inst
                    })
                    .collect();
                under_operation.extend(held_here);
                detections.push((adapter_id, Detection::Skipped(carried)));
                continue;
            }
            // Cloned into the task because `tokio::spawn` needs a 'static
            // future: iterating `values()` by reference would tie it to
            // `&self`. (Written as an explicit clone rather than
            // `.values().cloned()` only because clippy's
            // `unnecessary_to_owned` misreads the latter here.)
            let adapter = adapter.clone();
            let env = env.clone();
            detections.push((
                adapter_id,
                Detection::Spawned(AbortOnDropHandle::new(tokio::spawn(async move {
                    adapter.detect(&env).await
                }))),
            ));
        }
        let mut instances = Vec::new();
        let mut detect_errors = Vec::new();
        for (adapter_id, detection) in detections {
            let handle = match detection {
                Detection::Skipped(carried) => {
                    instances.extend(carried);
                    continue;
                }
                Detection::Spawned(handle) => handle,
            };
            match handle.await {
                Ok(found) => instances.extend(found),
                Err(_join_err) => {
                    // A detection that panicked (or was cancelled) said
                    // nothing at all, which is not the same news as "this
                    // source is not installed on this Mac". Dropping its
                    // instances took every package the user has from that
                    // manager off the screen -- the same vanishing act
                    // the per-instance error paths below exist to
                    // prevent, one level up.
                    //
                    // They come back marked unavailable, which is both
                    // halves of the truth: the rows are last round's, and
                    // nothing may be *offered* on a source whose presence
                    // could not be confirmed (the gate in `issue_plan`,
                    // spec §2.5, reads exactly this). Being unavailable
                    // also routes them through the skip-and-carry branch
                    // below, so this refresh never asks a source nothing
                    // detected for an inventory. An instance that was
                    // already unavailable for a more specific reason
                    // keeps that reason.
                    instances.extend(
                        previous
                            .instances
                            .iter()
                            .filter(|i| i.adapter_id == adapter_id)
                            .map(|i| ManagerInstance {
                                status: InstanceStatus {
                                    unavailable: i
                                        .status
                                        .unavailable
                                        .or(Some(Unavailable::NotResponding)),
                                    notes: i.status.notes.clone(),
                                },
                                ..i.clone()
                            }),
                    );
                    detect_errors.push(SourceError {
                        instance_id: adapter_id,
                        message: "internal error detecting this source".to_string(),
                    });
                }
            }
        }
        // Before anything is keyed on `inst.id` -- the ops registry just
        // below, the resource locks, the note merge-back, every
        // carry-forward filter -- because all of them assume one instance
        // per id and would otherwise misattribute silently.
        let (mut instances, duplicate_errors) = dedupe_instance_ids(instances);
        detect_errors.extend(duplicate_errors);
        for inst in &instances {
            self.ops.register_instance(inst.clone());
        }
        let detect = if instances.is_empty() {
            DetectOutcome::Missing
        } else {
            DetectOutcome::Found
        };
        // Which copy of each command runs (`commands`): the folders on
        // `PATH` and the Homebrew and npm bin folders are read on the
        // blocking pool while the fan-out below runs, and the verdicts
        // made once its rows are in, before the commit.
        let path_known = self.login_path.load(Ordering::SeqCst);
        let commands = crate::commands::start_reading(
            env,
            &instances,
            path_known,
            &self.commands_in_flight,
            crate::commands::CommandBudget::default(),
        );

        // Seeded before the fan-out because a skipped instance contributes
        // its carried-forward rows -- and, when an operation holds it, last
        // round's errors against it -- from inside the loop below.
        let mut artifacts = Vec::new();
        let mut updates = Vec::new();
        let mut errors = detect_errors;
        let mut handles = Vec::with_capacity(instances.len());
        // The preview's channel (`refresh_recording`'s `preview`): each
        // instance's task sends what it listed, and drops its end, before
        // its update check starts, and the collector after the fan-out
        // reads until every end is dropped. Only on a round before any has
        // committed: the window then has nothing to show but the startup
        // placeholder, and what it waits on is the update checks -- every
        // registry asked over the network, `brew update`. A later round's
        // list is on screen already, with its updates.
        let previewing = previous.round == 0;
        let (preview_tx, mut preview_rx) =
            tokio::sync::mpsc::unbounded_channel::<(usize, Vec<InstalledArtifact>)>();
        for inst in instances.clone() {
            // Task 11: a source that already told us it is not answering is
            // a reported state, not a failed refresh, so it is never fanned
            // out to. It stays in `snapshot.instances` so the UI can render
            // its notice and offer to start it -- and it keeps whatever it
            // reported last time, exactly as the error paths below already
            // do. Dropping those rows is what made the unreachable notice's
            // "below is what Banager saw last time" a lie: a stopped Ollama
            // rendered a group header, that sentence, and no rows at all.
            // `issue_plan`'s gate (spec §2.5) is what stops those rows
            // offering an Uninstall button that could not possibly work.
            // And an instance an operation is holding (`under_operation`,
            // above): its rows are last round's, and this round does not
            // wait for the operation's lock to take them again. Its errors
            // are last round's too. This round re-read nothing about it,
            // so whatever failed for it then has not been retried -- and
            // dropping that record, as this used to, let a Retry pressed
            // during the operation clear the "may be out of date" banner
            // over data exactly as old as before. The skip adds no error
            // of its own: nothing failed. An unavailable instance that no
            // operation holds carries none: its state is on screen in its
            // own notice, which `Snapshot::stale` deliberately does not
            // duplicate.
            if inst.status.unavailable.is_some() || under_operation.contains(&inst.id) {
                artifacts.extend(
                    previous
                        .artifacts
                        .iter()
                        .filter(|a| a.key.instance_id == inst.id)
                        .cloned(),
                );
                updates.extend(
                    previous
                        .updates
                        .iter()
                        .filter(|u| u.key.instance_id == inst.id)
                        .cloned(),
                );
                if under_operation.contains(&inst.id) {
                    errors.extend(
                        previous
                            .errors
                            .iter()
                            .filter(|e| e.instance_id == inst.id)
                            .cloned(),
                    );
                }
                continue;
            }
            let Some(adapter) = self.adapters.get(&inst.adapter_id).cloned() else {
                continue;
            };
            let ops = self.ops.clone();
            let previous = previous.clone();
            // This task's place in the fan-out, which the preview lists by.
            let place = handles.len();
            let preview_tx = previewing.then(|| preview_tx.clone());
            // `opts` is `Copy`, so the `async move` block below captures its
            // own value rather than borrowing this function's.
            handles.push((
                inst.id.clone(),
                AbortOnDropHandle::new(tokio::spawn(async move {
                    let _lock = ops
                        .acquire_resource_lock(ResourceLock(inst.id.clone()))
                        .await;
                    let mut artifacts = Vec::new();
                    let mut updates = Vec::new();
                    let mut errors = Vec::new();
                    let mut stale = false;
                    // Whether `artifacts` below is this round's answer or
                    // last round's, which decides whether it may be used
                    // as evidence against a carried-forward update.
                    let mut inventory_confirmed = true;
                    // The source declined to read its catalogue because it
                    // is being rewritten (`AdapterError::IndexUpdating`:
                    // brew, while `brew update` runs). Not an error and not
                    // `stale` -- nothing failed -- but the same carry-forward
                    // as a failed read, and a note saying why.
                    let mut index_updating = false;
                    match adapter.inventory(&inst).await {
                        Ok(items) => artifacts.extend(items),
                        Err(e) => {
                            if matches!(e, AdapterError::IndexUpdating) {
                                index_updating = true;
                            } else {
                                errors.push(SourceError {
                                    instance_id: inst.id.clone(),
                                    message: e.to_string(),
                                });
                                stale = true;
                            }
                            inventory_confirmed = false;
                            artifacts.extend(
                                previous
                                    .artifacts
                                    .iter()
                                    .filter(|a| a.key.instance_id == inst.id)
                                    .cloned(),
                            );
                        }
                    }
                    // What this round's own reading listed, for the
                    // preview, before the update check -- the slow half --
                    // starts. A read that failed or declined sends nothing:
                    // what it carries forward is last round's, and the only
                    // round that previews has none. Sent or not, this
                    // task's end of the channel is dropped here, so the
                    // preview waits on no update check. A send fails only
                    // once the round itself has been dropped, with nobody
                    // left to read it.
                    if let Some(tx) = preview_tx {
                        if inventory_confirmed {
                            let _ = tx.send((place, artifacts.clone()));
                        }
                    }
                    // What this source said about *itself* while checking,
                    // as opposed to about any one package: `brew update`
                    // failing means "no updates" may simply be wrong, and
                    // no package row can carry that. The join below merges
                    // these back into `instances` by id, because
                    // `instances` was cloned *before* this fan-out --
                    // writing a note onto `inst` here would write it onto
                    // the clone and lose it, the same trap `SourceError`'s
                    // `instance_id` already fell into once.
                    let mut notes: Vec<InstanceNote> = Vec::new();
                    // Not asked at all when the inventory already said the
                    // catalogue is mid-rewrite: its update check reads the
                    // same catalogue. Handed to the `Err` arm instead, which
                    // keeps every previous candidate here because
                    // `inventory_confirmed` is false.
                    let checked = if index_updating {
                        Err(AdapterError::IndexUpdating)
                    } else {
                        adapter.check_updates(&inst, &opts).await
                    };
                    match checked {
                        Ok(outcome) => {
                            updates.extend(outcome.candidates);
                            notes.extend(outcome.notes);
                        }
                        Err(e) => {
                            if matches!(e, AdapterError::IndexUpdating) {
                                index_updating = true;
                            } else {
                                errors.push(SourceError {
                                    instance_id: inst.id.clone(),
                                    message: e.to_string(),
                                });
                                stale = true;
                            }
                            // Keeping the last round's candidates is the
                            // right instinct -- a failed check is not
                            // news that everything is up to date -- but
                            // not where this round's inventory positively
                            // disproves one. It succeeded here, so a
                            // *checkable* candidate is dropped when the
                            // inventory no longer lists its package (it
                            // was uninstalled) or lists it at the very
                            // version the candidate targets (it was
                            // already upgraded). An *uncheckable*
                            // candidate has no "already upgraded" reading
                            // to disprove it with: `uncheckable_candidate`
                            // sets `target == current` by construction
                            // (`UpdateCandidate` has no "unknown" target),
                            // so comparing its version to its own target
                            // would read "still installed, unchanged" --
                            // true of every uncheckable row that has not
                            // moved -- as "disproved", dropping every
                            // "Banager couldn't check" row the instant the
                            // package sat still. An uncheckable row is
                            // disproved only by absence; whether it is
                            // still unresolved is for the next successful
                            // check to say, not this filter.
                            //
                            // Nothing else counts as disproof, and in
                            // particular a *disagreement* between the two
                            // readings does not. This filter used to
                            // require `a.version == u.current` -- the
                            // candidate's own recollection of what was
                            // installed -- which is a third reading of
                            // the same fact and can differ from the
                            // inventory's for reasons that say nothing
                            // about whether an update is pending.
                            //
                            // This still joins on `a.key == u.key`, so it
                            // still depends on the inventory and the
                            // update check agreeing on that spelling --
                            // brew is the live example: it keys inventory
                            // by `full_name`/`full_token` and `brew
                            // outdated --json=v2` by the bare `name`. The
                            // adapter closes that gap by re-querying
                            // `brew info` inside `check_updates` to rename
                            // each candidate to match, but on that query's
                            // own failure it deliberately keeps the short
                            // name (`brew/mod.rs`'s
                            // `unwrap_or_default`), so a real pending
                            // update can still be dropped here across two
                            // independent failures in consecutive rounds
                            // (the rename succeeding one round, `outdated`
                            // failing the next, or vice versa). Narrow,
                            // but this join is not spelling-independent.
                            //
                            // When the inventory failed too, or declined
                            // with `IndexUpdating`, `artifacts` is itself
                            // last round's, carried forward by the branch
                            // above, so there is no fresh evidence to test
                            // against and everything is kept exactly as
                            // before. When only the update check declined
                            // (brew's own `brew update` outlasting its
                            // patience), the inventory was read before
                            // that update started, so it is fresh evidence
                            // and the filter applies as for any failure.
                            updates.extend(
                                previous
                                    .updates
                                    .iter()
                                    .filter(|u| u.key.instance_id == inst.id)
                                    .filter(|u| {
                                        !inventory_confirmed
                                            || artifacts.iter().any(|a| {
                                                a.key == u.key
                                                    && (!u.checkable || a.version != u.target)
                                            })
                                    })
                                    .cloned(),
                            );
                        }
                    }
                    if index_updating {
                        notes.push(InstanceNote::IndexUpdating);
                    }
                    (artifacts, updates, errors, stale, notes)
                })),
            ));
        }

        // The preview, before the join below: every task has listed its
        // packages, or failed to, once the last end of the channel is
        // dropped -- this function's own first. A task that panics or is
        // aborted drops its end as it goes, so this waits on no task
        // longer than the join would. In fan-out order, whichever source
        // answered first, so one Mac previews one list. Handed to
        // `preview` and nowhere else: it is not committed, and nothing in
        // this session reads it (`InventoryPreview`).
        drop(preview_tx);
        if previewing && !handles.is_empty() {
            let mut answers = Vec::with_capacity(handles.len());
            while let Some(answer) = preview_rx.recv().await {
                answers.push(answer);
            }
            answers.sort_by_key(|(place, _)| *place);
            let mut listed: Vec<InstalledArtifact> =
                answers.into_iter().flat_map(|(_, items)| items).collect();
            // Which AI coding tool each row is, as the commit below will
            // say (`families::assign`), so the AI Tools filter has
            // something to show while the update checks run. Which copy
            // of a command runs is not judged yet: that needs the whole
            // round (`commands::finish`), so `commands` stays empty here.
            crate::families::assign(&instances, &mut listed);
            // Nothing listed is nothing to show: the startup placeholder
            // says as much until the round commits.
            if !listed.is_empty() {
                preview(InventoryPreview {
                    round,
                    instances: instances.clone(),
                    artifacts: listed,
                });
            }
        }

        // Exactly "a refresh attempt failed" -- this round's, or, for an
        // instance an operation holds, the last one that reached it (the
        // carry-forward above) -- which is all any reader does with it:
        // `SnapshotStatus` turns it into the one page-wide "some of this
        // may be out of date, try again" banner, over a count of `errors`.
        //
        // It briefly also meant "or some source is unavailable". Nothing
        // could observe that half -- the banner's own condition ruled it
        // out -- and widening it for its own sake would have been worse
        // than useless: an unavailable source already says so itself, in
        // its own words, with its own action, on both pages, through
        // `sourceNoticesFor`. A second page-wide banner saying something
        // vaguer about the same fact is noise, and its copy ("the last
        // refresh couldn't finish for {count} sources") would have been
        // false with a count of zero.
        let mut stale = !errors.is_empty();
        for (instance_id, handle) in handles {
            match handle.await {
                Ok((a, u, e, s, notes)) => {
                    artifacts.extend(a);
                    updates.extend(u);
                    errors.extend(e);
                    stale = stale || s;
                    merge_instance_notes(&mut instances, &instance_id, notes);
                }
                Err(_join_err) => {
                    // The task panicked or was cancelled, so it returned
                    // nothing -- including whatever it had already
                    // fetched successfully before it died. Appending
                    // nothing made every package from this source
                    // disappear until some later refresh happened to
                    // succeed, which is exactly what this function's
                    // documented guarantee rules out. The previous rows
                    // come forward here, once: this branch is mutually
                    // exclusive with the `Ok` arm above, and an
                    // unavailable instance never reaches the fan-out at
                    // all, so nothing else has carried them already.
                    artifacts.extend(
                        previous
                            .artifacts
                            .iter()
                            .filter(|a| a.key.instance_id == instance_id)
                            .cloned(),
                    );
                    updates.extend(
                        previous
                            .updates
                            .iter()
                            .filter(|u| u.key.instance_id == instance_id)
                            .cloned(),
                    );
                    errors.push(SourceError {
                        instance_id,
                        message: "internal error refreshing this instance".to_string(),
                    });
                    stale = true;
                }
            }
        }

        // The rows this round's inventories listed, judged against this
        // round's reading; a carried row keeps its own (`commands::finish`).
        // What it made of `PATH`'s folders is kept for the window's tool
        // setup check (`Session::path_folders`): this round's, or none.
        let path_folders = crate::commands::finish(
            commands,
            &instances,
            &mut artifacts,
            &env.home,
            path_known,
            &self.commands_in_flight,
            crate::commands::CommandBudget::default(),
        )
        .await;
        *self.path_folders.lock().unwrap() = path_folders;

        // Stamped because a refresh *ran*, not because it came back
        // perfect. Gated on `stale`, a Mac with one permanently unavailable
        // source carried `refreshed_at: None` for the rest of its life, and
        // `SnapshotStatus` reads a null timestamp as "Banager has never
        // finished a check" -- six healthy sources' worth of real data
        // described as no data at all. What "some of this may be old" means
        // is `stale`, and that is the flag that carries it.
        // `Snapshot::empty()` still has `refreshed_at: None`, which is what
        // keeps the startup branch in `SnapshotStatus` working: nothing but
        // an uncommitted snapshot can have a null timestamp now.
        let refreshed_at = Some(self.now());
        // Which AI coding tool each artifact is a copy of: once, here,
        // over every instance's rows -- this round's and the ones carried
        // forward alike -- rather than in each adapter's inventory.
        crate::families::assign(&instances, &mut artifacts);
        let candidate = Snapshot {
            generation: previous.generation,
            round,
            detect,
            instances,
            artifacts,
            updates,
            refreshed_at,
            stale,
            errors,
            // The shell's to fill in (`Snapshot::next_auto_check_at`).
            next_auto_check_at: None,
        };
        (round, self.commit(round, previous, candidate, record))
    }

    /// Assigns the real generation number (bumping only on a content
    /// change), hands the result to `record` (`refresh_recording`), stores
    /// it as the current snapshot, and records `round` as the last one
    /// committed regardless of whether `generation` moved (M5 in the design
    /// review -- see `last_committed_round`'s field doc).
    fn commit(
        &self,
        round: u64,
        previous: Snapshot,
        mut candidate: Snapshot,
        record: impl FnOnce(u64, &Snapshot),
    ) -> Snapshot {
        if !previous.same_content(&candidate) {
            candidate.generation = previous.generation + 1;
        }
        record(round, &candidate);
        *self.snapshot.lock().unwrap() = candidate.clone();
        self.last_committed_round.store(round, Ordering::SeqCst);
        candidate
    }
}

/// Keep the first instance for each id and drop every later one, with a
/// `SourceError` for each drop naming both adapters involved.
///
/// Every step after detection keys on `ManagerInstance.id` and assumes it
/// is unique: `merge_instance_notes` writes onto the *first* match, the
/// carry-forward filters pull `previous` rows by id (so two instances
/// sharing one would each carry the same rows, duplicating them), and
/// `OperationManager::register_instance` overwrites by id (so a plan for
/// one could run against the other's executable). `model::instance_id`
/// makes a collision *across* adapters impossible, but a collision within
/// one adapter is still possible -- pipx and uv have constant ids, so a
/// second instance from either would repeat it -- and no step downstream
/// would notice. Refusing it here, once, turns that silent
/// misattribution into a visible error.
///
/// Dropped rather than merged or renamed: there is no correct owner to
/// merge into, and a synthesised id would be a new source with no history
/// the next round could not reproduce. The instance that is kept is the
/// one detected first, in the adapter-id order `refresh` fans out in, then
/// the order its adapter returned them, so the choice is stable from one
/// refresh to the next and the same error does not bump `generation`.
///
/// `stale` follows from the error through the usual `!errors.is_empty()`:
/// something the detector reported is not on screen, which is exactly
/// what that banner means.
fn dedupe_instance_ids(
    instances: Vec<ManagerInstance>,
) -> (Vec<ManagerInstance>, Vec<SourceError>) {
    let mut kept: Vec<ManagerInstance> = Vec::with_capacity(instances.len());
    let mut errors = Vec::new();
    for inst in instances {
        match kept.iter().find(|k| k.id == inst.id) {
            Some(first) => errors.push(SourceError {
                instance_id: inst.id.clone(),
                message: format!(
                    "two sources reported the same instance id {:?} ({} and {}); \
                     showing only the first",
                    inst.id, first.adapter_id, inst.adapter_id
                ),
            }),
            None => kept.push(inst),
        }
    }
    (kept, errors)
}

/// Merge notes a fan-out task produced back onto the instance it belongs
/// to, matched by id.
///
/// Appends, so what the instance carries at this point has to be detect's
/// notes alone: that is what a fresh detect delivers, and what
/// `refresh_round`'s skip leaves on a carried sibling by stripping last
/// round's check notes first.
///
/// `refresh` clones `instances` before spawning, so every per-instance task
/// holds its own copy and anything it learns about the source has to travel
/// back by id or be lost. An id with no matching instance is dropped on
/// purpose: instances only ever shrink between the clone and the join if
/// something removed one, and inventing an entry for it would put a source
/// in the snapshot that no `detect()` reported.
fn merge_instance_notes(
    instances: &mut [ManagerInstance],
    instance_id: &str,
    notes: Vec<InstanceNote>,
) {
    if notes.is_empty() {
        return;
    }
    if let Some(inst) = instances.iter_mut().find(|i| i.id == instance_id) {
        inst.status.notes.extend(notes);
    }
}

#[cfg(test)]
mod tests {
    use crate::adapters::brew::BrewAdapter;
    use crate::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
    use crate::events::{EventSink, OpId, VecSink};
    use crate::model::{
        ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, InstanceId, InstanceNote,
        ManagerInstance, OpKind, OpRequest, OpStatus, Outcome, Plan, Reconciled, SearchHit,
        Unavailable, UpdateCandidate, UpdateChannel,
    };
    use crate::runner::{CommandOutput, HostEnv, MockRunner};
    use crate::session::test_support::{make_instance, non_root_env, root_env};
    use crate::session::{DetectOutcome, InventoryPreview, Session, Snapshot};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;

    /// The Homebrew layout the two root tests below run against: an Apple
    /// Silicon install, and nothing at `/usr/local`. Handed to
    /// `BrewAdapter::with_path_exists_fn` so these tests assert the same
    /// thing on an Intel Mac, on a machine with no Homebrew, and on one
    /// with both prefixes -- they are about brew's root policy, not about
    /// what the machine running them happens to have installed.
    fn apple_silicon_layout(path: &Path) -> bool {
        path == Path::new("/opt/homebrew/bin/brew")
    }

    struct FakeState {
        instances: Vec<ManagerInstance>,
        artifacts: HashMap<InstanceId, Vec<InstalledArtifact>>,
        updates: HashMap<InstanceId, Vec<UpdateCandidate>>,
        /// What `check_updates` reports *about the source itself*, as
        /// opposed to about a package -- the `CheckOutcome.notes` channel
        /// brew fills when `brew update` failed.
        notes: HashMap<InstanceId, Vec<InstanceNote>>,
        failing: Vec<InstanceId>,
        /// Instances whose `check_updates` fails once, the same one-shot
        /// shape as `failing` -- the half of a per-instance fetch that can
        /// fail while `inventory` succeeds.
        failing_updates: Vec<InstanceId>,
        /// Instances whose `inventory` / `check_updates` *panics* rather
        /// than returning an error: the per-instance task then never
        /// returns a value at all and `handle.await` yields a `JoinError`,
        /// which is a different code path from any `Err` an adapter can
        /// return. One-shot, like `failing`.
        panicking_inventory: Vec<InstanceId>,
        panicking_updates: Vec<InstanceId>,
        /// Whether `detect` panics, losing the whole adapter's answer.
        panicking_detect: bool,
        detect_delay: Duration,
        /// How long every `inventory` sleeps before answering: a slow
        /// source (cargo's per-crate lookups, pipx's PyPI calls) that keeps
        /// a refresh in flight after a quick one has returned.
        inventory_delay: Duration,
        detect_calls: usize,
        block_execute: bool,
        inventory_calls: Vec<InstanceId>,
        /// Instances whose `inventory` never returns: it records that it
        /// started, then waits forever. Stands in for a real adapter's
        /// long-running command (brew's is minutes), so a test can drop a
        /// refresh while a worker is inside it, holding the lock.
        blocking_inventory: Vec<InstanceId>,
        /// How many blocked `inventory` calls have started.
        inventory_blocked: usize,
        /// How many blocked `inventory` futures have been *dropped* -- which
        /// only happens if something cancelled the worker running them.
        inventory_dropped: usize,
        /// When set, every `check_updates` waits for a permit here before
        /// it answers (and gives it back): the slow half of a round held
        /// open, so a test can look at what a round hands out before it
        /// commits. One `add_permits(1)` lets every check through.
        check_gate: Option<Arc<tokio::sync::Semaphore>>,
    }

    /// Counts its own drop into `FakeState::inventory_dropped`: the only
    /// way a blocked `inventory` future can end is by being dropped, so
    /// this is the observable proof its worker was cancelled.
    struct DropSentinel(Arc<Mutex<FakeState>>);

    impl Drop for DropSentinel {
        fn drop(&mut self) {
            self.0.lock().unwrap().inventory_dropped += 1;
        }
    }

    struct FakeAdapter {
        meta: AdapterMeta,
        state: Arc<Mutex<FakeState>>,
    }

    impl FakeAdapter {
        fn new(id: &str) -> (Arc<FakeAdapter>, Arc<Mutex<FakeState>>) {
            let state = Arc::new(Mutex::new(FakeState {
                instances: Vec::new(),
                artifacts: HashMap::new(),
                updates: HashMap::new(),
                notes: HashMap::new(),
                failing: Vec::new(),
                failing_updates: Vec::new(),
                panicking_inventory: Vec::new(),
                panicking_updates: Vec::new(),
                panicking_detect: false,
                detect_delay: Duration::from_millis(0),
                inventory_delay: Duration::ZERO,
                detect_calls: 0,
                block_execute: false,
                inventory_calls: Vec::new(),
                blocking_inventory: Vec::new(),
                inventory_blocked: 0,
                inventory_dropped: 0,
                check_gate: None,
            }));
            let adapter = Arc::new(FakeAdapter {
                meta: crate::session::test_support::fake_adapter_meta(id),
                state: state.clone(),
            });
            (adapter, state)
        }
    }

    #[async_trait]
    impl Adapter for FakeAdapter {
        fn meta(&self) -> &AdapterMeta {
            &self.meta
        }

        async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
            let delay = {
                let mut s = self.state.lock().unwrap();
                s.detect_calls += 1;
                s.detect_delay
            };
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            // Read out, then panic with the guard already dropped: a
            // panic while holding `state` poisons it, and every *other*
            // instance's task would then panic on `lock().unwrap()` too,
            // turning one deliberate failure into an adapter-wide one.
            let panicking = self.state.lock().unwrap().panicking_detect;
            assert!(!panicking, "{} detect panicked on purpose", self.meta.id);
            self.state.lock().unwrap().instances.clone()
        }

        async fn inventory(
            &self,
            inst: &ManagerInstance,
        ) -> Result<Vec<InstalledArtifact>, AdapterError> {
            // Same reason as `detect` above: decide under the lock, panic
            // after dropping it, so one instance's deliberate panic does
            // not poison the fixture for every other instance.
            let panicking = {
                let mut s = self.state.lock().unwrap();
                s.inventory_calls.push(inst.id.clone());
                match s.panicking_inventory.iter().position(|id| id == &inst.id) {
                    Some(pos) => {
                        s.panicking_inventory.remove(pos);
                        true
                    }
                    None => false,
                }
            };
            assert!(!panicking, "{} inventory panicked on purpose", inst.id);
            let blocking = {
                let mut s = self.state.lock().unwrap();
                let blocking = s.blocking_inventory.contains(&inst.id);
                if blocking {
                    s.inventory_blocked += 1;
                }
                blocking
            };
            if blocking {
                let _sentinel = DropSentinel(self.state.clone());
                std::future::pending::<()>().await;
            }
            let delay = self.state.lock().unwrap().inventory_delay;
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            let mut s = self.state.lock().unwrap();
            if let Some(pos) = s.failing.iter().position(|id| id == &inst.id) {
                s.failing.remove(pos);
                return Err(AdapterError::CommandFailed {
                    code: Some(1),
                    stderr: format!("{} inventory failed", inst.id),
                });
            }
            Ok(s.artifacts.get(&inst.id).cloned().unwrap_or_default())
        }

        async fn check_updates(
            &self,
            inst: &ManagerInstance,
            _opts: &CheckOptions,
        ) -> Result<CheckOutcome, AdapterError> {
            let gate = self.state.lock().unwrap().check_gate.clone();
            if let Some(gate) = gate {
                let _permit = gate
                    .acquire()
                    .await
                    .expect("the check gate is never closed");
            }
            let panicking = {
                let mut s = self.state.lock().unwrap();
                match s.panicking_updates.iter().position(|id| id == &inst.id) {
                    Some(pos) => {
                        s.panicking_updates.remove(pos);
                        true
                    }
                    None => false,
                }
            };
            assert!(!panicking, "{} update check panicked on purpose", inst.id);
            let mut s = self.state.lock().unwrap();
            if let Some(pos) = s.failing_updates.iter().position(|id| id == &inst.id) {
                s.failing_updates.remove(pos);
                return Err(AdapterError::CommandFailed {
                    code: Some(1),
                    stderr: format!("{} update check failed", inst.id),
                });
            }
            Ok(CheckOutcome {
                candidates: s.updates.get(&inst.id).cloned().unwrap_or_default(),
                notes: s.notes.get(&inst.id).cloned().unwrap_or_default(),
            })
        }

        async fn search(
            &self,
            _inst: &ManagerInstance,
            _query: &str,
        ) -> Result<Vec<SearchHit>, AdapterError> {
            Ok(Vec::new())
        }

        async fn plan(
            &self,
            inst: &ManagerInstance,
            req: &OpRequest,
        ) -> Result<Plan, AdapterError> {
            Ok(crate::session::test_support::fake_plan(inst, req))
        }

        async fn execute(
            &self,
            _plan: &Plan,
            _sink: Arc<dyn EventSink>,
            _op_id: OpId,
            cancel: CancellationToken,
        ) -> Result<Outcome, AdapterError> {
            let block = self.state.lock().unwrap().block_execute;
            if block {
                cancel.cancelled().await;
                Ok(Outcome::Unconfirmed)
            } else {
                Ok(Outcome::Succeeded)
            }
        }

        async fn reconcile(
            &self,
            _inst: &ManagerInstance,
            _key: &crate::model::ArtifactKey,
        ) -> Result<Reconciled, AdapterError> {
            Ok(crate::session::test_support::fake_reconciled())
        }
    }

    fn make_artifact(instance_id: &str, name: &str) -> InstalledArtifact {
        make_artifact_at(instance_id, name, "1.0")
    }

    /// `make_artifact` at a stated version -- for the one thing an update
    /// candidate assumes about the package it offers to update.
    fn make_artifact_at(instance_id: &str, name: &str, version: &str) -> InstalledArtifact {
        InstalledArtifact {
            key: crate::model::ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Formula,
                name: name.to_string(),
            },
            display_name: name.to_string(),
            version: version.to_string(),
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

    #[tokio::test]
    async fn test_refresh_populates_snapshot_from_adapter() {
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let snapshot = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert_eq!(snapshot.detect, DetectOutcome::Found);
        assert_eq!(snapshot.instances.len(), 1);
        assert_eq!(snapshot.artifacts.len(), 1);
        assert_eq!(snapshot.artifacts[0].display_name, "jq");
        assert!(!snapshot.stale);
        assert!(snapshot.errors.is_empty());
        assert!(snapshot.refreshed_at.is_some());
        assert_eq!(snapshot.generation, 1);
    }

    /// Every artifact in a committed snapshot says which AI coding tool it
    /// is, set once where the round puts the snapshot together
    /// (`families::assign`) from the adapter its instance belongs to.
    #[tokio::test]
    async fn test_refresh_tags_each_artifact_with_its_ai_tool_family() {
        let (adapter, state) = FakeAdapter::new("brew");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("brew", "brew:/opt/homebrew")];
            s.artifacts.insert(
                "brew:/opt/homebrew".to_string(),
                vec![
                    make_artifact("brew:/opt/homebrew", "ollama"),
                    make_artifact("brew:/opt/homebrew", "jq"),
                ],
            );
        }
        let session = Session::with_adapters(Arc::new(VecSink::new()), vec![adapter], None);
        let snapshot = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        let families: Vec<_> = snapshot
            .artifacts
            .iter()
            .map(|a| (a.key.name.as_str(), a.facts.family.as_deref()))
            .collect();
        assert_eq!(families, [("ollama", Some("ollama")), ("jq", None)]);
    }

    /// The root decision lives entirely in `BrewAdapter::detect` now (spec:
    /// one adapter's root policy must not disable the other six). Session
    /// no longer special-cases root at all -- it just runs every adapter's
    /// `detect()` concurrently, exactly as for any other host state, and
    /// brew alone comes back unavailable. A real `BrewAdapter` is used
    /// (over a `MockRunner`, the same pattern `adapters/brew/mod.rs`'s own
    /// detect tests use) rather than a second `FakeAdapter`, because the
    /// whole point under test is brew's *own* root check, not a stand-in
    /// for it.
    #[tokio::test]
    async fn test_refresh_as_root_reports_brew_unavailable_and_leaves_the_others_alone() {
        let runner = Arc::new(MockRunner::new());
        // The Homebrew layout is pinned rather than read off whatever Mac
        // is running the suite: an Apple Silicon install and nothing else.
        // The canned `--version` would only be used if brew's root refusal
        // failed to stop `detect` short of asking the runner at all.
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "Homebrew 7.0.3\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let brew = Arc::new(BrewAdapter::new(runner).with_path_exists_fn(apple_silicon_layout));
        let (fake, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![brew, fake], None);

        let snapshot = session.refresh(&root_env(), &CheckOptions::default()).await;

        assert_eq!(
            snapshot.detect,
            DetectOutcome::Found,
            "the fake adapter still found an instance, so this is an ordinary Found, not a whole-app refusal"
        );
        let brew_instance = snapshot
            .instances
            .iter()
            .find(|i| i.adapter_id == "brew")
            .expect("brew is installed here, and saying otherwise is the whole bug");
        assert_eq!(
            brew_instance.status.unavailable,
            Some(crate::model::Unavailable::RefusesAsRoot),
            "brew is listed with its reason, not dropped"
        );
        assert!(
            snapshot.instances.iter().any(|i| i.id == "fake:1"),
            "an adapter with no root objection must detect normally, got {:?}",
            snapshot.instances
        );
        assert!(snapshot.artifacts.iter().any(|a| a.key.name == "jq"));
    }

    /// When brew is the *only* registered adapter, running as root is
    /// still not a whole-app state -- but it is not "nothing detected"
    /// either. `DetectOutcome::Missing` is what `SnapshotStatus` renders
    /// as "none of them are set up on this Mac yet", which on a Mac with
    /// Homebrew installed is simply false; the source is `Found`, and
    /// unavailable with a reason that names what to do.
    #[tokio::test]
    async fn test_refresh_as_root_with_only_brew_registered_still_finds_it() {
        let runner = Arc::new(MockRunner::new());
        let brew = Arc::new(BrewAdapter::new(runner).with_path_exists_fn(apple_silicon_layout));
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![brew], None);

        let snapshot = session.refresh(&root_env(), &CheckOptions::default()).await;

        assert_eq!(snapshot.detect, DetectOutcome::Found);
        assert_eq!(snapshot.instances.len(), 1, "got {:?}", snapshot.instances);
        assert_eq!(
            snapshot.instances[0].status.unavailable,
            Some(crate::model::Unavailable::RefusesAsRoot)
        );
        assert!(
            snapshot.errors.is_empty(),
            "a source that reported why it cannot answer is not a refresh error"
        );
        assert!(
            snapshot.refreshed_at.is_some(),
            "a refresh that ran is stamped even when every source came back empty"
        );
    }

    #[tokio::test]
    async fn test_refresh_with_no_instances_yields_missing() {
        let (adapter, _state) = FakeAdapter::new("fake");
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let snapshot = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert_eq!(snapshot.detect, DetectOutcome::Missing);
    }

    #[tokio::test]
    async fn test_refresh_keeps_previous_data_and_flags_stale_on_per_instance_failure() {
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![
                make_instance("fake", "fake:1"),
                make_instance("fake", "fake:2"),
            ];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
            s.artifacts
                .insert("fake:2".to_string(), vec![make_artifact("fake:2", "wget")]);
        }
        let sink = Arc::new(VecSink::new());
        // A pinned, *moving* clock: the second refresh must be able to
        // stamp a different timestamp from the first, which a real
        // wall clock only does if the two land either side of a second.
        static NOW: AtomicI64 = AtomicI64::new(1_700_000_000);
        let session = Session::with_adapters(
            sink,
            vec![adapter],
            Some(|| NOW.fetch_add(100, Ordering::SeqCst)),
        );
        let first = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert_eq!(first.artifacts.len(), 2);
        assert!(!first.stale);
        let first_refreshed_at = first.refreshed_at;

        state.lock().unwrap().failing.push("fake:1".to_string());
        let second = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert!(second.stale);
        assert_eq!(second.errors.len(), 1);
        assert_eq!(second.errors[0].instance_id, "fake:1");
        assert!(second.artifacts.iter().any(|a| a.key.name == "jq"));
        assert!(second.artifacts.iter().any(|a| a.key.name == "wget"));
        assert!(
            second.refreshed_at > first_refreshed_at,
            "a refresh that ran is stamped even when a source failed (spec §2.4-1): \
             {:?} should be newer than {:?}",
            second.refreshed_at,
            first_refreshed_at
        );
    }

    #[tokio::test]
    async fn test_refresh_generation_unchanged_when_nothing_changed() {
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let first = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        let second = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert_eq!(
            first.generation, second.generation,
            "identical data must not bump the generation"
        );

        state
            .lock()
            .unwrap()
            .artifacts
            .get_mut("fake:1")
            .unwrap()
            .push(make_artifact("fake:1", "wget"));
        let third = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert!(
            third.generation > second.generation,
            "new data must bump the generation"
        );
    }

    /// Starts a refresh that is still in its (slowed) detection when this
    /// returns, then `callers` more that all arrive while it is in flight,
    /// and hands back the first one's snapshot and the others'.
    async fn refresh_behind_one_in_flight(
        session: &Arc<Session>,
        callers: usize,
    ) -> (Snapshot, Vec<Snapshot>) {
        let first = {
            let session = session.clone();
            tokio::spawn(async move {
                session
                    .refresh(&non_root_env(), &CheckOptions::default())
                    .await
            })
        };
        // Well inside the 100 ms detection: the first round holds the gate.
        tokio::time::sleep(Duration::from_millis(20)).await;
        let behind: Vec<_> = (0..callers)
            .map(|_| {
                let session = session.clone();
                tokio::spawn(async move {
                    session
                        .refresh(&non_root_env(), &CheckOptions::default())
                        .await
                })
            })
            .collect();
        let first = first.await.expect("first task");
        let mut rest = Vec::new();
        for handle in behind {
            rest.push(handle.await.expect("caller task"));
        }
        (first, rest)
    }

    #[tokio::test]
    async fn test_concurrent_refresh_calls_are_coalesced() {
        // Three calls arrive together while a round is in flight. None may
        // be answered with that round -- it started before they arrived --
        // and they must not run one round each either: they share exactly
        // one follow-up round. Before the merge rule changed, all three
        // were handed the in-flight round's snapshot (one round in total).
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.detect_delay = Duration::from_millis(100);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);

        let (_first, rest) = refresh_behind_one_in_flight(&session, 3).await;

        assert_eq!(
            state.lock().unwrap().detect_calls,
            2,
            "the round in flight, then exactly one round shared by the three \
             calls that arrived during it"
        );
        assert!(
            rest.windows(2).all(|w| w[0] == w[1]),
            "the three share one snapshot: {rest:?}"
        );
    }

    #[tokio::test]
    async fn test_concurrent_refresh_calls_with_unchanged_content_are_still_coalesced() {
        // M5: the shared round finds nothing new, so `generation` does not
        // move. The callers behind it must still see that it committed
        // (`last_committed_round`), or each would run a round of its own.
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        let calls_before = state.lock().unwrap().detect_calls;

        state.lock().unwrap().detect_delay = Duration::from_millis(100);
        let (first, rest) = refresh_behind_one_in_flight(&session, 3).await;

        assert!(
            rest.iter().all(|s| s.generation == first.generation),
            "precondition: nothing changed, so no round moved the generation"
        );
        assert_eq!(
            state.lock().unwrap().detect_calls,
            calls_before + 2,
            "the round in flight and one shared round, even though neither \
             moved the generation"
        );
    }

    #[tokio::test]
    async fn test_calls_that_arrive_while_no_round_has_started_share_the_first_one() {
        // The gate is held (standing in for any moment before a round
        // numbers itself) while three calls arrive. The first to get it
        // runs a round that starts after all three arrived, so the other
        // two share it rather than running their own.
        let (adapter, state) = FakeAdapter::new("fake");
        state.lock().unwrap().instances = vec![make_instance("fake", "fake:1")];
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);

        let held = session.refresh_gate.lock().await;
        let callers: Vec<_> = (0..3)
            .map(|_| {
                let session = session.clone();
                tokio::spawn(async move {
                    session
                        .refresh(&non_root_env(), &CheckOptions::default())
                        .await
                })
            })
            .collect();
        // Let all three read `rounds_started` and queue on the gate.
        tokio::time::sleep(Duration::from_millis(20)).await;
        drop(held);
        let mut snapshots = Vec::new();
        for handle in callers {
            snapshots.push(handle.await.expect("caller task"));
        }

        assert_eq!(state.lock().unwrap().detect_calls, 1);
        assert!(snapshots.windows(2).all(|w| w[0] == w[1]));
    }

    #[tokio::test]
    async fn test_a_change_made_while_an_older_round_is_in_flight_reaches_the_next_refresh() {
        // F2 in the final concurrency review. A round is in flight: the
        // fast source's worker has read its list and released its lock,
        // the slow source is still going. An uninstall on the fast source
        // then runs and finishes (the change to `artifacts` below stands in
        // for it: the operation's own path takes the instance's resource
        // lock, which that worker no longer holds, and not `refresh_gate`),
        // and its follow-up refresh arrives. That refresh used to be
        // merged into the round in flight and handed back its snapshot,
        // read before the uninstall -- jq still listed, and nothing left
        // to correct it.
        let (fast, fast_state) = FakeAdapter::new("fast");
        {
            let mut s = fast_state.lock().unwrap();
            s.instances = vec![make_instance("fast", "fast:1")];
            s.artifacts.insert(
                "fast:1".to_string(),
                vec![
                    make_artifact("fast:1", "jq"),
                    make_artifact("fast:1", "wget"),
                ],
            );
        }
        let (slow, slow_state) = FakeAdapter::new("slow");
        {
            let mut s = slow_state.lock().unwrap();
            s.instances = vec![make_instance("slow", "slow:1")];
            s.inventory_delay = Duration::from_millis(400);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![fast, slow], None);

        let in_flight = {
            let session = session.clone();
            tokio::spawn(async move {
                session
                    .refresh(&non_root_env(), &CheckOptions::default())
                    .await
            })
        };
        // The fast worker has answered; the slow one is still sleeping.
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(fast_state.lock().unwrap().inventory_calls.len() == 1);
        fast_state
            .lock()
            .unwrap()
            .artifacts
            .get_mut("fast:1")
            .unwrap()
            .retain(|a| a.key.name != "jq");

        let follow_up = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        let in_flight = in_flight.await.expect("in-flight task");

        assert!(
            artifact_names(&in_flight).contains(&"jq".to_string()),
            "precondition: the round in flight read the list before the \
             uninstall"
        );
        assert!(
            !artifact_names(&follow_up).contains(&"jq".to_string()),
            "the refresh after the uninstall must not get a snapshot read \
             before it: {:?}",
            artifact_names(&follow_up)
        );
    }

    #[tokio::test]
    async fn test_snapshot_returns_cached_value_without_calling_adapters() {
        let (adapter, state) = FakeAdapter::new("fake");
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let before = session.snapshot();
        assert_eq!(before.generation, 0);
        assert_eq!(state.lock().unwrap().detect_calls, 0);
        let refreshed = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        let after = session.snapshot();
        assert_eq!(after, refreshed);
    }

    #[tokio::test]
    async fn test_refresh_detects_across_adapters_concurrently_so_a_slow_source_does_not_block_others(
    ) {
        let (slow_a, state_a) = FakeAdapter::new("slow-a");
        let (slow_b, state_b) = FakeAdapter::new("slow-b");
        state_a.lock().unwrap().detect_delay = Duration::from_millis(200);
        state_a.lock().unwrap().instances = vec![make_instance("slow-a", "slow-a:1")];
        state_b.lock().unwrap().detect_delay = Duration::from_millis(200);
        state_b.lock().unwrap().instances = vec![make_instance("slow-b", "slow-b:1")];
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![slow_a, slow_b], None);

        let started = Instant::now();
        let snapshot = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        let elapsed = started.elapsed();

        assert!(snapshot.instances.iter().any(|i| i.id == "slow-a:1"));
        assert!(snapshot.instances.iter().any(|i| i.id == "slow-b:1"));
        assert!(
            elapsed < Duration::from_millis(350),
            "two 200ms detects must overlap, not run back to back (took {elapsed:?})"
        );
    }

    #[tokio::test]
    async fn test_an_unavailable_instance_is_a_reported_state_not_a_failed_refresh() {
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![
                make_instance("fake", "fake:up"),
                crate::testing::unavailable_instance("fake", "fake:down", Unavailable::NotRunning),
            ];
            s.artifacts
                .insert("fake:up".to_string(), vec![make_artifact("fake:up", "jq")]);
            s.failing.push("fake:down".to_string());
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);

        let snapshot = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        assert!(
            snapshot.refreshed_at.is_some(),
            "a refresh that ran has a timestamp, whatever the sources said"
        );
        assert!(
            snapshot.errors.is_empty(),
            "a source that reported it is not running is not a refresh error"
        );
        assert!(
            !snapshot.stale,
            "a source that said why it cannot answer is not a failed refresh: it says so \
             itself, on both pages, through its own notice. `stale` means the refresh \
             attempt failed, which is the only thing any reader of it does with it"
        );
        assert!(
            snapshot.instances.iter().any(|i| i.id == "fake:down"),
            "the unavailable instance stays in the snapshot so the UI can offer to start it"
        );
        assert!(
            !state
                .lock()
                .unwrap()
                .inventory_calls
                .contains(&"fake:down".to_string()),
            "an instance reported as not running must never be inventoried"
        );
        assert!(snapshot.artifacts.iter().any(|a| a.key.name == "jq"));
    }

    #[tokio::test]
    async fn test_refresh_carries_an_instance_under_an_operation_forward_and_still_refreshes_the_others(
    ) {
        // Phase 4 step E (plan ruling 19): an instance an operation is
        // holding is neither detected nor inventoried this round -- its
        // rows are last round's, unchanged, and the refresh does not wait
        // for the operation to end -- while every other instance, even of
        // the same adapter, is refreshed as usual. Before this the refresh
        // waited on the instance's lock, for as long as the operation took.
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![
                make_instance("fake", "fake:1"),
                make_instance("fake", "fake:2"),
            ];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
            s.artifacts
                .insert("fake:2".to_string(), vec![make_artifact("fake:2", "wget")]);
            s.block_execute = true;
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let first = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        state.lock().unwrap().inventory_calls.clear();
        assert_eq!(state.lock().unwrap().detect_calls, 1);

        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = session.issue_plan(&req).await.expect("issue_plan");
        let op_id = session.submit(issued.id).expect("submit");
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if session
                .operations()
                .iter()
                .any(|o| o.id == op_id && o.status == OpStatus::Running)
            {
                break;
            }
            assert!(Instant::now() < deadline, "operation never reached Running");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // The refresh returns while the operation is still running: it
        // neither detects `fake` (one of its instances is held) nor
        // inventories `fake:1`; `fake:2` is inventoried as ever.
        let snapshot = tokio::time::timeout(
            Duration::from_secs(2),
            session.refresh(&non_root_env(), &CheckOptions::default()),
        )
        .await
        .expect("a refresh must not wait for an operation on one instance");
        {
            let s = state.lock().unwrap();
            assert!(
                s.inventory_calls.contains(&"fake:2".to_string()),
                "a different instance's refresh must proceed while fake:1 is held"
            );
            assert!(
                !s.inventory_calls.contains(&"fake:1".to_string()),
                "fake:1's refresh must not run while fake:1's operation holds its lock"
            );
            assert_eq!(
                s.detect_calls, 1,
                "the adapter's detect is not run this round"
            );
        }
        // Carried forward *unchanged*: the same instance, no notice, no
        // stale flag, its rows as they were.
        let fake_1 = snapshot
            .instances
            .iter()
            .find(|i| i.id == "fake:1")
            .expect("fake:1");
        assert_eq!(
            fake_1,
            first.instances.iter().find(|i| i.id == "fake:1").unwrap()
        );
        assert!(snapshot.artifacts.iter().any(|a| a.key.name == "jq"));
        assert!(snapshot.artifacts.iter().any(|a| a.key.name == "wget"));
        assert!(!snapshot.stale);
        assert!(snapshot.errors.is_empty(), "{:?}", snapshot.errors);

        // And once the operation is over, the next refresh reads again.
        session.cancel(op_id).expect("cancel a Running op");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !session
            .operations()
            .iter()
            .any(|o| o.id == op_id && o.status == OpStatus::Done)
        {
            assert!(
                Instant::now() < deadline,
                "the cancelled operation never finished"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        state.lock().unwrap().inventory_calls.clear();
        session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        let s = state.lock().unwrap();
        assert_eq!(s.detect_calls, 2);
        assert!(s.inventory_calls.contains(&"fake:1".to_string()));
    }

    #[tokio::test]
    async fn test_a_refresh_during_an_operation_carries_the_held_instances_previous_errors_forward()
    {
        // Step E's whole-step review: the held instance's rows come forward
        // from last round (the test above), and so must the error that
        // round recorded against it -- this round re-read nothing about
        // the instance, so nothing has been retried. Without this, a Retry
        // pressed during the operation cleared the "may be out of date"
        // banner over data exactly as old as before.
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![
                make_instance("fake", "fake:1"),
                make_instance("fake", "fake:2"),
            ];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
            s.artifacts
                .insert("fake:2".to_string(), vec![make_artifact("fake:2", "wget")]);
            s.block_execute = true;
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let first = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert!(first.errors.is_empty(), "{:?}", first.errors);

        // Last round: fake:1's inventory failed, and the snapshot says so.
        state.lock().unwrap().failing.push("fake:1".to_string());
        let failed = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert!(failed.stale, "precondition: the read failed");
        assert_eq!(failed.errors.len(), 1);
        assert_eq!(failed.errors[0].instance_id, "fake:1");
        state.lock().unwrap().inventory_calls.clear();

        // An operation now holds fake:1's lock. `failing` is one-shot and
        // was consumed above, so had this round read fake:1 at all it
        // would have read clean: an error in the snapshot below can only
        // have been carried forward.
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = session.issue_plan(&req).await.expect("issue_plan");
        let op_id = session.submit(issued.id).expect("submit");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !session
            .operations()
            .iter()
            .any(|o| o.id == op_id && o.status == OpStatus::Running)
        {
            assert!(Instant::now() < deadline, "operation never reached Running");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let during = tokio::time::timeout(
            Duration::from_secs(2),
            session.refresh(&non_root_env(), &CheckOptions::default()),
        )
        .await
        .expect("a refresh must not wait for an operation on one instance");
        {
            let s = state.lock().unwrap();
            assert!(
                !s.inventory_calls.contains(&"fake:1".to_string()),
                "fake:1 is not read while its operation holds its lock"
            );
            assert!(
                s.inventory_calls.contains(&"fake:2".to_string()),
                "fake:2 is read as ever"
            );
        }
        assert!(during.artifacts.iter().any(|a| a.key.name == "jq"));
        assert_eq!(
            during.errors, failed.errors,
            "the error recorded against the held instance comes forward with its rows"
        );
        assert!(
            during.stale,
            "nothing about fake:1 was re-read, so the banner stays"
        );
        assert_eq!(
            during.generation, failed.generation,
            "the same rows and the same error: nothing changed, nothing bumps"
        );

        // Once the operation is over, the next refresh reads fake:1 again
        // and, succeeding this time, is what clears the carried error.
        session.cancel(op_id).expect("cancel a Running op");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !session
            .operations()
            .iter()
            .any(|o| o.id == op_id && o.status == OpStatus::Done)
        {
            assert!(
                Instant::now() < deadline,
                "the cancelled operation never finished"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        state.lock().unwrap().inventory_calls.clear();
        let after = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert!(state
            .lock()
            .unwrap()
            .inventory_calls
            .contains(&"fake:1".to_string()));
        assert!(after.errors.is_empty(), "{:?}", after.errors);
        assert!(!after.stale);
    }

    #[tokio::test]
    async fn test_a_refresh_during_an_operation_does_not_pile_check_notes_onto_the_held_instances_sibling(
    ) {
        // Step E's whole-step review: while an operation holds `fake:1`,
        // the adapter's detect is skipped and *both* of its instances are
        // carried from last round -- `fake:2` included, though nothing
        // holds it and this round checks it as usual. Carried with the
        // note last round's check left on it, `fake:2` then had this
        // round's identical note appended: one more copy per refresh for
        // as long as the operation ran, and a note the check had stopped
        // reporting stayed on. The sibling has to enter the fan-out with
        // last round's check notes stripped, the way a fresh detect would
        // deliver it -- and only those: a note detect wrote stays, since
        // detect did not run to write it again.
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            let mut placed = make_instance("fake", "fake:2");
            placed.status.notes.push(InstanceNote::NotOnPath);
            s.instances = vec![make_instance("fake", "fake:1"), placed];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
            s.artifacts
                .insert("fake:2".to_string(), vec![make_artifact("fake:2", "wget")]);
            s.notes
                .insert("fake:1".to_string(), vec![InstanceNote::IndexMayBeStale]);
            s.notes
                .insert("fake:2".to_string(), vec![InstanceNote::IndexMayBeStale]);
            s.block_execute = true;
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let notes_of = |snapshot: &Snapshot, id: &str| -> Vec<InstanceNote> {
            snapshot
                .instances
                .iter()
                .find(|i| i.id == id)
                .unwrap_or_else(|| panic!("{id} is in the snapshot"))
                .status
                .notes
                .clone()
        };
        let first = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert_eq!(
            notes_of(&first, "fake:1"),
            vec![InstanceNote::IndexMayBeStale]
        );
        assert_eq!(
            notes_of(&first, "fake:2"),
            vec![InstanceNote::NotOnPath, InstanceNote::IndexMayBeStale],
            "precondition: detect's note, then the check's"
        );

        // An operation now holds fake:1's lock.
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = session.issue_plan(&req).await.expect("issue_plan");
        let op_id = session.submit(issued.id).expect("submit");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !session
            .operations()
            .iter()
            .any(|o| o.id == op_id && o.status == OpStatus::Running)
        {
            assert!(Instant::now() < deadline, "operation never reached Running");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Two refreshes in a row while it runs: the check answers the same
        // both times, so the sibling's notes read the same both times.
        let during = tokio::time::timeout(
            Duration::from_secs(2),
            session.refresh(&non_root_env(), &CheckOptions::default()),
        )
        .await
        .expect("a refresh must not wait for an operation on one instance");
        let again = tokio::time::timeout(
            Duration::from_secs(2),
            session.refresh(&non_root_env(), &CheckOptions::default()),
        )
        .await
        .expect("a refresh must not wait for an operation on one instance");
        for (which, snapshot) in [("first", &during), ("second", &again)] {
            assert_eq!(
                notes_of(snapshot, "fake:2"),
                vec![InstanceNote::NotOnPath, InstanceNote::IndexMayBeStale],
                "{which} refresh during the operation: the sibling's notes are detect's, \
                 then this round's check -- not one more copy per refresh"
            );
            assert_eq!(
                notes_of(snapshot, "fake:1"),
                vec![InstanceNote::IndexMayBeStale],
                "{which} refresh during the operation: the held instance keeps last \
                 round's note, since nothing about it was re-read"
            );
        }
        assert_eq!(
            during.generation, first.generation,
            "the same rows and the same notes: nothing changed, nothing bumps"
        );
        assert_eq!(again.generation, during.generation);

        // The check stops reporting the note. The sibling's clears -- it
        // was checked again -- while the held instance's stays, exactly
        // like its rows and its errors.
        state.lock().unwrap().notes.clear();
        let cleared = tokio::time::timeout(
            Duration::from_secs(2),
            session.refresh(&non_root_env(), &CheckOptions::default()),
        )
        .await
        .expect("a refresh must not wait for an operation on one instance");
        assert_eq!(
            notes_of(&cleared, "fake:2"),
            vec![InstanceNote::NotOnPath],
            "a note the check no longer reports does not stay on the sibling"
        );
        assert_eq!(
            notes_of(&cleared, "fake:1"),
            vec![InstanceNote::IndexMayBeStale],
            "the held instance's notes are last round's, whatever the check says now"
        );

        // Once the operation is over, the next refresh detects and checks
        // fake:1 again, and its note clears too.
        session.cancel(op_id).expect("cancel a Running op");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !session
            .operations()
            .iter()
            .any(|o| o.id == op_id && o.status == OpStatus::Done)
        {
            assert!(
                Instant::now() < deadline,
                "the cancelled operation never finished"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let after = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert!(notes_of(&after, "fake:1").is_empty());
        assert_eq!(notes_of(&after, "fake:2"), vec![InstanceNote::NotOnPath]);
    }

    #[tokio::test]
    async fn test_an_operation_on_another_adapters_instance_does_not_skip_this_adapters_detect() {
        // The skip is by the held lock's name against the adapter's own
        // previous instances: an operation on `a:1` leaves adapter `b`
        // entirely alone.
        let (a, a_state) = FakeAdapter::new("a");
        let (b, b_state) = FakeAdapter::new("b");
        {
            let mut s = a_state.lock().unwrap();
            s.instances = vec![make_instance("a", "a:1")];
            s.artifacts
                .insert("a:1".to_string(), vec![make_artifact("a:1", "jq")]);
            s.block_execute = true;
        }
        {
            let mut s = b_state.lock().unwrap();
            s.instances = vec![make_instance("b", "b:1")];
            s.artifacts
                .insert("b:1".to_string(), vec![make_artifact("b:1", "wget")]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![a, b], None);
        session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        b_state.lock().unwrap().inventory_calls.clear();

        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "a:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = session.issue_plan(&req).await.expect("issue_plan");
        let op_id = session.submit(issued.id).expect("submit");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !session
            .operations()
            .iter()
            .any(|o| o.id == op_id && o.status == OpStatus::Running)
        {
            assert!(Instant::now() < deadline, "operation never reached Running");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert_eq!(a_state.lock().unwrap().detect_calls, 1, "a is skipped");
        assert_eq!(b_state.lock().unwrap().detect_calls, 2, "b is detected");
        assert!(b_state
            .lock()
            .unwrap()
            .inventory_calls
            .contains(&"b:1".to_string()));
        session.cancel(op_id).expect("cancel");
    }

    /// rustup's native layout in a temp home, built by hand
    /// (`adapters::standalone::testing` is not visible from here): the
    /// launcher, its `cargo` proxy link, and a `HostEnv` whose `PATH`
    /// finds the proxy so the cargo adapter detects too.
    fn rustup_home() -> (PathBuf, HostEnv) {
        use std::os::unix::fs::PermissionsExt;
        let raw = std::env::temp_dir().join(format!(
            "banager-refresh-rustup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&raw).expect("temp home");
        let home = std::fs::canonicalize(&raw).expect("canonical temp home");
        let bin = home.join(".cargo/bin");
        std::fs::create_dir_all(&bin).expect("cargo bin");
        let launcher = bin.join("rustup");
        std::fs::write(&launcher, b"#!/bin/sh\n").expect("rustup");
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755))
            .expect("executable rustup");
        std::os::unix::fs::symlink("rustup", bin.join("cargo")).expect("cargo proxy");
        let env = HostEnv {
            path_dirs: vec![bin],
            home: home.clone(),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        (home, env)
    }

    fn exited_0(stdout: &str) -> CommandOutput {
        CommandOutput {
            exit_code: Some(0),
            stdout: stdout.to_string(),
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        }
    }

    #[tokio::test]
    async fn test_a_refresh_during_rustups_self_update_runs_neither_rustup_nor_cargo() {
        // Review Focus #6, with the real adapters: `rustup self update`
        // holds `standalone-rustup` and `cargo:<home>` (Task 6), and while
        // it runs a refresh must not run the rustup binary at all -- not
        // as `rustup --version`, not as `cargo --version` (the proxy is
        // the same binary, and both begin by deleting the updater the
        // operation is about to run: plan ruling 4). Both rows are carried
        // forward unchanged; the next refresh after the operation reads
        // again.
        use crate::adapters::cargo::CargoAdapter;
        use crate::adapters::standalone::recipes::RUSTUP;
        use crate::adapters::standalone::StandaloneAdapter;
        use crate::http::{HttpResponse, MockHttpClient};
        use crate::model::{Attention, ResourceLock};
        use crate::trash::MockTrasher;

        let (home, env) = rustup_home();
        let launcher = home.join(".cargo/bin/rustup").to_string_lossy().to_string();
        let cargo = home.join(".cargo/bin/cargo").to_string_lossy().to_string();
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![launcher.as_str(), "--version"],
            exited_0("rustup 1.29.1 (d95a37b6a 2026-08-13)\n"),
        );
        runner.respond(
            vec![cargo.as_str(), "--version"],
            exited_0("cargo 1.98.1 (797e8a9bc 2026-08-05)\n"),
        );
        // Illustrative log text (never recorded: the recording rules
        // forbid running it); the outcome rests on the exit code and the
        // two version readings alone. Slow enough for a refresh to land
        // while it runs.
        runner.respond(
            vec![launcher.as_str(), "self", "update"],
            exited_0("  rustup unchanged - 1.29.1\n"),
        );
        runner.delay(
            vec![launcher.as_str(), "self", "update"],
            Duration::from_millis(400),
        );
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "https://static.rust-lang.org/rustup/release-stable.toml",
            HttpResponse {
                status: 200,
                body: "schema-version = '1'\nversion = '1.29.1'\n".to_string(),
            },
        );
        let rustup: Arc<dyn Adapter> = Arc::new(StandaloneAdapter::new(
            &RUSTUP,
            runner.clone(),
            http.clone(),
            Arc::new(MockTrasher::new()),
        ));
        let cargo_adapter: Arc<dyn Adapter> =
            Arc::new(CargoAdapter::new(runner.clone(), http.clone()));
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![rustup.clone(), cargo_adapter], None);

        let first = session.refresh(&env, &CheckOptions::default()).await;
        let mut ids: Vec<&str> = first.instances.iter().map(|i| i.id.as_str()).collect();
        ids.sort();
        let cargo_id = format!("cargo:{}", home.join(".cargo").display());
        assert_eq!(ids, vec![cargo_id.as_str(), "standalone-rustup"]);
        assert!(first.errors.is_empty(), "{:?}", first.errors);

        let inst = first
            .instances
            .iter()
            .find(|i| i.id == "standalone-rustup")
            .expect("rustup's instance")
            .clone();
        let req = OpRequest {
            kind: OpKind::Upgrade,
            instance_id: "standalone-rustup".to_string(),
            artifact_kind: ArtifactKind::Binary,
            name: "rustup".to_string(),
        };
        let plan = rustup.plan(&inst, &req).await.expect("plan");
        assert_eq!(
            plan.locks,
            vec![
                ResourceLock("standalone-rustup".to_string()),
                ResourceLock(cargo_id.clone())
            ]
        );
        let op_id = session.ops.submit(plan);
        // Until the operation's own before-reading is done and `self
        // update` is under way: from here every call is the refresh's.
        let deadline = Instant::now() + Duration::from_secs(2);
        while !runner
            .calls()
            .iter()
            .any(|c| c.len() == 3 && c[1] == "self" && c[2] == "update")
        {
            assert!(Instant::now() < deadline, "self update never started");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(session
            .ops
            .locks_held()
            .contains(&ResourceLock(cargo_id.clone())));

        let before = runner.calls().len();
        let during = session.refresh(&env, &CheckOptions::default()).await;
        let new_calls = runner.calls()[before..].to_vec();
        assert!(
            new_calls.is_empty(),
            "a refresh during rustup's self update ran {new_calls:?}"
        );
        assert_eq!(during.instances, first.instances);
        assert_eq!(during.artifacts, first.artifacts);
        assert!(!during.stale);
        assert!(during.errors.is_empty(), "{:?}", during.errors);

        assert_eq!(
            session.ops.wait(op_id).await,
            Some(Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade))
        );
        assert!(session.ops.locks_held().is_empty());
        let before = runner.calls().len();
        session.refresh(&env, &CheckOptions::default()).await;
        let after: Vec<Vec<String>> = runner.calls()[before..].to_vec();
        assert!(
            after
                .iter()
                .any(|c| c[0] == launcher && c[1] == "--version"),
            "after the operation, rustup is read again: {after:?}"
        );
        assert!(
            after.iter().any(|c| c[0] == cargo && c[1] == "--version"),
            "and so is cargo: {after:?}"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Dropping a refresh future mid-flight must cancel its workers, not
    /// detach them. A detached worker keeps running its adapter's command
    /// under the instance's resource lock, so a user's operation on that
    /// instance waits for as long as the command takes -- here, forever,
    /// because this `inventory` never returns on its own.
    #[tokio::test]
    async fn test_dropping_a_refresh_cancels_its_workers_and_releases_their_locks() {
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let env = non_root_env();
        let opts = CheckOptions::default();
        // One ordinary round first, so `issue_plan` below has an instance
        // to plan against.
        session.refresh(&env, &opts).await;

        state.lock().unwrap().blocking_inventory = vec!["fake:1".to_string()];
        let mut refresh = Box::pin(session.refresh(&env, &opts));
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            tokio::select! {
                _ = &mut refresh => panic!("a refresh whose inventory never returns cannot finish"),
                _ = tokio::time::sleep(Duration::from_millis(10)) => {}
            }
            if state.lock().unwrap().inventory_blocked == 1 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the worker never reached inventory"
            );
        }
        // The worker is now inside `inventory`, holding fake:1's lock.
        drop(refresh);
        state.lock().unwrap().blocking_inventory.clear();

        let deadline = Instant::now() + Duration::from_secs(2);
        while state.lock().unwrap().inventory_dropped == 0 {
            assert!(
                Instant::now() < deadline,
                "dropping the refresh must cancel its worker, not detach it"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // The lock the cancelled worker held is free: an operation on the
        // same instance runs to completion promptly instead of waiting on
        // a worker nothing will ever finish.
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = session.issue_plan(&req).await.expect("issue_plan");
        let op_id = session.submit(issued.id).expect("submit");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !session
            .operations()
            .iter()
            .any(|o| o.id == op_id && o.status == OpStatus::Done)
        {
            assert!(
                Instant::now() < deadline,
                "an operation on fake:1 must get the lock the dropped refresh's worker held"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // And the refresh gate went with it: the next refresh runs and
        // finishes, with its data.
        let snapshot = tokio::time::timeout(Duration::from_secs(2), session.refresh(&env, &opts))
            .await
            .expect("a refresh after a dropped one must not hang");
        assert!(snapshot.artifacts.iter().any(|a| a.key.name == "jq"));
        assert!(snapshot.errors.is_empty(), "{:?}", snapshot.errors);
        assert_eq!(state.lock().unwrap().inventory_blocked, 1);
        assert_eq!(state.lock().unwrap().inventory_dropped, 1);
    }

    fn make_update(instance_id: &str, name: &str) -> UpdateCandidate {
        UpdateCandidate {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Formula,
                name: name.to_string(),
            },
            current: "1.0".to_string(),
            target: "1.1".to_string(),
            channel: UpdateChannel::Native,
            checkable: true,
            warnings: Vec::new(),
            blocked: None,
        }
    }

    /// A "Banager couldn't check" row, shaped the way
    /// `adapters::uncheckable_candidate` actually builds one:
    /// `target == current`, since `UpdateCandidate` has no "unknown"
    /// target to put there instead.
    fn make_uncheckable_update(instance_id: &str, name: &str, version: &str) -> UpdateCandidate {
        UpdateCandidate {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Formula,
                name: name.to_string(),
            },
            current: version.to_string(),
            target: version.to_string(),
            channel: UpdateChannel::Native,
            checkable: false,
            warnings: Vec::new(),
            blocked: None,
        }
    }

    /// The names of every artifact/update in `snapshot`, sorted, so an
    /// assertion says what is there rather than how many things are there.
    fn artifact_names(snapshot: &Snapshot) -> Vec<String> {
        let mut names: Vec<String> = snapshot
            .artifacts
            .iter()
            .map(|a| a.key.name.clone())
            .collect();
        names.sort();
        names
    }

    fn update_names(snapshot: &Snapshot) -> Vec<String> {
        let mut names: Vec<String> = snapshot
            .updates
            .iter()
            .map(|u| u.key.name.clone())
            .collect();
        names.sort();
        names
    }

    #[tokio::test]
    async fn test_a_failed_update_check_keeps_only_candidates_the_fresh_inventory_confirms() {
        // A successful inventory is *evidence*, and a carried-forward
        // update candidate that contradicts it is not "possibly stale
        // data", it is a row Banager knows is wrong. Uninstall jq and the
        // next inventory correctly drops it; if the update check then
        // fails, the old code carried jq's update forward anyway and the
        // Updates page offered to upgrade a package the same snapshot had
        // just confirmed is gone. The same shape keeps an update for a
        // package that was upgraded in the meantime, which does exactly
        // nothing when run.
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.artifacts.insert(
                "fake:1".to_string(),
                vec![
                    make_artifact("fake:1", "jq"),
                    make_artifact("fake:1", "wget"),
                    make_artifact("fake:1", "curl"),
                ],
            );
            s.updates.insert(
                "fake:1".to_string(),
                vec![
                    make_update("fake:1", "jq"),
                    make_update("fake:1", "wget"),
                    make_update("fake:1", "curl"),
                ],
            );
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let first = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert_eq!(update_names(&first), vec!["curl", "jq", "wget"]);

        {
            let mut s = state.lock().unwrap();
            // jq was uninstalled; wget was upgraded to the very version
            // its candidate targeted; curl is untouched.
            s.artifacts.insert(
                "fake:1".to_string(),
                vec![
                    make_artifact_at("fake:1", "wget", "1.1"),
                    make_artifact("fake:1", "curl"),
                ],
            );
            s.failing_updates.push("fake:1".to_string());
        }
        let second = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        assert!(second.stale, "precondition: the update check failed");
        assert_eq!(second.errors.len(), 1, "precondition: and said so");
        assert_eq!(
            artifact_names(&second),
            vec!["curl", "wget"],
            "precondition: the inventory succeeded and dropped jq"
        );
        assert_eq!(
            update_names(&second),
            vec!["curl"],
            "only the candidate the fresh inventory still confirms may be carried forward: \
             jq is gone and wget is already at the version its candidate targeted, got {:?}",
            second.updates
        );
    }

    #[tokio::test]
    async fn test_a_failed_update_check_keeps_a_candidate_whose_recorded_current_has_drifted() {
        // The narrowing that introduced this test's subject required
        // `a.version == u.current` -- the candidate's *recorded* idea of
        // what is installed -- before a carried-forward row could
        // survive. That is not disproof of a pending update; it is a
        // disagreement between two readings, and this app has one: brew
        // keys its inventory by `full_name`/`full_token`
        // (`gautham-v/tap/claudebar`) while `brew outdated --json=v2`
        // never emits `full_name` and falls back to `claudebar`, so the
        // two never matched and every tapped package with a real pending
        // update was filtered away the moment `brew outdated` had a bad
        // minute. That spelling is fixed in the brew adapter now; the
        // rule itself must also stop treating "these two readings differ"
        // as "this update is not real", because only two things actually
        // disprove a candidate: the package is gone, or it is already at
        // the target.
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.artifacts.insert(
                "fake:1".to_string(),
                vec![
                    make_artifact("fake:1", "jq"),
                    make_artifact("fake:1", "wget"),
                ],
            );
            s.updates.insert(
                "fake:1".to_string(),
                vec![make_update("fake:1", "jq"), make_update("fake:1", "wget")],
            );
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let first = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert_eq!(update_names(&first), vec!["jq", "wget"]);

        {
            let mut s = state.lock().unwrap();
            // Both are still installed and neither is at 1.1, the version
            // the candidates target. jq drifted to a version the
            // candidate did not record (1.0.1 -- a reinstall, a revision
            // bump, or simply the other reader's answer); wget is
            // untouched.
            s.artifacts.insert(
                "fake:1".to_string(),
                vec![
                    make_artifact_at("fake:1", "jq", "1.0.1"),
                    make_artifact("fake:1", "wget"),
                ],
            );
            s.failing_updates.push("fake:1".to_string());
        }
        let second = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        assert!(second.stale, "precondition: the update check failed");
        assert_eq!(
            artifact_names(&second),
            vec!["jq", "wget"],
            "precondition: the inventory succeeded and still lists both"
        );
        assert_eq!(
            update_names(&second),
            vec!["jq", "wget"],
            "a pending update is not disproved by the inventory reading a \
             different version than the candidate recorded, got {:?}",
            second.updates
        );
    }

    #[tokio::test]
    async fn test_a_failed_update_check_keeps_an_uncheckable_candidate_whose_package_is_unchanged()
    {
        // `uncheckable_candidate` sets `target == current` by
        // construction -- "Banager couldn't check" rows have no "unknown"
        // target to put there instead. Reusing the checkable disproof
        // rule (`a.version != u.target`) for these rows reads "still
        // installed, unchanged" -- true of every uncheckable row whose
        // package has not moved, which is every one of them, since a
        // registry outage says nothing about the package itself -- as
        // "already at target", i.e. disproved. That silently cleared the
        // Updates page's doubt exactly when a source outage should be
        // raising it. An uncheckable row must be disproved only by
        // absence.
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.artifacts.insert(
                "fake:1".to_string(),
                vec![make_artifact_at("fake:1", "some-model", "1.0")],
            );
            s.updates.insert(
                "fake:1".to_string(),
                vec![make_uncheckable_update("fake:1", "some-model", "1.0")],
            );
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let first = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert_eq!(update_names(&first), vec!["some-model"]);
        assert!(
            !first.updates[0].checkable,
            "precondition: the row is uncheckable"
        );

        {
            let mut s = state.lock().unwrap();
            // Still installed at the same version -- nothing changed,
            // it just still cannot be checked this round either.
            s.failing_updates.push("fake:1".to_string());
        }
        let second = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        assert!(second.stale, "precondition: the update check failed");
        assert_eq!(
            artifact_names(&second),
            vec!["some-model"],
            "precondition: the inventory still confirms the package is installed"
        );
        assert_eq!(
            update_names(&second),
            vec!["some-model"],
            "an uncheckable row must not be read as \"already at target\" \
             merely because the package has not moved; it is disproved \
             only by absence, got {:?}",
            second.updates
        );
    }

    #[tokio::test]
    async fn test_a_failed_inventory_still_carries_every_update_candidate_forward() {
        // The other side of the rule above, and the reason it is
        // conditional: when the inventory itself failed there is no fresh
        // evidence to reconfirm against -- the artifacts in this snapshot
        // are last round's, carried forward by the very same mechanism --
        // so filtering the candidates against them would only re-derive
        // last round's consistency while risking dropping rows on a
        // source that is merely having a bad minute.
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
            s.updates
                .insert("fake:1".to_string(), vec![make_update("fake:1", "jq")]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        {
            let mut s = state.lock().unwrap();
            s.failing.push("fake:1".to_string());
            s.failing_updates.push("fake:1".to_string());
        }
        let second = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        assert_eq!(second.errors.len(), 2, "precondition: both halves failed");
        assert_eq!(artifact_names(&second), vec!["jq"]);
        assert_eq!(
            update_names(&second),
            vec!["jq"],
            "with no fresh inventory to contradict it, the candidate stays"
        );
    }

    #[tokio::test]
    async fn test_a_panicking_per_instance_task_keeps_that_instances_previous_rows() {
        // This module's own contract, at the top of the file: a failing
        // instance keeps its previous rows so that a transient failure
        // never makes something the user installed appear to vanish. A
        // task that *panics* returns no value at all, so the join branch
        // had nothing to append and appended nothing -- every package
        // from that source disappeared from the UI until some later
        // refresh happened to succeed, which is the one outcome the
        // contract rules out. It is also the branch a cancelled task
        // takes.
        for panic_in_updates in [false, true] {
            let (adapter, state) = FakeAdapter::new("fake");
            {
                let mut s = state.lock().unwrap();
                s.instances = vec![
                    make_instance("fake", "fake:1"),
                    make_instance("fake", "fake:2"),
                ];
                s.artifacts
                    .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
                s.artifacts
                    .insert("fake:2".to_string(), vec![make_artifact("fake:2", "wget")]);
                s.updates
                    .insert("fake:1".to_string(), vec![make_update("fake:1", "jq")]);
            }
            let sink = Arc::new(VecSink::new());
            let session = Session::with_adapters(sink, vec![adapter], None);
            let first = session
                .refresh(&non_root_env(), &CheckOptions::default())
                .await;
            assert_eq!(artifact_names(&first), vec!["jq", "wget"]);
            assert_eq!(update_names(&first), vec!["jq"]);

            {
                let mut s = state.lock().unwrap();
                if panic_in_updates {
                    // The nastier half: the inventory succeeded inside
                    // the task and its result is lost with the panic, so
                    // the carry-forward is the only thing left.
                    s.panicking_updates.push("fake:1".to_string());
                } else {
                    s.panicking_inventory.push("fake:1".to_string());
                }
            }
            let second = session
                .refresh(&non_root_env(), &CheckOptions::default())
                .await;

            assert_eq!(second.errors.len(), 1, "the panic is reported as an error");
            assert_eq!(second.errors[0].instance_id, "fake:1");
            assert!(second.stale);
            assert_eq!(
                artifact_names(&second),
                vec!["jq", "wget"],
                "the panicking instance keeps its rows and the healthy one keeps its own \
                 (panic_in_updates={panic_in_updates})"
            );
            assert_eq!(
                update_names(&second),
                vec!["jq"],
                "carried forward exactly once -- not twice, and not zero times \
                 (panic_in_updates={panic_in_updates})"
            );
        }
    }

    #[tokio::test]
    async fn test_a_panicking_detect_keeps_that_adapters_previous_instances_and_rows() {
        // A detection that crashed said nothing -- which is not the same
        // thing as saying "this source is not installed on this Mac". The
        // join branch used to record an error and move on, so an adapter
        // whose `detect` panicked took every instance it had ever
        // reported, and every package under them, off the screen. They
        // are kept, marked unavailable: Banager could not ask, so it may
        // not claim the rows are current, and the actionability gate
        // (spec §2.5) must not offer operations against them.
        let (flaky, flaky_state) = FakeAdapter::new("flaky");
        let (steady, steady_state) = FakeAdapter::new("steady");
        {
            let mut s = flaky_state.lock().unwrap();
            s.instances = vec![make_instance("flaky", "flaky:1")];
            s.artifacts
                .insert("flaky:1".to_string(), vec![make_artifact("flaky:1", "jq")]);
            s.updates
                .insert("flaky:1".to_string(), vec![make_update("flaky:1", "jq")]);
        }
        {
            let mut s = steady_state.lock().unwrap();
            s.instances = vec![make_instance("steady", "steady:1")];
            s.artifacts.insert(
                "steady:1".to_string(),
                vec![make_artifact("steady:1", "wget")],
            );
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![flaky, steady], None);
        let first = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert_eq!(artifact_names(&first), vec!["jq", "wget"]);

        {
            let mut s = flaky_state.lock().unwrap();
            s.panicking_detect = true;
            s.inventory_calls.clear();
        }
        let second = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        assert_eq!(second.errors.len(), 1, "the panic is reported as an error");
        assert_eq!(second.errors[0].instance_id, "flaky");
        assert!(second.stale);
        let kept = second
            .instances
            .iter()
            .find(|i| i.id == "flaky:1")
            .expect("an adapter whose detect crashed keeps the instances it reported before");
        assert_eq!(
            kept.status.unavailable,
            Some(Unavailable::NotResponding),
            "kept as history, not as a source Banager can act on"
        );
        assert_eq!(
            artifact_names(&second),
            vec!["jq", "wget"],
            "its packages stay on screen, and the healthy adapter is untouched"
        );
        assert_eq!(update_names(&second), vec!["jq"]);
        assert!(
            flaky_state.lock().unwrap().inventory_calls.is_empty(),
            "a carried-forward instance must not be inventoried on this round: \
             nothing detected it, so there is nothing to ask"
        );

        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: "flaky:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        match session.issue_plan(&req).await {
            Err(crate::adapters::AdapterError::NotActionable { unavailable, .. }) => {
                assert_eq!(unavailable, Some(Unavailable::NotResponding));
            }
            other => panic!("a carried-forward row must offer no Uninstall, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_a_source_that_stops_answering_keeps_its_rows_but_offers_no_operations_on_them() {
        // Spec §2.4-3 and §2.5 together, and neither half is optional. The
        // notice a stopped source renders says "below is what Banager saw
        // last time"; before the carry-forward that sentence sat above an
        // empty group. With the rows back, every one of them would sprout
        // an Uninstall button that cannot possibly work -- so the gate in
        // `issue_plan` is what the second half of this test pins down.
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
            s.updates
                .insert("fake:1".to_string(), vec![make_update("fake:1", "jq")]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let first = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert_eq!(first.artifacts.len(), 1, "precondition: one artifact known");
        assert_eq!(first.updates.len(), 1, "precondition: one update known");

        {
            let mut s = state.lock().unwrap();
            s.instances = vec![crate::testing::unavailable_instance(
                "fake",
                "fake:1",
                Unavailable::NotRunning,
            )];
            s.inventory_calls.clear();
        }
        let second = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        assert!(
            second.artifacts.iter().any(|a| a.key.name == "jq"),
            "an unavailable source keeps the artifacts it reported last time, got {:?}",
            second.artifacts
        );
        assert!(
            second.updates.iter().any(|u| u.key.name == "jq"),
            "and the updates too, got {:?}",
            second.updates
        );
        assert!(
            !state
                .lock()
                .unwrap()
                .inventory_calls
                .contains(&"fake:1".to_string()),
            "carrying data forward must not mean asking the dead source again"
        );

        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        match session.issue_plan(&req).await {
            Err(crate::adapters::AdapterError::NotActionable {
                read_only,
                unavailable,
            }) => {
                assert_eq!(read_only, None);
                assert_eq!(unavailable, Some(Unavailable::NotRunning));
            }
            other => panic!("a carried-forward row must offer no Uninstall, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_refresh_stamps_refreshed_at_even_when_a_source_failed() {
        // Spec §2.4-1. Gated on `stale`, one permanently broken source left
        // `refreshed_at` null for the life of the machine, and a null
        // timestamp is how `SnapshotStatus` recognises "nothing has ever
        // been checked" -- six good sources described as no data at all.
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.failing.push("fake:1".to_string());
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], Some(|| 1_700_000_000));

        let snapshot = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        assert!(snapshot.stale, "precondition: the fetch failed");
        assert_eq!(snapshot.errors.len(), 1, "precondition: and said so");
        assert_eq!(
            snapshot.refreshed_at,
            Some(1_700_000_000),
            "a refresh that ran is stamped; `stale` is what says the data may be old"
        );
    }

    #[tokio::test]
    async fn test_a_round_whose_source_failed_is_the_days_check_unless_every_source_of_a_daily_one_did(
    ) {
        // The daily check is due a day after the last round that counts
        // as a check *ended*: the stamp `RoundLog` keeps of it is what the
        // shell hands `auto_check::tick`. A round in which a source failed
        // is stamped too, and counts -- so a source that keeps failing is
        // not asked again at every 15-minute tick -- unless it was a daily
        // one in which every source failed, which is one more failed daily
        // check instead (`RoundLog::failed_checks`).
        use crate::auto_check::{
            tick, CheckEvery, FailedChecks, RoundLog, RoundTrigger, Tick, DUE_AFTER_SECS,
        };
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![
                make_instance("fake", "fake:1"),
                make_instance("fake", "fake:2"),
            ];
            s.failing.push("fake:1".to_string());
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], Some(|| 1_700_000_000));
        let env = non_root_env();
        let opts = CheckOptions::default();
        let mut log = RoundLog::default();

        // One of two sources failed, in a daily round: it counts.
        let (round, snapshot) = session.refresh_with_round(&env, &opts).await;
        assert!(snapshot.stale, "precondition: the round failed in part");
        log.record_daily(round, 1_700_000_000 - 20, &snapshot);
        let ended = log.last_check_ended();
        assert_eq!(ended, snapshot.refreshed_at);
        assert_eq!(log.failed_checks(), None);
        assert!(!session.busy(), "precondition: nothing under way");
        assert_eq!(
            tick(
                1_700_000_000 + 15 * 60,
                ended,
                None,
                session.busy(),
                Some(CheckEvery::Day)
            ),
            Tick::NotDue
        );
        assert_eq!(
            tick(
                1_700_000_000 + DUE_AFTER_SECS,
                ended,
                None,
                session.busy(),
                Some(CheckEvery::Day)
            ),
            Tick::Check
        );

        // Both failed (`failing` is one-shot): a daily round does not
        // count, one of the window's does.
        state.lock().unwrap().failing = vec!["fake:1".to_string(), "fake:2".to_string()];
        let mut daily = RoundLog::default();
        let (round, snapshot) = session.refresh_with_round(&env, &opts).await;
        assert_eq!(snapshot.errors.len(), 2, "precondition: both failed");
        daily.record_daily(round, 1_700_000_000 - 20, &snapshot);
        assert_eq!(daily.last_check_ended(), None);
        assert_eq!(
            daily.failed_checks(),
            Some(FailedChecks {
                looked_at: 1_700_000_000 - 20,
                in_a_row: 1
            })
        );
        let mut window = RoundLog::default();
        window.record(round, RoundTrigger::Window, &snapshot);
        assert_eq!(window.last_check_ended(), snapshot.refreshed_at);
    }

    #[tokio::test]
    async fn test_refresh_with_round_numbers_each_round_and_gives_the_callers_who_share_one_its_number(
    ) {
        let (adapter, state) = FakeAdapter::new("fake");
        state.lock().unwrap().instances = vec![make_instance("fake", "fake:1")];
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let env = non_root_env();
        let opts = CheckOptions::default();

        let (first, _) = session.refresh_with_round(&env, &opts).await;
        let (second, _) = session.refresh_with_round(&env, &opts).await;
        assert!(second > first, "a later round has a higher number");

        // A round in flight, and three calls arriving during it: they share
        // the one round after it, so all three get that round's number.
        state.lock().unwrap().detect_delay = Duration::from_millis(100);
        let in_flight = {
            let session = session.clone();
            tokio::spawn(async move {
                session
                    .refresh_with_round(&non_root_env(), &CheckOptions::default())
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
        let behind: Vec<_> = (0..3)
            .map(|_| {
                let session = session.clone();
                tokio::spawn(async move {
                    session
                        .refresh_with_round(&non_root_env(), &CheckOptions::default())
                        .await
                })
            })
            .collect();
        let (in_flight_round, _) = in_flight.await.expect("in-flight task");
        let mut rounds = Vec::new();
        for handle in behind {
            let (round, snapshot) = handle.await.expect("caller task");
            assert_eq!(snapshot, session.snapshot(), "the shared round's snapshot");
            assert_eq!(snapshot.round, round, "which carries its number");
            rounds.push(round);
        }
        assert_eq!(in_flight_round, second + 1);
        assert_eq!(rounds, vec![second + 2; 3], "one shared round, one number");
        assert_eq!(state.lock().unwrap().detect_calls, 4);
    }

    #[tokio::test]
    async fn test_each_snapshot_carries_the_number_of_the_round_that_committed_it() {
        // `Snapshot::round`: the number `refresh_with_round` hands back,
        // higher each round, also when nothing changed and `generation`
        // stayed where it was.
        let (adapter, state) = FakeAdapter::new("fake");
        state.lock().unwrap().instances = vec![make_instance("fake", "fake:1")];
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        assert_eq!(
            session.snapshot().round,
            0,
            "Snapshot::empty(), before any round"
        );
        let env = non_root_env();
        let opts = CheckOptions::default();

        let (first, snapshot) = session.refresh_with_round(&env, &opts).await;
        assert_eq!(snapshot.round, first);
        let (second, again) = session.refresh_with_round(&env, &opts).await;
        assert_eq!(
            again.generation, snapshot.generation,
            "precondition: nothing changed"
        );
        assert_eq!(again.round, second);
        assert!(second > first);
        assert_eq!(session.snapshot().round, second);
    }

    #[tokio::test]
    async fn test_a_round_after_the_clock_is_set_back_still_has_the_higher_number() {
        // What the page tells the later of two snapshots by
        // (`isNewerSnapshot` in src/lib/events.ts). A round that found
        // nothing new, run after the clock was put back an hour, keeps the
        // generation and is stamped before the round it follows: only its
        // number says it is the later one.
        static NOW: AtomicI64 = AtomicI64::new(1_790_586_000);
        let (adapter, state) = FakeAdapter::new("fake");
        state.lock().unwrap().instances = vec![make_instance("fake", "fake:1")];
        let sink = Arc::new(VecSink::new());
        let session =
            Session::with_adapters(sink, vec![adapter], Some(|| NOW.load(Ordering::SeqCst)));
        let env = non_root_env();
        let opts = CheckOptions::default();

        let (first, before) = session.refresh_with_round(&env, &opts).await;
        NOW.fetch_sub(3600, Ordering::SeqCst);
        let (second, after) = session.refresh_with_round(&env, &opts).await;

        assert_eq!(after.generation, before.generation, "nothing new");
        assert!(
            after.refreshed_at < before.refreshed_at,
            "stamped by the clock put back: {:?} after {:?}",
            after.refreshed_at,
            before.refreshed_at
        );
        assert!(second > first, "and still the higher round");
        assert_eq!((before.round, after.round), (first, second));
    }

    #[tokio::test]
    async fn test_refresh_recording_records_a_round_before_anyone_can_see_it() {
        // The shell records who asked for a round through `record`: were
        // the round's snapshot committed first, a reader could fetch it --
        // the page, and report it -- before its record was in.
        let (adapter, state) = FakeAdapter::new("fake");
        state.lock().unwrap().instances = vec![make_instance("fake", "fake:1")];
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (round, snapshot) = session
            .refresh_recording(
                &non_root_env(),
                &CheckOptions::default(),
                {
                    let session = session.clone();
                    let seen = seen.clone();
                    move |round, snapshot| {
                        // What anyone reading the session sees as it is recorded.
                        let visible = session.snapshot().round;
                        seen.lock().unwrap().push((round, snapshot.round, visible));
                    }
                },
                |_| {},
            )
            .await;
        assert_eq!(
            *seen.lock().unwrap(),
            [(round, round, round - 1)],
            "recorded once, with its own number and snapshot, while the session still showed the round before"
        );
        assert_eq!(snapshot.round, round);
        assert_eq!(session.snapshot().round, round, "then committed");
    }

    #[tokio::test]
    async fn test_refresh_recording_records_a_shared_round_for_each_caller_that_takes_it() {
        let (adapter, state) = FakeAdapter::new("fake");
        state.lock().unwrap().instances = vec![make_instance("fake", "fake:1")];
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let recorded = Arc::new(Mutex::new(Vec::new()));

        // A round in flight, and three calls arriving during it: they share
        // the one round after it, and each records it as it takes it.
        state.lock().unwrap().detect_delay = Duration::from_millis(100);
        let in_flight = {
            let session = session.clone();
            let recorded = recorded.clone();
            tokio::spawn(async move {
                session
                    .refresh_recording(
                        &non_root_env(),
                        &CheckOptions::default(),
                        move |round, _| recorded.lock().unwrap().push(round),
                        |_| {},
                    )
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
        let behind: Vec<_> = (0..3)
            .map(|_| {
                let session = session.clone();
                let recorded = recorded.clone();
                tokio::spawn(async move {
                    session
                        .refresh_recording(
                            &non_root_env(),
                            &CheckOptions::default(),
                            move |round, _| recorded.lock().unwrap().push(round),
                            |_| {},
                        )
                        .await
                })
            })
            .collect();
        let (first, _) = in_flight.await.expect("in-flight task");
        for handle in behind {
            let (round, _) = handle.await.expect("caller task");
            assert_eq!(round, first + 1);
        }
        assert_eq!(
            *recorded.lock().unwrap(),
            [first, first + 1, first + 1, first + 1],
            "each call recorded the round that answered it, once"
        );
    }

    #[tokio::test]
    async fn test_busy_counts_a_refresh_from_its_arrival_until_it_returns_or_is_dropped() {
        let (adapter, state) = FakeAdapter::new("fake");
        state.lock().unwrap().instances = vec![make_instance("fake", "fake:1")];
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        assert!(!session.busy(), "nothing under way yet");

        state.lock().unwrap().blocking_inventory = vec!["fake:1".to_string()];
        let running = {
            let session = session.clone();
            tokio::spawn(async move {
                session
                    .refresh(&non_root_env(), &CheckOptions::default())
                    .await
            })
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        while state.lock().unwrap().inventory_blocked == 0 {
            assert!(
                Instant::now() < deadline,
                "the worker never reached inventory"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(session.busy(), "a round in flight");

        // A second call queues on the gate behind it.
        let queued = {
            let session = session.clone();
            tokio::spawn(async move {
                session
                    .refresh(&non_root_env(), &CheckOptions::default())
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(
            session.busy(),
            "a round in flight and a call waiting for it"
        );

        // The first is dropped mid-round; the second then runs its own.
        state.lock().unwrap().blocking_inventory.clear();
        running.abort();
        assert!(running.await.expect_err("aborted").is_cancelled());
        let snapshot = tokio::time::timeout(Duration::from_secs(2), queued)
            .await
            .expect("the queued call runs once the dropped one lets go")
            .expect("queued task");
        assert!(snapshot.errors.is_empty(), "{:?}", snapshot.errors);
        assert!(
            !session.busy(),
            "one call dropped and one returned: nothing is under way, and the \
             dropped one is not counted for the rest of the run"
        );
    }

    #[test]
    fn test_snapshot_empty_still_has_no_timestamp() {
        // The one snapshot that may carry `refreshed_at: None` now that
        // every completed refresh stamps one. `SnapshotStatus`'s startup
        // branch reads exactly this to tell "still loading" from "checked
        // and found nothing".
        assert_eq!(Snapshot::empty().refreshed_at, None);
    }

    #[tokio::test]
    async fn test_a_note_a_source_reports_while_checking_reaches_the_snapshot() {
        // The whole point of `CheckOutcome`: `brew update` failing is a
        // fact about the *source*, not about any one package, and the only
        // place the user can be told is the source's own notice. The
        // instance the fan-out task holds is a clone (`refresh` clones
        // `instances` before spawning), so a note that is not merged back
        // by id is silently lost -- the snapshot the UI renders would carry
        // an empty `notes` and the Updates page would go back to saying
        // "Everything is up to date" over a catalogue it could not
        // download.
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![
                make_instance("fake", "fake:1"),
                make_instance("fake", "fake:2"),
            ];
            s.notes
                .insert("fake:1".to_string(), vec![InstanceNote::IndexMayBeStale]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);

        let snapshot = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        let noted = snapshot
            .instances
            .iter()
            .find(|i| i.id == "fake:1")
            .expect("the instance that reported the note is still in the snapshot");
        assert_eq!(
            noted.status.notes,
            vec![InstanceNote::IndexMayBeStale],
            "a note from `check_updates` has to travel back to its instance by id"
        );
        let quiet = snapshot
            .instances
            .iter()
            .find(|i| i.id == "fake:2")
            .expect("the other instance is still there");
        assert!(
            quiet.status.notes.is_empty(),
            "and only to that instance, got {:?}",
            quiet.status.notes
        );
        assert!(
            snapshot.errors.is_empty(),
            "a note is not an error: the source answered, its answer just has a caveat"
        );
    }

    #[test]
    fn test_merge_instance_notes_merges_by_id_and_ignores_the_rest() {
        // `instances` is cloned before the fan-out, so a note a spawned
        // task produced has to travel back by id or be lost -- the same
        // trap `SourceError`'s `instance_id` fell into once already.
        let mut instances = vec![
            crate::testing::manager_instance("fake", "fake:1"),
            crate::testing::manager_instance("fake", "fake:2"),
        ];
        super::merge_instance_notes(
            &mut instances,
            "fake:2",
            vec![InstanceNote::IndexMayBeStale],
        );
        assert!(instances[0].status.notes.is_empty());
        assert_eq!(
            instances[1].status.notes,
            vec![InstanceNote::IndexMayBeStale]
        );

        super::merge_instance_notes(&mut instances, "fake:2", vec![]);
        assert_eq!(
            instances[1].status.notes,
            vec![InstanceNote::IndexMayBeStale],
            "an empty batch of notes changes nothing"
        );
        super::merge_instance_notes(
            &mut instances,
            "fake:gone",
            vec![InstanceNote::IndexMayBeStale],
        );
        assert!(
            instances.iter().all(|i| i.id != "fake:gone"),
            "a note for an instance nobody detected must not invent one"
        );
    }

    type FakePair = (Arc<FakeAdapter>, Arc<Mutex<FakeState>>);

    /// Two adapters with different adapter ids that both claim instance id
    /// `"shared"`: each with its own package, alpha with an update, beta
    /// with a note. Only a hand-built id can do this -- `model::instance_id`
    /// cannot -- which is exactly the case refresh has to stay safe
    /// against, because nothing downstream of detection checks.
    fn colliding_adapters() -> (FakePair, FakePair) {
        let (alpha, alpha_state) = FakeAdapter::new("alpha");
        let (beta, beta_state) = FakeAdapter::new("beta");
        {
            let mut s = alpha_state.lock().unwrap();
            s.instances = vec![make_instance("alpha", "shared")];
            s.artifacts
                .insert("shared".to_string(), vec![make_artifact("shared", "jq")]);
            s.updates
                .insert("shared".to_string(), vec![make_update("shared", "jq")]);
        }
        {
            let mut s = beta_state.lock().unwrap();
            s.instances = vec![make_instance("beta", "shared")];
            s.artifacts
                .insert("shared".to_string(), vec![make_artifact("shared", "wget")]);
            s.notes
                .insert("shared".to_string(), vec![InstanceNote::IndexMayBeStale]);
        }
        ((alpha, alpha_state), (beta, beta_state))
    }

    #[tokio::test]
    async fn test_a_duplicate_instance_id_across_adapters_keeps_the_first_and_says_so() {
        // Before refresh checked, both "shared" instances went through the
        // fan-out: beta's note was merged onto the *first* match (alpha's
        // instance, which never reported it), both packages were listed
        // under one source, and the ops registry kept whichever instance
        // registered last. Now the second one is refused, by name, and
        // the first is untouched. Registration order must not matter:
        // "first" is adapter-id order, so the same one wins every time.
        for alpha_registered_first in [true, false] {
            let ((alpha, _), (beta, _)) = colliding_adapters();
            let adapters: Vec<Arc<dyn Adapter>> = if alpha_registered_first {
                vec![alpha, beta]
            } else {
                vec![beta, alpha]
            };
            let sink = Arc::new(VecSink::new());
            let session = Session::with_adapters(sink, adapters, None);

            let snapshot = session
                .refresh(&non_root_env(), &CheckOptions::default())
                .await;

            assert_eq!(snapshot.instances.len(), 1, "{:?}", snapshot.instances);
            let kept = &snapshot.instances[0];
            assert_eq!(kept.id, "shared");
            assert_eq!(kept.adapter_id, "alpha", "adapter-id order decides");
            assert!(
                kept.status.notes.is_empty(),
                "beta's note must not land on alpha's instance, got {:?}",
                kept.status.notes
            );
            assert_eq!(artifact_names(&snapshot), vec!["jq"]);
            assert_eq!(update_names(&snapshot), vec!["jq"]);

            assert_eq!(snapshot.errors.len(), 1, "{:?}", snapshot.errors);
            let err = &snapshot.errors[0];
            assert_eq!(err.instance_id, "shared");
            assert!(
                err.message.contains("alpha") && err.message.contains("beta"),
                "the error has to name both sides of the collision: {}",
                err.message
            );
            assert!(snapshot.stale, "something detected is not on screen");

            // Deterministic: the same collision next round is the same
            // snapshot, so it does not bump `generation` either.
            let again = session
                .refresh(&non_root_env(), &CheckOptions::default())
                .await;
            assert_eq!(again.instances, snapshot.instances);
            assert_eq!(again.errors, snapshot.errors);
            assert_eq!(again.generation, snapshot.generation);
        }
    }

    #[tokio::test]
    async fn test_a_duplicate_instance_id_does_not_duplicate_carried_forward_rows() {
        // The carry-forward half of the same hazard: every carry-forward
        // filter pulls `previous` rows by id, so two instances sharing one
        // would each carry the same rows and the snapshot would list them
        // twice. Fail the kept instance's inventory and update check so
        // both of its previous rows come forward, and check they come
        // forward exactly once.
        let ((alpha, alpha_state), (beta, _)) = colliding_adapters();
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![alpha, beta], None);
        session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        {
            let mut s = alpha_state.lock().unwrap();
            s.failing.push("shared".to_string());
            s.failing_updates.push("shared".to_string());
        }

        let second = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        assert_eq!(artifact_names(&second), vec!["jq"]);
        assert_eq!(update_names(&second), vec!["jq"]);
    }

    #[tokio::test]
    async fn test_a_single_adapter_repeating_its_own_instance_id_is_refused_too() {
        // The collision `model::instance_id` cannot rule out: pipx and uv
        // build their id with no qualifier, so a second instance from
        // either would repeat it exactly. Same handling as across
        // adapters -- the first one detected wins.
        let (adapter, state) = FakeAdapter::new("pipx");
        {
            let mut s = state.lock().unwrap();
            let mut second = make_instance("pipx", "pipx");
            second.exe_path = std::path::PathBuf::from("/usr/local/bin/pipx");
            s.instances = vec![make_instance("pipx", "pipx"), second];
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);

        let snapshot = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        assert_eq!(snapshot.instances.len(), 1);
        assert_ne!(
            snapshot.instances[0].exe_path,
            std::path::PathBuf::from("/usr/local/bin/pipx"),
            "the first instance the adapter returned is the one kept"
        );
        assert_eq!(snapshot.errors.len(), 1);
        assert_eq!(snapshot.errors[0].instance_id, "pipx");
        let inventoried = state.lock().unwrap().inventory_calls.clone();
        assert_eq!(
            inventoried,
            vec!["pipx".to_string()],
            "fetched once, not twice"
        );
    }

    const BREW: &str = "/opt/homebrew/bin/brew";

    fn brew_answer(exit_code: i32, stdout: &str) -> CommandOutput {
        CommandOutput {
            exit_code: Some(exit_code),
            stdout: stdout.to_string(),
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        }
    }

    /// `brew info --installed --json=v2` listing jq at `version`.
    fn brew_info_jq(version: &str) -> String {
        format!(
            r#"{{"formulae":[{{"name":"jq","desc":"JSON processor","homepage":"https://jqlang.org","linked_keg":"{version}","installed":[{{"version":"{version}","installed_on_request":true,"installed_as_dependency":false,"time":1700000000}}]}}],"casks":[]}}"#
        )
    }

    const BREW_OUTDATED_JQ: &str = r#"{"formulae":[{"name":"jq","installed_versions":["1.6"],"current_version":"1.7.1","pinned":false,"pinned_version":null}],"casks":[]}"#;
    const BREW_NOTHING: &str = r#"{"formulae":[],"casks":[]}"#;

    /// How many times the catalogue has been read, by either reader.
    fn catalogue_reads(runner: &MockRunner) -> (usize, usize) {
        let calls = runner.calls();
        let count = |verb: &str| {
            calls
                .iter()
                .filter(|c| c.get(1).map(String::as_str) == Some(verb))
                .count()
        };
        (count("info"), count("outdated"))
    }

    fn brew_notes(snapshot: &Snapshot) -> Vec<InstanceNote> {
        snapshot
            .instances
            .iter()
            .find(|i| i.adapter_id == "brew")
            .expect("brew is detected")
            .status
            .notes
            .clone()
    }

    #[tokio::test]
    async fn test_a_refresh_behind_a_running_brew_update_reads_nothing_and_keeps_the_previous_rows()
    {
        // `brew update` git-merges Homebrew's own code and `curl`s the
        // package list over the file `brew info` and `brew outdated` read.
        // An install waits for it (`BrewAdapter::wait_for_update`); a
        // refresh used to report "still downloading" and then read that
        // catalogue anyway, where a half-written file could fail the
        // refresh or, worse, parse.
        let runner = Arc::new(MockRunner::new());
        runner.respond(vec![BREW, "--version"], brew_answer(0, "Homebrew 7.0.3\n"));
        runner.respond(
            vec![BREW, "info", "--installed", "--json=v2"],
            brew_answer(0, &brew_info_jq("1.6")),
        );
        runner.respond(
            vec![BREW, "outdated", "--json=v2"],
            brew_answer(0, BREW_OUTDATED_JQ),
        );
        // Round one's update fails at once, so it reads normally and
        // leaves no successful update behind: round two starts another.
        runner.respond(vec![BREW, "update"], brew_answer(1, ""));
        let background_change = Arc::new(tokio::sync::Notify::new());
        let brew = Arc::new(
            BrewAdapter::new(runner.clone())
                .with_path_exists_fn(apple_silicon_layout)
                .with_update_patience(Duration::from_millis(100))
                .with_background_change(background_change.clone()),
        );
        let session = Session::with_adapters(Arc::new(VecSink::new()), vec![brew], None);
        let env = non_root_env();
        let opts = CheckOptions::default();

        let first = session.refresh(&env, &opts).await;
        assert_eq!(artifact_names(&first), vec!["jq"]);
        assert_eq!(update_names(&first), vec!["jq"]);

        // Round two starts a `brew update` that outlasts its patience. The
        // inventory was read before it started, so it is fresh; the update
        // check is not run, and last round's candidate stays.
        runner.respond(vec![BREW, "update"], brew_answer(0, ""));
        runner.delay(vec![BREW, "update"], Duration::from_millis(1500));
        let reads_before = catalogue_reads(&runner);
        let second = session.refresh(&env, &opts).await;
        let reads_after = catalogue_reads(&runner);
        assert_eq!(
            reads_after,
            (reads_before.0 + 1, reads_before.1),
            "one `brew info` from the inventory before the update started, \
             and no `brew outdated` (nor `check_updates`' own `brew info`) \
             after it"
        );
        assert_eq!(brew_notes(&second), vec![InstanceNote::IndexUpdating]);
        assert_eq!(artifact_names(&second), vec!["jq"]);
        assert_eq!(update_names(&second), vec!["jq"]);
        assert!(
            second.errors.is_empty() && !second.stale,
            "a download still running is not a failed refresh: {:?}",
            second.errors
        );

        // Round three arrives with that update still running. Whatever the
        // catalogue would say now must not be read: if it were, jq would
        // move to 1.7.1 and its update would vanish.
        runner.respond(
            vec![BREW, "info", "--installed", "--json=v2"],
            brew_answer(0, &brew_info_jq("1.7.1")),
        );
        runner.respond(
            vec![BREW, "outdated", "--json=v2"],
            brew_answer(0, BREW_NOTHING),
        );
        let reads_before = catalogue_reads(&runner);
        let started = Instant::now();
        let third = session.refresh(&env, &opts).await;
        assert!(
            started.elapsed() < Duration::from_millis(1000),
            "a refresh behind a running update must not wait for it; took {:?}",
            started.elapsed()
        );
        assert_eq!(
            catalogue_reads(&runner),
            reads_before,
            "neither `brew info` nor `brew outdated` may run while `brew \
             update` is rewriting the catalogue: {:?}",
            runner.calls()
        );
        assert_eq!(brew_notes(&third), vec![InstanceNote::IndexUpdating]);
        assert_eq!(
            third.artifacts, second.artifacts,
            "the previous rows are kept"
        );
        assert_eq!(third.updates, second.updates, "the previous rows are kept");
        assert!(third.errors.is_empty() && !third.stale);

        // The update ends; the shell's automatic refresh reads normally.
        tokio::time::timeout(Duration::from_secs(10), background_change.notified())
            .await
            .expect("the end of an update a refresh reported must be announced");
        let fourth = session.refresh(&env, &opts).await;
        assert!(
            brew_notes(&fourth).is_empty(),
            "got {:?}",
            brew_notes(&fourth)
        );
        assert_eq!(
            catalogue_reads(&runner),
            (reads_before.0 + 2, reads_before.1 + 1),
            "the inventory's `brew info`, `brew outdated`, and \
             `check_updates`' own `brew info` for the names"
        );
        assert_eq!(fourth.artifacts[0].version, "1.7.1");
        assert!(update_names(&fourth).is_empty());
        assert!(fourth.errors.is_empty() && !fourth.stale);
    }

    #[tokio::test]
    async fn test_the_refresh_a_background_change_sets_off_is_not_merged_into_the_one_in_flight() {
        // The reviewer's probe (rereview F1), kept. A refresh finds a
        // `brew update` running and brew's worker returns at once with
        // `IndexUpdating`; the refresh itself is still in flight because
        // another source is slow. The update ends mid-refresh and wakes
        // `background_change`. `Session::refresh` used to merge the shell
        // loop's call into the refresh in flight and hand back its
        // snapshot -- still `IndexUpdating` -- and that wake-up was the
        // update's only one, so the notice stuck.
        let runner = Arc::new(MockRunner::new());
        runner.respond(vec![BREW, "--version"], brew_answer(0, "Homebrew 7.0.3\n"));
        runner.respond(vec![BREW, "update"], brew_answer(0, ""));
        runner.delay(vec![BREW, "update"], Duration::from_millis(400));
        runner.respond(
            vec![BREW, "info", "--installed", "--json=v2"],
            brew_answer(0, &brew_info_jq("1.7.1")),
        );
        runner.respond(
            vec![BREW, "outdated", "--json=v2"],
            brew_answer(0, BREW_NOTHING),
        );
        let background_change = Arc::new(tokio::sync::Notify::new());
        let brew = Arc::new(
            BrewAdapter::new(runner.clone())
                .with_path_exists_fn(apple_silicon_layout)
                .with_update_patience(Duration::from_millis(100))
                .with_background_change(background_change.clone()),
        );
        // Start the 400 ms update: a check that gives up on it after its
        // 100 ms patience, and so asks to hear when it ends.
        let inst = brew.detect(&non_root_env()).await.remove(0);
        let started = brew.check_updates(&inst, &CheckOptions::default()).await;
        assert!(
            matches!(started, Err(AdapterError::IndexUpdating)),
            "got {started:?}"
        );
        let (slow, slow_state) = FakeAdapter::new("slow");
        {
            let mut s = slow_state.lock().unwrap();
            s.instances = vec![make_instance("slow", "slow:1")];
            s.inventory_delay = Duration::from_millis(1500);
        }
        let session = Session::build(
            Arc::new(VecSink::new()),
            vec![brew, slow],
            None,
            background_change,
        );
        let env = non_root_env();
        let opts = CheckOptions::default();

        // What `ipc::refresh_on_background_change` does, one iteration.
        let follow_up = {
            let session = session.clone();
            let env = env.clone();
            tokio::spawn(async move {
                session.background_change().await;
                session.refresh(&env, &opts).await
            })
        };
        let in_flight = session.refresh(&env, &opts).await;
        assert_eq!(
            brew_notes(&in_flight),
            vec![InstanceNote::IndexUpdating],
            "the refresh in flight saw the update running"
        );
        let follow_up = tokio::time::timeout(Duration::from_secs(10), follow_up)
            .await
            .expect("the end of the update must set off a refresh")
            .expect("follow-up task");

        assert!(
            brew_notes(&follow_up).is_empty(),
            "the refresh after the update ended must read the catalogue, \
             not hand back the in-flight refresh's snapshot: notes {:?}, \
             generation {} vs {}, calls {:?}",
            brew_notes(&follow_up),
            follow_up.generation,
            in_flight.generation,
            runner.calls()
        );
        assert_eq!(
            catalogue_reads(&runner),
            (2, 1),
            "the inventory's `brew info`, `brew outdated`, and \
             `check_updates`' own `brew info` for the names"
        );
        assert_eq!(artifact_names(&follow_up), vec!["jq"]);
    }

    /// How many `brew update`s have been run.
    fn update_runs(runner: &MockRunner) -> usize {
        runner
            .calls()
            .iter()
            .filter(|c| c.get(1).map(String::as_str) == Some("update"))
            .count()
    }

    #[tokio::test]
    async fn test_a_background_change_after_a_failed_update_starts_no_update_of_its_own() {
        // F1 in the final concurrency review. An update a refresh
        // announced fails while another refresh is in flight -- one that
        // started before the failure but reaches brew after it, because a
        // slow source holds up detection. That refresh used to consume
        // `unreported_failure`, so the refresh the failure's wake-up sets
        // off found no flag, an expired TTL, and started a second `brew
        // update` nobody asked for.
        let runner = Arc::new(MockRunner::new());
        runner.respond(vec![BREW, "--version"], brew_answer(0, "Homebrew 7.0.3\n"));
        runner.respond(vec![BREW, "update"], brew_answer(1, ""));
        runner.delay(vec![BREW, "update"], Duration::from_millis(400));
        runner.respond(
            vec![BREW, "info", "--installed", "--json=v2"],
            brew_answer(0, &brew_info_jq("1.6")),
        );
        runner.respond(
            vec![BREW, "outdated", "--json=v2"],
            brew_answer(0, BREW_OUTDATED_JQ),
        );
        let background_change = Arc::new(tokio::sync::Notify::new());
        let brew = Arc::new(
            BrewAdapter::new(runner.clone())
                .with_path_exists_fn(apple_silicon_layout)
                .with_update_patience(Duration::from_millis(100))
                .with_background_change(background_change.clone()),
        );
        // Start the 400 ms update and give up on it after 100 ms, so its
        // end is announced.
        let inst = brew.detect(&non_root_env()).await.remove(0);
        let started = brew.check_updates(&inst, &CheckOptions::default()).await;
        assert!(
            matches!(started, Err(AdapterError::IndexUpdating)),
            "got {started:?}"
        );
        let (slow, slow_state) = FakeAdapter::new("slow");
        {
            let mut s = slow_state.lock().unwrap();
            s.instances = vec![make_instance("slow", "slow:1")];
            // Detection waits for every adapter, so brew's worker in the
            // refresh below starts after this -- after the update failed.
            s.detect_delay = Duration::from_millis(600);
        }
        let session = Session::build(
            Arc::new(VecSink::new()),
            vec![brew, slow],
            None,
            background_change,
        );
        let env = non_root_env();
        let opts = CheckOptions::default();

        // What `ipc::refresh_on_background_change` does, one iteration.
        let follow_up = {
            let session = session.clone();
            let env = env.clone();
            tokio::spawn(async move {
                session.background_change().await;
                session.refresh(&env, &opts).await
            })
        };
        let in_flight = session.refresh(&env, &opts).await;
        let follow_up = tokio::time::timeout(Duration::from_secs(10), follow_up)
            .await
            .expect("the end of the update must set off a refresh")
            .expect("follow-up task");

        assert_eq!(
            brew_notes(&in_flight),
            vec![InstanceNote::IndexMayBeStale],
            "the refresh in flight read brew after the failure, and says so"
        );
        assert_eq!(
            brew_notes(&follow_up),
            vec![InstanceNote::IndexMayBeStale],
            "the refresh the failure set off says so too"
        );
        assert_eq!(
            update_runs(&runner),
            1,
            "neither refresh may start another `brew update`: {:?}",
            runner.calls()
        );

        // The rule is "not by itself", not "never": the next refresh --
        // the notice's "Try again" -- does try again.
        session.refresh(&env, &opts).await;
        assert_eq!(update_runs(&runner), 2, "{:?}", runner.calls());
    }

    #[tokio::test]
    async fn test_a_first_refresh_behind_a_running_brew_update_shows_no_rows_it_does_not_have() {
        // The cold start: nothing to carry forward. The refresh still must
        // not read the catalogue, so this source has no rows and no error,
        // only the note. Its copy (`sourceNotice.indexUpdating` in both
        // locales) mentions no rows, so it promises none. Reachable only
        // through a refresh dropped after starting the update: any refresh
        // that runs to its commit puts this instance in the snapshot.
        let runner = Arc::new(MockRunner::new());
        runner.respond(vec![BREW, "--version"], brew_answer(0, "Homebrew 7.0.3\n"));
        runner.respond(vec![BREW, "update"], brew_answer(0, ""));
        runner.delay(vec![BREW, "update"], Duration::from_millis(1500));
        runner.respond(
            vec![BREW, "info", "--installed", "--json=v2"],
            brew_answer(0, &brew_info_jq("1.6")),
        );
        runner.respond(
            vec![BREW, "outdated", "--json=v2"],
            brew_answer(0, BREW_OUTDATED_JQ),
        );
        let brew = Arc::new(
            BrewAdapter::new(runner.clone())
                .with_path_exists_fn(apple_silicon_layout)
                .with_update_patience(Duration::from_millis(100)),
        );
        // An update left running by a check whose refresh never committed.
        let inst = brew.detect(&non_root_env()).await.remove(0);
        let started = brew.check_updates(&inst, &CheckOptions::default()).await;
        assert!(
            matches!(started, Err(AdapterError::IndexUpdating)),
            "got {started:?}"
        );
        let session = Session::with_adapters(Arc::new(VecSink::new()), vec![brew], None);

        let snapshot = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        assert_eq!(catalogue_reads(&runner), (0, 0), "{:?}", runner.calls());
        assert_eq!(brew_notes(&snapshot), vec![InstanceNote::IndexUpdating]);
        assert!(snapshot.artifacts.is_empty(), "{:?}", snapshot.artifacts);
        assert!(snapshot.updates.is_empty(), "{:?}", snapshot.updates);
        assert!(snapshot.errors.is_empty() && !snapshot.stale);
    }

    #[test]
    #[should_panic(expected = "two adapters registered with the same id")]
    fn test_registering_two_adapters_with_one_id_fails_at_construction() {
        // A duplicate adapter id used to replace the first adapter in the
        // map without a word, and adapter-id uniqueness is half of what
        // makes `model::instance_id` collision-free across adapters.
        let (first, _) = FakeAdapter::new("fake");
        let (second, _) = FakeAdapter::new("fake");
        let sink = Arc::new(VecSink::new());
        let _ = Session::with_adapters(sink, vec![first, second], None);
    }

    // ---- The first round's preview (`refresh_recording`'s `preview`) ----

    /// A source under `adapter_id` with one instance, `<adapter_id>:1`,
    /// listing `names`, each with an update, and its update check held at
    /// `gate` until the test opens it.
    fn gated_source(
        adapter_id: &str,
        names: &[&str],
        gate: &Arc<tokio::sync::Semaphore>,
    ) -> (Arc<FakeAdapter>, Arc<Mutex<FakeState>>) {
        let (adapter, state) = FakeAdapter::new(adapter_id);
        let id = format!("{adapter_id}:1");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance(adapter_id, &id)];
            s.artifacts.insert(
                id.clone(),
                names.iter().map(|name| make_artifact(&id, name)).collect(),
            );
            s.updates.insert(
                id.clone(),
                names.iter().map(|name| make_update(&id, name)).collect(),
            );
            s.check_gate = Some(gate.clone());
        }
        (adapter, state)
    }

    /// A `preview` callback that sends what it is handed down `tx`.
    fn send_preview(
        tx: tokio::sync::mpsc::UnboundedSender<InventoryPreview>,
    ) -> impl FnOnce(InventoryPreview) + Send + 'static {
        move |preview| {
            let _ = tx.send(preview);
        }
    }

    /// One refresh, run in a task of its own, its previews sent down the
    /// returned receiver.
    fn spawn_previewing_refresh(
        session: &Arc<Session>,
    ) -> (
        tokio::task::JoinHandle<(u64, Snapshot)>,
        tokio::sync::mpsc::UnboundedReceiver<InventoryPreview>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let session = session.clone();
        let handle = tokio::spawn(async move {
            session
                .refresh_recording(
                    &non_root_env(),
                    &CheckOptions::default(),
                    |_, _| {},
                    send_preview(tx),
                )
                .await
        });
        (handle, rx)
    }

    async fn next_preview(
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<InventoryPreview>,
    ) -> InventoryPreview {
        tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("a preview while the update checks are held")
            .expect("the round previewed")
    }

    fn preview_names(preview: &InventoryPreview) -> Vec<&str> {
        preview
            .artifacts
            .iter()
            .map(|a| a.key.name.as_str())
            .collect()
    }

    #[tokio::test]
    async fn test_the_first_round_previews_every_list_before_its_update_checks_end() {
        // The slow half of the first round -- every update check -- is
        // held open; what the sources listed is handed out meanwhile,
        // without a thing committed.
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let (a, a_state) = gated_source("a", &["jq"], &gate);
        // Asked first, and answers last: listed first all the same.
        a_state.lock().unwrap().inventory_delay = Duration::from_millis(50);
        let (b, _) = gated_source("b", &["ripgrep", "fd"], &gate);
        let session = Session::with_adapters(Arc::new(VecSink::new()), vec![b, a], None);

        let (refresh, mut previews) = spawn_previewing_refresh(&session);
        let preview = next_preview(&mut previews).await;

        assert_eq!(preview.round, 1, "the round still running");
        assert_eq!(
            preview
                .instances
                .iter()
                .map(|i| i.id.as_str())
                .collect::<Vec<_>>(),
            ["a:1", "b:1"]
        );
        assert_eq!(preview_names(&preview), ["jq", "ripgrep", "fd"]);
        assert_eq!(
            session.snapshot(),
            Snapshot::empty(),
            "nothing committed: the session still has the startup placeholder"
        );
        assert!(!refresh.is_finished(), "the update checks are still held");

        gate.add_permits(1);
        let (round, snapshot) = refresh.await.expect("refresh task");
        assert_eq!(round, preview.round, "the round it previewed committed");
        assert_eq!(snapshot.artifacts, preview.artifacts);
        assert_eq!(update_names(&snapshot), ["fd", "jq", "ripgrep"]);
        assert!(
            previews.recv().await.is_none(),
            "one preview a round, and no more"
        );
    }

    /// The preview names each AI coding tool's family as the round's commit
    /// will (`families::assign`), so the AI Tools filter works on it.
    #[tokio::test]
    async fn test_the_first_rounds_preview_tags_each_ai_tool_with_its_family() {
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let (brew, _) = gated_source("brew", &["ollama", "jq"], &gate);
        let session = Session::with_adapters(Arc::new(VecSink::new()), vec![brew], None);

        let (refresh, mut previews) = spawn_previewing_refresh(&session);
        let preview = next_preview(&mut previews).await;
        let families: Vec<_> = preview
            .artifacts
            .iter()
            .map(|a| (a.key.name.as_str(), a.facts.family.as_deref()))
            .collect();
        assert_eq!(families, [("ollama", Some("ollama")), ("jq", None)]);

        gate.add_permits(1);
        let (_, snapshot) = refresh.await.expect("refresh task");
        assert_eq!(snapshot.artifacts, preview.artifacts);
    }

    #[tokio::test]
    async fn test_a_preview_is_no_commit_so_a_caller_waiting_on_the_round_gets_it_whole() {
        // Committing the preview would mark the round done: a caller
        // queued behind it would be handed a snapshot with no updates in
        // it. A second call while the first round is still checking --
        // the menu bar's Check Again during the startup refresh: it gets
        // the whole of a round, never the preview, and only once a round
        // has committed.
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let (a, _) = gated_source("a", &["jq"], &gate);
        let session = Session::with_adapters(Arc::new(VecSink::new()), vec![a], None);

        let (first, mut previews) = spawn_previewing_refresh(&session);
        let preview = next_preview(&mut previews).await;
        // Nothing on the list can be acted on yet: there is no committed
        // source to plan against, so the window's Uninstall, held off over
        // the list, would be refused here too.
        let jq = &preview.artifacts[0].key;
        let refused = session
            .issue_plan(&OpRequest {
                kind: OpKind::Uninstall,
                instance_id: jq.instance_id.clone(),
                artifact_kind: jq.kind,
                name: jq.name.clone(),
            })
            .await;
        assert!(
            matches!(refused, Err(AdapterError::SourceGone { .. })),
            "{refused:?}"
        );
        let second = {
            let session = session.clone();
            tokio::spawn(async move {
                session
                    .refresh(&non_root_env(), &CheckOptions::default())
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !second.is_finished(),
            "nothing has committed for it to take"
        );
        assert_eq!(session.snapshot(), Snapshot::empty());

        gate.add_permits(1);
        let (_, whole) = first.await.expect("first refresh");
        let second = second.await.expect("second refresh");
        for snapshot in [&whole, &second] {
            assert_eq!(update_names(snapshot), ["jq"], "{snapshot:?}");
            assert!(snapshot.refreshed_at.is_some());
        }
    }

    #[tokio::test]
    async fn test_only_a_round_before_any_commit_previews() {
        let gate = Arc::new(tokio::sync::Semaphore::new(1));
        let (a, _) = gated_source("a", &["jq"], &gate);
        let session = Session::with_adapters(Arc::new(VecSink::new()), vec![a], None);
        let previews = Arc::new(Mutex::new(Vec::new()));
        for _ in 0..3 {
            let previews = previews.clone();
            session
                .refresh_recording(
                    &non_root_env(),
                    &CheckOptions::default(),
                    |_, _| {},
                    move |preview| previews.lock().unwrap().push(preview.round),
                )
                .await;
        }
        assert_eq!(
            *previews.lock().unwrap(),
            [1],
            "the first round, and not the two after it, which had its list on screen"
        );
    }

    #[tokio::test]
    async fn test_a_first_round_dropped_before_it_commits_leaves_the_next_one_to_preview() {
        // "First" means "before any round has committed", not "the first
        // round numbered": a round dropped mid-flight left the window on
        // the startup placeholder, and the next one previews again.
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let (a, _) = gated_source("a", &["jq"], &gate);
        let session = Session::with_adapters(Arc::new(VecSink::new()), vec![a], None);

        let (dropped, mut previews) = spawn_previewing_refresh(&session);
        assert_eq!(next_preview(&mut previews).await.round, 1);
        dropped.abort();
        let _ = dropped.await;
        assert_eq!(session.snapshot(), Snapshot::empty(), "it never committed");

        let (next, mut previews) = spawn_previewing_refresh(&session);
        assert_eq!(next_preview(&mut previews).await.round, 2);
        gate.add_permits(1);
        let (round, _) = next.await.expect("the next refresh");
        assert_eq!(round, 2);
    }

    #[tokio::test]
    async fn test_a_source_whose_list_failed_is_absent_from_the_preview() {
        // "a"'s read fails, "b"'s panics: neither is in the preview, and
        // neither holds it back. Both sources are still among its
        // instances, as detection found them; what the round commits
        // says what failed.
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let (a, a_state) = gated_source("a", &["jq"], &gate);
        a_state.lock().unwrap().failing = vec!["a:1".to_string()];
        let (b, b_state) = gated_source("b", &["fd"], &gate);
        b_state.lock().unwrap().panicking_inventory = vec!["b:1".to_string()];
        let (c, _) = gated_source("c", &["ripgrep"], &gate);
        let session = Session::with_adapters(Arc::new(VecSink::new()), vec![a, b, c], None);

        let (refresh, mut previews) = spawn_previewing_refresh(&session);
        let preview = next_preview(&mut previews).await;
        assert_eq!(preview_names(&preview), ["ripgrep"]);
        assert_eq!(preview.instances.len(), 3);

        gate.add_permits(1);
        let (_, snapshot) = refresh.await.expect("refresh task");
        let failed: Vec<&str> = snapshot
            .errors
            .iter()
            .map(|e| e.instance_id.as_str())
            .collect();
        assert_eq!(failed, ["a:1", "b:1"]);
    }

    #[tokio::test]
    async fn test_no_preview_when_nothing_was_listed() {
        // A source that listed nothing, a source that is not answering
        // (never asked for its list), and no source at all: nothing to
        // show early, and the startup placeholder says so until the round
        // commits.
        let (empty, empty_state) = FakeAdapter::new("empty");
        empty_state.lock().unwrap().instances = vec![make_instance("empty", "empty:1")];
        let (stopped, stopped_state) = FakeAdapter::new("stopped");
        stopped_state.lock().unwrap().instances =
            vec![crate::session::test_support::make_unavailable_instance(
                "stopped",
                "stopped:1",
                Unavailable::NotRunning,
            )];
        let previews = Arc::new(Mutex::new(0));
        for adapters in [
            vec![empty as Arc<dyn Adapter>, stopped as Arc<dyn Adapter>],
            vec![],
        ] {
            let session = Session::with_adapters(Arc::new(VecSink::new()), adapters, None);
            let previews = previews.clone();
            let (round, _) = session
                .refresh_recording(
                    &non_root_env(),
                    &CheckOptions::default(),
                    |_, _| {},
                    move |_| *previews.lock().unwrap() += 1,
                )
                .await;
            assert_eq!(round, 1);
        }
        assert_eq!(*previews.lock().unwrap(), 0);
    }

    #[test]
    fn test_an_inventory_preview_round_trips_in_the_shape_the_window_reads() {
        let preview = InventoryPreview {
            round: 1,
            instances: vec![make_instance("fake", "fake:1")],
            artifacts: vec![make_artifact("fake:1", "jq")],
        };
        let json = serde_json::to_value(&preview).expect("serialize");
        let mut keys: Vec<&str> = json
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["artifacts", "instances", "round"],
            "nothing about updates, errors or staleness"
        );
        assert_eq!(json["round"], 1);
        assert_eq!(json["artifacts"][0]["key"]["name"], "jq");
        let back: InventoryPreview = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, preview);
    }
}
