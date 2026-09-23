//! Fixture constructors for tests.
//!
//! Public, not `#[cfg(test)]`, for one reason: the integration tests in
//! `crates/canager-core/tests/` and the Tauri shell's own tests are separate
//! crates and cannot see a `#[cfg(test)]` item here. `ManagerInstance` is
//! built in 45 places across this workspace, seven of which are production
//! `detect()` implementations that stay struct literals on purpose -- the
//! compiler's exhaustiveness check is what makes every adapter answer a new
//! field's question. The other ~38 are fixtures that only ever want "a
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

use crate::model::{InstanceStatus, ManagerInstance, ReadOnlyReason, Scope, Unavailable};
use std::path::PathBuf;

/// A plausible, available, writable instance of `adapter_id` under `id`.
///
/// Meant to be combined with struct update syntax for whatever the test
/// actually cares about:
///
/// ```
/// use canager_core::model::ManagerInstance;
/// use canager_core::testing::manager_instance;
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
        unverified_version: None,
        read_only_reason: None,
        status: InstanceStatus::default(),
    }
}

/// `manager_instance`, but for a source Canager may list and never change --
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
