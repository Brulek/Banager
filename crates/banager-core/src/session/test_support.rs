//! Shared `#[cfg(test)]` scaffolding for `session`'s split test modules
//! (`session::tests`, `session::refresh::tests`, `session::plans::tests`).
//!
//! Hoisted in the Task 14 fix round: `non_root_env`/`root_env` and the
//! common parts of each file's `FakeAdapter` were duplicated verbatim
//! across all three test modules before this (the review finding this file
//! addresses). Only pieces that really are identical across every test
//! module live here; each file keeps whatever is actually specific to it
//! (its own `FakeAdapter`/`FakeState` shape, and the tests themselves).

use crate::adapters::AdapterMeta;
use crate::model::{
    CancelPolicy, ManagerInstance, OpRequest, Plan, PlanAction, ReadOnlyReason, Reconciled,
    ResourceLock, Unavailable,
};
use crate::runner::HostEnv;
use std::path::PathBuf;

/// `HostEnv` for a non-root, ordinary-user refresh -- the common case every
/// test that isn't specifically exercising the root refusal uses.
pub(super) fn non_root_env() -> HostEnv {
    HostEnv {
        path_dirs: vec![],
        home: PathBuf::from("/tmp"),
        euid: 501,
        cargo_home: None,
        rustup_home: None,
        zdotdir: None,
        ollama_host: None,
    }
}

/// `HostEnv` for a root user, used by the tests covering
/// `BrewAdapter::refuses_as_root`.
pub(super) fn root_env() -> HostEnv {
    HostEnv {
        path_dirs: vec![],
        home: PathBuf::from("/var/root"),
        euid: 0,
        cargo_home: None,
        rustup_home: None,
        zdotdir: None,
        ollama_host: None,
    }
}

/// A minimal `ManagerInstance` for a fake adapter, keyed by `id` under
/// `adapter_id`.
pub(super) fn make_instance(adapter_id: &str, id: &str) -> ManagerInstance {
    crate::testing::manager_instance(adapter_id, id)
}

/// `make_instance`, but for a source Canager may list and never change --
/// the capability half of the actionability invariant (spec §2.5).
pub(super) fn make_read_only_instance(
    adapter_id: &str,
    id: &str,
    reason: ReadOnlyReason,
) -> ManagerInstance {
    ManagerInstance {
        read_only_reason: Some(reason),
        ..make_instance(adapter_id, id)
    }
}

/// `make_instance`, but for a source that did not answer the last refresh
/// -- the state half of that same invariant.
pub(super) fn make_unavailable_instance(
    adapter_id: &str,
    id: &str,
    unavailable: Unavailable,
) -> ManagerInstance {
    crate::testing::unavailable_instance(adapter_id, id, unavailable)
}

/// The `AdapterMeta` every `FakeAdapter` in these test modules registers
/// itself under.
pub(super) fn fake_adapter_meta(id: &str) -> AdapterMeta {
    AdapterMeta {
        id: id.to_string(),
        name: id.to_string(),
        kind: "fake".to_string(),
        platforms: vec!["macos".to_string()],
        homepage: "https://example.invalid".to_string(),
        schema_version: 1,
        verified_versions: vec![],
    }
}

/// The `Plan` every `FakeAdapter::plan` in these test modules returns.
pub(super) fn fake_plan(inst: &ManagerInstance, req: &OpRequest) -> Plan {
    Plan {
        request: req.clone(),
        action: PlanAction::Command {
            program: inst.exe_path.clone(),
            args: vec!["do".to_string(), req.name.clone()],
            env: vec![],
        },
        needs_password: false,
        locks: vec![ResourceLock(inst.id.clone())],
        cancel_policy: CancelPolicy::KillThenReconcile,
        warnings: vec![],
        affected: vec![],
        timeout_secs: 60,
    }
}

/// The `Reconciled` every `FakeAdapter::reconcile` in these test modules
/// returns.
pub(super) fn fake_reconciled() -> Reconciled {
    Reconciled {
        present: true,
        version: None,
    }
}
