//! `Session::scan_unknown`: the unknown-source scan over this session's
//! last committed snapshot. Its own file, like `refresh.rs` and
//! `plans.rs`, so the facade in `mod.rs` stays a facade.

use super::Session;
use crate::runner::HostEnv;
use crate::scan::{self, ScanBudget, UnknownScan};

impl Session {
    /// Which programs in the usual bin directories none of the registered
    /// sources account for, judged against the snapshot as it is *now*
    /// (`scan::scan_unknown`).
    ///
    /// Synchronous and blocking: up to `ScanBudget::default()` worth of
    /// directory reads. The Tauri shell runs it on the blocking pool
    /// (`ipc::scan_unknown`). The snapshot's instances and artifacts are
    /// cloned under the mutex and it is released before any file is
    /// touched; a refresh committing meanwhile neither waits for this nor
    /// changes what it already decided. No resource lock is taken --
    /// nothing here reads a package manager's own files, only directory
    /// entries and their metadata -- and nothing is written back: the
    /// result is the caller's, not session state, and does not enter the
    /// `Snapshot` (it is not about the managed sources, and would either
    /// bump `same_content` on every scan or be ignored by it). The
    /// backup-file patterns of every registered standalone recipe
    /// (`recipes::backup_globs`) are handed in for rule 4; only the ones
    /// with an instance in the snapshot claim anything.
    pub fn scan_unknown(&self, env: &HostEnv) -> UnknownScan {
        let (instances, artifacts) = {
            let snapshot = self.snapshot.lock().unwrap();
            (snapshot.instances.clone(), snapshot.artifacts.clone())
        };
        scan::scan_unknown(
            env,
            &instances,
            &artifacts,
            &crate::adapters::standalone::recipes::backup_globs(),
            ScanBudget::default(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support;
    use crate::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
    use crate::events::{EventSink, OpId, VecSink};
    use crate::model::{
        ArtifactKey, InstalledArtifact, ManagerInstance, OpRequest, Outcome, Plan, Reconciled,
        SearchHit,
    };
    use crate::runner::HostEnv;
    use crate::scan::EntryKind;
    use crate::session::Session;
    use async_trait::async_trait;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    /// Reports exactly the instance it was built with, and nothing
    /// installed under it. Enough for what this file has to prove: that
    /// the scan reads the *committed* snapshot and leaves it alone.
    struct FakeAdapter {
        meta: AdapterMeta,
        instance: ManagerInstance,
    }

    #[async_trait]
    impl Adapter for FakeAdapter {
        fn meta(&self) -> &AdapterMeta {
            &self.meta
        }

        async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
            vec![self.instance.clone()]
        }

        async fn inventory(
            &self,
            _inst: &ManagerInstance,
        ) -> Result<Vec<InstalledArtifact>, AdapterError> {
            Ok(Vec::new())
        }

        async fn check_updates(
            &self,
            _inst: &ManagerInstance,
            _opts: &CheckOptions,
        ) -> Result<CheckOutcome, AdapterError> {
            Ok(CheckOutcome::default())
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
            Ok(test_support::fake_plan(inst, req))
        }

        async fn execute(
            &self,
            _plan: &Plan,
            _sink: Arc<dyn EventSink>,
            _op_id: OpId,
            _cancel: CancellationToken,
        ) -> Result<Outcome, AdapterError> {
            Ok(Outcome::Succeeded)
        }

        async fn reconcile(
            &self,
            _inst: &ManagerInstance,
            _key: &ArtifactKey,
        ) -> Result<Reconciled, AdapterError> {
            Ok(test_support::fake_reconciled())
        }
    }

    /// A canonical temp home (`/var` → `/private/var` on macOS), removed
    /// by the test itself at the end.
    fn temp_home(tag: &str) -> PathBuf {
        let raw = std::env::temp_dir().join(format!(
            "banager-session-scan-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&raw).expect("create temp home");
        std::fs::canonicalize(&raw).expect("canonical temp home")
    }

    fn exe(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"x").expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    #[tokio::test]
    async fn test_scan_unknown_judges_against_the_committed_snapshot_and_never_commits_one() {
        let home = temp_home("committed");
        let bin = home.join(".local/bin");
        std::fs::create_dir_all(&bin).expect("create bin");
        let launcher = exe(&bin, "tool");
        exe(&bin, "stray");
        let instance = ManagerInstance {
            exe_path: launcher,
            prefix: home.join(".local/share/tool"),
            ..test_support::make_instance("fake", "fake")
        };
        let adapter: Arc<dyn Adapter> = Arc::new(FakeAdapter {
            meta: test_support::fake_adapter_meta("fake"),
            instance,
        });
        let session = Session::with_adapters(Arc::new(VecSink::new()), vec![adapter], None);
        let env = HostEnv {
            path_dirs: vec![bin],
            home: home.clone(),
            euid: std::fs::metadata(&home).expect("stat home").uid(),
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let tool = Path::new("~/.local/bin/tool");
        let stray = Path::new("~/.local/bin/stray");

        // Before any refresh the snapshot is empty, so nothing can be
        // claimed: the launcher is as unknown as the stray file. The scan
        // is over the snapshot as committed, not over what the adapters
        // would say if asked -- a scan never asks them.
        let before = session.scan_unknown(&env);
        assert!(before.entries.iter().any(|e| e.path == tool), "{before:?}");
        assert!(before.entries.iter().any(|e| e.path == stray), "{before:?}");
        assert_eq!(session.snapshot().generation, 0);

        session.refresh(&env, &CheckOptions::default()).await;
        let generation = session.snapshot().generation;

        let after = session.scan_unknown(&env);
        assert!(!after.entries.iter().any(|e| e.path == tool), "{after:?}");
        let listed = after
            .entries
            .iter()
            .find(|e| e.path == stray)
            .expect("the stray file is still listed");
        assert_eq!(listed.kind, EntryKind::File);
        assert!(after.attributed >= 1);
        assert_eq!(
            session.snapshot().generation,
            generation,
            "a scan reads the snapshot; it never commits one"
        );
        let _ = std::fs::remove_dir_all(&home);
    }
}
