//! Fixture constructors for tests.
//!
//! Public, not `#[cfg(test)]`, for one reason: the integration tests in
//! `crates/banager-core/tests/` and the Tauri shell's own tests are separate
//! crates and cannot see a `#[cfg(test)]` item here. `ManagerInstance` is
//! built in dozens of places across this workspace. Every adapter's
//! production `detect()` stays a struct literal on purpose -- the
//! compiler's exhaustiveness check is what makes every adapter answer a new
//! field's question. Most of the rest are fixtures that only ever want "a
//! plausible instance, with this one thing different", and each new field
//! cost all of them an edit until this existed (spec §5's note).
//!
//! Nothing in production may call the fixture constructors above; they
//! build instances that describe no real machine, so it is harmless for
//! them to be reachable from a release build.
//!
//! `expire_issued_plans` below is not one of those fixtures: it reaches
//! into a live `Session` and mutates real state, rather than building a
//! disconnected value. It is gated behind the `test-support` feature (on
//! top of `cfg(test)`, which alone would hide it from `src-tauri`'s own
//! tests -- a separate crate, so `cfg(test)` there does not apply to this
//! one) so that it cannot end up in a release build. See its own doc
//! comment and this crate's `Cargo.toml` `[features]` section.
//!
//! `unique_temp_path`, `TempTree` and `Rng` at the end are `#[cfg(test)]`
//! and crate-private: only this crate's own unit tests use them.

use crate::model::{
    ArtifactFacts, ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, InstanceStatus,
    ManagerInstance, Plan, PlanAction, ReadOnlyReason, Scope, Unavailable,
};
use std::path::{Path, PathBuf};

/// A plausible, available, writable instance of `adapter_id` under `id`.
///
/// Meant to be combined with struct update syntax for whatever the test
/// actually cares about:
///
/// ```
/// use banager_core::model::ManagerInstance;
/// use banager_core::testing::manager_instance;
/// use std::path::PathBuf;
///
/// let inst = ManagerInstance {
///     prefix: PathBuf::from("/opt/homebrew"),
///     ..manager_instance("brew", "brew:/opt/homebrew")
/// };
/// assert!(inst.writable() && inst.available());
/// ```
pub fn manager_instance(adapter_id: &str, id: &str) -> ManagerInstance {
    ManagerInstance {
        id: id.to_string(),
        adapter_id: adapter_id.to_string(),
        exe_path: PathBuf::from("/bin/true"),
        prefix: PathBuf::from("/"),
        scope: Scope::User,
        version: Some("1.0".to_string()),
        answered_at: None,
        unverified_version: None,
        read_only_reason: None,
        status: InstanceStatus::default(),
    }
}

/// A plausible installed artifact: `name` of `kind` in `instance_id`, at
/// version 1.0, asked for by the user, with nothing else known. As with
/// `manager_instance`, combine it with struct update syntax for what the
/// test cares about; a new field of `InstalledArtifact` is then one edit
/// here, not one in every test's own builder.
pub fn installed_artifact(instance_id: &str, kind: ArtifactKind, name: &str) -> InstalledArtifact {
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
        facts: ArtifactFacts::default(),
    }
}

/// `manager_instance`, but for a source Banager may list and never change --
/// the capability half of the actionability invariant (spec §2.5).
pub fn read_only_instance(adapter_id: &str, id: &str, reason: ReadOnlyReason) -> ManagerInstance {
    ManagerInstance {
        read_only_reason: Some(reason),
        ..manager_instance(adapter_id, id)
    }
}

/// `manager_instance`, but for a source that did not answer the last
/// refresh -- the state half of that same invariant.
pub fn unavailable_instance(
    adapter_id: &str,
    id: &str,
    unavailable: Unavailable,
) -> ManagerInstance {
    ManagerInstance {
        status: InstanceStatus {
            unavailable: Some(unavailable),
            notes: Vec::new(),
        },
        ..manager_instance(adapter_id, id)
    }
}

/// The parts of a plan that runs a command, for a test that asserts what
/// argv an adapter built -- three accessors rather than a destructuring at
/// each of the ~40 assertions that read `plan.program`/`args`/`env` before
/// `PlanAction` existed. Each panics on a `TrashPaths` plan, saying so: a
/// test that expected a command and got a path list has found a bug, and
/// the message names it. Test-only by nature: a production reader matches
/// both arms (`run_plan`, `OperationManager::summaries`) and never calls
/// these.
fn command_parts(plan: &Plan) -> (&Path, &[String], &[(String, String)]) {
    match &plan.action {
        PlanAction::Command { program, args, env } => (program, args, env),
        PlanAction::TrashPaths { paths, .. } => panic!(
            "this plan runs no command: it moves {} path(s) to the Trash",
            paths.len()
        ),
    }
}

/// The program a `Command` plan runs. See `command_parts`.
pub fn command_program(plan: &Plan) -> &Path {
    command_parts(plan).0
}

/// The argv (without the program) a `Command` plan runs. See `command_parts`.
pub fn command_args(plan: &Plan) -> &[String] {
    command_parts(plan).1
}

/// The environment a `Command` plan adds. See `command_parts`.
pub fn command_env(plan: &Plan) -> &[(String, String)] {
    command_parts(plan).2
}

/// Ages every plan `session` is currently holding past its lifetime, so
/// that the next `submit` of one reports `SubmitError::Expired`.
///
/// Exists for the Tauri shell's own tests, which are a separate crate and
/// cannot reach into `Session` themselves. A plan's lifetime is measured
/// on the monotonic clock precisely so that nothing anyone can set
/// decides it (see `session::plans::StoredPlan`), which leaves a test no
/// clock to move and no ten minutes to spare.
///
/// `cfg`-gated behind `test-support` (see `Cargo.toml`) so this mutator
/// cannot ship in a release build: `src-tauri`'s tests enable the feature
/// through a `[dev-dependencies]` entry on this crate, which resolver v2
/// keeps out of the release binary's dependency graph.
#[cfg(any(test, feature = "test-support"))]
pub fn expire_issued_plans(session: &crate::session::Session) {
    let lifetime = crate::session::PLAN_LIFETIME + std::time::Duration::from_secs(1);
    for stored in session.issued_plans.lock().unwrap().values_mut() {
        stored.issued_monotonic = stored
            .issued_monotonic
            .checked_sub(lifetime)
            .expect("the monotonic clock is at least a plan lifetime past its origin");
    }
}

/// A `Session` over `adapters` whose `background_change` is `background_change`
/// itself, rather than a `Notify` `Session::with_adapters` makes on its own
/// and hands to nobody. Calling `background_change.notify_one()` then wakes
/// this session's `Session::background_change()` directly, the way a real
/// `BrewAdapter`'s clone of the *same* `Notify` does in `Session::new`
/// (`with_background_change`) when a `brew update` a refresh stopped
/// waiting for ends.
///
/// Exists for the Tauri shell's own tests: `ipc::refresh_on_background_change`
/// loops on `Session::background_change()` for the life of the app, and
/// proving it really refreshes when that resolves needs a way to wake it
/// on demand, without a real `BrewAdapter` and a real `brew update` to wait
/// out. `src-tauri` is a separate crate and cannot reach `Session::build`
/// (`pub(crate)` to this one) itself.
///
/// Same `cfg` gate as `expire_issued_plans` above, for the same reason: a
/// test-only seam into `Session`, kept out of a release build via the
/// `test-support` feature `src-tauri`'s `[dev-dependencies]` entry turns on.
#[cfg(any(test, feature = "test-support"))]
pub fn session_with_background_change(
    sink: std::sync::Arc<dyn crate::events::EventSink>,
    adapters: Vec<std::sync::Arc<dyn crate::adapters::Adapter>>,
    background_change: std::sync::Arc<tokio::sync::Notify>,
) -> std::sync::Arc<crate::session::Session> {
    crate::session::Session::build(sink, adapters, None, background_change)
}

#[cfg(test)]
pub(crate) use tree::{unique_temp_path, Rng, TempTree};

/// Fixtures for this crate's own unit tests that make folders and files
/// under the temp folder: test-only and crate-private, in a module with a
/// body so `tests/safety_source_test.rs` reads none of it as production.
#[cfg(test)]
pub(crate) mod tree {
    use std::path::{Path, PathBuf};

    /// A unique path under the temp folder, `banager-{label}-{pid}-{nanos}`,
    /// so tests that create files on disk cannot collide with each other or
    /// with a previous run. Nothing is made there; test-only.
    pub(crate) fn unique_temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "banager-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    /// A fresh folder under the temp folder, canonical (`/var` is a link on a
    /// Mac), removed with everything in it when dropped -- folders locked by
    /// the test unlocked first. For the unit tests that build a Mac's folders
    /// and links to walk (`protected::round`, `commands`); test-only.
    pub(crate) struct TempTree {
        pub(crate) root: PathBuf,
        locked: Vec<PathBuf>,
    }

    impl TempTree {
        pub(crate) fn new(tag: &str) -> TempTree {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let raw = std::env::temp_dir().join(format!(
                "banager-tree-{tag}-{}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&raw).unwrap();
            TempTree {
                root: std::fs::canonicalize(raw).unwrap(),
                locked: Vec::new(),
            }
        }

        pub(crate) fn at(&self, rel: &str) -> PathBuf {
            self.root.join(rel)
        }

        pub(crate) fn dir(&self, rel: &str) -> PathBuf {
            let path = self.at(rel);
            std::fs::create_dir_all(&path).unwrap();
            path
        }

        /// A file of `mode` at `rel`, its folders made; its contents are never run.
        pub(crate) fn file(&self, rel: &str, mode: u32) -> PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let path = self.at(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"never run").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
            path
        }

        /// A link at `rel` to `target`, its folders made. Panics where `rel`
        /// is there already.
        pub(crate) fn link(&self, rel: &str, target: impl AsRef<Path>) -> PathBuf {
            let path = self.at(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(target, &path).unwrap();
            path
        }

        /// `link`, but a name linked twice keeps its first link.
        pub(crate) fn link_keeping_first(&self, rel: &str, target: impl AsRef<Path>) -> PathBuf {
            let path = self.at(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let _ = std::os::unix::fs::symlink(target, &path);
            path
        }

        /// `rel` set to `mode` (a folder that cannot be searched or read),
        /// set back before the tree is removed.
        pub(crate) fn lock(&mut self, rel: &str, mode: u32) {
            use std::os::unix::fs::PermissionsExt;
            let path = self.at(rel);
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
            self.locked.push(path);
        }

        /// `rel` under the tree, spelled from the data volume.
        pub(crate) fn on_data_volume(&self, rel: &str) -> PathBuf {
            Path::new(crate::protected::DATA_VOLUME)
                .join(self.root.strip_prefix("/").unwrap())
                .join(rel)
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            for path in self.locked.iter().rev() {
                let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
            }
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// Reproducible numbers, with no new dependency (xorshift): the random
    /// trees the walk tests build from a seed.
    pub(crate) struct Rng(pub(crate) u64);

    impl Rng {
        pub(crate) fn below(&mut self, n: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % n as u64) as usize
        }

        pub(crate) fn pick<'a>(&mut self, items: &'a [String]) -> &'a str {
            &items[self.below(items.len())]
        }

        /// `rel` with one of its names, picked at random, spelled another
        /// way a Mac's disk takes for the same name: in capitals, or with a
        /// long s, a Kelvin sign or an `st` ligature (`protected::AS_ASCII`).
        pub(crate) fn shout(&mut self, rel: &str) -> String {
            let mut names: Vec<String> = rel.split('/').map(str::to_string).collect();
            let at = self.below(names.len());
            names[at] = match self.below(4) {
                0 => names[at].replace(['s', 'S'], "\u{17F}"),
                1 => names[at].replace(['k', 'K'], "\u{212A}"),
                2 => names[at]
                    .replace("st", "\u{FB06}")
                    .replace("St", "\u{FB05}"),
                _ => names[at].to_ascii_uppercase(),
            };
            names.join("/")
        }
    }
}
