//! `Session::artifact_icon`: the icon of the app a Homebrew cask installed,
//! judged against this session's last committed snapshot. Its own file,
//! like `scan.rs`, so the facade in `mod.rs` stays a facade.

use super::Session;
use crate::icon::{self, AppIcons};
use crate::model::ArtifactKey;

impl Session {
    /// The icon Finder shows for the app the cask `key` names installed, as
    /// a `data:image/png;base64,...` URL drawn by `icons`; `None` when the
    /// snapshot as it is *now* has no row with this key, when that row is
    /// not a cask or names no `.app` (`icon::cask_app_bundle`), and when
    /// that `.app` is not a folder there or the system gives it no icon
    /// (`AppIcons`).
    ///
    /// `key` is only ever compared, whole, with the snapshot's rows: its
    /// `name` and `instance_id` are never read as a path, so the folder
    /// whose icon is drawn is always the `path` the row's own source
    /// reported -- for a cask, Homebrew. From outside this crate this is
    /// the one way to have `AppIcons` draw, and it takes a key.
    ///
    /// Synchronous and blocking -- an `lstat`, and on a first request for a
    /// folder, a drawing -- so the Tauri shell runs it on the blocking pool
    /// (`ipc::artifact_icon`). Only the row's path is cloned under the
    /// snapshot's mutex, which is released before anything on disk is
    /// looked at; nothing is written back to the session.
    pub fn artifact_icon(&self, icons: &AppIcons, key: &ArtifactKey) -> Option<String> {
        let bundle = {
            let snapshot = self.snapshot.lock().unwrap();
            let row = snapshot.artifacts.iter().find(|row| &row.key == key)?;
            icon::cask_app_bundle(row)?.to_path_buf()
        };
        icons.bundle_icon(&bundle)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support;
    use crate::adapters::{Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome};
    use crate::events::{EventSink, OpId, VecSink};
    use crate::icon::{AppIcons, MockIconRenderer};
    use crate::model::{
        ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, ManagerInstance, OpRequest,
        Outcome, Plan, Reconciled, SearchHit,
    };
    use crate::runner::HostEnv;
    use crate::session::Session;
    use async_trait::async_trait;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use tokio_util::sync::CancellationToken;

    /// Reports one instance and, under it, whatever rows the test put in
    /// `rows` at the time of each refresh.
    struct FakeAdapter {
        meta: AdapterMeta,
        instance: ManagerInstance,
        rows: Arc<Mutex<Vec<InstalledArtifact>>>,
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
            Ok(self.rows.lock().unwrap().clone())
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

    /// A canonical temp folder holding one `.app` folder, removed by the
    /// test itself at the end.
    fn temp_app(tag: &str) -> (PathBuf, PathBuf) {
        let raw = std::env::temp_dir().join(format!(
            "banager-session-icon-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(raw.join("iTerm.app")).expect("create the .app folder");
        let dir = std::fs::canonicalize(&raw).expect("canonical temp dir");
        let app = dir.join("iTerm.app");
        (dir, app)
    }

    fn key(kind: ArtifactKind, name: &str) -> ArtifactKey {
        ArtifactKey {
            instance_id: "fake:1".to_string(),
            kind,
            name: name.to_string(),
        }
    }

    fn row(kind: ArtifactKind, name: &str, path: &Path) -> InstalledArtifact {
        InstalledArtifact {
            key: key(kind, name),
            display_name: name.to_string(),
            version: "1.0".to_string(),
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path: Some(path.to_path_buf()),
            auto_updates: false,
            uninstall_blocked: None,
            facts: Default::default(),
        }
    }

    fn session_over(rows: Arc<Mutex<Vec<InstalledArtifact>>>) -> Arc<Session> {
        let adapter: Arc<dyn Adapter> = Arc::new(FakeAdapter {
            meta: test_support::fake_adapter_meta("fake"),
            instance: test_support::make_instance("fake", "fake:1"),
            rows,
        });
        Session::with_adapters(Arc::new(VecSink::new()), vec![adapter], None)
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nstand-in";

    #[tokio::test]
    async fn test_artifact_icon_draws_the_path_the_snapshot_holds_for_the_key_and_nothing_else() {
        let (dir, app) = temp_app("snapshot");
        // A cask and, at the very same path, a formula: only the cask's
        // row shows an icon.
        let rows = Arc::new(Mutex::new(vec![
            row(ArtifactKind::Cask, "iterm2", &app),
            row(ArtifactKind::Formula, "iterm2-formula", &app),
        ]));
        let session = session_over(rows);
        let renderer = Arc::new(MockIconRenderer::answering(PNG));
        let icons = AppIcons::new(renderer.clone());
        let cask = key(ArtifactKind::Cask, "iterm2");

        // Before the first refresh the snapshot has no rows at all.
        assert_eq!(session.artifact_icon(&icons, &cask), None);
        assert!(renderer.calls().is_empty());

        session
            .refresh(&test_support::non_root_env(), &CheckOptions::default())
            .await;
        let icon = session
            .artifact_icon(&icons, &cask)
            .expect("the cask's icon");
        assert!(icon.starts_with("data:image/png;base64,"), "{icon}");
        assert_eq!(renderer.calls(), vec![app.clone()]);

        // The formula at the same path, and keys the snapshot has no row
        // for -- among them a key that *is* the path, in every field a
        // window could fill -- draw nothing.
        let app_text = app.display().to_string();
        for other in [
            key(ArtifactKind::Formula, "iterm2-formula"),
            key(ArtifactKind::Formula, "iterm2"),
            key(ArtifactKind::Cask, "iTerm"),
            key(ArtifactKind::Cask, &app_text),
            ArtifactKey {
                instance_id: app_text.clone(),
                kind: ArtifactKind::Cask,
                name: app_text.clone(),
            },
            ArtifactKey {
                instance_id: "fake:2".to_string(),
                ..cask.clone()
            },
        ] {
            assert_eq!(session.artifact_icon(&icons, &other), None, "{other:?}");
        }
        assert_eq!(
            renderer.calls(),
            vec![app],
            "only the cask's path was drawn"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_artifact_icon_answers_for_the_snapshot_as_it_is_now() {
        let (dir, app) = temp_app("now");
        let rows = Arc::new(Mutex::new(vec![row(ArtifactKind::Cask, "iterm2", &app)]));
        let session = session_over(rows.clone());
        let icons = AppIcons::new(Arc::new(MockIconRenderer::answering(PNG)));
        let cask = key(ArtifactKind::Cask, "iterm2");
        let env = test_support::non_root_env();

        session.refresh(&env, &CheckOptions::default()).await;
        assert!(session.artifact_icon(&icons, &cask).is_some());

        // Uninstalled since: the next snapshot has no row for it, and its
        // icon, although drawn and remembered, is not handed out.
        rows.lock().unwrap().clear();
        session.refresh(&env, &CheckOptions::default()).await;
        assert_eq!(session.artifact_icon(&icons, &cask), None);

        // Back, but its folder is gone: nothing to draw.
        rows.lock()
            .unwrap()
            .push(row(ArtifactKind::Cask, "iterm2", &app));
        session.refresh(&env, &CheckOptions::default()).await;
        std::fs::remove_dir_all(&dir).expect("remove the temp dir");
        assert_eq!(session.artifact_icon(&icons, &cask), None);
    }
}
