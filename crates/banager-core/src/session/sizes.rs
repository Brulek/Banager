//! `Session::sizes`: how much disk each installed thing takes, measured
//! after each round commits (`size::SizeMeter`). Its own file, like
//! `icon.rs` and `scan.rs`, so the facade in `mod.rs` stays a facade.

use super::{Session, Snapshot};
use crate::runner::HostEnv;
use crate::size::Sizes;

impl Session {
    /// What the measurements of the newest round say so far
    /// (`size::Sizes`), for the window's `get_sizes`; empty for a session
    /// that does not measure, and before the first round. Not part of the
    /// snapshot, and never written into it: a size that moves can never
    /// move `generation`.
    pub fn sizes(&self) -> Sizes {
        self.sizes
            .as_ref()
            .map(|meter| meter.sizes())
            .unwrap_or_default()
    }

    /// Starts measuring what the snapshot of `round` lists, on a thread of
    /// its own, and returns at once. Called by `refresh_recording` after
    /// the round has committed and released the refresh gate, so a
    /// measurement holds no lock a refresh or an operation waits on; a
    /// newer round stops it. `env.home` is whose protected folders are
    /// never entered (`size::Protected`).
    pub(super) fn measure_sizes(&self, round: u64, snapshot: &Snapshot, env: &HostEnv) {
        if let Some(meter) = &self.sizes {
            meter.measure(round, &snapshot.instances, &snapshot.artifacts, &env.home);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support;
    use crate::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
    use crate::events::{EventSink, OpId, OperationEvent};
    use crate::model::{
        ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, ManagerInstance, OpRequest,
        Outcome, Plan, Reconciled, SearchHit,
    };
    use crate::runner::HostEnv;
    use crate::session::Session;
    use crate::size::Sizes;
    use async_trait::async_trait;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;

    /// A uv of a test's own: one tool, whose environment is a folder the
    /// test made.
    struct FakeUv {
        meta: AdapterMeta,
        instance: ManagerInstance,
        tool: PathBuf,
    }

    #[async_trait]
    impl Adapter for FakeUv {
        fn meta(&self) -> &AdapterMeta {
            &self.meta
        }

        async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
            vec![self.instance.clone()]
        }

        async fn inventory(
            &self,
            inst: &ManagerInstance,
        ) -> Result<Vec<InstalledArtifact>, AdapterError> {
            Ok(vec![InstalledArtifact {
                key: ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Tool,
                    name: "ruff".to_string(),
                },
                display_name: "ruff".to_string(),
                version: "0.14.3".to_string(),
                reason: InstallReason::Requested,
                description: None,
                homepage: None,
                size_bytes: None,
                installed_at: None,
                path: Some(self.tool.clone()),
                auto_updates: false,
                uninstall_blocked: None,
                facts: Default::default(),
            }])
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

    /// Records each round `sizes_changed` is told of.
    #[derive(Default)]
    struct RoundsSink {
        rounds: Mutex<Vec<u64>>,
    }

    impl EventSink for RoundsSink {
        fn emit(&self, _event: OperationEvent) {}

        fn sizes_changed(&self, round: u64) {
            self.rounds.lock().unwrap().push(round);
        }
    }

    /// A home folder with one uv tool in it, removed when dropped.
    struct Home(PathBuf);

    impl Home {
        fn new(tag: &str) -> Home {
            let dir = std::env::temp_dir().join(format!(
                "banager-session-sizes-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let bin = dir.join(".local/share/uv/tools/ruff/bin");
            std::fs::create_dir_all(&bin).unwrap();
            std::fs::write(bin.join("ruff"), vec![1u8; 64_000]).unwrap();
            Home(dir)
        }

        fn tool(&self) -> PathBuf {
            self.0.join(".local/share/uv/tools/ruff")
        }

        fn env(&self) -> HostEnv {
            HostEnv {
                home: self.0.clone(),
                ..test_support::non_root_env()
            }
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn fake_uv(home: &Home) -> Arc<dyn Adapter> {
        Arc::new(FakeUv {
            meta: test_support::fake_adapter_meta("uv"),
            instance: ManagerInstance {
                prefix: home.0.clone(),
                ..test_support::make_instance("uv", "uv")
            },
            tool: home.tool(),
        })
    }

    async fn until_done(session: &Session, round: u64) -> Sizes {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let sizes = session.sizes();
            if sizes.round == round && sizes.done {
                return sizes;
            }
            assert!(
                Instant::now() < deadline,
                "round {round} never finished: {sizes:?}"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    fn bytes_of(path: &Path) -> u64 {
        use std::os::unix::fs::MetadataExt;
        let mut sum = 0;
        let mut stack = vec![path.to_path_buf()];
        while let Some(p) = stack.pop() {
            let meta = std::fs::symlink_metadata(&p).unwrap();
            sum += meta.blocks() * 512;
            if meta.is_dir() {
                for entry in std::fs::read_dir(&p).unwrap() {
                    stack.push(entry.unwrap().path());
                }
            }
        }
        sum
    }

    #[tokio::test]
    async fn test_a_committed_round_is_measured_after_it_and_never_moves_the_generation() {
        let home = Home::new("measured");
        let sink = Arc::new(RoundsSink::default());
        let session = Session::with_adapters_and_sizes(sink.clone(), vec![fake_uv(&home)], None);
        let first = session
            .refresh_with_round(&home.env(), &CheckOptions::default())
            .await;
        let sizes = until_done(&session, first.0).await;
        let ruff = &sizes.artifacts[0];
        assert_eq!(ruff.key.name, "ruff");
        assert_eq!(ruff.version, "0.14.3");
        assert_eq!(ruff.measured.unwrap().bytes, bytes_of(&home.tool()));
        assert!(
            sink.rounds.lock().unwrap().contains(&first.0),
            "the window is told, with the round"
        );
        // The sizes are not in the snapshot: the next round, with nothing
        // changed, keeps the generation.
        let second = session
            .refresh_with_round(&home.env(), &CheckOptions::default())
            .await;
        assert_eq!(second.1.generation, first.1.generation);
        assert_eq!(session.snapshot().generation, first.1.generation);
        let again = until_done(&session, second.0).await;
        assert_eq!(again.artifacts, sizes.artifacts);
    }

    #[tokio::test]
    async fn test_a_session_that_does_not_measure_answers_no_sizes() {
        let home = Home::new("off");
        let sink = Arc::new(RoundsSink::default());
        let session = Session::with_adapters(sink.clone(), vec![fake_uv(&home)], None);
        session.refresh(&home.env(), &CheckOptions::default()).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(session.sizes(), Sizes::default());
        assert!(sink.rounds.lock().unwrap().is_empty());
    }
}
