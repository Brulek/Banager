pub mod parse;

#[cfg(test)]
mod manifest_layout_tests;
#[cfg(test)]
mod registry_change_tests;

use crate::adapters::{
    ensure_instance_match, get_ok, reconcile_from, run_plan, uncheckable_candidate,
    url_path_segment, Adapter, AdapterError, AdapterMeta, CheckOptions, CheckOutcome,
    LookupFailure,
};
use crate::events::{EventSink, OpId};
use crate::http::{HttpClient, HttpRequest};
use crate::model::{
    ArtifactKey, CancelPolicy, InstalledArtifact, InstanceStatus, ManagerInstance, OpKind,
    OpRequest, Outcome, Plan, PlanAction, Reconciled, ResourceLock, Scope, SearchHit, Unavailable,
    UninstallScope, UpdateCandidate, UpdateChannel, Warning,
};
use crate::protected::{look, Protected};
use crate::runner::redact::{without_ollama_login, Redactor};
use crate::runner::{resolve_exe, CommandRunner, CommandSpec, HostEnv, OutputUse};
use async_trait::async_trait;
use parse::{
    changed_blob_bytes, config_digest, is_manifest_list, layer_digests, parse_tags, parse_version,
    split_model_reference,
};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
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

/// What `OllamaAdapter::compare_digests` learned of a model the registry
/// has republished: the registry manifest's own digest, the update's
/// `target` (`manifest_digest`), and the most pulling it can download
/// (`parse::changed_blob_bytes`), the candidate's `download_bytes`.
#[derive(Debug)]
struct RegistryChange {
    manifest: String,
    download_bytes: Option<u64>,
}

/// Joins a model reference's `namespace`/`name`/`tag` onto the Ollama
/// manifests root, refusing any reference that would not land inside it.
///
/// All three parts come out of `split_model_reference` applied to a name in
/// the body of `GET {host}/api/tags` — i.e. straight off the network, from a
/// daemon Banager does not control, on every background refresh. Two shapes
/// escape a bare `manifests_root.join(namespace).join(name).join(tag)`:
/// an **absolute** part (`a//etc/passwd` splits into name `/etc/passwd`,
/// and `Path::join` with an absolute component throws the base away
/// entirely, so the join *is* `/etc/passwd`), and a **`..` walk**
/// (`x/../../../../etc/passwd`), which climbs out of the root. The file is
/// then read, following symlinks, by `compare_digests`.
///
/// The defence is structural rather than a blocklist of known-bad strings:
/// every component of every untrusted part must be a plain
/// `Component::Normal` segment, so anything rooted (`RootDir`), climbing
/// (`ParentDir`), drive-qualified (`Prefix`) or degenerate (`CurDir`, or an
/// empty part contributing no component at all) is refused before any path
/// is built — no filesystem access and no canonicalisation needed, which
/// matters because the file legitimately may not exist. The `starts_with`
/// re-check afterwards is belt and braces on the join itself.
///
/// `validate_model_reference` is deliberately *not* what guards this:
/// `a//etc/passwd` passes it (it does not start with `/`, and no
/// `/`-separated segment equals `..` — the segments are `a`, ``, `etc`,
/// `passwd`), and it is only ever called from `plan()` anyway.
///
/// The error string is a human-readable reason, because the caller turns it
/// into one `checkable: false` candidate for this model alone: a single
/// hostile or malformed name from the daemon must never abort the refresh
/// for every other model.
fn contained_manifest_path(
    manifests_root: &Path,
    namespace: &str,
    name: &str,
    tag: &str,
) -> Result<PathBuf, String> {
    for (label, part) in [("namespace", namespace), ("name", name), ("tag", tag)] {
        let plain = !part.is_empty()
            && Path::new(part)
                .components()
                .all(|component| matches!(component, Component::Normal(_)));
        if !plain {
            return Err(format!(
                "refusing to read a manifest outside {}: the model reference's {label} {part:?} is not a plain path segment",
                manifests_root.display()
            ));
        }
    }
    let path = manifests_root.join(namespace).join(name).join(tag);
    if !path.starts_with(manifests_root) {
        return Err(format!(
            "refusing to read a manifest outside {}: {}",
            manifests_root.display(),
            path.display()
        ));
    }
    Ok(path)
}

/// The digest a registry manifest is known by, `sha256:<hex>` of its bytes
/// as served -- what the registry's `Docker-Content-Digest` would say, and
/// an available update's `target`. It names the whole manifest, so every
/// republish of a tag has one of its own: a change of weights alone, which
/// leaves the config (and so its digest) as it was, has a new one too.
/// Ollama writes the manifest it pulls as the registry sent it (0.40
/// `server/images.go`, `WriteManifestData(n, manifestData)`), so once
/// pulled the model's `/api/tags` digest is normally this same hex -- but
/// nothing compares the two (`check_one_model`).
fn manifest_digest(body: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("sha256:{:x}", Sha256::digest(body.as_bytes()))
}

/// Ollama's own registry: the one `check_updates` looks a model up in.
const OLLAMA_REGISTRY: &str = "registry.ollama.ai";

/// Ollama's own registry, and the one documented mirror it understands.
/// A reference whose first segment looks like a host and is neither of
/// these names a third party.
const DEFAULT_REGISTRIES: [&str; 2] = [OLLAMA_REGISTRY, "hf.co"];

/// The third-party registry a model reference points at, if any.
///
/// `validate_model_reference` permits `/` and `.`, so
/// `evil.example.com/ns/model:tag` is a perfectly valid reference — and
/// Ollama reads a host-like first segment as a registry, so pulling it
/// fetches from that host rather than from Ollama's library. The operation
/// preview does show the argv, but this app's audience is people who do not
/// write code: a hostname sitting inside what looks like a model name does
/// not read as a warning to them.
///
/// The first segment only counts as a registry when the reference actually
/// has more than one segment: a bare `qwen3.8:27b-mlx` contains a dot but
/// is a model name, and warning about it would teach the user to ignore the
/// warning that matters.
fn third_party_registry(reference: &str) -> Option<&str> {
    named_registry(reference).filter(|host| !DEFAULT_REGISTRIES.contains(host))
}

/// The registry a model reference names, if it names one: its first
/// segment, where it has more than one and that segment looks like a host
/// (it has a `.`) -- `hf.co` of `hf.co/user/repo:tag` -- and nothing of
/// `qwen3.8:27b-mlx` or `someuser/somemodel:tag`, which are Ollama's own
/// library's.
fn named_registry(reference: &str) -> Option<&str> {
    let (first, _rest) = reference.split_once('/')?;
    first.contains('.').then_some(first)
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

/// The public daemon endpoint, without login information. Detection retains
/// the authenticated endpoint privately in the adapter; local-host checks
/// need only this credential-free address.
fn host_of(inst: &ManagerInstance) -> &str {
    inst.id.strip_prefix("ollama:").unwrap_or(DEFAULT_HOST)
}

/// The two places a macOS install of Ollama puts its app bundle: the
/// system-wide `/Applications`, and the per-user `~/Applications` that a
/// drag-install into the user's own folder produces.
///
/// A list rather than a single path so the caller can be told where to
/// look, which is what makes [`ollama_app_in`] testable without an
/// Ollama.app on the machine running the tests.
pub fn ollama_app_roots(home: &Path) -> Vec<PathBuf> {
    ollama_app_roots_under(Path::new("/Applications"), home)
}

/// [`ollama_app_roots`] with `applications` standing for `/Applications`:
/// a test's own folder in tests, so that none looks at this Mac's.
fn ollama_app_roots_under(applications: &Path, home: &Path) -> Vec<PathBuf> {
    vec![applications.to_path_buf(), home.join("Applications")]
}

/// The Ollama.app bundle under any of `roots`, or `None` when there is
/// none.
///
/// A bundle is a directory, so "is a folder" is the check -- "exists"
/// would also answer yes to a stray file named `Ollama.app`. Looked up one
/// step at a time and never into or through a place `protected` keeps out
/// (`protected::look`): an `~/Applications` kept in `~/Documents` is not
/// looked into.
pub fn ollama_app_in(roots: &[PathBuf], protected: &Protected) -> Option<PathBuf> {
    roots
        .iter()
        .map(|root| root.join("Ollama.app"))
        .find(|path| look::target(path, protected).is_ok_and(|(_, meta)| meta.is_dir()))
}

/// [`ollama_app_in`] over [`ollama_app_roots`]: the real question, asked
/// the real way. Used by `detect` to decide whether there is anything for
/// the Open Ollama button to open, and by the Tauri command behind that
/// button to say why it cannot.
pub fn ollama_app_path(home: &Path) -> Option<PathBuf> {
    ollama_app_path_under(Path::new("/Applications"), home)
}

/// [`ollama_app_path`] with `applications` standing for `/Applications`
/// (`ollama_app_roots_under`).
fn ollama_app_path_under(applications: &Path, home: &Path) -> Option<PathBuf> {
    ollama_app_in(
        &ollama_app_roots_under(applications, home),
        &Protected::new(home),
    )
}

/// Whether `host` -- a daemon URL as [`host_for`] builds it -- names this
/// Mac. Only then can launching the local Ollama.app bring it up: with
/// `OLLAMA_HOST` pointing at another machine, the app would start a
/// daemon here and the one Banager is asking would stay exactly as silent.
///
/// The unspecified addresses count as this Mac because that is how a
/// client treats them: `OLLAMA_HOST=0.0.0.0` is Ollama's documented way to
/// listen on every interface, and a request to `0.0.0.0` lands on the
/// loopback. Anything that does not parse is not known to be local.
fn host_is_this_mac(host: &str) -> bool {
    match url::Url::parse(host)
        .ok()
        .and_then(|u| u.host().map(|h| h.to_owned()))
    {
        Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback() || ip.is_unspecified(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback() || ip.is_unspecified(),
        None => false,
    }
}

/// Whether `url` is an `https` URL that `RealHttpClient::send` refuses
/// without connecting: an `https://` `OLLAMA_HOST`, whose host is not on
/// `ALLOWED_HTTPS_HOSTS` (`http::real::host_allowed`). Only `https`: a
/// plain `http` URL is always allowed, and `normalize_ollama_host`
/// (`runner/path_env.rs`) lets no other scheme through.
fn https_refused(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|parsed| parsed.scheme() == "https")
        && crate::http::real::host_allowed(url).is_err()
}

/// Whether `inst`'s models are on this Mac, in its `prefix`'s `models`
/// folder: the daemon it was detected against is this Mac's
/// (`host_is_this_mac`). With `OLLAMA_HOST` naming another machine they are
/// there, and a `~/.ollama` here holds none of them. Read by the size
/// measurement (`size::plan_round`), which walks that folder only then.
pub(crate) fn models_on_this_mac(inst: &ManagerInstance) -> bool {
    host_is_this_mac(host_of(inst))
}

fn real_ollama_app_present(env: &HostEnv) -> bool {
    ollama_app_path(&env.home).is_some()
}

pub struct OllamaAdapter {
    runner: Arc<dyn CommandRunner>,
    http: Arc<dyn HttpClient>,
    meta: AdapterMeta,
    // Runtime authentication stays in the adapter, not in snapshot/operation ids.
    hosts: Mutex<HashMap<String, String>>,
    /// Whether Ollama.app is installed, which is the difference between
    /// the two states a silent daemon can be in. Production always gets
    /// [`real_ollama_app_present`]; tests inject a fixed answer through
    /// the `#[cfg(test)]`-only `with_app_present_fn`, mirroring
    /// `NpmAdapter::with_prefix_writable_fn` -- without which every test
    /// of this would pass or fail depending on whether the machine
    /// running it happens to have Ollama installed.
    app_present_fn: fn(&HostEnv) -> bool,
}

impl OllamaAdapter {
    pub fn new(runner: Arc<dyn CommandRunner>, http: Arc<dyn HttpClient>) -> OllamaAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../../adapters/meta/ollama.toml"))
            .expect("adapters/meta/ollama.toml must parse");
        OllamaAdapter {
            runner,
            http,
            meta,
            hosts: Mutex::new(HashMap::new()),
            app_present_fn: real_ollama_app_present,
        }
    }

    fn endpoint(&self, inst: &ManagerInstance) -> String {
        self.hosts
            .lock()
            .unwrap()
            .get(&inst.id)
            .cloned()
            .unwrap_or_else(|| host_of(inst).to_string())
    }

    /// Test-only hook (see `app_present_fn`). Public with the
    /// `test-support` feature, for the integration tests, which are built
    /// without `cfg(test)` and so would otherwise look for this Mac's
    /// Ollama.app.
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_app_present_fn(mut self, f: fn(&HostEnv) -> bool) -> OllamaAdapter {
        self.app_present_fn = f;
        self
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
                    output_use: OutputUse::Parsed,
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
        let url = format!("{host}/api/tags");
        // An `https://` `OLLAMA_HOST` is one `RealHttpClient::send` would
        // refuse before connecting: its allowlist exempts `http` only, and
        // no daemon's host is on it. So it is not sent at all, and the
        // instance says why (`HttpsHostRefused`) instead of passing the
        // refusal off as a daemon that did not answer. The same rule the
        // client applies, read from the same function, so the two cannot
        // disagree about which addresses are refused.
        let refused = https_refused(&url);
        // Any other error -- no connection, a timeout -- is a daemon that
        // did not answer, which is all `detect` needs to know of it.
        let answering = !refused
            && self
                .http
                .send(HttpRequest {
                    method: "GET",
                    url,
                    headers: Vec::new(),
                    timeout: Duration::from_secs(10),
                })
                .await
                .map(|r| r.status == 200)
                .unwrap_or(false);
        let unverified_version = self.meta.unverified_version(&version);
        let id = without_ollama_login(&crate::model::instance_id(&self.meta.id, Some(&host)))
            .into_owned();
        self.hosts.lock().unwrap().insert(id.clone(), host.clone());
        vec![ManagerInstance {
            id,
            adapter_id: self.meta.id.clone(),
            exe_path,
            prefix: env.home.join(".ollama"),
            scope: Scope::User,
            status: InstanceStatus {
                // `NotRunning` is the one state whose notice carries a
                // button that fixes it from inside Banager: Open Ollama,
                // which runs `open -a Ollama`. So it is given only when
                // that button can work -- the daemon Banager asked is on
                // this Mac, and there is an Ollama.app here to open.
                // Anything else that is not answering is `NotResponding`,
                // whose notice claims nothing Banager cannot do.
                //
                // The comment that used to sit here called Ollama "the
                // one source whose not-answering the user can fix from
                // inside Banager" and gave `NotRunning` to any silent
                // daemon with `ollama` on PATH. That was false twice
                // over. `brew install ollama` -- Homebrew being the first
                // source this project's README lists -- installs the CLI
                // and no app, so those users got a button that ran `open
                // -a Ollama`, failed into a nulled stderr, and did
                // nothing, every time, with no message. And with
                // `OLLAMA_HOST` naming another machine, opening the app
                // here starts a daemon here, which is not the one being
                // asked.
                unavailable: if answering {
                    None
                } else if refused {
                    Some(Unavailable::HttpsHostRefused)
                } else if host_is_this_mac(&host) && (self.app_present_fn)(env) {
                    Some(Unavailable::NotRunning)
                } else {
                    Some(Unavailable::NotResponding)
                },
                notes: Vec::new(),
                no_answer: None,
            },
            version,
            answered_at: None,
            unverified_version,
            read_only_reason: None,
        }]
    }

    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let host = self.endpoint(inst);
        let redactor = Redactor::for_settings([("OLLAMA_HOST", host.as_str())]);
        let url = format!("{host}/api/tags");
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
                stderr: redactor.redact(&format!("GET {url}: {e}")).into_owned(),
            })?;
        if resp.status != 200 {
            return Err(AdapterError::CommandFailed {
                code: Some(resp.status as i32),
                stderr: redactor.redact(&resp.body).into_owned(),
            });
        }
        parse_tags(&resp.body, &inst.id).map_err(|error| match error {
            AdapterError::Parse(message) => {
                AdapterError::Parse(redactor.redact(&message).into_owned())
            }
            other => other,
        })
    }

    /// Returns `Ok(None)` when the local and registry manifests' layer-digest
    /// sets are identical and so are their config digests (model up to
    /// date), `Ok(Some(change))` when either differs — carrying the
    /// registry manifest's own digest (`manifest_digest`), which is what an
    /// available update's `target` must be, since the tag (`27b-mlx`) is
    /// unchanged by a republish and `UpdateCandidate`'s contract is that
    /// current and target differ, and the most the pull can download, from
    /// the same two manifests (`changed_blob_bytes`) — or `Err(reason)` when
    /// either manifest could not be read/fetched/parsed or the local bytes
    /// do not match the daemon's live manifest digest. A network failure
    /// or a 404 for a model removed upstream must not crash the whole
    /// `check_updates` call, so the caller turns that into a single
    /// `checkable: false` candidate for just this model. A reference whose
    /// parts would escape `models_root` is refused the same way, by
    /// `contained_manifest_path`, before anything is read. Only a request
    /// with no answer, or a 408, 429 or 5xx, is one checking again can get
    /// past (`LookupFailure`): a 404 -- a model made with `ollama create`,
    /// one removed upstream -- and a local manifest that cannot be read or
    /// parsed are said again next time. A local manifest that is not there
    /// -- the models kept elsewhere through `OLLAMA_MODELS`, which
    /// Banager's environment does not carry -- or that is in a protected
    /// place Banager does not read is no lookup at all: nothing is asked,
    /// at this check or the next (`LookupFailure::not_looked_up`).
    async fn compare_digests(
        &self,
        models_root: &Path,
        namespace: &str,
        name: &str,
        tag: &str,
        live_digest: &str,
    ) -> Result<Option<RegistryChange>, LookupFailure> {
        // Ollama 0.40 canonicalizes the public registry's on-disk host to
        // ollama.com in manifests-v2 (manifest/paths.go, canonicalV2Name).
        // The network request below still goes only to registry.ollama.ai.
        let v2_path = contained_manifest_path(
            &models_root.join("manifests-v2/ollama.com"),
            namespace,
            name,
            tag,
        )?;
        let legacy_path = contained_manifest_path(
            &models_root.join("manifests/registry.ollama.ai"),
            namespace,
            name,
            tag,
        )?;
        let protected = Protected::of_this_process();
        let read = |path: &Path| crate::adapters::read_file::read_text(path, &protected);
        // Per-model fallback, only for a missing entry/target (as Ollama's
        // resolveManifestPath does). A malformed, unreadable, oversized or
        // protected v2 manifest must not be hidden by the old downgrade anchor.
        // Both reads follow links through the same protected-place checks and
        // regular-file/16 MiB bound; no blob is opened via a separate reader.
        let (local_path, local_json) = match read(&v2_path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                (&legacy_path, read(&legacy_path))
            }
            result => (&v2_path, result),
        };
        let local_json = local_json.map_err(|e| {
            let reason = format!(
                "could not read local manifest {}: {e}",
                local_path.display()
            );
            if e.kind() == std::io::ErrorKind::NotFound || look::is_protected(&e) {
                LookupFailure::not_looked_up(reason)
            } else {
                LookupFailure::from(reason)
            }
        })?;
        // An Ollama 0.40 manifest list (one model per runner) has no layers
        // of its own, and its /api/tags row carries the selected child's
        // digest. Not compared, by design: the same at every check.
        if is_manifest_list(&local_json) {
            return Err(LookupFailure::not_looked_up(format!(
                "local manifest {} is a manifest list (one model per runner), which is not compared",
                local_path.display()
            )));
        }
        let local_digests = layer_digests(&local_json)
            .map_err(|e| format!("could not parse local manifest: {e}"))?;
        let local_config = config_digest(&local_json)
            .map_err(|e| format!("could not parse local manifest: {e}"))?;

        // /api/tags identifies the manifest bytes, not its config or layer
        // digests. A loopback daemon may use a different OLLAMA_MODELS store.
        use sha2::{Digest, Sha256};
        let local_digest = format!("{:x}", Sha256::digest(local_json.as_bytes()));
        if local_digest != live_digest {
            return Err(LookupFailure::not_looked_up(
                "local manifest does not match the model reported by this daemon".to_string(),
            ));
        }

        // Percent-encoded per segment: these three come off the network in
        // an `/api/tags` body, and raw they can re-point the request within
        // the registry (a `?` in the tag turns the rest of the path into a
        // query, a `#` truncates it at a fragment). The scheme and host are
        // fixed above the first interpolation point, so the request always
        // goes to registry.ollama.ai either way.
        let registry_url = format!(
            "https://registry.ollama.ai/v2/{}/{}/manifests/{}",
            url_path_segment(namespace)?,
            url_path_segment(name)?,
            url_path_segment(tag)?
        );
        let response = get_ok(
            self.http.as_ref(),
            registry_url,
            vec![(
                "Accept".to_string(),
                "application/vnd.docker.distribution.manifest.v2+json".to_string(),
            )],
            "registry request failed",
            "registry",
        )
        .await?;
        let registry_digests = layer_digests(&response.body)
            .map_err(|e| format!("could not parse registry manifest: {e}"))?;
        let registry_config = config_digest(&response.body)
            .map_err(|e| format!("could not parse registry manifest: {e}"))?;
        // The config is as much the model as a layer is: `ollama pull`
        // fetches it with them and writes the new manifest (0.40
        // `server/images.go` PullModel), and it alone carries the model's
        // renderer, parser, capabilities and sampler defaults -- and, for a
        // cloud model (`gpt-oss:120b-cloud`, `"layers":[]`), all there is.
        // So the model is up to date only when both are the same.
        if local_digests == registry_digests && local_config == registry_config {
            return Ok(None);
        }
        // Every Ollama model's manifest has a config, a cloud model's too:
        // a 200 answer without one is no model to offer.
        if registry_config.is_none() {
            return Err("registry manifest has no config digest".to_string().into());
        }
        Ok(Some(RegistryChange {
            manifest: manifest_digest(&response.body),
            download_bytes: changed_blob_bytes(&local_json, &response.body),
        }))
    }

    async fn check_one_model(
        &self,
        models_root: &Path,
        artifact: &InstalledArtifact,
    ) -> Option<UpdateCandidate> {
        // A model from another registry -- `hf.co/user/repo:tag`, the one
        // mirror Ollama documents, or any other host -- is not looked up:
        // Ollama keeps its manifest under a different host directory in
        // `manifests-v2` or `manifests`, and registry.ollama.ai is the one registry
        // this check asks. Nothing is read and nothing asked, at this
        // check or the next (`LookupFailure::not_looked_up`).
        if let Some(host) =
            named_registry(&artifact.key.name).filter(|host| *host != OLLAMA_REGISTRY)
        {
            return Some(uncheckable_candidate(
                artifact.key.clone(),
                artifact.version.clone(),
                UpdateChannel::Digest,
                LookupFailure::not_looked_up(format!(
                    "models from {host} are not looked up; only those from {OLLAMA_REGISTRY} are"
                )),
            ));
        }
        let (namespace, name, tag) = split_model_reference(&artifact.key.name);
        match self
            .compare_digests(models_root, &namespace, &name, &tag, &artifact.version)
            .await
        {
            Ok(None) => None,
            // `current` is the local manifest's digest as `/api/tags` gives
            // it and `parse_tags` stored it, the artifact's version (bare
            // hex); `target` is the registry manifest's (`manifest_digest`,
            // `sha256:<hex>`). Reporting the tag on both sides (`27b-mlx ->
            // 27b-mlx`) would satisfy no reader, so these two carry the
            // change instead. Each names one whole manifest -- every layer
            // and the config -- so each republish of the tag has a `target`
            // of its own, and Skip This Version skips that one build. Still:
            // never compare, diff or equality-check them, and never render
            // them as a version jump the way npm's or cargo's version
            // strings can be. The up-to-date decision is made above by
            // `compare_digests` on the layer-digest sets and the config
            // digests, never by these fields; an `UpdateChannel::Digest` row
            // is a "changed / not changed" marker.
            Ok(Some(change)) => Some(UpdateCandidate {
                key: artifact.key.clone(),
                current: artifact.version.clone(),
                target: change.manifest,
                channel: UpdateChannel::Digest,
                checkable: true,
                warnings: Vec::new(),
                blocked: None,
                download_bytes: change.download_bytes,
            }),
            // Uncheckable: there is no target to claim. `checkable: false`
            // is what stops the UI offering an Update button for this row
            // (Task 12); `current` and `target` are both the local digest
            // precisely because nothing was learned about the remote one.
            Err(reason) => Some(uncheckable_candidate(
                artifact.key.clone(),
                artifact.version.clone(),
                UpdateChannel::Digest,
                reason,
            )),
        }
    }

    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        let installed = self.inventory(inst).await?;
        if !models_on_this_mac(inst) {
            return Ok(installed
                .iter()
                .map(|artifact| {
                    uncheckable_candidate(
                        artifact.key.clone(),
                        artifact.version.clone(),
                        UpdateChannel::Digest,
                        LookupFailure::not_looked_up(
                            "remote daemon manifests cannot be checked from this Mac".to_string(),
                        ),
                    )
                })
                .collect::<Vec<_>>()
                .into());
        }
        let models_root = inst.prefix.join("models");
        let answers = super::registry_checks(
            installed
                .iter()
                .map(|artifact| async { Ok(self.check_one_model(&models_root, artifact).await) })
                .collect(),
        )
        .await;
        let out: Vec<_> = installed
            .iter()
            .zip(answers)
            .filter_map(|(artifact, answer)| {
                answer.unwrap_or_else(|reason| {
                    Some(uncheckable_candidate(
                        artifact.key.clone(),
                        artifact.version.clone(),
                        UpdateChannel::Digest,
                        reason,
                    ))
                })
            })
            .collect();
        Ok(out.into())
    }

    pub async fn search(
        &self,
        _inst: &ManagerInstance,
        _query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        Err(AdapterError::Unsupported(
            "Ollama has no search command Banager uses; browse the model library directly"
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
        ensure_instance_match(req, inst)?;
        validate_model_reference(&req.name)?;
        if req.kind == OpKind::Link {
            return Err(super::links_nothing(&self.meta.id));
        }
        let lock = ResourceLock(inst.id.clone());
        // The registry warning belongs to the arm that fetches, not to the
        // operation as a whole. Computed before this `match` it also rode
        // onto Uninstall plans, where `UninstallDialog` puts it under
        // "Before you continue:" -- the one destructive confirmation
        // screen in the app -- to say something that is no reason to
        // hesitate about deleting anything.
        //
        // Named, never blocked: pulling from a third-party registry is a
        // legitimate thing to want, it just has to be said out loud.
        // `Plan::warnings` is already rendered in the preview.
        let (args, warnings) = match req.kind {
            OpKind::Link => return Err(super::links_nothing(&self.meta.id)),
            OpKind::Install | OpKind::Upgrade => {
                let mut warnings: Vec<Warning> = third_party_registry(&req.name)
                    .map(|host| Warning::ThirdPartyRegistry {
                        host: host.to_string(),
                    })
                    .into_iter()
                    .collect();
                // An upgrade fetches what changed, which can be gigabytes
                // of new weights: said, as a crate that compiles is.
                if req.kind == OpKind::Upgrade {
                    warnings.push(Warning::DownloadsModelChanges);
                }
                (vec!["pull".to_string(), req.name.clone()], warnings)
            }
            // What `ollama rm` removes and leaves (Ollama 0.34.1
            // `server/routes.go:1249-1299`: the model's manifest and the
            // layers no other model uses), said under the tool -- not under
            // "Before you continue:", which is `warningGroup`'s to decide.
            OpKind::Uninstall => (
                vec!["rm".to_string(), req.name.clone()],
                vec![Warning::UninstallScope {
                    what: UninstallScope::Ollama,
                }],
            ),
        };
        Ok(Plan {
            request: req.clone(),
            action: PlanAction::Command {
                program: inst.exe_path.clone(),
                args,
                env: vec![("OLLAMA_HOST".to_string(), self.endpoint(inst))],
            },
            needs_password: false,
            locks: vec![lock],
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings,
            affected: Vec::new(),
            basis: None,
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
        Ok(reconcile_from(artifacts, key))
    }
}

#[async_trait]
impl Adapter for OllamaAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
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
    ) -> Result<CheckOutcome, AdapterError> {
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
    #[derive(Default)]
    struct EnvRecorder(Mutex<Vec<CommandSpec>>);

    #[async_trait]
    impl CommandRunner for EnvRecorder {
        async fn run(
            &self,
            spec: CommandSpec,
            _on_line: Option<crate::runner::LineCallback>,
            _cancel: CancellationToken,
        ) -> Result<crate::runner::CommandOutput, crate::runner::RunnerError> {
            self.0.lock().unwrap().push(spec);
            Ok(crate::runner::CommandOutput {
                stdout: "ollama version is 0.34.1".into(),
                stderr: String::new(),
                exit_code: Some(0),
                timed_out: false,
                cancelled: false,
                stderr_cause: Default::default(),
            })
        }
    }

    /// Detection, inventory, planning, the operation's summary and its
    /// execution, for an `OLLAMA_HOST` with a login and for the ordinary
    /// one with none. With a login, nothing the window is sent carries it
    /// (the instance, the model keys, the plan, the summary), while every
    /// request and every command still gets the address as written. With
    /// none, the id, the keys and the plan's `OLLAMA_HOST` are exactly the
    /// address, as before: the keys saved in settings and history match.
    #[tokio::test]
    async fn login_stays_private_through_detect_plan_execution_and_inventory() {
        let tags = std::fs::read_to_string("../../adapters/fixtures/ollama/0.34.1/api-tags.json")
            .expect("read ollama api-tags.json fixture");
        for (host, public_id, preview) in [
            (
                "http://alice:s%40cret@server:11434",
                "ollama:http://server:11434",
                "http://****:****@server:11434",
            ),
            (
                DEFAULT_HOST,
                "ollama:http://127.0.0.1:11434",
                "http://127.0.0.1:11434",
            ),
        ] {
            let dir = isolated_path_dir("private-login");
            let url = format!("{host}/api/tags");
            let runner = Arc::new(EnvRecorder::default());
            let http = Arc::new(MockHttpClient::new());
            http.respond(
                &url,
                HttpResponse {
                    status: 200,
                    body: tags.clone(),
                },
            );
            let adapter = Arc::new(OllamaAdapter::new(runner.clone(), http.clone()));
            let env = HostEnv {
                path_dirs: vec![dir.clone()],
                home: dir.clone(),
                euid: 501,
                cargo_home: None,
                rustup_home: None,
                zdotdir: None,
                ollama_host: Some(host.into()),
            };
            let inst = adapter.detect(&env).await.remove(0);
            assert_eq!(inst.id, public_id);
            assert!(!serde_json::to_string(&inst).unwrap().contains("alice"));
            let models = adapter.inventory(&inst).await.unwrap();
            assert_eq!(models.len(), 1, "{host}");
            for model in &models {
                assert_eq!(model.key.instance_id, public_id);
            }
            assert!(!serde_json::to_string(&models).unwrap().contains("alice"));
            let mut manager =
                crate::ops::OperationManager::new(Arc::new(crate::events::VecSink::new()));
            manager.register_adapter(adapter.clone());
            let manager = Arc::new(manager);
            manager.register_instance(inst.clone());
            for kind in [OpKind::Install, OpKind::Upgrade, OpKind::Uninstall] {
                let req = OpRequest {
                    kind,
                    instance_id: inst.id.clone(),
                    artifact_kind: crate::model::ArtifactKind::Model,
                    name: "qwen3.8:27b-mlx".into(),
                };
                let plan = adapter.plan(&inst, &req).await.unwrap();
                let wire = serde_json::to_value(&plan).unwrap();
                assert_eq!(
                    wire["action"]["Command"]["env"],
                    serde_json::json!([["OLLAMA_HOST", preview]])
                );
                let wire = wire.to_string();
                assert!(!wire.contains("alice"));
                assert!(!wire.contains("s%40cret"));
                let id = manager.submit(plan);
                manager.wait(id).await;
                let summary = manager
                    .summaries()
                    .into_iter()
                    .find(|s| s.id == id)
                    .unwrap();
                assert_eq!(
                    summary.env_preview,
                    vec![("OLLAMA_HOST".into(), preview.into())]
                );
                let wire = serde_json::to_string(&summary).unwrap();
                assert!(!wire.contains("alice"));
                let back: crate::ops::OpSummary = serde_json::from_str(&wire).unwrap();
                assert_eq!(back, summary);
            }
            let calls = runner.0.lock().unwrap();
            let writes: Vec<_> = calls.iter().filter(|c| c.args[0] != "--version").collect();
            assert_eq!(writes.len(), 3);
            for call in writes {
                assert_eq!(call.env, vec![("OLLAMA_HOST".into(), host.into())]);
            }
            // Detection, the inventory and each operation's check after it.
            assert!(http.calls().len() >= 5, "{:?}", http.calls());
            assert!(http.calls().iter().all(|called| called == &url));
            drop(calls);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[tokio::test]
    async fn inventory_masks_transport_status_and_parse_errors() {
        let host = "http://alice:s%40cret@server:11434";
        let url = format!("{host}/api/tags");
        let inst = test_instance(host, PathBuf::from("/mock"));
        for status in [0, 401, 200] {
            let http = Arc::new(MockHttpClient::new());
            let echoed = format!("{url} alice s@cret s%40cret");
            if status == 0 {
                http.fail(&url, &echoed);
            } else {
                http.respond(
                    &url,
                    HttpResponse {
                        status,
                        body: if status == 200 {
                            serde_json::json!({"models": echoed}).to_string()
                        } else {
                            echoed
                        },
                    },
                );
            }
            let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http.clone());
            let error = adapter.inventory(&inst).await.unwrap_err().to_string();
            for secret in ["alice", "s%40cret", "s@cret"] {
                assert!(!error.contains(secret), "{error}");
            }
            assert_eq!(http.calls(), vec![url.clone()]);
        }
    }

    #[tokio::test]
    async fn regression_f01_local_daemon_rejects_an_unrelated_manifest() {
        // The default store keeps the recorded qwen3.8 manifest as a
        // leftover backup, while the daemon -- another loopback port, its
        // own OLLAMA_MODELS -- has since pulled a republished one. Its
        // `/api/tags` row (the recorded row, with that manifest's digest)
        // names a manifest the backup is not.
        use sha2::{Digest, Sha256};
        let backup = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/local-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read local manifest fixture");
        let mut republished: serde_json::Value =
            serde_json::from_str(&backup).expect("fixture parses");
        republished["config"]["digest"] = format!("sha256:{}", "c".repeat(64)).into();
        let republished = republished.to_string();
        let live = format!("{:x}", Sha256::digest(republished.as_bytes()));
        let recorded = "5642e97495e1a088883805981563dcdc4a040c2f53388b7a41d1f24d3622cf7e";
        let tags = std::fs::read_to_string("../../adapters/fixtures/ollama/0.34.1/api-tags.json")
            .expect("read ollama api-tags.json fixture")
            .replace(recorded, &live);
        let home = tempfile::tempdir().unwrap();
        let dir = home
            .path()
            .join("models/manifests/registry.ollama.ai/library/qwen3.8");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("27b-mlx"), &backup).unwrap();
        // Either way round the registry stands, comparing with the backup
        // would be wrong: equal to it hides the daemon's state, equal to
        // the daemon's offers what it already has.
        for registry in [&backup, &republished] {
            let http = Arc::new(MockHttpClient::new());
            http.respond(
                "http://127.0.0.1:11435/api/tags",
                HttpResponse {
                    status: 200,
                    body: tags.clone(),
                },
            );
            http.respond(
                "https://registry.ollama.ai/v2/library/qwen3.8/manifests/27b-mlx",
                HttpResponse {
                    status: 200,
                    body: registry.clone(),
                },
            );
            let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http.clone());
            let inst = test_instance("http://127.0.0.1:11435", home.path().into());
            let rows = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .unwrap()
                .candidates;
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].key.name, "qwen3.8:27b-mlx");
            assert!(!rows[0].checkable);
            assert_eq!(rows[0].download_bytes, None);
            assert_eq!(
                http.calls().len(),
                1,
                "unrelated local data must never reach registry comparison"
            );
        }
    }

    #[tokio::test]
    async fn test_remote_models_are_uncheckable_without_reading_local_manifests() {
        // A real, readable manifest is essential: with a missing file the
        // local comparison also returns uncheckable without a registry call,
        // so that setup cannot detect a missing remote-daemon guard.
        let home = tempfile::tempdir().unwrap();
        let model_dir = home
            .path()
            .join("models/manifests/registry.ollama.ai/library/qwen3.8");
        std::fs::create_dir_all(&model_dir).unwrap();
        std::fs::write(
            model_dir.join("27b-mlx"),
            std::fs::read(
                "../../adapters/fixtures/ollama/0.34.1/local-manifest-qwen3.8-27b-mlx.json",
            )
            .unwrap(),
        )
        .unwrap();
        let tags =
            std::fs::read_to_string("../../adapters/fixtures/ollama/0.34.1/api-tags.json").unwrap();
        let http = Arc::new(MockHttpClient::new());
        for host in ["http://server:11434", "http://127.0.0.1:11434"] {
            http.respond(
                &format!("{host}/api/tags"),
                HttpResponse {
                    status: 200,
                    body: tags.clone(),
                },
            );
        }
        let registry = "https://registry.ollama.ai/v2/library/qwen3.8/manifests/27b-mlx";
        let mut registry_manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(
                "../../adapters/fixtures/ollama/0.34.1/registry-manifest-qwen3.8-27b-mlx.json",
            )
            .unwrap(),
        )
        .unwrap();
        registry_manifest["layers"][0]["digest"] = format!("sha256:{}", "a".repeat(64)).into();
        http.respond(
            registry,
            HttpResponse {
                status: 200,
                body: registry_manifest.to_string(),
            },
        );
        let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http.clone());
        let inst = test_instance("http://server:11434", home.path().into());
        let rows = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .unwrap()
            .candidates;
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].checkable);
        assert_eq!(rows[0].current, rows[0].target);
        // Never looked up here, by design, rather than a lookup that did
        // not succeed (independent review r6, F5).
        assert_eq!(
            rows[0].warnings,
            vec![
                Warning::Message("remote daemon manifests cannot be checked from this Mac".into()),
                Warning::NotLookedUpHere,
            ]
        );
        assert_eq!(http.calls(), vec!["http://server:11434/api/tags"]);

        // The same local files and registry response must be actionable
        // for a local daemon, proving that neither fixture is vacuous.
        let local = test_instance("http://127.0.0.1:11434", home.path().into());
        let rows = adapter
            .check_updates(&local, &CheckOptions::default())
            .await
            .unwrap()
            .candidates;
        assert_eq!(rows.len(), 1);
        assert!(rows[0].checkable);
        assert_eq!(
            http.calls(),
            vec![
                "http://server:11434/api/tags",
                "http://127.0.0.1:11434/api/tags",
                registry,
            ]
        );
    }

    use super::*;
    use crate::events::VecSink;
    use crate::http::{HttpResponse, MockHttpClient};
    use crate::model::ArtifactKind;
    use crate::runner::{CommandOutput, MockRunner};
    use crate::testing::command_args;
    use std::path::PathBuf;

    /// These tests assert on the *text* of a dynamic, not-yet-localised
    /// `Warning::Message` (spec §6's backlog item -- ollama's warnings are
    /// out of this phase's scope). This reads the message back out so the
    /// assertions below can stay string-based.
    fn warnings_text(warnings: &[Warning]) -> String {
        warnings
            .iter()
            .map(|w| match w {
                Warning::Message(m) => m.clone(),
                other => format!("{other:?}"),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn test_instance(host: &str, prefix: PathBuf) -> ManagerInstance {
        ManagerInstance {
            exe_path: PathBuf::from("/usr/local/bin/ollama"),
            prefix,
            version: Some("0.34.1".to_string()),
            ..crate::testing::manager_instance("ollama", &format!("ollama:{host}"))
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

        let tmp_root = crate::testing::unique_temp_path("ollama-manifests");
        let model_dir = tmp_root.join("manifests/registry.ollama.ai/library/qwen3.8");
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
            .compare_digests(
                &tmp_root,
                "library",
                "qwen3.8",
                "27b-mlx",
                "5642e97495e1a088883805981563dcdc4a040c2f53388b7a41d1f24d3622cf7e",
            )
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

        let home = crate::testing::unique_temp_path("ollama-home");
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
            .expect("check_updates")
            .candidates;
        assert!(candidates.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_check_updates_flags_the_model_as_outdated_when_the_registry_manifest_differs() {
        // The committed fixture pair is deliberately the already-up-to-date
        // case (identical 1209-digest sets), so the branch that produces a
        // real, checkable candidate cannot be reached from fixtures alone.
        // Keep the recorded local manifest and mock the registry side with
        // inline JSON carrying a different config digest and different layer
        // digests — "Inline JSON in a unit test is fine and is not a fixture"
        // (Global Constraints), the same technique the cargo adapter uses in
        // `test_check_updates_flags_the_fixture_crate_as_outdated`.
        const REPUBLISHED_CONFIG_DIGEST: &str =
            "sha256:9f1c0b6d2e4a58c3719d84b0ff62a7d5c1e830469b2a4f7d8c6051e39ab7d240";
        let republished_manifest = format!(
            r#"{{"schemaVersion":2,
                "mediaType":"application/vnd.docker.distribution.manifest.v2+json",
                "config":{{"mediaType":"application/vnd.docker.container.image.v1+json",
                          "digest":"{REPUBLISHED_CONFIG_DIGEST}","size":251}},
                "layers":[
                  {{"mediaType":"application/vnd.ollama.image.tensor",
                   "digest":"sha256:3c9d1f0a77b45e2681df0c4a95b3e7182d6a0f4c58b91e7d03a26f8c4b1d95e0",
                   "size":715161924,"name":"lm_head.weight"}},
                  {{"mediaType":"application/vnd.ollama.image.tensor",
                   "digest":"sha256:b7e4a1c05f38d296471ea80c3b95d6f2081c74a3e9d05b6f2a8c41739de60bb2",
                   "size":2542796928,"name":"model.layers.0.mlp.down_proj.weight"}}
                ]}}"#
        );

        let tags_json =
            std::fs::read_to_string("../../adapters/fixtures/ollama/0.34.1/api-tags.json")
                .expect("read ollama api-tags.json fixture");
        let local_json = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/local-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read local manifest fixture");

        let home = crate::testing::unique_temp_path("ollama-outdated");
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
                body: republished_manifest.clone(),
            },
        );
        let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http);
        let inst = test_instance("http://127.0.0.1:11434", home.clone());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;

        assert_eq!(candidates.len(), 1, "one installed model, one candidate");
        let candidate = &candidates[0];
        assert_eq!(candidate.key.name, "qwen3.8:27b-mlx");
        assert!(
            candidate.checkable,
            "a differing registry manifest is a real, offerable update"
        );
        assert!(candidate.warnings.is_empty());
        assert_eq!(candidate.channel, UpdateChannel::Digest);
        // `target` is the registry manifest's own digest, not the tag: the
        // tag (`27b-mlx`) is unchanged by a republish, so it could never
        // show a difference. Nor the config digest: a republish of new
        // weights alone keeps that (r40 R40-4).
        assert_eq!(candidate.target, manifest_digest(&republished_manifest));
        assert_ne!(candidate.target, REPUBLISHED_CONFIG_DIGEST);
        // `current` is the local digest `/api/tags` reported, so the two
        // sides of the candidate really differ.
        assert_eq!(
            candidate.current,
            "5642e97495e1a088883805981563dcdc4a040c2f53388b7a41d1f24d3622cf7e"
        );
        assert_ne!(candidate.current, candidate.target);
        // Both layers and the config are new to this Mac: the most the pull
        // downloads is all three, as the registry manifest sizes them.
        assert_eq!(
            candidate.download_bytes,
            Some(715_161_924 + 2_542_796_928 + 251)
        );

        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_check_updates_says_the_most_a_model_downloads_and_none_when_a_size_is_missing() {
        // One layer the recorded local manifest already names (lm_head,
        // 715161924 bytes) and one it does not, under the same config: only
        // the new layer counts. Without its size, the number is unknown.
        const LOCAL_CONFIG: &str =
            "sha256:25a98d24af806ec8c25c21df601953c6a42f154dfcd8637bc82ec581f1c849aa";
        const SHARED_LAYER: &str =
            "sha256:830bcce777461c80d35963b0c43a0ae31f5ebbb6fdcf1e3dbbadafb5c8d39991";
        const NEW_LAYER: &str =
            "sha256:b7e4a1c05f38d296471ea80c3b95d6f2081c74a3e9d05b6f2a8c41739de60bb2";
        let registry_manifest = |new_size: &str| {
            format!(
                r#"{{"schemaVersion":2,
                    "mediaType":"application/vnd.docker.distribution.manifest.v2+json",
                    "config":{{"digest":"{LOCAL_CONFIG}","size":251}},
                    "layers":[
                      {{"digest":"{SHARED_LAYER}","size":715161924}},
                      {{"digest":"{NEW_LAYER}"{new_size}}}
                    ]}}"#
            )
        };
        let tags_json =
            std::fs::read_to_string("../../adapters/fixtures/ollama/0.34.1/api-tags.json")
                .expect("read ollama api-tags.json fixture");
        let local_json = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/local-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read local manifest fixture");
        for (new_size, expected) in [(r#","size":2542796928"#, Some(2_542_796_928)), ("", None)] {
            let home = crate::testing::unique_temp_path("ollama-download");
            let model_dir = home.join("models/manifests/registry.ollama.ai/library/qwen3.8");
            std::fs::create_dir_all(&model_dir).expect("create fixture manifest dir");
            std::fs::write(model_dir.join("27b-mlx"), &local_json).expect("write local manifest");
            let http = Arc::new(MockHttpClient::new());
            http.respond(
                "http://127.0.0.1:11434/api/tags",
                HttpResponse {
                    status: 200,
                    body: tags_json.clone(),
                },
            );
            http.respond(
                "https://registry.ollama.ai/v2/library/qwen3.8/manifests/27b-mlx",
                HttpResponse {
                    status: 200,
                    body: registry_manifest(new_size),
                },
            );
            let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http);
            let inst = test_instance("http://127.0.0.1:11434", home.clone());
            let candidates = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("check_updates")
                .candidates;
            assert_eq!(candidates.len(), 1);
            assert!(candidates[0].checkable, "still an offerable update");
            // The config is the installed one; the update is still a build
            // of its own, named by its manifest (r40 R40-4).
            assert_eq!(
                candidates[0].target,
                manifest_digest(&registry_manifest(new_size))
            );
            assert_ne!(candidates[0].target, LOCAL_CONFIG);
            assert_eq!(candidates[0].download_bytes, expected, "size {new_size:?}");
            let _ = std::fs::remove_dir_all(&home);
        }
    }

    #[tokio::test]
    async fn test_check_updates_tells_a_registry_404_from_a_request_with_no_answer() {
        // A model made with `ollama create`, or one gone from the library,
        // gets a 404 at every check: a failure no later check gets past,
        // so no `TransientLookupFailure`. A request that got no answer has
        // one (walk-2 review 1.1).
        let tags_json =
            std::fs::read_to_string("../../adapters/fixtures/ollama/0.34.1/api-tags.json")
                .expect("read ollama api-tags.json fixture");
        let local_json = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/local-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read local manifest fixture");
        let registry = "https://registry.ollama.ai/v2/library/qwen3.8/manifests/27b-mlx";
        for (status, transient) in [(Some(404), false), (None, true)] {
            let home = crate::testing::unique_temp_path("ollama-lookup");
            let model_dir = home.join("models/manifests/registry.ollama.ai/library/qwen3.8");
            std::fs::create_dir_all(&model_dir).expect("create fixture manifest dir");
            std::fs::write(model_dir.join("27b-mlx"), &local_json).expect("write local manifest");
            let http = Arc::new(MockHttpClient::new());
            http.respond(
                "http://127.0.0.1:11434/api/tags",
                HttpResponse {
                    status: 200,
                    body: tags_json.clone(),
                },
            );
            match status {
                Some(status) => http.respond(
                    registry,
                    HttpResponse {
                        status,
                        body: r#"{"errors":[{"code":"MANIFEST_UNKNOWN"}]}"#.to_string(),
                    },
                ),
                None => http.fail(registry, "dns error: failed to lookup address information"),
            }
            let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http);
            let inst = test_instance("http://127.0.0.1:11434", home.clone());
            let candidates = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("check_updates")
                .candidates;
            assert_eq!(candidates.len(), 1);
            assert!(!candidates[0].checkable);
            assert_eq!(
                candidates[0]
                    .warnings
                    .contains(&Warning::TransientLookupFailure),
                transient,
                "{status:?}: {:?}",
                candidates[0].warnings
            );
            let _ = std::fs::remove_dir_all(&home);
        }
    }

    #[tokio::test]
    async fn test_compare_digests_never_reads_a_manifest_kept_in_a_protected_place() {
        // `~/.ollama` a link into `~/Documents`: the local manifest is not
        // read there -- the model cannot be checked, as when the file
        // cannot be read -- and is read where nothing is protected.
        let local_json = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/local-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read local manifest fixture");
        let registry_json = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/registry-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read registry manifest fixture");
        let home = std::fs::canonicalize(isolated_path_dir("ollama-manifest-protected")).unwrap();
        let kept = home.join("Documents/ollama");
        let model_dir = kept.join("models/manifests/registry.ollama.ai/library/qwen3.8");
        std::fs::create_dir_all(&model_dir).unwrap();
        std::fs::write(model_dir.join("27b-mlx"), &local_json).unwrap();
        std::os::unix::fs::symlink(&kept, home.join(".ollama")).unwrap();
        let models_root = home.join(".ollama/models");
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "https://registry.ollama.ai/v2/library/qwen3.8/manifests/27b-mlx",
            HttpResponse {
                status: 200,
                body: registry_json,
            },
        );
        let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http);
        let read = adapter
            .compare_digests(
                &models_root,
                "library",
                "qwen3.8",
                "27b-mlx",
                "5642e97495e1a088883805981563dcdc4a040c2f53388b7a41d1f24d3622cf7e",
            )
            .await;
        assert!(read.is_ok(), "read where nothing is protected");
        let as_if = crate::protected::as_if_home(&home);
        let refused = adapter
            .compare_digests(
                &models_root,
                "library",
                "qwen3.8",
                "27b-mlx",
                "5642e97495e1a088883805981563dcdc4a040c2f53388b7a41d1f24d3622cf7e",
            )
            .await;
        drop(as_if);
        // No request made, now or next time: not looked up on this Mac
        // (F5 review), as the models of an Ollama on another Mac are.
        let refused = refused.expect_err("refused in a protected place");
        assert!(refused.not_looked_up, "{refused:?}");
        assert!(!refused.transient, "{refused:?}");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_check_updates_marks_a_model_uncheckable_when_the_local_manifest_is_missing() {
        // Edge case the fixture cannot show directly: the local manifest
        // file is absent -- the models kept elsewhere through
        // `OLLAMA_MODELS`, which Banager's environment does not carry, or
        // deleted out from under Banager. No request is made, at this
        // check or the next: not looked up on this Mac (F5 review), so
        // it keeps no Overview from its all good for good.
        let tags_json =
            std::fs::read_to_string("../../adapters/fixtures/ollama/0.34.1/api-tags.json")
                .expect("read ollama api-tags.json fixture");
        let home = crate::testing::unique_temp_path("ollama-missing-manifest");
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "http://127.0.0.1:11434/api/tags",
            HttpResponse {
                status: 200,
                body: tags_json.clone(),
            },
        );
        let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http.clone());
        let inst = test_instance("http://127.0.0.1:11434", home.clone());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates should not fail outright when one model's manifest is missing")
            .candidates;
        assert_eq!(candidates.len(), 1);
        assert!(!candidates[0].checkable);
        assert!(
            candidates[0].warnings.contains(&Warning::NotLookedUpHere),
            "{:?}",
            candidates[0].warnings
        );
        assert!(!candidates[0]
            .warnings
            .contains(&Warning::TransientLookupFailure));
        assert!(warnings_text(&candidates[0].warnings).contains("could not read local manifest"));
        assert_eq!(http.calls(), vec!["http://127.0.0.1:11434/api/tags"]);

        // A local manifest that is there but does not parse is a lookup
        // that did not succeed, not one never made.
        let model_dir = home.join("models/manifests/registry.ollama.ai/library/qwen3.8");
        std::fs::create_dir_all(&model_dir).expect("create fixture manifest dir");
        std::fs::write(model_dir.join("27b-mlx"), "not json").expect("write local manifest");
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        assert_eq!(candidates.len(), 1);
        assert!(!candidates[0].checkable);
        assert!(
            !candidates[0].warnings.contains(&Warning::NotLookedUpHere),
            "{:?}",
            candidates[0].warnings
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_check_updates_does_not_look_up_a_model_from_another_registry() {
        // `hf.co/user/repo:tag`, the one mirror Ollama documents (and any
        // other host, `modelscope.cn/…`): Ollama keeps its manifest under
        // `manifests/hf.co/…`, not under `manifests/registry.ollama.ai`,
        // and registry.ollama.ai is the one registry this check asks. So
        // nothing is read and nothing asked: not looked up on this Mac
        // (F5 review), rather than a lookup that fails the same way at
        // every check and keeps the Overview grey for good.
        let model = "hf.co/bartowski/Llama-3.2-3B-Instruct-GGUF:Q4_K_M";
        let local_json = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/local-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read local manifest fixture");
        let home = crate::testing::unique_temp_path("ollama-other-registry");
        // Where Ollama keeps it, and where a lookup here would have read it.
        for root in [
            "models/manifests/hf.co",
            "models/manifests/registry.ollama.ai/hf.co",
        ] {
            let dir = home.join(root).join("bartowski/Llama-3.2-3B-Instruct-GGUF");
            std::fs::create_dir_all(&dir).expect("create fixture manifest dir");
            std::fs::write(dir.join("Q4_K_M"), &local_json).expect("write local manifest");
        }
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "http://127.0.0.1:11434/api/tags",
            HttpResponse {
                status: 200,
                body: tags_body_naming(model),
            },
        );
        let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http.clone());
        let inst = test_instance("http://127.0.0.1:11434", home.clone());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates")
            .candidates;
        assert_eq!(candidates.len(), 1);
        assert!(!candidates[0].checkable);
        assert_eq!(candidates[0].current, candidates[0].target);
        assert_eq!(
            candidates[0].warnings,
            vec![
                Warning::Message(
                    "models from hf.co are not looked up; only those from registry.ollama.ai are"
                        .into()
                ),
                Warning::NotLookedUpHere,
            ]
        );
        assert_eq!(http.calls(), vec!["http://127.0.0.1:11434/api/tags"]);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn test_named_registry_is_a_host_like_first_segment_of_a_longer_reference() {
        assert_eq!(named_registry("hf.co/user/repo:tag"), Some("hf.co"));
        assert_eq!(
            named_registry("modelscope.cn/Qwen/Qwen3-8B"),
            Some("modelscope.cn")
        );
        assert_eq!(
            named_registry("registry.ollama.ai/library/qwen3.8:27b-mlx"),
            Some("registry.ollama.ai")
        );
        // Ollama's own library: a dotted model name, a user's namespace.
        assert_eq!(named_registry("qwen3.8:27b-mlx"), None);
        assert_eq!(named_registry("someuser/somemodel:sometag"), None);
        assert_eq!(named_registry("library/qwen3.8:27b-mlx"), None);
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
            assert_eq!(command_args(&plan), expected);
            assert_eq!(
                crate::testing::command_env(&plan),
                vec![("OLLAMA_HOST".to_string(), host_of(&inst).to_string())]
            );
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
                stderr_cause: Default::default(),
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
            rustup_home: None,
            zdotdir: None,
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
            "banager-ollama-{label}-{}-{}",
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
        let tmp_dir = isolated_path_dir("detect-running");
        let exe_path = tmp_dir.join("ollama");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![exe_path.to_str().expect("utf8 temp path"), "--version"],
            CommandOutput {
                stderr_cause: Default::default(),
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
            rustup_home: None,
            zdotdir: None,
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
        assert!(instances[0].available());
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
    async fn test_detect_marks_the_instance_not_running_when_the_daemon_does_not_answer() {
        let tmp_dir = isolated_path_dir("detect-not-running");
        let exe_path = tmp_dir.join("ollama");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![exe_path.to_str().expect("utf8 temp path"), "--version"],
            CommandOutput {
                stderr_cause: Default::default(),
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
            rustup_home: None,
            zdotdir: None,
            // A non-default port on this Mac, so the id assertion below
            // still proves the instance is keyed by the configured host.
            ollama_host: Some("http://localhost:11500".to_string()),
        };
        let adapter = OllamaAdapter::new(runner, Arc::new(MockHttpClient::new()))
            .with_app_present_fn(|_| true);
        let instances = adapter.detect(&env).await;

        // Still one instance, so the UI can offer to start the daemon
        // (Task 12) rather than the source vanishing from the list.
        assert_eq!(instances.len(), 1);
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::NotRunning),
            "a daemon that does not answer /api/tags, on a Mac that has \
             Ollama.app, is NotRunning -- which is what puts the Open \
             Ollama button on its notice"
        );
        assert_eq!(instances[0].id, "ollama:http://localhost:11500");
        assert_eq!(instances[0].unverified_version, Some("9.9.9".to_string()));

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[tokio::test]
    async fn test_detect_does_not_offer_to_open_an_app_that_is_not_installed() {
        // `brew install ollama` installs the CLI and no app. Such a user
        // used to get "Ollama isn't running" over an Open Ollama button
        // that ran `open -a Ollama`, failed, and did nothing at all --
        // every launch, forever, with no message. `NotRunning` is the one
        // state whose notice carries that button, so it is now reserved
        // for the case where there is something to open; without the app
        // the daemon is simply not answering.
        let tmp_dir = isolated_path_dir("detect-cli-only");
        let exe_path = tmp_dir.join("ollama");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![exe_path.to_str().expect("utf8 temp path"), "--version"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: "ollama version is 9.9.9\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let env = HostEnv {
            path_dirs: vec![tmp_dir.clone()],
            home: PathBuf::from("/tmp/fake-home"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let adapter = OllamaAdapter::new(runner, Arc::new(MockHttpClient::new()))
            .with_app_present_fn(|_| false);
        let instances = adapter.detect(&env).await;

        assert_eq!(instances.len(), 1, "the source must still be listed");
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::NotResponding),
            "with no Ollama.app there is nothing for the Open button to \
             open, so this must not be the state that shows one"
        );

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[tokio::test]
    async fn test_detect_does_not_offer_to_open_the_app_for_a_daemon_on_another_machine() {
        // With `OLLAMA_HOST` naming another machine, Open Ollama would
        // start a daemon on this Mac and leave the one Banager is asking
        // exactly as silent -- a button that cannot do what it says. So
        // even with Ollama.app installed here, this is NotResponding.
        let tmp_dir = isolated_path_dir("detect-remote-host");
        let exe_path = tmp_dir.join("ollama");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![exe_path.to_str().expect("utf8 temp path"), "--version"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: "ollama version is 9.9.9\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let env = HostEnv {
            path_dirs: vec![tmp_dir.clone()],
            home: PathBuf::from("/tmp/fake-home"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: Some("http://10.0.0.5:11434".to_string()),
        };
        let adapter = OllamaAdapter::new(runner, Arc::new(MockHttpClient::new()))
            .with_app_present_fn(|_| true);
        let instances = adapter.detect(&env).await;

        assert_eq!(instances.len(), 1, "the source must still be listed");
        assert_eq!(instances[0].id, "ollama:http://10.0.0.5:11434");
        assert_eq!(
            instances[0].status.unavailable,
            Some(Unavailable::NotResponding),
            "opening Ollama.app here cannot bring up a daemon on 10.0.0.5"
        );

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[tokio::test]
    async fn test_detect_says_an_https_ollama_host_is_refused_and_never_asks_it() {
        // `RealHttpClient` connects to no https host off its allowlist, and
        // a daemon's never is on it. That refusal used to be discarded and
        // shown as a daemon that did not answer -- or, on this Mac with
        // Ollama.app installed, as one that was not running, over an Open
        // Ollama button that could not help, since the next request is
        // refused the same way. Now it is its own state, and the request
        // is not even made: the mock would answer 200 if it were.
        let tmp_dir = isolated_path_dir("detect-https-host");
        let exe_path = tmp_dir.join("ollama");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![exe_path.to_str().expect("utf8 temp path"), "--version"],
            CommandOutput {
                stderr_cause: Default::default(),
                exit_code: Some(0),
                stdout: "ollama version is 9.9.9\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        for host in ["https://localhost:11434", "https://ollama.home.lan"] {
            let http = Arc::new(MockHttpClient::new());
            http.respond(
                &format!("{host}/api/tags"),
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
                rustup_home: None,
                zdotdir: None,
                ollama_host: Some(host.to_string()),
            };
            let adapter = OllamaAdapter::new(runner.clone(), http.clone())
                // Ollama.app is here: without the refusal, a localhost
                // daemon that did not answer would be `NotRunning`.
                .with_app_present_fn(|_| true);
            let instances = adapter.detect(&env).await;

            assert_eq!(
                instances.len(),
                1,
                "{host}: the source must still be listed"
            );
            assert_eq!(instances[0].id, format!("ollama:{host}"));
            assert_eq!(
                instances[0].status.unavailable,
                Some(Unavailable::HttpsHostRefused),
                "{host}"
            );
            assert_eq!(instances[0].version, Some("9.9.9".to_string()));
            assert!(
                http.calls().is_empty(),
                "{host}: a request the client would refuse must not be made: {:?}",
                http.calls()
            );
        }

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_https_refused_is_the_clients_own_rule() {
        // Exactly what `host_allowed` refuses, and only for https: a plain
        // http daemon is always asked.
        assert!(https_refused("https://ollama.home.lan/api/tags"));
        assert!(https_refused("https://127.0.0.1:11434/api/tags"));
        assert!(!https_refused("http://ollama.home.lan/api/tags"));
        assert!(!https_refused(&format!("{DEFAULT_HOST}/api/tags")));
        // On the allowlist, so the client would send it.
        assert!(!https_refused("https://registry.ollama.ai/api/tags"));
        // Not a URL at all: not this state (normalize_ollama_host never
        // lets one through).
        assert!(!https_refused("not a url"));
    }

    #[test]
    fn test_host_is_this_mac_accepts_only_addresses_that_reach_this_mac() {
        for local in [
            DEFAULT_HOST,
            "http://localhost:11434",
            "http://LOCALHOST:11434",
            "http://127.0.0.2:11434",
            "http://0.0.0.0:11434",
            "http://[::1]:11434",
            "http://[::]:11434",
        ] {
            assert!(host_is_this_mac(local), "{local} is this Mac");
        }
        for remote in [
            "http://10.0.0.5:11434",
            "http://gpu-box.local:11434",
            "https://ollama.example.com",
            "not a url",
        ] {
            assert!(
                !host_is_this_mac(remote),
                "{remote} is not known to be this Mac"
            );
        }
    }

    #[test]
    fn test_ollama_app_in_finds_the_bundle_in_either_root_and_nothing_otherwise() {
        let base = isolated_path_dir("ollama-app-roots");
        let system = base.join("Applications");
        let user = base.join("home/Applications");
        std::fs::create_dir_all(&system).expect("create system root");
        std::fs::create_dir_all(&user).expect("create user root");
        let roots = vec![system.clone(), user.clone()];

        assert_eq!(
            ollama_app_in(&roots, &Protected::new(&base.join("home"))),
            None,
            "nothing installed yet"
        );

        std::fs::create_dir_all(user.join("Ollama.app")).expect("create user bundle");
        assert_eq!(
            ollama_app_in(&roots, &Protected::new(&base.join("home"))),
            Some(user.join("Ollama.app")),
            "a drag-install into ~/Applications counts"
        );

        std::fs::create_dir_all(system.join("Ollama.app")).expect("create system bundle");
        assert_eq!(
            ollama_app_in(&roots, &Protected::new(&base.join("home"))),
            Some(system.join("Ollama.app")),
            "the first root wins when both have one"
        );

        // A plain file of the right name is not a bundle.
        let only_file = base.join("only-file");
        std::fs::create_dir_all(&only_file).expect("create third root");
        std::fs::write(only_file.join("Ollama.app"), b"not a bundle").expect("write decoy");
        assert_eq!(
            ollama_app_in(&[only_file], &Protected::new(&base.join("home"))),
            None
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_ollama_app_in_never_looks_into_a_protected_place() {
        // `~/Applications` a link into `~/Documents`: the bundle there is
        // never looked at, so it is not found, as one Banager cannot see.
        let base = std::fs::canonicalize(isolated_path_dir("ollama-app-protected")).unwrap();
        let home = base.join("home");
        std::fs::create_dir_all(home.join("Documents/Apps/Ollama.app")).unwrap();
        std::os::unix::fs::symlink(home.join("Documents/Apps"), home.join("Applications")).unwrap();
        let roots = vec![base.join("no-system-apps"), home.join("Applications")];
        assert_eq!(
            ollama_app_in(&roots, &Protected::new(&base.join("someone-else"))),
            Some(home.join("Applications/Ollama.app")),
            "found where nothing is protected"
        );
        assert_eq!(ollama_app_in(&roots, &Protected::new(&home)), None);
        // And as `detect` asks it (`ollama_app_path`), with the test's own
        // folder standing for `/Applications`, never this Mac's.
        assert_eq!(
            ollama_app_path_under(&base.join("no-system-apps"), &home),
            None
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_ollama_app_roots_are_the_two_places_macos_puts_an_app() {
        let roots = ollama_app_roots(Path::new("/Users/someone"));
        assert_eq!(
            roots,
            vec![
                PathBuf::from("/Applications"),
                PathBuf::from("/Users/someone/Applications"),
            ]
        );
    }

    #[tokio::test]
    async fn test_detect_returns_no_instance_when_ollama_is_not_on_path() {
        let tmp_dir = crate::testing::unique_temp_path("ollama-detect-absent");
        std::fs::create_dir_all(&tmp_dir).expect("create temp PATH dir");
        let env = HostEnv {
            path_dirs: vec![tmp_dir.clone()],
            home: PathBuf::from("/tmp/fake-home"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let adapter =
            OllamaAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        assert!(adapter.detect(&env).await.is_empty());

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    /// A scratch directory for the manifest-containment tests below, plus a
    /// decoy manifest written *outside* the manifests root. The escape tests
    /// aim a daemon-supplied model name at that decoy: if the adapter ever
    /// reads it, `compare_digests` gets a parsable manifest and goes on to
    /// query the registry, so the absence of a registry call in `calls()` is
    /// hard proof that no read outside the root happened.
    struct EscapeFixture {
        root_home: PathBuf,
        manifests_root: PathBuf,
        decoy_dir: PathBuf,
    }

    /// The manifest planted outside the manifests root. Its layer digests
    /// differ from `DECOY_REGISTRY_MANIFEST`'s, so a successful read of it
    /// could only end in a *checkable* candidate — never in "up to date"
    /// and never in a read error. That is what makes the escape visible.
    const DECOY_MANIFEST: &str = r#"{"schemaVersion":2,
        "mediaType":"application/vnd.docker.distribution.manifest.v2+json",
        "config":{"digest":"sha256:decoy0000000000000000000000000000000000000000000000000000000000","size":1},
        "layers":[{"digest":"sha256:decoy1111111111111111111111111111111111111111111111111111111111","size":2}]}"#;

    /// What the (mocked) registry answers for the hostile reference.
    const DECOY_REGISTRY_MANIFEST: &str = r#"{"schemaVersion":2,
        "mediaType":"application/vnd.docker.distribution.manifest.v2+json",
        "config":{"digest":"sha256:remote000000000000000000000000000000000000000000000000000000000","size":1},
        "layers":[{"digest":"sha256:remote111111111111111111111111111111111111111111111111111111111","size":2}]}"#;

    fn escape_fixture(label: &str) -> EscapeFixture {
        let root_home = std::env::temp_dir().join(format!(
            "banager-ollama-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let manifests_root = root_home.join("models/manifests/registry.ollama.ai");
        std::fs::create_dir_all(&manifests_root).expect("create manifests root");
        let decoy_dir = root_home.join("decoy");
        std::fs::create_dir_all(&decoy_dir).expect("create decoy dir");
        std::fs::write(decoy_dir.join("mani"), DECOY_MANIFEST).expect("write decoy manifest");
        EscapeFixture {
            root_home,
            manifests_root,
            decoy_dir,
        }
    }

    /// An `/api/tags` body naming exactly one model, so a test can put an
    /// arbitrary (hostile) reference in front of the adapter the way a
    /// compromised or buggy daemon would. Inline JSON in a unit test is not
    /// a fixture (Global Constraints).
    fn tags_body_naming(model: &str) -> String {
        format!(
            r#"{{"models":[{{"name":{},"digest":"sha256:local000000000000000000000000000000000000000000000000000000000","size":7}}]}}"#,
            serde_json::to_string(model).expect("json-encode the model name")
        )
    }

    #[tokio::test]
    async fn test_check_updates_refuses_a_model_name_whose_absolute_path_escapes_the_manifests_root(
    ) {
        // `split_model_reference("a//x/y:mani")` yields namespace `a`, name
        // `/x/y`, tag `mani` — and `Path::join` with an absolute component
        // throws the base away, so the naive join reads `/x/y/mani`. Here
        // `/x/y` is a real temp directory holding a real manifest.
        let fx = escape_fixture("escape-absolute");
        let decoy = fx.decoy_dir.to_str().expect("utf8 temp path").to_string();
        let hostile = format!("a/{decoy}:mani");

        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "http://127.0.0.1:11434/api/tags",
            HttpResponse {
                status: 200,
                body: tags_body_naming(&hostile),
            },
        );
        // Registered so that a naive implementation reaches a *checkable*
        // candidate rather than tripping over MockHttpClient's NoMock.
        http.respond(
            &format!("https://registry.ollama.ai/v2/a/{decoy}/manifests/mani"),
            HttpResponse {
                status: 200,
                body: DECOY_REGISTRY_MANIFEST.to_string(),
            },
        );
        let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http.clone());
        let inst = test_instance("http://127.0.0.1:11434", fx.root_home.clone());

        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("one hostile name must not fail the whole check")
            .candidates;

        assert_eq!(candidates.len(), 1);
        assert!(
            !candidates[0].checkable,
            "a reference that escapes the manifests root has no target to offer"
        );
        let warning = warnings_text(&candidates[0].warnings);
        assert!(
            warning.contains("outside"),
            "the warning must say the reference was refused for leaving the manifests root, got {warning:?}"
        );
        assert!(
            !warning.contains("could not read local manifest"),
            "the read must be refused before it is attempted, got {warning:?}"
        );
        assert_eq!(
            http.calls(),
            vec!["http://127.0.0.1:11434/api/tags".to_string()],
            "nothing beyond the inventory call may go out for a refused reference"
        );

        let _ = std::fs::remove_dir_all(&fx.root_home);
    }

    #[tokio::test]
    async fn test_check_updates_refuses_a_model_name_whose_dotdot_escapes_the_manifests_root() {
        // `x/../../../../decoy:mani` -> namespace `x`, name
        // `../../../../decoy`, tag `mani`, i.e.
        // `<root>/x/../../../../decoy/mani`, which climbs out of
        // `models/manifests/registry.ollama.ai` and lands on the decoy.
        let fx = escape_fixture("escape-dotdot");
        // The `..` walk is resolved by the OS, so the first segment has to
        // exist as a real directory for a naive read to succeed.
        std::fs::create_dir_all(fx.manifests_root.join("x")).expect("create traversal anchor");
        let hostile = "x/../../../../decoy:mani";

        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "http://127.0.0.1:11434/api/tags",
            HttpResponse {
                status: 200,
                body: tags_body_naming(hostile),
            },
        );
        http.respond(
            "https://registry.ollama.ai/v2/x/../../../../decoy/manifests/mani",
            HttpResponse {
                status: 200,
                body: DECOY_REGISTRY_MANIFEST.to_string(),
            },
        );
        let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http.clone());
        let inst = test_instance("http://127.0.0.1:11434", fx.root_home.clone());

        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("one hostile name must not fail the whole check")
            .candidates;

        assert_eq!(candidates.len(), 1);
        assert!(!candidates[0].checkable);
        let warning = warnings_text(&candidates[0].warnings);
        assert!(
            warning.contains("outside"),
            "expected a containment refusal, got {warning:?}"
        );
        assert!(
            !warning.contains("could not read local manifest"),
            "the read must be refused before it is attempted, got {warning:?}"
        );
        assert_eq!(
            http.calls(),
            vec!["http://127.0.0.1:11434/api/tags".to_string()],
            "nothing beyond the inventory call may go out for a refused reference"
        );

        let _ = std::fs::remove_dir_all(&fx.root_home);
    }

    #[test]
    fn test_manifest_path_accepts_ordinary_references_and_rejects_every_escape_shape() {
        let root = Path::new("/home/u/.ollama/models/manifests/registry.ollama.ai");

        // The ordinary case, and the two-segment name an `hf.co/user/repo`
        // reference produces, both stay inside the root.
        assert_eq!(
            contained_manifest_path(root, "library", "qwen3.8", "27b-mlx").expect("ordinary name"),
            root.join("library").join("qwen3.8").join("27b-mlx")
        );
        assert!(contained_manifest_path(root, "hf.co", "user/repo", "latest").is_ok());

        // Escape shapes, all reachable from a `/api/tags` body.
        assert!(contained_manifest_path(root, "a", "/etc/passwd", "latest").is_err());
        assert!(contained_manifest_path(root, "a", "../../../../etc/passwd", "latest").is_err());
        assert!(contained_manifest_path(root, "..", "etc", "passwd").is_err());
        assert!(contained_manifest_path(root, "a", "b", "../../../../etc/passwd").is_err());
        assert!(contained_manifest_path(root, "", "b", "c").is_err());
        assert!(contained_manifest_path(root, "a", "", "c").is_err());
        assert!(contained_manifest_path(root, "a", "b", "").is_err());
        assert!(contained_manifest_path(root, "a", "./b", "c").is_err());
    }

    #[tokio::test]
    async fn test_compare_digests_percent_encodes_the_reference_into_the_registry_url() {
        // `namespace`/`name`/`tag` are interpolated into the registry URL
        // straight out of a `/api/tags` body. Unencoded, a `?` in the tag
        // turns the rest of the path into a query string and a `#` truncates
        // it at a fragment, so the request no longer addresses the manifest
        // the local file was read for.
        let fx = escape_fixture("encode-url");
        let model_dir = fx.manifests_root.join("library").join("qwen3.8");
        std::fs::create_dir_all(&model_dir).expect("create manifest dir");
        std::fs::write(model_dir.join("27b-mlx?x=1#f"), DECOY_MANIFEST)
            .expect("write local manifest");

        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "https://registry.ollama.ai/v2/library/qwen3.8/manifests/27b-mlx%3Fx=1%23f",
            HttpResponse {
                status: 200,
                body: DECOY_MANIFEST.to_string(),
            },
        );
        let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http.clone());

        let outcome = adapter
            .compare_digests(
                &fx.root_home.join("models"),
                "library",
                "qwen3.8",
                "27b-mlx?x=1#f",
                "4ac6cfed28d63e05cb031035ec11ada72d31b40316bab204cb971e9495946171",
            )
            .await;

        assert!(
            outcome.is_ok(),
            "the encoded url must be the one requested, got {outcome:?}"
        );
        assert_eq!(
            http.calls(),
            vec![
                "https://registry.ollama.ai/v2/library/qwen3.8/manifests/27b-mlx%3Fx=1%23f"
                    .to_string()
            ]
        );

        let _ = std::fs::remove_dir_all(&fx.root_home);
    }

    async fn plan_for_kind(name: &str, kind: OpKind) -> Plan {
        let adapter =
            OllamaAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(
            "http://127.0.0.1:11434",
            PathBuf::from("/Users/brulek/.ollama"),
        );
        let req = OpRequest {
            kind,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Model,
            name: name.to_string(),
        };
        OllamaAdapter::plan(&adapter, &inst, &req)
            .await
            .expect("plan")
    }

    async fn plan_for(name: &str) -> Plan {
        plan_for_kind(name, OpKind::Install).await
    }

    #[tokio::test]
    async fn test_plan_warns_when_the_reference_names_a_third_party_registry() {
        // Ollama reads a host-like first segment as a registry, so this
        // pulls from evil.example.com rather than from Ollama's library.
        // The preview shows the argv, but this app's audience will not read
        // a hostname inside a model name as a warning.
        //
        // A payload-carrying variant, not a `Message`: `Message` is the
        // escape hatch for text Banager cannot know ahead of time, and
        // this sentence is entirely knowable -- only the host is not. As a
        // `Message` it was an English sentence assembled in Rust, which is
        // the exact trap spec §6 exists to close.
        let plan = plan_for("evil.example.com/ns/model:tag").await;
        assert_eq!(
            plan.warnings,
            vec![Warning::ThirdPartyRegistry {
                host: "evil.example.com".to_string()
            }],
        );
        // Named, not blocked: the operation still runs as requested.
        assert_eq!(
            command_args(&plan),
            vec!["pull", "evil.example.com/ns/model:tag"]
        );
    }

    #[tokio::test]
    async fn test_plan_does_not_warn_about_the_registry_when_removing_a_model() {
        // Where a model came from is a reason to think before *fetching*
        // it, and no reason at all to hesitate before deleting it. The
        // warning used to be computed before `match req.kind`, so it rode
        // onto the uninstall plan too -- and `UninstallDialog` renders
        // `plan.warnings` under "Before you continue:", the one
        // destructive confirmation screen in the app. All the uninstall
        // carries is its sentence about what goes and what stays, which the
        // dialog says under the model itself (`warningGroup`'s `scope`).
        let plan = plan_for_kind("modelscope.cn/Qwen/Qwen3-8B", OpKind::Uninstall).await;
        assert_eq!(
            plan.warnings,
            vec![Warning::UninstallScope {
                what: UninstallScope::Ollama
            }],
            "the uninstall confirmation must carry no registry warning"
        );
        assert_eq!(
            command_args(&plan),
            vec!["rm", "modelscope.cn/Qwen/Qwen3-8B"]
        );

        // Upgrading re-pulls, so it does warn -- and says, after the
        // registry, that it downloads what changed.
        let upgrade = plan_for_kind("modelscope.cn/Qwen/Qwen3-8B", OpKind::Upgrade).await;
        assert_eq!(
            upgrade.warnings,
            vec![
                Warning::ThirdPartyRegistry {
                    host: "modelscope.cn".to_string()
                },
                Warning::DownloadsModelChanges,
            ],
        );
    }

    #[tokio::test]
    async fn test_plan_says_an_upgrade_downloads_what_changed_and_an_install_does_not() {
        // The model's note in the update confirmation: an upgrade of a
        // model from Ollama's own library carries it alone.
        assert_eq!(
            plan_for_kind("qwen3.8:27b-mlx", OpKind::Upgrade)
                .await
                .warnings,
            vec![Warning::DownloadsModelChanges],
        );
        assert_eq!(
            plan_for_kind("hf.co/user/repo:tag", OpKind::Upgrade)
                .await
                .warnings,
            vec![Warning::DownloadsModelChanges],
        );
        // An install downloads the whole model, which is what was asked for.
        assert!(plan_for_kind("qwen3.8:27b-mlx", OpKind::Install)
            .await
            .warnings
            .is_empty());
    }

    #[tokio::test]
    async fn test_plan_does_not_warn_for_ollamas_own_library_or_the_documented_mirror() {
        // A dotted *model* name is the false positive to avoid: `qwen3.8`
        // is not a hostname, and warning about it would train the user to
        // ignore the warning that matters.
        assert!(plan_for("qwen3.8:27b-mlx").await.warnings.is_empty());
        assert!(plan_for("library/qwen3.8:27b-mlx")
            .await
            .warnings
            .is_empty());
        assert!(plan_for("hf.co/user/repo:tag").await.warnings.is_empty());
        assert!(plan_for("registry.ollama.ai/library/qwen3.8:27b-mlx")
            .await
            .warnings
            .is_empty());
    }

    #[tokio::test]
    async fn test_compare_digests_holds_to_the_shared_lookup_failure_table() {
        let local_json = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/local-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read local manifest fixture");
        let root = crate::testing::unique_temp_path("ollama-table");
        let model_dir = root.join("manifests/registry.ollama.ai/library/qwen3.8");
        std::fs::create_dir_all(&model_dir).expect("create fixture manifest dir");
        std::fs::write(model_dir.join("27b-mlx"), &local_json).expect("write local manifest");
        let at = &root;
        crate::adapters::lookup_cases::hold_to_the_table(
            "https://registry.ollama.ai/v2/library/qwen3.8/manifests/27b-mlx",
            "registry request failed",
            "registry",
            |http| async move {
                OllamaAdapter::new(Arc::new(MockRunner::new()), http)
                    .compare_digests(
                        at,
                        "library",
                        "qwen3.8",
                        "27b-mlx",
                        "5642e97495e1a088883805981563dcdc4a040c2f53388b7a41d1f24d3622cf7e",
                    )
                    .await
            },
        )
        .await;
        let _ = std::fs::remove_dir_all(&root);
    }
}
