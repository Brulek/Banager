pub mod parse;

use crate::adapters::{run_plan, Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions};
use crate::events::{EventSink, OpId};
use crate::http::{HttpClient, HttpRequest};
use crate::model::{
    ArtifactKey, CancelPolicy, InstalledArtifact, ManagerInstance, OpKind, OpRequest, Outcome,
    Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate, UpdateChannel,
};
use crate::runner::{resolve_exe, CommandRunner, CommandSpec, HostEnv};
use async_trait::async_trait;
use parse::{config_digest, layer_digests, parse_tags, parse_version, split_model_reference};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Ollama model references are `name:tag` or `namespace/name:tag`.
/// `validate_package_name` in `adapters/mod.rs` rejects the colon, since
/// Homebrew formula/cask names never contain one — Ollama's naming scheme
/// does, so this adapter validates model references with its own,
/// colon-inclusive rule instead of reusing that function.
fn validate_model_reference(name: &str) -> Result<(), AdapterError> {
    if name.is_empty()
        || name.starts_with('-')
        || name.starts_with('/')
        || name.starts_with('.')
        || name.split('/').any(|segment| segment == "..")
    {
        return Err(AdapterError::InvalidName(name.to_string()));
    }
    let valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '+' | '/' | '-' | ':'));
    if !valid {
        return Err(AdapterError::InvalidName(name.to_string()));
    }
    Ok(())
}

/// Ollama's own default daemon URL, used whenever the host environment did
/// not set `OLLAMA_HOST`.
pub const DEFAULT_HOST: &str = "http://127.0.0.1:11434";

/// The daemon URL for this host: `HostEnv::ollama_host` (Task 9) when the
/// environment set one, else Ollama's default. Never read from `std::env`
/// here — a machine or CI runner with `OLLAMA_HOST` set would otherwise
/// silently change every URL these tests mock.
fn host_for(env: &HostEnv) -> String {
    env.ollama_host
        .clone()
        .unwrap_or_else(|| DEFAULT_HOST.to_string())
}

/// The daemon URL an instance was detected against. `detect` encodes it in
/// the instance id as `ollama:{host}`, which is how `inventory` and
/// `check_updates` — which the `Adapter` trait gives only an instance —
/// reach it without a `HostEnv` of their own or any adapter-level state.
fn host_of(inst: &ManagerInstance) -> &str {
    inst.id.strip_prefix("ollama:").unwrap_or(DEFAULT_HOST)
}

pub struct OllamaAdapter {
    runner: Arc<dyn CommandRunner>,
    http: Arc<dyn HttpClient>,
    meta: AdapterMeta,
}

impl OllamaAdapter {
    pub fn new(runner: Arc<dyn CommandRunner>, http: Arc<dyn HttpClient>) -> OllamaAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../../adapters/meta/ollama.toml"))
            .expect("adapters/meta/ollama.toml must parse");
        OllamaAdapter { runner, http, meta }
    }

    /// The `ollama` binary's own version is read via a lightweight CLI call
    /// (`ollama --version`), which — unlike `ollama list` — touches neither
    /// the daemon nor the macOS GUI app. Daemon health is read separately,
    /// over HTTP, so a background refresh never shells out to `ollama list`
    /// (this phase's ruling: reads stay on HTTP, in part because `ollama
    /// list` launches Ollama.app as a side effect on macOS).
    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let Some(exe_path) = resolve_exe("ollama", env) else {
            return Vec::new();
        };
        let version_output = self
            .runner
            .run(
                CommandSpec {
                    program: exe_path.clone(),
                    args: vec!["--version".to_string()],
                    env: Vec::new(),
                    cwd: None,
                    timeout: Duration::from_secs(30),
                },
                None,
                CancellationToken::new(),
            )
            .await;
        let version = match version_output {
            Ok(o) if o.exit_code == Some(0) => parse_version(&o.stdout),
            _ => None,
        };
        let host = host_for(env);
        let healthy = self
            .http
            .send(HttpRequest {
                method: "GET",
                url: format!("{host}/api/tags"),
                headers: Vec::new(),
                timeout: Duration::from_secs(10),
            })
            .await
            .map(|r| r.status == 200)
            .unwrap_or(false);
        let unverified_version = self.meta.unverified_version(&version);
        vec![ManagerInstance {
            id: format!("ollama:{host}"),
            adapter_id: self.meta.id.clone(),
            exe_path,
            prefix: env.home.join(".ollama"),
            scope: Scope::User,
            healthy,
            version,
            unverified_version,
        }]
    }

    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let url = format!("{}/api/tags", host_of(inst));
        let resp = self
            .http
            .send(HttpRequest {
                method: "GET",
                url: url.clone(),
                headers: Vec::new(),
                timeout: Duration::from_secs(30),
            })
            .await
            .map_err(|e| AdapterError::CommandFailed {
                code: None,
                stderr: format!("GET {url}: {e}"),
            })?;
        if resp.status != 200 {
            return Err(AdapterError::CommandFailed {
                code: Some(resp.status as i32),
                stderr: resp.body,
            });
        }
        parse_tags(&resp.body, &inst.id)
    }

    /// Returns `Ok(None)` when the local and registry manifests' layer-digest
    /// sets are identical (model up to date), `Ok(Some(registry_config))`
    /// when they differ — carrying the registry's config digest, which is
    /// what an available update's `target` must be, since the tag
    /// (`27b-mlx`) is unchanged by a republish and `UpdateCandidate`'s
    /// contract is that current and target differ — or `Err(reason)` when
    /// either manifest could not be read/fetched/parsed. A network failure
    /// or a 404 for a model removed upstream must not crash the whole
    /// `check_updates` call, so the caller turns that into a single
    /// `checkable: false` candidate for just this model.
    async fn compare_digests(
        &self,
        manifests_root: &Path,
        namespace: &str,
        name: &str,
        tag: &str,
    ) -> Result<Option<String>, String> {
        let local_path = manifests_root.join(namespace).join(name).join(tag);
        let local_json = std::fs::read_to_string(&local_path).map_err(|e| {
            format!(
                "could not read local manifest {}: {e}",
                local_path.display()
            )
        })?;
        let local_digests = layer_digests(&local_json)
            .map_err(|e| format!("could not parse local manifest: {e}"))?;

        let registry_url =
            format!("https://registry.ollama.ai/v2/{namespace}/{name}/manifests/{tag}");
        let response = self
            .http
            .send(HttpRequest {
                method: "GET",
                url: registry_url,
                headers: vec![(
                    "Accept".to_string(),
                    "application/vnd.docker.distribution.manifest.v2+json".to_string(),
                )],
                timeout: Duration::from_secs(30),
            })
            .await
            .map_err(|e| format!("registry request failed: {e}"))?;
        if response.status != 200 {
            return Err(format!("registry returned status {}", response.status));
        }
        let registry_digests = layer_digests(&response.body)
            .map_err(|e| format!("could not parse registry manifest: {e}"))?;
        if local_digests == registry_digests {
            return Ok(None);
        }
        let registry_config = config_digest(&response.body)
            .map_err(|e| format!("could not parse registry manifest: {e}"))?
            .ok_or_else(|| "registry manifest has no config digest".to_string())?;
        Ok(Some(registry_config))
    }

    async fn check_one_model(
        &self,
        manifests_root: &Path,
        artifact: &InstalledArtifact,
    ) -> Option<UpdateCandidate> {
        let (namespace, name, tag) = split_model_reference(&artifact.key.name);
        match self
            .compare_digests(manifests_root, &namespace, &name, &tag)
            .await
        {
            Ok(None) => None,
            // `current` is the local digest `parse_tags` stored as the
            // artifact's version and `target` is the registry's config
            // digest, so the two really differ — reporting `27b-mlx ->
            // 27b-mlx` (the tag on both sides) would satisfy no reader.
            Ok(Some(registry_config)) => Some(UpdateCandidate {
                key: artifact.key.clone(),
                current: artifact.version.clone(),
                target: registry_config,
                channel: UpdateChannel::Digest,
                checkable: true,
                warnings: Vec::new(),
            }),
            // Uncheckable: there is no target to claim. `checkable: false`
            // is what stops the UI offering an Update button for this row
            // (Task 12); `current` and `target` are both the local digest
            // precisely because nothing was learned about the remote one.
            Err(reason) => Some(UpdateCandidate {
                key: artifact.key.clone(),
                current: artifact.version.clone(),
                target: artifact.version.clone(),
                channel: UpdateChannel::Digest,
                checkable: false,
                warnings: vec![reason],
            }),
        }
    }

    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        let installed = self.inventory(inst).await?;
        let manifests_root = inst.prefix.join("models/manifests/registry.ollama.ai");
        let mut out = Vec::new();
        for artifact in &installed {
            if let Some(candidate) = self.check_one_model(&manifests_root, artifact).await {
                out.push(candidate);
            }
        }
        Ok(out)
    }

    pub async fn search(
        &self,
        _inst: &ManagerInstance,
        _query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        Err(AdapterError::Unsupported(
            "Ollama has no search command Canager uses; browse the model library directly"
                .to_string(),
        ))
    }

    /// Writes go through the CLI, not the HTTP API (this phase's ruling):
    /// `ollama pull {model}` for Install/Upgrade (pulling an already-present
    /// model re-fetches it in place, which is how Ollama itself upgrades a
    /// model) and `ollama rm {model}` for Uninstall.
    pub async fn plan(
        &self,
        inst: &ManagerInstance,
        req: &OpRequest,
    ) -> Result<Plan, AdapterError> {
        if req.instance_id != inst.id {
            return Err(AdapterError::Refused(format!(
                "plan requested for instance {} but given instance {}",
                req.instance_id, inst.id
            )));
        }
        validate_model_reference(&req.name)?;
        let lock = ResourceLock(inst.id.clone());
        let args = match req.kind {
            OpKind::Install | OpKind::Upgrade => vec!["pull".to_string(), req.name.clone()],
            OpKind::Uninstall => vec!["rm".to_string(), req.name.clone()],
        };
        Ok(Plan {
            request: req.clone(),
            program: inst.exe_path.clone(),
            args,
            env: Vec::new(),
            needs_password: false,
            locks: vec![lock],
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings: Vec::new(),
            affected: Vec::new(),
            timeout_secs: 3600,
        })
    }

    pub async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        run_plan(&self.runner, plan, sink, op_id, cancel).await
    }

    pub async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        let artifacts = self.inventory(inst).await?;
        match artifacts
            .into_iter()
            .find(|a| a.key.kind == key.kind && a.key.name == key.name)
        {
            Some(a) => Ok(Reconciled {
                present: true,
                version: Some(a.version),
            }),
            None => Ok(Reconciled {
                present: false,
                version: None,
            }),
        }
    }
}

#[async_trait]
impl Adapter for OllamaAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            search: false,
            per_item_upgrade: true,
            upgrade_all: false,
            uninstall: true,
            background_check: true,
            cancel_safe: true,
        }
    }

    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        OllamaAdapter::detect(self, env).await
    }

    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        OllamaAdapter::inventory(self, inst).await
    }

    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        OllamaAdapter::check_updates(self, inst, opts).await
    }

    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        OllamaAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        OllamaAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        OllamaAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        OllamaAdapter::reconcile(self, inst, key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::VecSink;
    use crate::http::{HttpResponse, MockHttpClient};
    use crate::model::ArtifactKind;
    use crate::runner::{CommandOutput, MockRunner};
    use std::path::PathBuf;

    fn test_instance(host: &str, prefix: PathBuf) -> ManagerInstance {
        ManagerInstance {
            id: format!("ollama:{host}"),
            adapter_id: "ollama".to_string(),
            exe_path: PathBuf::from("/usr/local/bin/ollama"),
            prefix,
            scope: Scope::User,
            version: Some("0.34.1".to_string()),
            healthy: true,
            unverified_version: None,
        }
    }

    #[tokio::test]
    async fn test_inventory_parses_the_recorded_fixture_via_http() {
        let json = std::fs::read_to_string("../../adapters/fixtures/ollama/0.34.1/api-tags.json")
            .expect("read ollama api-tags.json fixture");
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "http://127.0.0.1:11434/api/tags",
            HttpResponse {
                status: 200,
                body: json,
            },
        );
        let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http);
        let inst = test_instance(
            "http://127.0.0.1:11434",
            PathBuf::from("/Users/brulek/.ollama"),
        );
        let artifacts = adapter.inventory(&inst).await.expect("inventory");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].key.name, "qwen3.8:27b-mlx");
    }

    #[tokio::test]
    async fn test_inventory_fails_clearly_when_the_daemon_answers_with_an_error_status() {
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "http://127.0.0.1:11434/api/tags",
            HttpResponse {
                status: 500,
                body: "boom".to_string(),
            },
        );
        let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http);
        let inst = test_instance(
            "http://127.0.0.1:11434",
            PathBuf::from("/Users/brulek/.ollama"),
        );
        let result = adapter.inventory(&inst).await;
        match result {
            Err(AdapterError::CommandFailed {
                code: Some(500), ..
            }) => {}
            other => panic!("expected CommandFailed with code 500, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_compare_digests_reports_up_to_date_for_the_recorded_fixture_pair() {
        let local_json = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/local-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read local manifest fixture");
        let registry_json = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/registry-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read registry manifest fixture");

        let tmp_root = std::env::temp_dir().join(format!(
            "canager-ollama-manifests-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let model_dir = tmp_root.join("library").join("qwen3.8");
        std::fs::create_dir_all(&model_dir).expect("create fixture manifest dir");
        std::fs::write(model_dir.join("27b-mlx"), &local_json).expect("write local manifest");

        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "https://registry.ollama.ai/v2/library/qwen3.8/manifests/27b-mlx",
            HttpResponse {
                status: 200,
                body: registry_json,
            },
        );
        let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http.clone());

        let difference = adapter
            .compare_digests(&tmp_root, "library", "qwen3.8", "27b-mlx")
            .await
            .expect("compare_digests should succeed against the recorded fixture pair");
        assert!(
            difference.is_none(),
            "the recorded local/registry manifest pair is the already-up-to-date case"
        );

        // The anonymous registry only returns a v2 manifest when this header
        // is sent; without it the digest sets would never match and every
        // model would look outdated. MockHttpClient::calls() keeps only urls,
        // so this is the one assertion that can see it.
        let registry_request = http
            .requests()
            .into_iter()
            .find(|r| r.url.starts_with("https://registry.ollama.ai/"))
            .expect("the registry was queried");
        assert!(
            registry_request
                .headers
                .iter()
                .any(|(name, value)| name == "Accept"
                    && value == "application/vnd.docker.distribution.manifest.v2+json"),
            "the registry request must carry the v2 manifest Accept header, got {:?}",
            registry_request.headers
        );

        let _ = std::fs::remove_dir_all(&tmp_root);
    }

    #[tokio::test]
    async fn test_check_updates_reports_nothing_for_the_recorded_up_to_date_fixture() {
        let tags_json =
            std::fs::read_to_string("../../adapters/fixtures/ollama/0.34.1/api-tags.json")
                .expect("read ollama api-tags.json fixture");
        let local_json = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/local-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read local manifest fixture");
        let registry_json = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/registry-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read registry manifest fixture");

        let home = std::env::temp_dir().join(format!(
            "canager-ollama-home-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let model_dir = home.join("models/manifests/registry.ollama.ai/library/qwen3.8");
        std::fs::create_dir_all(&model_dir).expect("create fixture manifest dir");
        std::fs::write(model_dir.join("27b-mlx"), &local_json).expect("write local manifest");

        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "http://127.0.0.1:11434/api/tags",
            HttpResponse {
                status: 200,
                body: tags_json,
            },
        );
        http.respond(
            "https://registry.ollama.ai/v2/library/qwen3.8/manifests/27b-mlx",
            HttpResponse {
                status: 200,
                body: registry_json,
            },
        );
        let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http);
        let inst = test_instance("http://127.0.0.1:11434", home.clone());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert!(candidates.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_check_updates_marks_a_model_uncheckable_when_the_local_manifest_is_missing() {
        // Edge case the fixture cannot show directly: the local manifest
        // file is absent (e.g. deleted out from under Canager).
        let tags_json =
            std::fs::read_to_string("../../adapters/fixtures/ollama/0.34.1/api-tags.json")
                .expect("read ollama api-tags.json fixture");
        let home = std::env::temp_dir().join(format!(
            "canager-ollama-missing-manifest-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "http://127.0.0.1:11434/api/tags",
            HttpResponse {
                status: 200,
                body: tags_json,
            },
        );
        let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http);
        let inst = test_instance("http://127.0.0.1:11434", home);
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates should not fail outright when one model's manifest is missing");
        assert_eq!(candidates.len(), 1);
        assert!(!candidates[0].checkable);
    }

    #[tokio::test]
    async fn test_plan_refuses_when_request_instance_id_does_not_match_given_instance() {
        let adapter =
            OllamaAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(
            "http://127.0.0.1:11434",
            PathBuf::from("/Users/brulek/.ollama"),
        );
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "not-ollama".to_string(),
            artifact_kind: ArtifactKind::Model,
            name: "qwen3.8:27b-mlx".to_string(),
        };
        let result = OllamaAdapter::plan(&adapter, &inst, &req).await;
        match result {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_plan_install_and_upgrade_both_pull_uninstall_removes() {
        let adapter =
            OllamaAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(
            "http://127.0.0.1:11434",
            PathBuf::from("/Users/brulek/.ollama"),
        );
        for (kind, expected) in [
            (OpKind::Install, vec!["pull", "qwen3.8:27b-mlx"]),
            (OpKind::Upgrade, vec!["pull", "qwen3.8:27b-mlx"]),
            (OpKind::Uninstall, vec!["rm", "qwen3.8:27b-mlx"]),
        ] {
            let req = OpRequest {
                kind,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Model,
                name: "qwen3.8:27b-mlx".to_string(),
            };
            let plan = OllamaAdapter::plan(&adapter, &inst, &req)
                .await
                .expect("plan");
            assert_eq!(plan.args, expected);
            assert!(!plan.needs_password);
        }
    }

    #[tokio::test]
    async fn test_plan_rejects_a_model_name_with_shell_metacharacters_but_allows_the_colon() {
        let adapter =
            OllamaAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(
            "http://127.0.0.1:11434",
            PathBuf::from("/Users/brulek/.ollama"),
        );
        let bad_req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Model,
            name: "-rf".to_string(),
        };
        assert!(matches!(
            OllamaAdapter::plan(&adapter, &inst, &bad_req).await,
            Err(AdapterError::InvalidName(_))
        ));
        // A colon-bearing model:tag reference — validate_package_name in
        // adapters/mod.rs would reject this, but validate_model_reference
        // must accept it.
        let good_req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Model,
            name: "qwen3.8:27b-mlx".to_string(),
        };
        assert!(OllamaAdapter::plan(&adapter, &inst, &good_req)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_execute_streams_log_events_and_succeeds() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/usr/local/bin/ollama", "pull", "qwen3.8:27b-mlx"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "pulling manifest\nsuccess\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = OllamaAdapter::new(runner, Arc::new(MockHttpClient::new()));
        let inst = test_instance(
            "http://127.0.0.1:11434",
            PathBuf::from("/Users/brulek/.ollama"),
        );
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Model,
            name: "qwen3.8:27b-mlx".to_string(),
        };
        let plan = OllamaAdapter::plan(&adapter, &inst, &req)
            .await
            .expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome =
            OllamaAdapter::execute(&adapter, &plan, sink.clone(), 1, CancellationToken::new())
                .await
                .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(sink.snapshot().len(), 2);
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_and_absent() {
        let json = std::fs::read_to_string("../../adapters/fixtures/ollama/0.34.1/api-tags.json")
            .expect("read ollama api-tags.json fixture");
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "http://127.0.0.1:11434/api/tags",
            HttpResponse {
                status: 200,
                body: json,
            },
        );
        let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http);
        let inst = test_instance(
            "http://127.0.0.1:11434",
            PathBuf::from("/Users/brulek/.ollama"),
        );
        let present = OllamaAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey {
                instance_id: inst.id.clone(),
                kind: ArtifactKind::Model,
                name: "qwen3.8:27b-mlx".to_string(),
            },
        )
        .await
        .expect("reconcile present");
        assert!(present.present);
        let absent = OllamaAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey {
                instance_id: inst.id.clone(),
                kind: ArtifactKind::Model,
                name: "missing:latest".to_string(),
            },
        )
        .await
        .expect("reconcile absent");
        assert!(!absent.present);
    }

    #[tokio::test]
    async fn test_search_is_unsupported() {
        let adapter =
            OllamaAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(
            "http://127.0.0.1:11434",
            PathBuf::from("/Users/brulek/.ollama"),
        );
        let result = <OllamaAdapter as Adapter>::search(&adapter, &inst, "qwen").await;
        assert!(matches!(result, Err(AdapterError::Unsupported(_))));
    }

    #[test]
    fn test_host_travels_on_the_instance_id_and_defaults_when_the_environment_sets_none() {
        // The daemon URL is a property of the detected instance, not of the
        // adapter: nothing here reads OLLAMA_HOST, so a machine or CI runner
        // that has it set cannot change the url any of these tests mock.
        let inst = test_instance("http://127.0.0.1:9999", PathBuf::from("/tmp/.ollama"));
        assert_eq!(host_of(&inst), "http://127.0.0.1:9999");

        let mut env = HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            ollama_host: None,
        };
        assert_eq!(host_for(&env), DEFAULT_HOST);
        env.ollama_host = Some("http://10.0.0.5:11434".to_string());
        assert_eq!(host_for(&env), "http://10.0.0.5:11434");
    }

    /// A dedicated temp directory used only as a fake PATH entry — never a
    /// real system path — so a detect test cannot collide with, depend on,
    /// or modify whatever Ollama the machine running it actually has.
    fn isolated_path_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "canager-ollama-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        std::fs::write(dir.join("ollama"), b"#!/bin/sh\n").expect("write fake ollama executable");
        dir
    }

    #[tokio::test]
    async fn test_detect_reads_the_version_over_the_cli_and_daemon_health_over_http() {
        let tmp_dir = isolated_path_dir("detect-healthy");
        let exe_path = tmp_dir.join("ollama");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![exe_path.to_str().expect("utf8 temp path"), "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "ollama version is 0.34.1\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "http://127.0.0.1:11434/api/tags",
            HttpResponse {
                status: 200,
                body: r#"{"models":[]}"#.to_string(),
            },
        );
        let env = HostEnv {
            path_dirs: vec![tmp_dir.clone()],
            home: PathBuf::from("/tmp/fake-home"),
            euid: 501,
            cargo_home: None,
            ollama_host: None,
        };
        let adapter = OllamaAdapter::new(runner.clone(), http);
        let instances = adapter.detect(&env).await;

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].id, "ollama:http://127.0.0.1:11434");
        assert_eq!(instances[0].prefix, PathBuf::from("/tmp/fake-home/.ollama"));
        assert_eq!(instances[0].version, Some("0.34.1".to_string()));
        // 0.34.1 is what adapters/meta/ollama.toml pins, so a future meta
        // edit cannot silently start flagging the recorded version.
        assert_eq!(instances[0].unverified_version, None);
        assert!(instances[0].healthy);
        // The ruling this adapter is built on: a background refresh must
        // never run `ollama list`, which launches Ollama.app on macOS.
        assert_eq!(
            runner.calls(),
            vec![vec![
                exe_path.to_string_lossy().to_string(),
                "--version".to_string()
            ]]
        );

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[tokio::test]
    async fn test_detect_marks_the_instance_unhealthy_when_the_daemon_does_not_answer() {
        let tmp_dir = isolated_path_dir("detect-unhealthy");
        let exe_path = tmp_dir.join("ollama");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![exe_path.to_str().expect("utf8 temp path"), "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "ollama version is 9.9.9\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        // No canned response for the daemon: MockHttpClient answers NoMock,
        // which is what a refused connection looks like to this adapter.
        let env = HostEnv {
            path_dirs: vec![tmp_dir.clone()],
            home: PathBuf::from("/tmp/fake-home"),
            euid: 501,
            cargo_home: None,
            ollama_host: Some("http://10.0.0.5:11434".to_string()),
        };
        let adapter = OllamaAdapter::new(runner, Arc::new(MockHttpClient::new()));
        let instances = adapter.detect(&env).await;

        // Still one instance, so the UI can offer to start the daemon
        // (Task 12) rather than the source vanishing from the list.
        assert_eq!(instances.len(), 1);
        assert!(!instances[0].healthy);
        assert_eq!(instances[0].id, "ollama:http://10.0.0.5:11434");
        assert_eq!(instances[0].unverified_version, Some("9.9.9".to_string()));

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[tokio::test]
    async fn test_detect_returns_no_instance_when_ollama_is_not_on_path() {
        let tmp_dir = std::env::temp_dir().join(format!(
            "canager-ollama-detect-absent-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp_dir).expect("create temp PATH dir");
        let env = HostEnv {
            path_dirs: vec![tmp_dir.clone()],
            home: PathBuf::from("/tmp/fake-home"),
            euid: 501,
            cargo_home: None,
            ollama_host: None,
        };
        let adapter =
            OllamaAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        assert!(adapter.detect(&env).await.is_empty());

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
