//! `Session::refresh`: detect every registered adapter's instances, then
//! fetch inventory + updates per instance, merging failures into `errors`
//! and `stale` without ever aborting the whole refresh. Split out of
//! `session/mod.rs` (Task 14); no behaviour change from what shipped there.

use super::{DetectOutcome, Session, Snapshot, SourceError};
use crate::adapters::CheckOptions;
use crate::model::{InstanceNote, InstanceStatus, ManagerInstance, ResourceLock, Unavailable};
use crate::runner::HostEnv;
use std::sync::atomic::Ordering;
use tokio_util::task::AbortOnDropHandle;

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
    /// Concurrent calls are serialised: a call that starts while another is
    /// already running waits for it, then returns the snapshot that other
    /// call produced instead of running a second, redundant refresh -- see
    /// `refresh_seq` on `Session` for why that check cannot use
    /// `generation`. An instance whose `status.unavailable` is set is
    /// skipped by the per-instance fetch -- that is a *reported state*, not
    /// a failed refresh (Task 11) -- but it keeps the previous round's
    /// artifacts and updates, so the "here is what Canager saw last time"
    /// copy its notice carries is true rather than a promise over an empty
    /// group. An adapter whose `detect()` itself panics or is cancelled is
    /// the same story a level up: the instances it reported last time are
    /// kept, marked unavailable, rather than being taken off the screen as
    /// if the manager had been uninstalled.
    pub async fn refresh(
        self: &std::sync::Arc<Self>,
        env: &HostEnv,
        opts: &CheckOptions,
    ) -> Snapshot {
        let seq_before = self.refresh_seq.load(Ordering::SeqCst);
        let _gate = self.refresh_gate.lock().await;
        if self.refresh_seq.load(Ordering::SeqCst) != seq_before {
            return self.snapshot.lock().unwrap().clone();
        }

        let previous = self.snapshot.lock().unwrap().clone();
        // Owned copy (CheckOptions is Copy): each per-instance spawned task
        // below needs its own 'static value, and the caller's `&opts`
        // reference cannot outlive this function.
        let opts: CheckOptions = *opts;

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
        // group -- see `runner::real`'s `GroupKiller`).
        //
        // Deliberately not a `JoinSet`: that yields in completion order,
        // and both joins below depend on *fan-out* order -- detection for
        // `dedupe_instance_ids`' "first wins", the fan-out for pairing a
        // panicked task's `JoinError` with the instance whose rows it must
        // carry forward. An abort-on-drop handle awaits exactly like the
        // `JoinHandle` it wraps, so a refresh that runs to completion sees
        // identical join results; the only new `Err` it can produce is a
        // cancellation, and nothing but dropping this future cancels.
        let mut adapters: Vec<_> = self.adapters.values().collect();
        adapters.sort_by(|a, b| a.meta().id.cmp(&b.meta().id));
        let mut detect_handles = Vec::with_capacity(adapters.len());
        for adapter in adapters {
            // Cloned into the task because `tokio::spawn` needs a 'static
            // future: iterating `values()` by reference would tie it to
            // `&self`. (Written as an explicit clone rather than
            // `.values().cloned()` only because clippy's
            // `unnecessary_to_owned` misreads the latter here.)
            let adapter = adapter.clone();
            let env = env.clone();
            detect_handles.push((
                adapter.meta().id.clone(),
                AbortOnDropHandle::new(tokio::spawn(async move { adapter.detect(&env).await })),
            ));
        }
        let mut instances = Vec::new();
        let mut detect_errors = Vec::new();
        for (adapter_id, handle) in detect_handles {
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

        // Seeded before the fan-out because a skipped instance contributes
        // its carried-forward rows from inside the loop below.
        let mut artifacts = Vec::new();
        let mut updates = Vec::new();
        let mut handles = Vec::with_capacity(instances.len());
        for inst in instances.clone() {
            // Task 11: a source that already told us it is not answering is
            // a reported state, not a failed refresh, so it is never fanned
            // out to. It stays in `snapshot.instances` so the UI can render
            // its notice and offer to start it -- and it keeps whatever it
            // reported last time, exactly as the error paths below already
            // do. Dropping those rows is what made the unreachable notice's
            // "below is what Canager saw last time" a lie: a stopped Ollama
            // rendered a group header, that sentence, and no rows at all.
            // `issue_plan`'s gate (spec §2.5) is what stops those rows
            // offering an Uninstall button that could not possibly work.
            if inst.status.unavailable.is_some() {
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
                continue;
            }
            let Some(adapter) = self.adapters.get(&inst.adapter_id).cloned() else {
                continue;
            };
            let ops = self.ops.clone();
            let previous = previous.clone();
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
                    match adapter.inventory(&inst).await {
                        Ok(items) => artifacts.extend(items),
                        Err(e) => {
                            errors.push(SourceError {
                                instance_id: inst.id.clone(),
                                message: e.to_string(),
                            });
                            stale = true;
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
                    match adapter.check_updates(&inst, &opts).await {
                        Ok(outcome) => {
                            updates.extend(outcome.candidates);
                            notes.extend(outcome.notes);
                        }
                        Err(e) => {
                            errors.push(SourceError {
                                instance_id: inst.id.clone(),
                                message: e.to_string(),
                            });
                            stale = true;
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
                            // "Canager couldn't check" row the instant the
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
                            // When the inventory failed too, `artifacts`
                            // is itself last round's, carried forward by
                            // the branch above, so there is no fresh
                            // evidence to test against and everything is
                            // kept exactly as before.
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
                    (artifacts, updates, errors, stale, notes)
                })),
            ));
        }

        let mut errors = detect_errors;
        // Exactly "a refresh attempt failed", which is all any reader does
        // with it: `SnapshotStatus` turns it into the one page-wide "some
        // of this may be out of date, try again" banner, over a count of
        // `errors`.
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

        // Stamped because a refresh *ran*, not because it came back
        // perfect. Gated on `stale`, a Mac with one permanently unavailable
        // source carried `refreshed_at: None` for the rest of its life, and
        // `SnapshotStatus` reads a null timestamp as "Canager has never
        // finished a check" -- six healthy sources' worth of real data
        // described as no data at all. What "some of this may be old" means
        // is `stale`, and that is the flag that carries it.
        // `Snapshot::empty()` still has `refreshed_at: None`, which is what
        // keeps the startup branch in `SnapshotStatus` working: nothing but
        // an uncommitted snapshot can have a null timestamp now.
        let refreshed_at = Some(self.now());
        let candidate = Snapshot {
            generation: previous.generation,
            detect,
            instances,
            artifacts,
            updates,
            refreshed_at,
            stale,
            errors,
        };
        self.commit(previous, candidate)
    }

    /// Assigns the real generation number (bumping only on a content
    /// change), stores the result as the current snapshot, and marks this
    /// refresh complete via `refresh_seq` regardless of whether `generation`
    /// moved (M5 in the design review -- see `refresh_seq`'s field doc).
    fn commit(&self, previous: Snapshot, mut candidate: Snapshot) -> Snapshot {
        if !previous.same_content(&candidate) {
            candidate.generation = previous.generation + 1;
        }
        *self.snapshot.lock().unwrap() = candidate.clone();
        self.refresh_seq.fetch_add(1, Ordering::SeqCst);
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
    use crate::session::{DetectOutcome, Session, Snapshot};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::path::Path;
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
                detect_calls: 0,
                block_execute: false,
                inventory_calls: Vec::new(),
                blocking_inventory: Vec::new(),
                inventory_blocked: 0,
                inventory_dropped: 0,
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

    #[tokio::test]
    async fn test_concurrent_refresh_calls_are_coalesced() {
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.detect_delay = Duration::from_millis(100);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let session_a = session.clone();
        let session_b = session.clone();
        let (a, b) = tokio::join!(
            tokio::spawn(async move {
                session_a
                    .refresh(&non_root_env(), &CheckOptions::default())
                    .await
            }),
            tokio::spawn(async move {
                session_b
                    .refresh(&non_root_env(), &CheckOptions::default())
                    .await
            }),
        );
        let snap_a = a.expect("task a");
        let snap_b = b.expect("task b");
        assert_eq!(snap_a.generation, snap_b.generation);
        assert_eq!(
            state.lock().unwrap().detect_calls,
            1,
            "two concurrent refreshes must run detect() only once between them"
        );
    }

    #[tokio::test]
    async fn test_concurrent_refresh_calls_with_unchanged_content_are_still_coalesced() {
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
        let session_a = session.clone();
        let session_b = session.clone();
        let (a, b) = tokio::join!(
            tokio::spawn(async move {
                session_a
                    .refresh(&non_root_env(), &CheckOptions::default())
                    .await
            }),
            tokio::spawn(async move {
                session_b
                    .refresh(&non_root_env(), &CheckOptions::default())
                    .await
            }),
        );
        let snap_a = a.expect("task a");
        let snap_b = b.expect("task b");
        assert_eq!(snap_a.generation, snap_b.generation);
        assert_eq!(
            state.lock().unwrap().detect_calls,
            calls_before + 1,
            "two concurrent refreshes over unchanged content must still run detect() only once between them"
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
    async fn test_refresh_is_mutually_exclusive_with_an_operation_on_the_same_instance_but_not_others(
    ) {
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
        session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        state.lock().unwrap().inventory_calls.clear();

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

        let session_for_refresh = session.clone();
        let refresh_task = tokio::spawn(async move {
            session_for_refresh
                .refresh(&non_root_env(), &CheckOptions::default())
                .await
        });

        tokio::time::sleep(Duration::from_millis(200)).await;
        {
            let calls = state.lock().unwrap().inventory_calls.clone();
            assert!(
                calls.contains(&"fake:2".to_string()),
                "a different instance's refresh must proceed while fake:1 is locked"
            );
            assert!(
                !calls.contains(&"fake:1".to_string()),
                "fake:1's refresh must not run while fake:1's operation is still holding its lock"
            );
        }

        session.cancel(op_id);
        let snapshot = tokio::time::timeout(Duration::from_secs(2), refresh_task)
            .await
            .expect("refresh must not hang once the blocking operation is cancelled")
            .expect("refresh task panicked");
        assert!(snapshot.artifacts.iter().any(|a| a.key.name == "jq"));
        assert!(snapshot.artifacts.iter().any(|a| a.key.name == "wget"));
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
        }
    }

    /// A "Canager couldn't check" row, shaped the way
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
        // data", it is a row Canager knows is wrong. Uninstall jq and the
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
        // construction -- "Canager couldn't check" rows have no "unknown"
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
        // are kept, marked unavailable: Canager could not ask, so it may
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
            "kept as history, not as a source Canager can act on"
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
        // notice a stopped source renders says "below is what Canager saw
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
}
