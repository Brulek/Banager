# Banager Phase 3 Implementation Plan: The Other Sources

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Banager see everything a neglected Mac actually accumulated, not just Homebrew: global npm packages, pipx and uv tools, pip packages (read-only), cargo binaries, and Ollama models — each with the same guarantees Homebrew already has (an exact command preview before anything destructive, a live log, a working cancel, and a post-execution check that the machine really changed).

**Architecture:** Six new adapters behind the existing `Adapter` trait. Two of them need the network, so this phase adds an `HttpClient` trait with a real and a mock implementation, mirroring how `CommandRunner` already makes subprocess work testable. The trait gains a `CheckOptions` parameter so a setting can reach `check_updates` (the change `greedy_casks` needed). Everything destructive still flows through a subprocess `Plan`, including Ollama — see the ruling below.

**Tech Stack:** Rust (banager-core, tauri 2.11.x), reqwest with rustls, tokio; React 19, TanStack Query v5, Zustand, i18next, vitest.

## Ruling: Ollama writes go through the CLI, not the HTTP API

Spec §4.2 specifies `POST /api/pull` and `DELETE /api/delete` for Ollama's destructive operations. This plan uses `ollama pull {model}` and `ollama rm {model}` instead, and keeps HTTP for reads only (`GET /api/tags`, and the registry manifest comparison).

Why: every guarantee the product makes about a destructive action is built on the subprocess `Plan` — the preview shows `program` plus `args`, cancellation kills the process group, the log drawer streams the child's output, and `reconcile` re-checks afterwards. An HTTP write would need a second, parallel version of all four, and the preview would have to show something that is not a command while spec §6 promises "the exact command that will run". The CLI is present whenever the daemon is (they ship together), `ollama pull` streams progress on stdout, and `ollama rm` is instant. Reads stay on HTTP because `ollama list` shells out to the same daemon anyway and, on macOS, launches Ollama.app as a side effect — which a background refresh must never do.

Recorded as a deliberate deviation. If a later phase needs byte-level progress bars, revisit it then; spec §4.2's own fallback ("last pulled + pull again") already tolerates less precision.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-09-17-banager-design.md`. §3 (data flow), §4.1–4.2 (adapters, per-source commands), §5 (data model), §6 (execution and safety), §7 (UI), §11 (testing) bind this phase.
- Backlog: `docs/superpowers/backlog.md`. Everything under "阶段 3（其余来源）/ 存储与刷新层" is implemented here and must not be deferred again. The phase-2 deferrals named in Task 13 are also in scope.
- macOS only; minimum macOS 13.3; Tauri ≥ 2.11.1. `banager-core` never depends on `tauri` and never creates a tokio runtime.
- Commands are argv arrays with an absolute program path, never a shell string. Package names pass `validate_package_name` before they reach any argv.
- Every destructive action previews its exact command before running. Operations that need a password say so first. The front end never builds an argv, never decides what is safe to remove, and never guesses an outcome.
- Network access is confined to the `HttpClient` trait. No adapter calls reqwest directly, so every network path has a mock in tests. Requests carry a `banager/{version}` User-Agent and a 30-second timeout.
- A background refresh never launches an application, never prompts for a password, and never writes to the machine.
- No business logic in TypeScript. Components never call `invoke`; only `src/lib/api.ts` does. No hard-coded user-visible strings — every one goes through `t()`, with matching keys in `en.json` and `zh-CN.json` (the parity test enforces this).
- Colours use `bg-[var(--color-x)]` arbitrary values only; never bare semantic classes, never hex.
- Interactions that assert on DOM changes use `fireEvent` plus `findBy*`/`waitFor`, never a naked `.click()` followed by a synchronous assertion.
- Fixtures come from real machines only — record them, never hand-write them. Inline JSON in a unit test is fine and is not a fixture.
- Definition of done for every task: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm test`, `pnpm exec tsc -p tsconfig.json` — all clean, no new warnings.
- Commit messages end with a blank line then `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>`.

## What already exists (do not rebuild)

`banager-core` has: `model.rs` (ManagerInstance, InstalledArtifact, UpdateCandidate, ArtifactKey, Plan, OpRequest, Outcome, …), `events.rs` (EventSink, OperationEvent), `runner/` (CommandRunner with RealRunner and MockRunner, HostEnv PATH hydration, process-group kill, timeouts), `adapters/` (the Adapter trait, AdapterMeta from TOML, validate_package_name, and the Homebrew adapter), `ops/` (OperationManager with resource locks, a 3-permit semaphore, cancel semantics and post-execution reconcile), `session/` (Session facade, generation-numbered Snapshot, refresh coalescing, server-issued single-use IssuedPlan), `settings.rs`.

`src-tauri` has nine IPC commands and a Channel event bridge. The React app has the installed, updates and settings pages, an operation bar, a log drawer, an uninstall dialog, empty and error states, and en + zh-CN.

125 Rust tests and 124 front-end tests pass on main; CI is green including a live Homebrew install/uninstall smoke test.

---

## File Structure

```
crates/banager-core/src/
├── http/mod.rs             NEW  HttpClient trait, HttpRequest/HttpResponse, HttpError
├── http/real.rs            NEW  RealHttpClient (reqwest, rustls, 30s timeout, banager UA)
├── http/mock.rs            NEW  MockHttpClient (url -> canned response, records calls)
├── adapters/mod.rs         MOD  CheckOptions; Adapter::check_updates gains it; AdapterMeta::unverified_version; shared run_plan + second_token helpers
├── adapters/brew/mod.rs    MOD  honour CheckOptions.include_self_updating (--greedy); degrade a failed `brew update` to a warning instead of failing the whole check; serialise maybe_update per instance
├── adapters/npm.rs         NEW
├── adapters/pipx.rs        NEW
├── adapters/uv.rs          NEW
├── adapters/pip.rs         NEW  read-only
├── adapters/cargo.rs       NEW  ~/.cargo/.crates2.json + crates.io
├── adapters/ollama/mod.rs  NEW  HTTP reads, CLI writes
├── adapters/ollama/parse.rs NEW /api/tags and registry manifest shapes
├── session/mod.rs          SPLIT into session/mod.rs (facade) + session/refresh.rs + session/plans.rs
├── runner/path_env.rs      MOD  HostEnv gains cargo_home and ollama_host (no adapter reads std::env)
├── settings.rs             MOD  include_self_updating
└── lib.rs                  MOD  pub mod http;

adapters/meta/{npm,pipx,uv,pip,cargo,ollama}.toml   NEW
adapters/fixtures/{npm,pipx,uv,pip,cargo,ollama}/   NEW  recorded on this Mac

src-tauri/src/state.rs      MOD  pass CheckOptions from Settings into Session
src/
├── lib/types.ts            MOD  CheckOptions-derived Settings field; unverified-version flag
├── pages/InstalledPage.tsx MOD  per-source affordances (pip read-only note, cargo compile warning)
├── pages/SettingsPage.tsx  MOD  the include-self-updating toggle
├── components/SourceNotice.tsx NEW  per-source banner (Ollama not running, pip read-only)
└── i18n/{en,zh-CN}.json    MOD
```

## Core Interfaces (authoritative — every task uses these names verbatim)

```rust
// crates/banager-core/src/http/mod.rs
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: &'static str,        // "GET" only in this phase
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub timeout: std::time::Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse { pub status: u16, pub body: String }

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("network error: {0}")] Network(String),
    #[error("timed out after {0:?}")] Timeout(std::time::Duration),
    #[error("no canned response for {0}")] NoMock(String),
}

#[async_trait::async_trait]
pub trait HttpClient: Send + Sync {
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse, HttpError>;
}

pub struct RealHttpClient { /* private */ }
impl RealHttpClient { pub fn new() -> RealHttpClient; }   // rustls, 30s default, UA "banager/{CARGO_PKG_VERSION}"

pub struct MockHttpClient { /* private */ }
impl MockHttpClient {
    pub fn new() -> MockHttpClient;
    pub fn respond(&self, url: &str, response: HttpResponse);
    pub fn fail(&self, url: &str, error: &str);
    pub fn calls(&self) -> Vec<String>;                    // urls, in order
}
```

```rust
// crates/banager-core/src/adapters/mod.rs  (changed)
/// Options a caller passes down to `check_updates`. Adapters ignore fields
/// that do not apply to them; a new field must never change behaviour for an
/// adapter that does not read it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckOptions {
    /// Homebrew only: include casks that update themselves (`brew outdated --greedy`).
    pub include_self_updating: bool,
}

// `Capabilities` is unchanged by this phase and gains no field. An earlier
// draft added a network-dependency flag here; it was cut because
// `Adapter::capabilities()` has zero call sites in the whole workspace —
// not `session/`, not `ops/`, not `src-tauri/src/ipc.rs` — and
// `Capabilities` is absent from `src/lib/types.ts`, so it never crosses
// IPC. Anything added to it would ship unread. That pre-existing
// dead-method problem is recorded in `docs/superpowers/backlog.md` under
// "阶段 4 之前"; until it has a consumer, this struct does not grow.
pub struct Capabilities {
    pub search: bool,
    pub per_item_upgrade: bool,
    pub upgrade_all: bool,
    pub uninstall: bool,
    pub background_check: bool,
    pub cancel_safe: bool,
}

#[async_trait]
pub trait Adapter: Send + Sync {
    // unchanged: meta, capabilities, detect, inventory, search, plan, execute, reconcile
    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError>;
}
```

```rust
// crates/banager-core/src/model.rs  (added field, additive only)
pub struct ManagerInstance {
    // … existing fields …
    /// None when the adapter's metadata lists no verified versions, or when
    /// the detected version is among them. Some(detected) when it is not, so
    /// the UI can mark the source as running an unverified version (spec §4.1).
    pub unverified_version: Option<String>,
}
```

```rust
// crates/banager-core/src/settings.rs  (added field)
pub struct Settings {
    pub language: Language,
    pub show_technical_details: bool,
    pub ignored_updates: Vec<ArtifactKey>,
    /// Feeds CheckOptions.include_self_updating. Default false: most people
    /// do not want Chrome and Docker listed as updatable when those apps
    /// update themselves.
    pub include_self_updating: bool,
}
```

### Per-adapter contracts

Every adapter implements the same trait; these are the details each one must get right. Commands are given exactly as they must appear in argv.

| Adapter | Instance key | Inventory | Check updates | Write ops | Search | Notes |
|---|---|---|---|---|---|---|
| `npm` | one per `npm` executable, keyed by `npm prefix -g` | `npm ls -g --depth=0 --json`, accept exit 0 or 1, parse stdout only; top level is a `dependencies` **object** | `npm outdated -g --json`, exit 1 means "has updates", object keyed by name with current/wanted/latest | `npm install -g {id}` / `npm uninstall -g {id}` / `npm install -g {id}@latest` | `npm search --json --searchlimit 20 {query}` | env `NO_COLOR=1 npm_config_update_notifier=false npm_config_fund=false`; read-only when the prefix is not writable |
| `pipx` | the `pipx` executable | `pipx list --json`, version at `venvs.<name>.metadata.main_package.package_version` | `pipx list --outdated` when `pipx --version` ≥ 1.16, else compare PyPI JSON per package through `HttpClient` | `pipx install/uninstall/upgrade {id}` (upgrade-all deferred: `OpKind` has no variant for it, so `plan()` can never receive such a request — see Task 6) | none | |
| `uv` | the `uv` executable | `uv tool list --show-paths` (text: `name vX.Y.Z (path)` then `- binary` lines) | `uv tool list --outdated` | `uv tool install/uninstall/upgrade {id}` | none | |
| `pip` | one per interpreter, invoked as `{python} -m pip` | `-m pip list --format=json`; `--not-required` marks "nothing else depends on this", which is **not** the same as "the user asked for it" — reason is `Unknown` | `-m pip list --outdated --format=json` | **none** — read-only. `capabilities()` returns false for per_item_upgrade, upgrade_all and uninstall | none | the UI shows a note pointing at pipx and uv |
| `cargo` | the cargo home (`CARGO_HOME` or `~/.cargo`) | read `~/.cargo/.crates2.json` | crates.io `GET /api/v1/crates/{name}` per package, `max_stable_version`; git and path sources are `checkable: false` with a reason | `cargo install {id}` (or `cargo binstall -y {id}` when cargo-binstall is present); `cargo uninstall {id}` | none | the upgrade plan carries a warning that it compiles locally and can take minutes |
| `ollama` | `OLLAMA_HOST`, default `http://127.0.0.1:11434` | `GET {host}/api/tags` | local manifest under `~/.ollama/models/manifests/…` versus `GET https://registry.ollama.ai/v2/{ns}/{name}/manifests/{tag}`, comparing the **set of layer digests**; on any failure, `checkable: false` | `ollama pull {model}` / `ollama rm {model}` (see the ruling above) | none | when the daemon does not answer, `detect` returns an instance with `healthy: false` and the UI offers to start it; never shell out to `ollama list` |

## Recorded formats (fixtures are already in the repo — read them before writing a parser)

All six sources were probed on this Mac on 2026-09-20 and their real output is committed under `adapters/fixtures/<source>/<version>/`, each with a README naming the commands and the traps. Four of those traps would each have produced a broken parser if the implementer had worked from the spec's prose alone:

- **`pipx list --outdated` is prose, not data.** One `name: old -> new` line per outdated tool, and the literal sentence `pipx found no available upgrades.` when there are none. An unmatched line means "no updates", never an error.
- **`uv tool list --outdated` prints nothing at all** when nothing is outdated — no message, no newline. With an update it prints `name vOLD [latest: NEW]` followed by its `- binary` lines. With no tools installed at all it prints `No tools installed`.
- **`.crates2.json` keeps the package name, version and source in the JSON *key*** (`"hexyl 0.17.0 (registry+https://github.com/rust-lang/crates.io-index)"`). The value has only `bins`, `features`, `profile`, `rustc`, `target`, `version_req`. A parser looking for a `name` field in the value finds nothing.
- **`npm ls -g --depth=0 --json` returns a `dependencies` object, not an array,** and `npm outdated -g --json` exits 1 when anything is outdated. Exit 1 here is a result, not a failure.

The Ollama update check was also verified end to end against real data before being planned: the local manifest and the registry manifest for the installed model each carry 1209 layers with identical digest sets and the same config digest, and the registry answers anonymously with no token exchange. Comparing the **set of layer digests** rather than the serialized file is what makes this work — Ollama rewrites the local manifest on disk, so a byte comparison reports false "outdated".

## Task List

| # | Task | Deliverable |
|---|---|---|
| 1 | `HttpClient` trait, real and mock | network is testable before any adapter needs it |
| 2 | `CheckOptions` + trait change + brew honours `--greedy` + the setting | the backlog's `greedy_casks` item, done end to end |
| 3 | brew `maybe_update` degrades and serialises | a failed `brew update` no longer fails the whole check |
| 4 | `unverified_version` on ManagerInstance + the UI badge | spec §4.1's unverified-version signal |
| 5 | npm adapter | inventory, updates, install/uninstall/upgrade, search |
| 6 | pipx adapter | including the pre-1.16 PyPI fallback |
| 7 | uv adapter | text parsing with fixtures |
| 8 | pip adapter, read-only | plus the guidance note in the UI |
| 9 | cargo adapter | `.crates2.json` + crates.io, with the compile warning |
| 10 | ollama adapter | HTTP reads, CLI writes, daemon-not-running state |
| 11 | Session registers all seven adapters | grouped, per-instance refresh, no cross-source interference |
| 12 | Per-source UI affordances | pip read-only note, cargo compile warning, Ollama not running |
| 13 | Phase-2 deferrals | `issued_plans` sweep, `records` cap, `useRefresh` through the coalescer, no double `SnapshotChanged` |
| 14 | Split `session/mod.rs`, record fixtures, CI | the file is 1181 lines; fixtures for six new sources |

---

### Task 1: `HttpClient` trait, real and mock

**Files:**
- Create: `crates/banager-core/src/http/mod.rs`
- Create: `crates/banager-core/src/http/mock.rs`
- Create: `crates/banager-core/src/http/real.rs`
- Modify: `crates/banager-core/src/lib.rs`
- Modify: `crates/banager-core/Cargo.toml`
- Test: inline `#[cfg(test)]` modules in `crates/banager-core/src/http/mod.rs`, `mock.rs`, `real.rs`

**Interfaces:**
- Consumes: nothing from earlier tasks (this is the first task).
- Produces (authoritative, from the phase skeleton, verbatim):
  ```rust
  pub struct HttpRequest { pub method: &'static str, pub url: String, pub headers: Vec<(String, String)>, pub timeout: std::time::Duration }
  pub struct HttpResponse { pub status: u16, pub body: String }
  pub enum HttpError { Network(String), Timeout(std::time::Duration), NoMock(String) }
  pub trait HttpClient: Send + Sync { async fn send(&self, req: HttpRequest) -> Result<HttpResponse, HttpError>; }
  pub struct RealHttpClient; impl RealHttpClient { pub fn new() -> RealHttpClient; }
  pub struct MockHttpClient;
  impl MockHttpClient {
      pub fn new() -> MockHttpClient;
      pub fn respond(&self, url: &str, response: HttpResponse);
      pub fn fail(&self, url: &str, error: &str);
      pub fn calls(&self) -> Vec<String>;
      pub fn requests(&self) -> Vec<HttpRequest>;
  }
  ```
  Later tasks import these as `crate::http::{HttpClient, HttpRequest, HttpResponse, HttpError, MockHttpClient, RealHttpClient}`.

- [ ] **Step 1: Write the HttpClient contract types and a Display-format test**

Create `crates/banager-core/src/http/mod.rs`:

```rust
//! `HttpClient`: the network seam every adapter that needs the internet
//! goes through, mirroring how `crate::runner::CommandRunner` makes
//! subprocess work testable (see `crate::runner`). No adapter is allowed to
//! call `reqwest` directly — Global Constraints — so every network path in
//! this crate has a `MockHttpClient` double in tests.

use async_trait::async_trait;

/// `method` is always `"GET"` in this phase — no adapter added in this plan
/// ever writes over HTTP (Ollama's writes go through its CLI; see the
/// ruling in the phase 3 plan).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub timeout: std::time::Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("network error: {0}")]
    Network(String),
    #[error("timed out after {0:?}")]
    Timeout(std::time::Duration),
    #[error("no canned response for {0}")]
    NoMock(String),
}

#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse, HttpError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_error_display_messages_match_the_documented_wording() {
        assert_eq!(
            HttpError::Network("dns lookup failed".to_string()).to_string(),
            "network error: dns lookup failed"
        );
        assert_eq!(
            HttpError::Timeout(std::time::Duration::from_secs(30)).to_string(),
            "timed out after 30s"
        );
        assert_eq!(
            HttpError::NoMock("https://pypi.org/pypi/jq/json".to_string()).to_string(),
            "no canned response for https://pypi.org/pypi/jq/json"
        );
    }
}
```

Modify `crates/banager-core/src/lib.rs` — insert `pub mod http;` alphabetically between the existing `pub mod events;` and `pub mod model;` lines, so the module list reads:

```rust
pub mod adapters;
pub mod events;
pub mod http;
pub mod model;
pub mod ops;
pub mod runner;
pub mod session;
pub mod settings;
```

- [ ] **Step 2: Run to verify it passes**

Run: `cargo test -p banager-core --lib http::`
Expected: PASS — `test http::tests::test_http_error_display_messages_match_the_documented_wording ... ok` (1 test; `HttpRequest`/`HttpResponse`/`HttpClient` have no behaviour of their own yet, so this is the only test at this point).

- [ ] **Step 3: Write the failing test for `MockHttpClient`**

Modify `crates/banager-core/src/http/mod.rs` — add `pub mod mock;` right after the `use async_trait::async_trait;` line (do not add a `pub use` yet — `MockHttpClient` does not exist until Step 5, and re-exporting a name that does not exist yet would just trade one compile error for another, less informative one).

Create `crates/banager-core/src/http/mock.rs`:

```rust
use super::{HttpClient, HttpError, HttpRequest, HttpResponse};

#[cfg(test)]
mod tests {
    use super::*;

    /// Inside `mod tests` deliberately: it is used only by these tests, and
    /// a non-test-gated helper here would be `dead_code` under the
    /// `-D warnings` gate this task's Step 11 runs.
    fn get_request(url: &str) -> HttpRequest {
        HttpRequest {
            method: "GET",
            url: url.to_string(),
            headers: vec![],
            timeout: std::time::Duration::from_secs(30),
        }
    }

    #[tokio::test]
    async fn test_mock_http_client_returns_canned_response_and_records_the_url() {
        let client = MockHttpClient::new();
        client.respond(
            "https://pypi.org/pypi/jq/json",
            HttpResponse {
                status: 200,
                body: "{}".to_string(),
            },
        );
        let response = client
            .send(get_request("https://pypi.org/pypi/jq/json"))
            .await
            .expect("mocked call");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "{}");
        assert_eq!(
            client.calls(),
            vec!["https://pypi.org/pypi/jq/json".to_string()]
        );
    }

    #[tokio::test]
    async fn test_mock_http_client_fail_returns_a_network_error() {
        let client = MockHttpClient::new();
        client.fail("https://pypi.org/pypi/missing/json", "connection refused");
        let err = client
            .send(get_request("https://pypi.org/pypi/missing/json"))
            .await
            .expect_err("expected a failure");
        match err {
            HttpError::Network(msg) => assert_eq!(msg, "connection refused"),
            other => panic!("expected Network, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_mock_http_client_errors_on_an_unconfigured_url() {
        let client = MockHttpClient::new();
        let result = client.send(get_request("https://example.invalid/unset")).await;
        assert!(matches!(result, Err(HttpError::NoMock(_))));
    }
}
```

- [ ] **Step 4: Run to verify it fails**

Run: `cargo test -p banager-core --lib http::mock`
Expected: FAIL to compile — `error[E0433]`/`error[E0412]`: cannot find type/function `MockHttpClient` in this scope (referenced three times in the test module; nothing in `mock.rs` defines it yet).

- [ ] **Step 5: Implement `MockHttpClient`**

Rewrite `crates/banager-core/src/http/mock.rs` (keeping the `tests` module from Step 3 unchanged, appended below):

```rust
use super::{HttpClient, HttpError, HttpRequest, HttpResponse};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

/// Mirrors `crate::runner::MockRunner`'s ergonomics: canned responses and
/// failures keyed by exact URL, plus a call log in request order.
pub struct MockHttpClient {
    responses: Mutex<HashMap<String, HttpResponse>>,
    failures: Mutex<HashMap<String, String>>,
    calls: Mutex<Vec<String>>,
    requests: Mutex<Vec<HttpRequest>>,
}

impl MockHttpClient {
    pub fn new() -> MockHttpClient {
        MockHttpClient {
            responses: Mutex::new(HashMap::new()),
            failures: Mutex::new(HashMap::new()),
            calls: Mutex::new(Vec::new()),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub fn respond(&self, url: &str, response: HttpResponse) {
        self.responses
            .lock()
            .unwrap()
            .insert(url.to_string(), response);
    }

    pub fn fail(&self, url: &str, error: &str) {
        self.failures
            .lock()
            .unwrap()
            .insert(url.to_string(), error.to_string());
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    /// Every request this client has been sent, in order, with its method,
    /// headers and timeout intact. `calls()` keeps only the urls, which is
    /// enough for most assertions; a test that has to prove a *header* went
    /// out needs this one — Task 10's registry `Accept:` header is the only
    /// reason the anonymous Ollama registry returns a v2 manifest at all,
    /// and nothing else in this crate can observe it.
    pub fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Default for MockHttpClient {
    fn default() -> Self {
        MockHttpClient::new()
    }
}

#[async_trait]
impl HttpClient for MockHttpClient {
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.calls.lock().unwrap().push(req.url.clone());
        self.requests.lock().unwrap().push(req.clone());
        if let Some(error) = self.failures.lock().unwrap().get(&req.url) {
            return Err(HttpError::Network(error.clone()));
        }
        self.responses
            .lock()
            .unwrap()
            .get(&req.url)
            .cloned()
            .ok_or_else(|| HttpError::NoMock(req.url.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Inside `mod tests` deliberately: see Step 3.
    fn get_request(url: &str) -> HttpRequest {
        HttpRequest {
            method: "GET",
            url: url.to_string(),
            headers: vec![],
            timeout: std::time::Duration::from_secs(30),
        }
    }

    #[tokio::test]
    async fn test_mock_http_client_returns_canned_response_and_records_the_url() {
        let client = MockHttpClient::new();
        client.respond(
            "https://pypi.org/pypi/jq/json",
            HttpResponse {
                status: 200,
                body: "{}".to_string(),
            },
        );
        let response = client
            .send(get_request("https://pypi.org/pypi/jq/json"))
            .await
            .expect("mocked call");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "{}");
        assert_eq!(
            client.calls(),
            vec!["https://pypi.org/pypi/jq/json".to_string()]
        );
    }

    #[tokio::test]
    async fn test_mock_http_client_fail_returns_a_network_error() {
        let client = MockHttpClient::new();
        client.fail("https://pypi.org/pypi/missing/json", "connection refused");
        let err = client
            .send(get_request("https://pypi.org/pypi/missing/json"))
            .await
            .expect_err("expected a failure");
        match err {
            HttpError::Network(msg) => assert_eq!(msg, "connection refused"),
            other => panic!("expected Network, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_mock_http_client_errors_on_an_unconfigured_url() {
        let client = MockHttpClient::new();
        let result = client.send(get_request("https://example.invalid/unset")).await;
        assert!(matches!(result, Err(HttpError::NoMock(_))));
    }
}
```

Modify `crates/banager-core/src/http/mod.rs` — add `pub use mock::MockHttpClient;` right after `pub mod mock;`.

- [ ] **Step 6: Run to verify it passes**

Run: `cargo test -p banager-core --lib http::`
Expected: PASS — 4 tests ok (the `HttpError` Display test plus the 3 new `MockHttpClient` tests).

- [ ] **Step 7: Commit**

```bash
git add crates/banager-core/src/http/mod.rs crates/banager-core/src/http/mock.rs crates/banager-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(core): add the HttpClient contract and MockHttpClient

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 8: Write the failing test for `RealHttpClient`**

Modify `crates/banager-core/Cargo.toml` — add the `net` feature to the existing `tokio` line (needed by the loopback test fixture below), so it reads:

```toml
tokio = { version = "1", features = ["rt-multi-thread", "macros", "process", "io-util", "time", "sync", "net"] }
```

Modify `crates/banager-core/src/http/mod.rs` — add `pub mod real;` right after `pub use mock::MockHttpClient;` (no re-export yet, same reasoning as Step 3).

Create `crates/banager-core/src/http/real.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{HttpClient, HttpError, HttpRequest};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Binds an ephemeral localhost port, accepts exactly one connection,
    /// reads until the blank line ending the request headers, writes back
    /// `response_bytes` verbatim, then closes the socket. Runs entirely
    /// offline (loopback only), so this is deterministic in CI without
    /// depending on real internet access — the same reasoning
    /// `crate::runner::real`'s tests spawn a real `/bin/sh` rather than
    /// mocking the OS.
    async fn serve_one_response(response_bytes: &'static [u8]) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = socket.read(&mut chunk).await.expect("read request");
                if n == 0 {
                    return;
                }
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            socket
                .write_all(response_bytes)
                .await
                .expect("write response");
            let _ = socket.shutdown().await;
        });
        addr
    }

    #[tokio::test]
    async fn test_real_http_client_fetches_status_and_body_from_a_real_socket() {
        let addr = serve_one_response(
            b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\nConnection: close\r\n\r\nhello, world!",
        )
        .await;
        let client = RealHttpClient::new();
        let req = HttpRequest {
            method: "GET",
            url: format!("http://{addr}/"),
            headers: vec![],
            timeout: std::time::Duration::from_secs(5),
        };
        let response = client.send(req).await.expect("real request over loopback");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "hello, world!");
    }

    #[tokio::test]
    async fn test_real_http_client_sends_the_banager_user_agent() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local_addr");
        let captured = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = socket.read(&mut chunk).await.expect("read request");
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .expect("write response");
            let _ = socket.shutdown().await;
            String::from_utf8_lossy(&buf).into_owned()
        });

        let client = RealHttpClient::new();
        let req = HttpRequest {
            method: "GET",
            url: format!("http://{addr}/"),
            headers: vec![],
            timeout: std::time::Duration::from_secs(5),
        };
        client.send(req).await.expect("real request over loopback");

        let request_text = captured.await.expect("server task panicked");
        assert!(
            request_text.contains(&format!("banager/{}", env!("CARGO_PKG_VERSION"))),
            "expected the banager User-Agent in the request, got: {request_text}"
        );
    }

    #[tokio::test]
    async fn test_real_http_client_times_out_when_the_server_never_responds() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            let (_socket, _) = listener.accept().await.expect("accept");
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let client = RealHttpClient::new();
        let req = HttpRequest {
            method: "GET",
            url: format!("http://{addr}/"),
            headers: vec![],
            timeout: std::time::Duration::from_millis(200),
        };
        let result = client.send(req).await;
        match result {
            Err(HttpError::Timeout(d)) => assert_eq!(d, std::time::Duration::from_millis(200)),
            other => panic!("expected Timeout, got {other:?}"),
        }
    }
}
```

- [ ] **Step 9: Run to verify it fails**

Run: `cargo test -p banager-core --lib http::real`
Expected: FAIL to compile — `error[E0433]`/`error[E0412]`: cannot find struct `RealHttpClient` in this scope (`real.rs` currently has only a test module; nothing defines it).

- [ ] **Step 10: Implement `RealHttpClient`**

Modify `crates/banager-core/Cargo.toml` — add the `reqwest` dependency right after the `tokio-util` line. Note: `reqwest` 0.13.5 is already present in `Cargo.lock` (pulled in transitively by `tauri-plugin-updater`) resolved against rustls, not native-tls or OpenSSL (its lock entry lists `rustls`, `hyper-rustls`, `tokio-rustls`, `rustls-platform-verifier` — no `native-tls`/`openssl`); pinning `default-features = false, features = ["rustls"]` here keeps this crate's own dependency on the same TLS backend and never opts into `native-tls`:

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
async-trait = "0.1"
thiserror = "2"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "process", "io-util", "time", "sync", "net"] }
tokio-util = { version = "0.7", features = ["rt"] }
reqwest = { version = "0.13", default-features = false, features = ["rustls"] }
libc = "0.2"
toml = "1"
```

Rewrite `crates/banager-core/src/http/real.rs` (keeping the `tests` module from Step 8 unchanged, appended below):

```rust
//! `RealHttpClient` wraps a `reqwest::Client` pinned to the rustls TLS
//! backend (never native-tls/openssl — Global Constraints). Every request
//! carries the `banager/{version}` User-Agent and a 30-second client-wide
//! default timeout; `HttpRequest::timeout` overrides that default on a
//! per-request basis.

use super::{HttpClient, HttpError, HttpRequest, HttpResponse};
use async_trait::async_trait;

pub struct RealHttpClient {
    client: reqwest::Client,
}

impl RealHttpClient {
    pub fn new() -> RealHttpClient {
        let user_agent = format!("banager/{}", env!("CARGO_PKG_VERSION"));
        let client = reqwest::Client::builder()
            .tls_backend_rustls()
            .user_agent(user_agent)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client with the rustls TLS backend must build");
        RealHttpClient { client }
    }
}

impl Default for RealHttpClient {
    fn default() -> Self {
        RealHttpClient::new()
    }
}

#[async_trait]
impl HttpClient for RealHttpClient {
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        let method = reqwest::Method::from_bytes(req.method.as_bytes())
            .map_err(|e| HttpError::Network(format!("invalid method {:?}: {e}", req.method)))?;
        let mut builder = self
            .client
            .request(method, req.url.as_str())
            .timeout(req.timeout);
        for (name, value) in &req.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let response = builder.send().await.map_err(|e| {
            if e.is_timeout() {
                HttpError::Timeout(req.timeout)
            } else {
                HttpError::Network(e.to_string())
            }
        })?;
        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|e| HttpError::Network(e.to_string()))?;
        Ok(HttpResponse { status, body })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{HttpClient, HttpError, HttpRequest};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn serve_one_response(response_bytes: &'static [u8]) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = socket.read(&mut chunk).await.expect("read request");
                if n == 0 {
                    return;
                }
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            socket
                .write_all(response_bytes)
                .await
                .expect("write response");
            let _ = socket.shutdown().await;
        });
        addr
    }

    #[tokio::test]
    async fn test_real_http_client_fetches_status_and_body_from_a_real_socket() {
        let addr = serve_one_response(
            b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\nConnection: close\r\n\r\nhello, world!",
        )
        .await;
        let client = RealHttpClient::new();
        let req = HttpRequest {
            method: "GET",
            url: format!("http://{addr}/"),
            headers: vec![],
            timeout: std::time::Duration::from_secs(5),
        };
        let response = client.send(req).await.expect("real request over loopback");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "hello, world!");
    }

    #[tokio::test]
    async fn test_real_http_client_sends_the_banager_user_agent() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local_addr");
        let captured = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = socket.read(&mut chunk).await.expect("read request");
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .expect("write response");
            let _ = socket.shutdown().await;
            String::from_utf8_lossy(&buf).into_owned()
        });

        let client = RealHttpClient::new();
        let req = HttpRequest {
            method: "GET",
            url: format!("http://{addr}/"),
            headers: vec![],
            timeout: std::time::Duration::from_secs(5),
        };
        client.send(req).await.expect("real request over loopback");

        let request_text = captured.await.expect("server task panicked");
        assert!(
            request_text.contains(&format!("banager/{}", env!("CARGO_PKG_VERSION"))),
            "expected the banager User-Agent in the request, got: {request_text}"
        );
    }

    #[tokio::test]
    async fn test_real_http_client_times_out_when_the_server_never_responds() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            let (_socket, _) = listener.accept().await.expect("accept");
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let client = RealHttpClient::new();
        let req = HttpRequest {
            method: "GET",
            url: format!("http://{addr}/"),
            headers: vec![],
            timeout: std::time::Duration::from_millis(200),
        };
        let result = client.send(req).await;
        match result {
            Err(HttpError::Timeout(d)) => assert_eq!(d, std::time::Duration::from_millis(200)),
            other => panic!("expected Timeout, got {other:?}"),
        }
    }
}
```

Modify `crates/banager-core/src/http/mod.rs` — add `pub use real::RealHttpClient;` right after `pub mod real;`.

- [ ] **Step 11: Run to verify it passes**

Run: `cargo test -p banager-core --lib http::`
Expected: PASS — 7 tests ok (1 `HttpError` test + 3 `MockHttpClient` tests + 3 `RealHttpClient` tests). Also run `cargo clippy -p banager-core --all-targets -- -D warnings` — Expected: clean (no warnings) to confirm the new `reqwest`/`tokio::net` code introduces none.

- [ ] **Step 12: Commit**

```bash
git add Cargo.lock crates/banager-core/Cargo.toml crates/banager-core/src/http/mod.rs crates/banager-core/src/http/real.rs
git commit -m "$(cat <<'EOF'
feat(core): add RealHttpClient backed by reqwest with the rustls TLS backend

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: `CheckOptions` + trait change + brew honours `--greedy` + the setting

**Files:**
- Modify: `crates/banager-core/src/adapters/mod.rs`
- Modify: `crates/banager-core/src/adapters/brew/mod.rs`
- Modify: `crates/banager-core/src/session/mod.rs`
- Modify: `crates/banager-core/src/settings.rs`
- Modify: `crates/banager-core/examples/brew_smoke.rs`
- Modify: `crates/banager-core/tests/ops_semaphore_test.rs`
- Modify: `crates/banager-core/tests/ops_cancel_test.rs`
- Modify: `crates/banager-core/tests/ops_lock_test.rs`
- Modify: `crates/banager-core/tests/ops_panic_test.rs`
- Modify: `crates/banager-core/tests/ops_outcome_test.rs`
- Modify: `crates/banager-core/tests/ops_summaries_test.rs`
- Modify: `src-tauri/src/ipc.rs`

**Interfaces:**
- Consumes: nothing from Task 1 (this task does not touch HTTP).
- Produces (authoritative, from the phase skeleton, verbatim):
  ```rust
  #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
  pub struct CheckOptions { pub include_self_updating: bool }

  #[async_trait]
  pub trait Adapter: Send + Sync {
      async fn check_updates(&self, inst: &ManagerInstance, opts: &CheckOptions) -> Result<Vec<UpdateCandidate>, AdapterError>;
      // meta, capabilities, detect, inventory, search, plan, execute, reconcile unchanged
  }
  ```
  `Capabilities` is **not** changed by this task, and gains no new field anywhere in this phase. `Adapter::capabilities()` has zero call sites in the whole workspace today — not `session/`, not `ops/`, not `src-tauri/src/ipc.rs` — and `Capabilities` is absent from `src/lib/types.ts`, so it never crosses IPC: anything added to it would ship unread. Wiring it up means a `ManagerInstance` wire-format change, a TypeScript mirror, UI state and tests, and this phase already has fourteen tasks. That pre-existing dead-method problem is recorded in `docs/superpowers/backlog.md` under "阶段 4 之前"; until it has a consumer, `Capabilities` does not grow.

  `Settings` gains `pub include_self_updating: bool` (Task 2 also wires this; see Step 9). Task 3 builds directly on `BrewAdapter::check_updates`'s new `(inst, opts)` signature. Task 5's `NpmAdapter::check_updates` uses the same signature.

- [ ] **Step 1: Add `CheckOptions` and change the trait signature**

Modify `crates/banager-core/src/adapters/mod.rs` — add `CheckOptions` right before the `Capabilities` struct and change `Adapter::check_updates`'s signature. `Capabilities` itself is left exactly as it is (see this task's Interfaces block for why it gains no new field):

```rust
/// Options a caller passes down to `check_updates`. Adapters ignore fields
/// that do not apply to them; a new field must never change behaviour for an
/// adapter that does not read it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckOptions {
    /// Homebrew only: include casks that update themselves (`brew outdated --greedy`).
    pub include_self_updating: bool,
}
```

And in the `Adapter` trait:

```rust
    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError>;
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --workspace`
Expected: FAIL to compile with many errors of two shapes: `error[E0050]: method `check_updates` has 1 parameter but the declaration in trait `Adapter` has 2` (every `impl Adapter for ...` block: `BrewAdapter` in `brew/mod.rs`, the `FakeAdapter`s in `session/mod.rs`, `ipc.rs`, and the six `tests/ops_*_test.rs` files), plus `error[E0061]: this function takes 2 arguments but 1 argument was supplied` at every call site of `check_updates` and `Session::refresh`. No `Capabilities` literal is affected, because this task does not change that struct.

- [ ] **Step 3: Fix every implementor and call site so the workspace compiles**

Modify `crates/banager-core/src/adapters/brew/mod.rs`:
- Change the `use` line to `use crate::adapters::{validate_package_name, Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions};`.
- Change the inherent `check_updates` method's signature to accept the new parameter, ignored for now:
  ```rust
      pub async fn check_updates(
          &self,
          inst: &ManagerInstance,
          _opts: &CheckOptions,
      ) -> Result<Vec<UpdateCandidate>, AdapterError> {
  ```
  (leave the body unchanged for this step).
- Change the trait-impl forwarding method to accept and pass the parameter through:
  ```rust
      async fn check_updates(
          &self,
          inst: &ManagerInstance,
          opts: &CheckOptions,
      ) -> Result<Vec<UpdateCandidate>, AdapterError> {
          BrewAdapter::check_updates(self, inst, opts).await
      }
  ```
- Leave `capabilities()` exactly as it is.
- Fix the six existing test call sites of `check_updates` in the `mod tests` block:
  Run:
  ```bash
  sed -i '' -E 's/\.check_updates\((&inst[a-z_]*)\)/.check_updates(\1, \&CheckOptions::default())/g' crates/banager-core/src/adapters/brew/mod.rs
  ```
  Expected effect (verify with `grep -n "check_updates(&inst" crates/banager-core/src/adapters/brew/mod.rs`): every `.check_updates(&inst)`, `.check_updates(&inst_opt)`, `.check_updates(&inst_local)` becomes e.g. `.check_updates(&inst, &CheckOptions::default())`.

Modify `crates/banager-core/src/session/mod.rs`:
- Change `use crate::adapters::{Adapter, AdapterError};` to `use crate::adapters::{Adapter, AdapterError, CheckOptions};`.
- Change `Session::refresh`'s signature and thread `opts` down to the per-instance `check_updates` call. This is the complete function — the only changes from the current version are the new `opts: &CheckOptions` parameter, the new `let opts: CheckOptions = *opts;` line, and `&opts` added to the `check_updates` call inside the spawned task; everything else is byte-for-byte identical to today's `refresh`:
  ```rust
      pub async fn refresh(self: &Arc<Self>, env: &HostEnv, opts: &CheckOptions) -> Snapshot {
          let seq_before = self.refresh_seq.load(Ordering::SeqCst);
          let _gate = self.refresh_gate.lock().await;
          if self.refresh_seq.load(Ordering::SeqCst) != seq_before {
              // Another call already completed a refresh while we waited for
              // the gate. Its result is exactly what we would produce — even
              // when its content was identical to what came before and so
              // left `generation` unchanged (M5 in the design review): a
              // second, redundant run of the adapters must not happen just
              // because nothing looked different.
              return self.snapshot.lock().unwrap().clone();
          }

          let previous = self.snapshot.lock().unwrap().clone();
          // Owned copy (CheckOptions is Copy): each per-instance spawned task
          // below needs its own 'static value, and the caller's `&opts`
          // reference cannot outlive this function.
          let opts: CheckOptions = *opts;

          if BrewAdapter::refuses_as_root(env) {
              // This refresh ran to completion: it did not fail, it answered
              // "Banager cannot run as root", which is a definitive result
              // about the host and not a missing one. So it stamps
              // `refreshed_at` like any other completed refresh. Carrying
              // `previous.refreshed_at` forward instead left it `None` on a
              // process's first refresh, and the front end reads a null
              // `refreshed_at` with no errors as "no refresh has finished
              // yet" — which, since a process's euid never changes, would
              // have been true forever.
              let refused = Snapshot {
                  generation: previous.generation,
                  detect: DetectOutcome::RefusedAsRoot,
                  instances: Vec::new(),
                  artifacts: Vec::new(),
                  updates: Vec::new(),
                  refreshed_at: Some(self.now()),
                  stale: previous.stale,
                  errors: Vec::new(),
              };
              return self.commit(previous, refused);
          }

          let mut instances = Vec::new();
          for adapter in self.adapters.values() {
              instances.extend(adapter.detect(env).await);
          }
          for inst in &instances {
              self.ops.register_instance(inst.clone());
          }
          let detect = if instances.is_empty() {
              DetectOutcome::Missing
          } else {
              DetectOutcome::Found
          };

          // M8 in the design review: take the *same* per-instance resource
          // lock a submitted install/upgrade/uninstall holds for the whole
          // inventory+check_updates segment below, so a refresh can never
          // observe a half-updated filesystem while an operation on that
          // instance is running (and vice versa). Each instance's fetch is
          // its own spawned task so that a lock held by a slow or blocked
          // operation on *one* instance only ever delays that instance's
          // fetch — spec §6's "same lock serial, different locks parallel"
          // applies here exactly as it does to operations themselves.
          let mut handles = Vec::with_capacity(instances.len());
          for inst in instances.clone() {
              let Some(adapter) = self.adapters.get(&inst.adapter_id).cloned() else {
                  continue;
              };
              let ops = self.ops.clone();
              let previous = previous.clone();
              let opts = opts;
              handles.push((
                  inst.id.clone(),
                  tokio::spawn(async move {
                      let _lock = ops
                          .acquire_resource_lock(ResourceLock(inst.id.clone()))
                          .await;
                      let mut artifacts = Vec::new();
                      let mut updates = Vec::new();
                      let mut errors = Vec::new();
                      let mut stale = false;
                      match adapter.inventory(&inst).await {
                          Ok(items) => artifacts.extend(items),
                          Err(e) => {
                              errors.push(SourceError {
                                  instance_id: inst.id.clone(),
                                  message: e.to_string(),
                              });
                              stale = true;
                              artifacts.extend(
                                  previous
                                      .artifacts
                                      .iter()
                                      .filter(|a| a.key.instance_id == inst.id)
                                      .cloned(),
                              );
                          }
                      }
                      match adapter.check_updates(&inst, &opts).await {
                          Ok(items) => updates.extend(items),
                          Err(e) => {
                              errors.push(SourceError {
                                  instance_id: inst.id.clone(),
                                  message: e.to_string(),
                              });
                              stale = true;
                              updates.extend(
                                  previous
                                      .updates
                                      .iter()
                                      .filter(|u| u.key.instance_id == inst.id)
                                      .cloned(),
                              );
                          }
                      }
                      (artifacts, updates, errors, stale)
                  }),
              ));
          }

          let mut artifacts = Vec::new();
          let mut updates = Vec::new();
          let mut errors = Vec::new();
          let mut stale = false;
          for (instance_id, handle) in handles {
              match handle.await {
                  Ok((a, u, e, s)) => {
                      artifacts.extend(a);
                      updates.extend(u);
                      errors.extend(e);
                      stale = stale || s;
                  }
                  Err(_join_err) => {
                      errors.push(SourceError {
                          instance_id,
                          message: "internal error refreshing this instance".to_string(),
                      });
                      stale = true;
                  }
              }
          }

          let refreshed_at = if stale {
              previous.refreshed_at
          } else {
              Some(self.now())
          };
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
  ```
  (the `opts` shadow inside the `for inst in instances.clone()` loop — `let opts = opts;` — is not strictly required since `CheckOptions` is `Copy` and the outer `opts` is not otherwise moved, but it documents at the call site that each spawned task gets its own value; leave it in for clarity).
- In the test module's `FakeAdapter`, change:
  ```rust
          async fn check_updates(
              &self,
              inst: &ManagerInstance,
              _opts: &CheckOptions,
          ) -> Result<Vec<UpdateCandidate>, AdapterError> {
              let s = self.state.lock().unwrap();
              Ok(s.updates.get(&inst.id).cloned().unwrap_or_default())
          }
  ```
- Change the test module's import line to `use crate::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions};`.
- Fix every test call to `session.refresh(...)`/`session_a.refresh(...)`/`session_b.refresh(...)`/`session_for_refresh.refresh(...)`:
  Run:
  ```bash
  sed -i '' -E 's/\.refresh\(&(non_root_env|root_env)\(\)\)/.refresh(\&\1(), \&CheckOptions::default())/g' crates/banager-core/src/session/mod.rs
  ```
  Expected effect (verify with `grep -c '&CheckOptions::default()' crates/banager-core/src/session/mod.rs`): 22 occurrences (one per pre-existing `.refresh(&non_root_env())` / `.refresh(&root_env())` call).

Modify `src-tauri/src/ipc.rs`:
- Add `use banager_core::adapters::CheckOptions;` to the top-level imports.
- In `refresh_impl`, thread a placeholder `CheckOptions` through for now (Step 9 below replaces this with the real one built from `Settings`):
  ```rust
  pub(crate) async fn refresh_impl(state: &AppState) -> Result<Snapshot, String> {
      let generation_before = state.session.snapshot().generation;
      let snapshot = state
          .session
          .refresh(&HostEnv::discover(), &CheckOptions::default())
          .await;
      if snapshot.generation != generation_before {
          state.channel_sink.broadcast(UiEvent::SnapshotChanged {
              generation: snapshot.generation,
          });
      }
      Ok(snapshot)
  }
  ```
- In the test module, change the import to `use banager_core::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions};` and change `FakeAdapter`'s `check_updates`:
  ```rust
          async fn check_updates(
              &self,
              _inst: &ManagerInstance,
              _opts: &CheckOptions,
          ) -> Result<Vec<UpdateCandidate>, AdapterError> {
              Ok(Vec::new())
          }
  ```

Modify `crates/banager-core/examples/brew_smoke.rs`:
- Add `use banager_core::adapters::CheckOptions;`.
- Change the call to:
  ```rust
          let outdated = adapter
              .check_updates(inst, &CheckOptions::default())
              .await
              .expect("check_updates failed");
  ```

Modify each of the six `crates/banager-core/tests/ops_*_test.rs` files:
- Change their `use banager_core::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities};` line to add `CheckOptions`.
- Add `_opts: &CheckOptions` as the third parameter of every `check_updates` implementation (`ops_semaphore_test.rs` has two `FakeAdapter`-shaped structs and needs both fixed).

Run for the `CheckOptions` import and signature edits across all six (each file has exactly one `use banager_core::adapters::{...};` line and one or two `check_updates` impls; do these edits directly since the six files are not identical enough for one safe blanket `sed`):
```bash
sed -i '' -E 's/use banager_core::adapters::\{Adapter, AdapterError, AdapterMeta, Capabilities\};/use banager_core::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions};/' \
  crates/banager-core/tests/ops_semaphore_test.rs \
  crates/banager-core/tests/ops_cancel_test.rs \
  crates/banager-core/tests/ops_lock_test.rs \
  crates/banager-core/tests/ops_panic_test.rs \
  crates/banager-core/tests/ops_outcome_test.rs \
  crates/banager-core/tests/ops_summaries_test.rs
sed -i '' -E '/async fn check_updates\(/,/-> Result<Vec<UpdateCandidate>, AdapterError> \{/ s/_inst: &ManagerInstance,$/_inst: \&ManagerInstance,\n        _opts: \&CheckOptions,/' \
  crates/banager-core/tests/ops_semaphore_test.rs \
  crates/banager-core/tests/ops_cancel_test.rs \
  crates/banager-core/tests/ops_lock_test.rs \
  crates/banager-core/tests/ops_panic_test.rs \
  crates/banager-core/tests/ops_outcome_test.rs \
  crates/banager-core/tests/ops_summaries_test.rs
```
After running these, verify each `check_updates` in the six files reads `&self, _inst: &ManagerInstance, _opts: &CheckOptions,`; fix by hand any signature whose indentation does not match `_inst: &ManagerInstance,$` exactly (verify with `rg -n "_opts: &CheckOptions" crates/banager-core/tests/` — expected 7 matches, one per `check_updates` impl across the six files, `ops_semaphore_test.rs` contributing two).

Finally, run `cargo fmt --all` to normalize the sed-inserted lines' indentation.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --workspace`
Expected: PASS — every pre-existing test still green (125+ Rust tests), no behaviour has changed yet; `opts` is threaded through every implementor and call site but not yet read anywhere, and every call site passes `CheckOptions::default()`.

- [ ] **Step 5: Commit the mechanical refactor**

```bash
git add crates/banager-core/src/adapters/mod.rs crates/banager-core/src/adapters/brew/mod.rs crates/banager-core/src/session/mod.rs crates/banager-core/examples/brew_smoke.rs crates/banager-core/tests/ops_semaphore_test.rs crates/banager-core/tests/ops_cancel_test.rs crates/banager-core/tests/ops_lock_test.rs crates/banager-core/tests/ops_panic_test.rs crates/banager-core/tests/ops_outcome_test.rs crates/banager-core/tests/ops_summaries_test.rs src-tauri/src/ipc.rs
git commit -m "$(cat <<'EOF'
refactor(core): thread CheckOptions through Adapter::check_updates

Adds the CheckOptions parameter the backlog's greedy_casks item
needs, and updates every implementor and call site so the workspace
compiles. No behaviour changes yet.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 6: Write the failing tests for `--greedy`, the `Settings` field, and end-to-end wiring**

Modify `crates/banager-core/src/adapters/brew/mod.rs` — add this test inside the existing `mod tests` block (after `test_check_updates_ttl_is_tracked_per_instance`):

```rust
    #[tokio::test]
    async fn test_check_updates_passes_greedy_flag_when_include_self_updating_is_true() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "update"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let empty_outdated = r#"{"formulae":[],"casks":[]}"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2", "--greedy"],
            CommandOutput {
                exit_code: Some(0),
                stdout: empty_outdated.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let opts = CheckOptions {
            include_self_updating: true,
        };
        let result = adapter
            .check_updates(&test_instance(), &opts)
            .await
            .expect("check_updates with --greedy");
        assert!(result.is_empty());
    }
```

Modify `crates/banager-core/src/settings.rs`:
- Add the field to `Settings` and to its `Default` impl:
  ```rust
  #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
  pub struct Settings {
      pub language: Language,
      pub show_technical_details: bool,
      pub ignored_updates: Vec<ArtifactKey>,
      /// Feeds CheckOptions.include_self_updating. Default false: most people
      /// do not want Chrome and Docker listed as updatable when those apps
      /// update themselves. `#[serde(default)]` so a settings.json written by
      /// an older Banager version (or a front end not yet sending this field)
      /// still deserializes instead of losing every other field to
      /// `Settings::default()` in `load()`.
      #[serde(default)]
      pub include_self_updating: bool,
  }

  impl Default for Settings {
      fn default() -> Settings {
          Settings {
              language: Language::System,
              show_technical_details: false,
              ignored_updates: Vec::new(),
              include_self_updating: false,
          }
      }
  }
  ```
- Update the existing `test_default_settings_serialize_with_no_renames` test to also assert the new field:
  ```rust
      #[test]
      fn test_default_settings_serialize_with_no_renames() {
          let json = serde_json::to_string(&Settings::default()).expect("serialize");
          assert!(json.contains("\"language\":\"System\""));
          assert!(json.contains("\"show_technical_details\":false"));
          assert!(json.contains("\"ignored_updates\":[]"));
          assert!(json.contains("\"include_self_updating\":false"));
      }
  ```
- Update the existing `test_save_then_load_round_trips_a_non_default_settings` test's literal to include the new field:
  ```rust
      #[test]
      fn test_save_then_load_round_trips_a_non_default_settings() {
          let path = temp_settings_path("roundtrip");
          let settings = Settings {
              language: Language::ZhCn,
              show_technical_details: true,
              ignored_updates: vec![ArtifactKey {
                  instance_id: "brew:/opt/homebrew".to_string(),
                  kind: ArtifactKind::Formula,
                  name: "jq".to_string(),
              }],
              include_self_updating: true,
          };
          save(&path, &settings).expect("save");
          let loaded = load(&path);
          assert_eq!(loaded, settings);
          let _ = std::fs::remove_file(&path);
      }
  ```
- Add a new backward-compatibility test (place it after `test_load_of_malformed_json_returns_defaults`):
  ```rust
      #[test]
      fn test_load_of_json_missing_include_self_updating_defaults_it_to_false_and_keeps_the_rest() {
          // A settings.json written before this field existed (or sent by a
          // not-yet-updated front end) must still load its other fields
          // rather than falling back to Settings::default() entirely — that
          // would silently discard a user's language and ignored_updates.
          let path = temp_settings_path("no-include-self-updating");
          std::fs::write(
              &path,
              br#"{"language":"ZhCn","show_technical_details":true,"ignored_updates":[]}"#,
          )
          .expect("write settings.json without include_self_updating");
          let loaded = load(&path);
          assert_eq!(loaded.language, Language::ZhCn);
          assert!(loaded.show_technical_details);
          assert!(!loaded.include_self_updating);
          let _ = std::fs::remove_file(&path);
      }
  ```

Modify `src-tauri/src/ipc.rs`'s test module to prove the setting reaches `check_updates` end to end:
- Add a `check_options_calls` recorder to `FakeAdapter` and use it:
  ```rust
      struct FakeAdapter {
          meta: AdapterMeta,
          instance: ManagerInstance,
          execute_calls: Arc<AtomicUsize>,
          check_options_calls: Arc<Mutex<Vec<CheckOptions>>>,
      }
  ```
  ```rust
          async fn check_updates(
              &self,
              _inst: &ManagerInstance,
              opts: &CheckOptions,
          ) -> Result<Vec<UpdateCandidate>, AdapterError> {
              self.check_options_calls.lock().unwrap().push(*opts);
              Ok(Vec::new())
          }
  ```
- Change `state_with_fake_adapter_and_now` to build and return that recorder:
  ```rust
      fn state_with_fake_adapter_and_now(
          now_fn: Option<fn() -> i64>,
      ) -> (AppState, Arc<AtomicUsize>, Arc<Mutex<Vec<CheckOptions>>>) {
          let instance = ManagerInstance {
              id: "fake:1".to_string(),
              adapter_id: "fake".to_string(),
              exe_path: PathBuf::from("/bin/true"),
              prefix: PathBuf::from("/"),
              scope: Scope::User,
              version: Some("1.0".to_string()),
              healthy: true,
          };
          let meta = AdapterMeta {
              id: "fake".to_string(),
              name: "fake".to_string(),
              kind: "fake".to_string(),
              platforms: vec!["macos".to_string()],
              homepage: "https://example.invalid".to_string(),
              schema_version: 1,
              verified_versions: vec![],
          };
          let execute_calls = Arc::new(AtomicUsize::new(0));
          let check_options_calls = Arc::new(Mutex::new(Vec::new()));
          let adapter: Arc<dyn Adapter> = Arc::new(FakeAdapter {
              meta,
              instance,
              execute_calls: execute_calls.clone(),
              check_options_calls: check_options_calls.clone(),
          });
          let sink = ChannelSink::new();
          let session =
              banager_core::session::Session::with_adapters(sink.clone(), vec![adapter], now_fn);
          let state = AppState {
              session,
              settings_path: temp_settings_path("appstate"),
              settings: std::sync::Mutex::new(Settings::default()),
              channel_sink: sink,
          };
          (state, execute_calls, check_options_calls)
      }
  ```
  and `state_with_fake_adapter`:
  ```rust
      fn state_with_fake_adapter() -> AppState {
          let (state, _execute_calls, _check_options_calls) = state_with_fake_adapter_and_now(None);
          state
      }
  ```
- Update the three existing direct callers of `state_with_fake_adapter_and_now` to destructure the new third element as `_check_options_calls`:
  ```bash
  sed -i '' -E 's/let \(state, execute_calls\) = state_with_fake_adapter_and_now\(/let (state, execute_calls, _check_options_calls) = state_with_fake_adapter_and_now(/g' src-tauri/src/ipc.rs
  ```
  (this matches `test_submit_operation_impl_rejects_an_unissued_plan_id`, `test_submit_operation_impl_rejects_the_same_plan_id_submitted_twice`, and `test_submit_operation_impl_rejects_an_expired_plan` — verify with `grep -c "_check_options_calls) = state_with_fake_adapter_and_now" src-tauri/src/ipc.rs`, expected 3).
- Add two new tests (after `test_get_and_set_settings_impl_round_trip`):
  ```rust
      #[tokio::test]
      async fn test_refresh_impl_passes_include_self_updating_from_settings_to_check_updates() {
          // Backend end of the backlog's `greedy_casks` item: the Settings
          // toggle must actually reach `Adapter::check_updates` on every
          // refresh, not just round-trip through get_settings/set_settings.
          let (state, _execute_calls, check_options_calls) = state_with_fake_adapter_and_now(None);
          let mut settings = get_settings_impl(&state).expect("get_settings_impl");
          settings.include_self_updating = true;
          set_settings_impl(&state, settings).expect("set_settings_impl");

          refresh_impl(&state).await.expect("refresh_impl");

          let calls = check_options_calls.lock().unwrap().clone();
          assert_eq!(calls.len(), 1);
          assert!(
              calls[0].include_self_updating,
              "refresh_impl must read the persisted setting and thread it through, got {calls:?}"
          );
      }

      #[tokio::test]
      async fn test_refresh_impl_defaults_include_self_updating_to_false() {
          let (state, _execute_calls, check_options_calls) = state_with_fake_adapter_and_now(None);
          refresh_impl(&state).await.expect("refresh_impl");
          let calls = check_options_calls.lock().unwrap().clone();
          assert_eq!(calls.len(), 1);
          assert!(!calls[0].include_self_updating);
      }
  ```

- [ ] **Step 7: Run to verify it fails**

Run: `cargo test --workspace`
Expected: FAIL — `test_check_updates_passes_greedy_flag_when_include_self_updating_is_true` panics (`RunnerError::NoMock` for `["outdated","--json=v2","--greedy"]`, since brew does not send `--greedy` yet); `settings.rs` and `ipc.rs` additionally fail to *compile* (`error[E0560]`/`E0609`: `Settings` has no field `include_self_updating`; `ipc.rs`'s `FakeAdapter` has no field `check_options_calls` and `refresh_impl` still passes `&CheckOptions::default()`), so the whole `cargo test` invocation fails at the build step.

- [ ] **Step 8: Implement `--greedy`, the `Settings` field, and end-to-end wiring**

Modify `crates/banager-core/src/adapters/brew/mod.rs` — rewrite `check_updates` to honour the flag:

```rust
    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        self.maybe_update(inst).await?;
        let mut args = vec!["outdated".to_string(), "--json=v2".to_string()];
        if opts.include_self_updating {
            args.push("--greedy".to_string());
        }
        let output = self.run_brew(inst, args, Duration::from_secs(120)).await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        parse_outdated(&output.stdout, &inst.id)
    }
```

Modify `src-tauri/src/ipc.rs`'s `refresh_impl` to build the real `CheckOptions` from persisted `Settings`:

```rust
pub(crate) async fn refresh_impl(state: &AppState) -> Result<Snapshot, String> {
    let generation_before = state.session.snapshot().generation;
    let opts = CheckOptions {
        include_self_updating: state.get_settings().include_self_updating,
    };
    let snapshot = state.session.refresh(&HostEnv::discover(), &opts).await;
    if snapshot.generation != generation_before {
        state.channel_sink.broadcast(UiEvent::SnapshotChanged {
            generation: snapshot.generation,
        });
    }
    Ok(snapshot)
}
```

`crates/banager-core/src/settings.rs`'s field/tests are already complete from Step 6 (no further code change needed there — Step 6's writes are the implementation, since the field addition itself IS the fix for the compile errors and the backward-compat test already exercises the real `#[serde(default)]` behaviour).

- [ ] **Step 9: Run to verify it passes**

Run: `cargo test --workspace`
Expected: PASS — every test green, including `test_check_updates_passes_greedy_flag_when_include_self_updating_is_true`, the three `settings.rs` tests, and the two new `ipc.rs` end-to-end tests. Also run `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` — Expected: both clean.

- [ ] **Step 10: Commit**

```bash
git add crates/banager-core/src/adapters/brew/mod.rs crates/banager-core/src/settings.rs src-tauri/src/ipc.rs
git commit -m "$(cat <<'EOF'
feat(brew): honour include_self_updating via --greedy, wired end to end

Settings.include_self_updating now flows through IPC's refresh_impl,
Session::refresh, and Adapter::check_updates into brew's `--greedy`
flag, with #[serde(default)] keeping old settings.json files loadable.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: brew `maybe_update` degrades and serialises

**Files:**
- Modify: `crates/banager-core/src/adapters/brew/mod.rs`
- Modify: `crates/banager-core/src/runner/mock.rs`
- Test: inline `#[cfg(test)]` modules in both files above

**Interfaces:**
- Consumes: `BrewAdapter::check_updates(&self, inst: &ManagerInstance, opts: &CheckOptions)` and `UpdateCandidate.warnings: Vec<String>` (both from Task 2 / pre-existing `model.rs`).
- Produces:
  ```rust
  // crates/banager-core/src/runner/mock.rs (new, additive method)
  impl MockRunner {
      pub fn delay(&self, argv: Vec<&str>, delay: std::time::Duration);
  }
  ```
  `MockRunner::delay` exists for this task's own concurrency test (Step 5) and has no other caller in this phase — it is shared test infrastructure in the same sense `crate::events::VecSink` is, available to any later timing test that needs it, not something Tasks 6-10 are expected to use.

- [ ] **Step 1: Write the failing test for degrading a failed `brew update` to a warning**

Modify `crates/banager-core/src/adapters/brew/mod.rs` — add this test inside the existing `mod tests` block (after `test_check_updates_passes_greedy_flag_when_include_self_updating_is_true`):

```rust
    #[tokio::test]
    async fn test_check_updates_degrades_a_failed_brew_update_to_a_warning() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "update"],
            CommandOutput {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "error: brew update failed: no such remote".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let outdated_json = r#"{"formulae":[{"name":"jq","installed_versions":["1.6"],"current_version":"1.7.1","pinned":false,"pinned_version":null}],"casks":[]}"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: outdated_json.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("a failed `brew update` must not fail check_updates");
        assert_eq!(candidates.len(), 1);
        assert!(
            candidates[0]
                .warnings
                .iter()
                .any(|w| w.contains("brew update failed")),
            "expected a brew-update-failed warning, got {:?}",
            candidates[0].warnings
        );
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib adapters::brew::tests::test_check_updates_degrades_a_failed_brew_update_to_a_warning`
Expected: FAIL — the test panics on `.expect("a failed \`brew update\` must not fail check_updates")` because `check_updates` currently propagates `maybe_update`'s error via `self.maybe_update(inst).await?;`, so the whole call returns `Err(AdapterError::CommandFailed { .. })`.

- [ ] **Step 3: Degrade the failure to a per-candidate warning**

Modify `crates/banager-core/src/adapters/brew/mod.rs` — rewrite `check_updates`:

```rust
    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        let update_warning = match self.maybe_update(inst).await {
            Ok(()) => None,
            Err(e) => Some(format!(
                "brew update failed ({e}); showing potentially stale results"
            )),
        };
        let mut args = vec!["outdated".to_string(), "--json=v2".to_string()];
        if opts.include_self_updating {
            args.push("--greedy".to_string());
        }
        let output = self.run_brew(inst, args, Duration::from_secs(120)).await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        let mut candidates = parse_outdated(&output.stdout, &inst.id)?;
        if let Some(warning) = &update_warning {
            for candidate in &mut candidates {
                candidate.warnings.push(warning.clone());
            }
        }
        Ok(candidates)
    }
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib adapters::brew::`
Expected: PASS — the new test passes, and every pre-existing `check_updates`/TTL test still passes (they all mock a *successful* `brew update`, so `update_warning` stays `None` and their output is unchanged).

- [ ] **Step 5: Write the failing test for serialising `maybe_update` per instance**

Modify `crates/banager-core/src/adapters/brew/mod.rs` — add this test inside `mod tests` (after the test from Step 1):

```rust
    #[tokio::test]
    async fn test_check_updates_serialises_maybe_update_across_concurrent_callers() {
        // Before this fix, two concurrent `check_updates` calls for the same
        // instance could both observe the TTL expired and both run `brew
        // update` — this test makes the first `update` slow enough that a
        // second, concurrent call is guaranteed to reach its own TTL check
        // while the first is still in flight, and proves the fix serialises
        // them: only one `brew update` process ever runs.
        let runner = Arc::new(MockRunner::new());
        runner.delay(
            vec!["/opt/homebrew/bin/brew", "update"],
            Duration::from_millis(200),
        );
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "update"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let empty_outdated = r#"{"formulae":[],"casks":[]}"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "outdated", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: empty_outdated.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = Arc::new(
            BrewAdapter::new(runner.clone()).with_update_ttl(Duration::from_secs(3600)),
        );
        let inst = test_instance();

        let task_a = {
            let adapter = adapter.clone();
            let inst = inst.clone();
            tokio::spawn(async move { adapter.check_updates(&inst, &CheckOptions::default()).await })
        };
        // Give task_a time to acquire the per-instance update lock and start
        // its (slow) `brew update` before task_b starts.
        tokio::time::sleep(Duration::from_millis(20)).await;
        let task_b = {
            let adapter = adapter.clone();
            let inst = inst.clone();
            tokio::spawn(async move { adapter.check_updates(&inst, &CheckOptions::default()).await })
        };

        task_a.await.expect("task a panicked").expect("check_updates a");
        task_b.await.expect("task b panicked").expect("check_updates b");

        let update_calls = runner
            .calls()
            .iter()
            .filter(|c| c.get(1).map(String::as_str) == Some("update"))
            .count();
        assert_eq!(
            update_calls, 1,
            "two concurrent check_updates calls for the same instance must run `brew update` at most once"
        );
    }
```

- [ ] **Step 6: Run to verify it fails**

Run: `cargo test -p banager-core --lib adapters::brew::tests::test_check_updates_serialises_maybe_update_across_concurrent_callers`
Expected: FAIL to compile — `error[E0599]: no method named \`delay\` found for struct \`MockRunner\`` (`MockRunner` has no such method yet).

- [ ] **Step 7: Add `MockRunner::delay` and serialise `maybe_update` per instance**

Modify `crates/banager-core/src/runner/mock.rs` — add a `delays` map and the `delay` method, and apply the delay in `run`:

```rust
use super::{CommandOutput, CommandRunner, CommandSpec, LineCallback, RunnerError};
use crate::events::Stream;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub struct MockRunner {
    responses: Mutex<HashMap<Vec<String>, CommandOutput>>,
    delays: Mutex<HashMap<Vec<String>, Duration>>,
    calls: Mutex<Vec<Vec<String>>>,
}

impl MockRunner {
    pub fn new() -> MockRunner {
        MockRunner {
            responses: Mutex::new(HashMap::new()),
            delays: Mutex::new(HashMap::new()),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn argv(spec: &CommandSpec) -> Vec<String> {
        let mut v = vec![spec.program.to_string_lossy().to_string()];
        v.extend(spec.args.iter().cloned());
        v
    }

    /// `argv` includes the program path as its first element, exactly as the
    /// caller built the `CommandSpec` — lookup is by exact argv match.
    pub fn respond(&self, argv: Vec<&str>, output: CommandOutput) {
        let key: Vec<String> = argv.into_iter().map(|s| s.to_string()).collect();
        self.responses.lock().unwrap().insert(key, output);
    }

    /// Makes the canned response for `argv` (registered via `respond`)
    /// available only after `delay` has elapsed, so a test can observe what
    /// happens *while* a call is still in flight — e.g. proving two
    /// concurrent callers serialise on a lock rather than both proceeding
    /// immediately.
    pub fn delay(&self, argv: Vec<&str>, delay: Duration) {
        let key: Vec<String> = argv.into_iter().map(|s| s.to_string()).collect();
        self.delays.lock().unwrap().insert(key, delay);
    }

    pub fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

impl Default for MockRunner {
    fn default() -> Self {
        MockRunner::new()
    }
}

#[async_trait]
impl CommandRunner for MockRunner {
    async fn run(
        &self,
        spec: CommandSpec,
        on_line: Option<LineCallback>,
        _cancel: CancellationToken,
    ) -> Result<CommandOutput, RunnerError> {
        let key = Self::argv(&spec);
        self.calls.lock().unwrap().push(key.clone());
        let delay = self.delays.lock().unwrap().get(&key).copied();
        if let Some(delay) = delay {
            tokio::time::sleep(delay).await;
        }
        let output = self
            .responses
            .lock()
            .unwrap()
            .get(&key)
            .cloned()
            .ok_or_else(|| RunnerError::NoMock(key))?;
        if let Some(cb) = on_line {
            for line in output.stdout.split('\n') {
                if !line.is_empty() {
                    cb(Stream::Stdout, line.to_string());
                }
            }
            for line in output.stderr.split('\n') {
                if !line.is_empty() {
                    cb(Stream::Stderr, line.to_string());
                }
            }
        }
        Ok(output)
    }
}
```
(the existing `#[cfg(test)] mod tests` block in this file is unchanged — leave it exactly as is, appended below the code above).

Modify `crates/banager-core/src/adapters/brew/mod.rs`:
- Add the per-instance lock map to `BrewAdapter`'s fields and constructor:
  ```rust
  pub struct BrewAdapter {
      runner: Arc<dyn CommandRunner>,
      meta: AdapterMeta,
      last_update: Mutex<HashMap<InstanceId, Instant>>,
      /// Serialises `maybe_update` per instance: without this, two
      /// concurrent `check_updates` calls for the same instance could both
      /// observe "TTL expired" before either had recorded a fresh
      /// timestamp, and both run `brew update` concurrently — wasteful, and
      /// two `brew update` processes writing the same Homebrew cache
      /// directory at once is not something Homebrew is designed to
      /// tolerate. Keyed the same way as `last_update`.
      update_locks: Mutex<HashMap<InstanceId, Arc<tokio::sync::Mutex<()>>>>,
      update_ttl: Duration,
      euid_fn: fn() -> u32,
  }
  ```
  ```rust
      pub fn new(runner: Arc<dyn CommandRunner>) -> BrewAdapter {
          let meta = AdapterMeta::from_toml(include_str!("../../../../../adapters/meta/brew.toml"))
              .expect("adapters/meta/brew.toml must parse");
          BrewAdapter {
              runner,
              meta,
              last_update: Mutex::new(HashMap::new()),
              update_locks: Mutex::new(HashMap::new()),
              update_ttl: Duration::from_secs(6 * 3600),
              euid_fn: || unsafe { libc::geteuid() },
          }
      }
  ```
- Add a helper to get-or-create the per-instance lock, and rewrite `maybe_update` to hold it across the whole decide-then-run-then-record sequence:
  ```rust
      fn update_lock_for(&self, inst_id: &InstanceId) -> Arc<tokio::sync::Mutex<()>> {
          let mut locks = self.update_locks.lock().unwrap();
          locks
              .entry(inst_id.clone())
              .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
              .clone()
      }

      async fn maybe_update(&self, inst: &ManagerInstance) -> Result<(), AdapterError> {
          let lock = self.update_lock_for(&inst.id);
          let _guard = lock.lock().await;
          let needs_update = {
              let last = self.last_update.lock().unwrap();
              match last.get(&inst.id) {
                  Some(t) => t.elapsed() >= self.update_ttl,
                  None => true,
              }
          };
          if !needs_update {
              return Ok(());
          }
          let output = self
              .run_brew(inst, vec!["update".to_string()], Duration::from_secs(120))
              .await?;
          if output.exit_code != Some(0) {
              return Err(AdapterError::CommandFailed {
                  code: output.exit_code,
                  stderr: output.stderr,
              });
          }
          self.last_update
              .lock()
              .unwrap()
              .insert(inst.id.clone(), Instant::now());
          Ok(())
      }
  ```

- [ ] **Step 8: Run to verify it passes**

Run: `cargo test -p banager-core --lib`
Expected: PASS — all brew adapter tests pass, including both new ones from Steps 1 and 5, and `runner::mock`'s existing tests are unaffected (the `delays` map defaults empty, so no pre-existing `MockRunner` behaviour changes).

- [ ] **Step 9: Commit**

```bash
git add crates/banager-core/src/adapters/brew/mod.rs crates/banager-core/src/runner/mock.rs
git commit -m "$(cat <<'EOF'
fix(brew): degrade a failed brew update to a warning and serialise it

A failed `brew update` no longer fails the whole check_updates call —
outdated results are still returned, tagged with a warning. Two
concurrent check_updates calls for the same instance now serialise on
a per-instance lock instead of both running `brew update`.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: `unverified_version` on ManagerInstance + the UI badge

**Files:**
- Modify: `crates/banager-core/src/model.rs`
- Modify: `crates/banager-core/src/adapters/mod.rs`
- Modify: `crates/banager-core/src/adapters/brew/mod.rs`
- Modify: `crates/banager-core/src/session/mod.rs`
- Modify: `crates/banager-core/tests/ops_semaphore_test.rs`
- Modify: `crates/banager-core/tests/ops_cancel_test.rs`
- Modify: `crates/banager-core/tests/ops_lock_test.rs`
- Modify: `crates/banager-core/tests/ops_panic_test.rs`
- Modify: `crates/banager-core/tests/ops_outcome_test.rs`
- Modify: `crates/banager-core/tests/ops_summaries_test.rs`
- Modify: `src-tauri/src/ipc.rs`
- Modify: `src/lib/types.ts`
- Modify: `src/pages/InstalledPage.tsx`
- Modify: `src/pages/InstalledPage.test.tsx`
- Modify: `src/lib/types.test.ts`
- Modify: `src/App.test.tsx`
- Modify: `src/i18n/en.json`
- Modify: `src/i18n/zh-CN.json`

**Interfaces:**
- Consumes: `AdapterMeta.verified_versions: Vec<String>` (pre-existing, from `adapters/mod.rs`).
- Produces (authoritative, from the phase skeleton, verbatim):
  ```rust
  pub struct ManagerInstance {
      // … existing fields …
      pub unverified_version: Option<String>,
  }
  ```
  Plus the single implementation of the rule itself, so the next six adapters
  do not each write their own copy:
  ```rust
  // crates/banager-core/src/adapters/mod.rs
  impl AdapterMeta {
      pub fn unverified_version(&self, detected: &Option<String>) -> Option<String>;
  }
  ```
  Tasks 5-10's `detect()` implementations all call `self.meta.unverified_version(&version)`; none of them reimplements the comparison. `BrewAdapter::detect` (Step 3 below) is the first caller.

- [ ] **Step 1: Write the failing tests for the new field**

Modify `crates/banager-core/src/model.rs` — update the existing round-trip test's literal and add a second one for the `Some` case (place both after the struct/enum definitions, inside the existing `mod tests` block):

```rust
    #[test]
    fn test_manager_instance_round_trips_through_json() {
        let instance = ManagerInstance {
            id: "brew:/opt/homebrew".to_string(),
            adapter_id: "brew".to_string(),
            exe_path: PathBuf::from("/opt/homebrew/bin/brew"),
            prefix: PathBuf::from("/opt/homebrew"),
            scope: Scope::User,
            version: Some("7.0.3".to_string()),
            healthy: true,
            unverified_version: None,
        };
        let json = serde_json::to_string(&instance).expect("serialize");
        let back: ManagerInstance = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(instance, back);
    }

    #[test]
    fn test_manager_instance_with_unverified_version_round_trips_through_json() {
        let instance = ManagerInstance {
            id: "brew:/opt/homebrew".to_string(),
            adapter_id: "brew".to_string(),
            exe_path: PathBuf::from("/opt/homebrew/bin/brew"),
            prefix: PathBuf::from("/opt/homebrew"),
            scope: Scope::User,
            version: Some("99.9.9".to_string()),
            healthy: true,
            unverified_version: Some("99.9.9".to_string()),
        };
        let json = serde_json::to_string(&instance).expect("serialize");
        assert!(json.contains("\"unverified_version\":\"99.9.9\""));
        let back: ManagerInstance = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(instance, back);
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --workspace`
Expected: FAIL to compile — `error[E0560]: struct \`ManagerInstance\` has no field named \`unverified_version\`` at both literals above, and (once that is the case) every other `ManagerInstance` literal in the workspace will fail the same way the moment the field is added, so this run is expected to fail purely on the two literals just written.

- [ ] **Step 3: Add the field, the shared rule on `AdapterMeta`, real `detect()` logic, and fix every other Rust literal**

Modify `crates/banager-core/src/model.rs`:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagerInstance {
    pub id: InstanceId,
    pub adapter_id: AdapterId,
    pub exe_path: PathBuf,
    pub prefix: PathBuf,
    pub scope: Scope,
    pub version: Option<String>,
    pub healthy: bool,
    /// None when the adapter's metadata lists no verified versions, or when
    /// the detected version is among them. Some(detected) when it is not,
    /// so the UI can mark the source as running an unverified version (spec
    /// §4.1).
    pub unverified_version: Option<String>,
}
```

Modify `crates/banager-core/src/adapters/mod.rs` — add the rule once, as an inherent method on `AdapterMeta`, right after its `from_toml` constructor:

```rust
impl AdapterMeta {
    /// `None` when this adapter's metadata lists no verified versions, or
    /// when `detected` is among them (or absent). `Some(detected)` when it
    /// is not, so the UI can mark the source as running an unverified
    /// version (spec §4.1).
    ///
    /// Defined once, here, rather than per adapter: every `detect()` in the
    /// workspace calls this, so the rule can only ever mean one thing. An
    /// earlier draft of this phase had seven copies of it.
    pub fn unverified_version(&self, detected: &Option<String>) -> Option<String> {
        detected
            .as_ref()
            .filter(|v| !self.verified_versions.is_empty() && !self.verified_versions.contains(v))
            .cloned()
    }
}
```

and its test, inside that file's existing `#[cfg(test)] mod tests` block (after `test_from_toml_parses_the_committed_brew_meta_file`):

```rust
    #[test]
    fn test_unverified_version_flags_only_a_version_outside_a_non_empty_verified_list() {
        let meta = AdapterMeta {
            id: "fake".to_string(),
            name: "fake".to_string(),
            kind: "fake".to_string(),
            platforms: vec!["macos".to_string()],
            homepage: "https://example.invalid".to_string(),
            schema_version: 1,
            verified_versions: vec!["1.0".to_string()],
        };
        assert_eq!(meta.unverified_version(&Some("1.0".to_string())), None);
        assert_eq!(
            meta.unverified_version(&Some("9.9".to_string())),
            Some("9.9".to_string())
        );
        assert_eq!(meta.unverified_version(&None), None);

        // An adapter whose meta file lists no verified versions has nothing
        // to compare against, so it never flags anything.
        let unpinned = AdapterMeta {
            verified_versions: vec![],
            ..meta
        };
        assert_eq!(unpinned.unverified_version(&Some("9.9".to_string())), None);
    }
```

Modify `crates/banager-core/src/adapters/brew/mod.rs` — compute the real value in `detect()` by calling that method:

```rust
    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        if Self::refuses_as_root(env) {
            return Vec::new();
        }
        let mut found = Vec::new();
        for candidate in Self::CANDIDATE_PATHS {
            let path = PathBuf::from(candidate);
            if !path.exists() {
                continue;
            }
            let spec = CommandSpec {
                program: path.clone(),
                args: vec!["--version".to_string()],
                env: self.env_vec(),
                cwd: None,
                timeout: Duration::from_secs(30),
            };
            let output = self.runner.run(spec, None, CancellationToken::new()).await;
            let version = match output {
                Ok(o) if o.exit_code == Some(0) => parse_version(&o.stdout),
                _ => None,
            };
            let unverified_version = self.meta.unverified_version(&version);
            let prefix = Self::prefix_for(&path);
            found.push(ManagerInstance {
                id: Self::instance_id_for(&prefix),
                adapter_id: self.meta.id.clone(),
                exe_path: path,
                prefix,
                scope: Scope::User,
                healthy: version.is_some(),
                version,
                unverified_version,
            });
        }
        found
    }
```

Fix the four pre-existing `ManagerInstance` test literals in this same file and add two new `detect()` tests. Run:
```bash
sed -i '' -E 's/^([[:space:]]*)healthy: true,$/\1healthy: true,\n\1unverified_version: None,/' crates/banager-core/src/adapters/brew/mod.rs
```
Then add, in `mod tests` right after `test_detect_finds_opt_homebrew_on_this_apple_silicon_mac`:

```rust
    #[tokio::test]
    async fn test_detect_finds_opt_homebrew_and_its_version_is_verified() {
        let runner = Arc::new(MockRunner::new());
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
        let adapter = BrewAdapter::new(runner);
        let env = HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/tmp"),
            euid: 501,
        };
        let instances = adapter.detect(&env).await;
        assert_eq!(instances.len(), 1);
        assert!(
            instances[0].unverified_version.is_none(),
            "7.0.3 is listed in adapters/meta/brew.toml's verified_versions"
        );
    }

    #[tokio::test]
    async fn test_detect_flags_an_unverified_version_not_in_brew_toml() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "Homebrew 99.9.9\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let env = HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/tmp"),
            euid: 501,
        };
        let instances = adapter.detect(&env).await;
        assert_eq!(instances.len(), 1);
        assert_eq!(
            instances[0].unverified_version,
            Some("99.9.9".to_string()),
            "a version not listed in adapters/meta/brew.toml's verified_versions must be flagged"
        );
    }
```

Fix every other Rust `ManagerInstance` literal in the workspace (`session/mod.rs`'s `make_instance` helper, `ipc.rs`'s `FakeAdapter` instance literal, and the six `ops_*_test.rs` files). Run:
```bash
sed -i '' -E 's/^([[:space:]]*)healthy: true,$/\1healthy: true,\n\1unverified_version: None,/' \
  crates/banager-core/src/session/mod.rs \
  src-tauri/src/ipc.rs \
  crates/banager-core/tests/ops_semaphore_test.rs \
  crates/banager-core/tests/ops_cancel_test.rs \
  crates/banager-core/tests/ops_lock_test.rs \
  crates/banager-core/tests/ops_panic_test.rs \
  crates/banager-core/tests/ops_outcome_test.rs \
  crates/banager-core/tests/ops_summaries_test.rs
```
Then run `cargo fmt --all` to normalize indentation from all the sed edits in this step.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --workspace`
Expected: PASS — every Rust test green, including the five new/updated ones in `model.rs`, `adapters/mod.rs` and `brew/mod.rs`.

- [ ] **Step 5: Commit the Rust side**

```bash
git add crates/banager-core/src/model.rs crates/banager-core/src/adapters/mod.rs crates/banager-core/src/adapters/brew/mod.rs crates/banager-core/src/session/mod.rs src-tauri/src/ipc.rs crates/banager-core/tests/ops_semaphore_test.rs crates/banager-core/tests/ops_cancel_test.rs crates/banager-core/tests/ops_lock_test.rs crates/banager-core/tests/ops_panic_test.rs crates/banager-core/tests/ops_outcome_test.rs crates/banager-core/tests/ops_summaries_test.rs
git commit -m "$(cat <<'EOF'
feat(core): add ManagerInstance.unverified_version

AdapterMeta::unverified_version states the spec §4.1 rule once, and
BrewAdapter::detect() calls it to flag a detected version that is not
in adapters/meta/brew.toml's verified_versions.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 6: Add the field to the TypeScript mirror and fix existing fixtures**

Modify `src/lib/types.ts`:

```ts
export interface ManagerInstance {
  id: string;
  adapter_id: string;
  exe_path: string;
  prefix: string;
  scope: "User" | "System";
  version: string | null;
  healthy: boolean;
  unverified_version: string | null;
}
```

Fix the three existing `ManagerInstance` literals so `tsc`/`pnpm test` keep passing. Run:
```bash
sed -i '' -E 's/^([[:space:]]*)healthy: true,$/\1healthy: true,\n\1unverified_version: null,/' \
  src/pages/InstalledPage.test.tsx \
  src/lib/types.test.ts \
  src/App.test.tsx
```

- [ ] **Step 7: Run to verify it passes**

Run: `pnpm test && pnpm exec tsc -p tsconfig.json`
Expected: PASS — no new UI behaviour yet, so every existing test and the type check both stay green.

- [ ] **Step 8: Write the failing test for the unverified-version badge**

Modify `src/pages/InstalledPage.test.tsx` — add this test inside the `describe("InstalledPage", ...)` block (after the last existing test):

```tsx
  it("shows an unverified-version badge next to a source whose detected version is not verified", async () => {
    const unverifiedSnapshot: Snapshot = {
      ...snapshot,
      instances: [{ ...snapshot.instances[0], unverified_version: "99.9.9" }],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(unverifiedSnapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      return Promise.resolve(undefined);
    });

    const { findByText } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    await findByText("Unverified version (99.9.9)");
  });
```

- [ ] **Step 9: Run to verify it fails**

Run: `pnpm exec vitest run src/pages/InstalledPage.test.tsx`
Expected: FAIL — `Unable to find an element with the text: Unverified version (99.9.9)` (`InstalledPage.tsx` does not render `unverified_version` yet, and the `installed.unverifiedVersion` i18n key does not exist yet either).

- [ ] **Step 10: Render the badge**

Modify `src/i18n/en.json` — add to the `"installed"` object (after `"showDependencies_other"`):
```json
  "unverifiedVersion": "Unverified version ({{version}})"
```

Modify `src/i18n/zh-CN.json` — add to the `"installed"` object (after `"showDependencies_other"`):
```json
  "unverifiedVersion": "未验证版本（{{version}}）"
```

Modify `src/pages/InstalledPage.tsx`:

```tsx
type ListItem =
  | { type: "group"; instanceId: string; label: string; unverifiedVersion: string | null }
  | { type: "artifact"; artifact: InstalledArtifact }
  | { type: "toggle"; instanceId: string; hiddenCount: number };
```

In the `items` `useMemo`, when pushing the group entry:

```tsx
      result.push({
        type: "group",
        instanceId: instance.id,
        label: labelKey ? t(labelKey) : instance.adapter_id,
        unverifiedVersion: instance.unverified_version,
      });
```

In the render, for the group branch:

```tsx
                {item.type === "group" ? (
                  <p className="px-4 py-2 text-xs font-semibold uppercase text-[var(--color-muted)]">
                    {item.label}
                    {item.unverifiedVersion ? (
                      <span className="ml-2 normal-case text-[var(--color-danger)]">
                        {t("installed.unverifiedVersion", { version: item.unverifiedVersion })}
                      </span>
                    ) : null}
                  </p>
                ) : item.type === "toggle" ? (
```
(only the opening of the `group` branch changes; the `toggle`/`artifact` branches that follow are unchanged).

- [ ] **Step 11: Run to verify it passes**

Run: `pnpm test`
Expected: PASS — the new badge test passes, `src/i18n/completeness.test.ts` (the en/zh-CN key-parity test) stays green since both files gained the same `installed.unverifiedVersion` key, and every other existing test is unaffected.

- [ ] **Step 12: Commit the TypeScript side**

```bash
git add src/lib/types.ts src/pages/InstalledPage.tsx src/pages/InstalledPage.test.tsx src/lib/types.test.ts src/App.test.tsx src/i18n/en.json src/i18n/zh-CN.json
git commit -m "$(cat <<'EOF'
feat(ui): show an unverified-version badge on the Installed page

Renders ManagerInstance.unverified_version next to a source's group
header instead of leaving the new backend field unused by the UI.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: npm adapter

**Files:**
- Create: `crates/banager-core/src/adapters/npm.rs`
- Create: `adapters/meta/npm.toml`
- Modify: `crates/banager-core/src/adapters/mod.rs` (`pub mod npm;`, plus the shared `run_plan`/`second_token` helpers)
- Test: inline `#[cfg(test)]` modules in `crates/banager-core/src/adapters/npm.rs` (driven by the committed fixtures in `adapters/fixtures/npm/12.0.2/`) and `crates/banager-core/src/adapters/mod.rs`

**Interfaces:**
- Consumes: `Adapter` trait, `CheckOptions`, `Capabilities` (Task 2); `ManagerInstance.unverified_version` (Task 4); `crate::adapters::validate_package_name`, `crate::runner::{CommandRunner, CommandSpec, HostEnv, resolve_exe}` (pre-existing).
- Produces:
  ```rust
  // crates/banager-core/src/adapters/npm.rs (new; not the skeleton's own
  // names, introduced here to set the pattern the next five adapters copy)
  pub struct NpmAdapter { /* private */ }
  impl NpmAdapter {
      pub fn new(runner: Arc<dyn CommandRunner>) -> NpmAdapter;
      #[cfg(test)]
      fn with_prefix_writable_fn(self, f: fn(&std::path::Path) -> bool) -> NpmAdapter;
      // detect, inventory, check_updates, search, plan, execute, reconcile,
      // capabilities: same shapes as the Adapter trait.
  }
  ```
  Plus two shared helpers in `crates/banager-core/src/adapters/mod.rs`, introduced here because npm is the first of six adapters that would otherwise each carry a byte-identical copy of them (Tasks 6-10 call these and write neither):
  ```rust
  /// Runs a plan through the runner, streaming each line to the sink, and maps
  /// the result the way every adapter must: a clean exit is `Succeeded`, a
  /// cancelled or timed-out run is `Unconfirmed` (the operation may or may not
  /// have taken effect — only `reconcile` can say), and a non-zero exit is
  /// `Failed` carrying the last five stderr lines.
  pub async fn run_plan(
      runner: &Arc<dyn CommandRunner>,
      plan: &Plan,
      sink: Arc<dyn EventSink>,
      op_id: OpId,
      cancel: CancellationToken,
  ) -> Result<Outcome, AdapterError>;

  /// `"cargo 1.98.1 (…)"` -> `Some("1.98.1")`. Several tools print their version
  /// as the second whitespace-separated token; this is that rule, once.
  pub fn second_token(text: &str) -> Option<String>;
  ```
  No task in this slice consumes `NpmAdapter` yet — Task 11 (out of this slice) registers it with `Session`.

  `NpmAdapter::search` (and `parse_search`, and `adapters/fixtures/npm/12.0.2/search-jq.json`) has **no production caller in this phase**: `Adapter::search` is never invoked in `banager-core` or `src-tauri`, and `src-tauri/src/lib.rs` registers no `search` IPC command. It is built here because the phase's task list names search as this adapter's deliverable and because brew already carries the same unused method; the IPC command that will call it belongs to the discovery page, a later phase. This is a recorded decision, not an oversight.

- [ ] **Step 1: Write the failing fixture-driven parser tests**

Modify `crates/banager-core/src/adapters/mod.rs` — add `pub mod npm;` on its own line after `pub mod brew;`, so Step 2's run really compiles this file and really fails. (Not "alphabetically": Tasks 6-10 append `pipx`, `uv`, `pip`, `cargo`, `ollama` in that order, which is the order the phase introduces them, not alphabetical order. Whoever adds the eighth adapter should append it the same way.)

Create `crates/banager-core/src/adapters/npm.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ls_global_matches_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
            .expect("read adapters/fixtures/npm/12.0.2/ls-global.json");
        let artifacts = parse_ls_global(&json, "npm:/opt/homebrew/lib").expect("parse");
        assert_eq!(artifacts.len(), 6);
        let names: Vec<&str> = artifacts.iter().map(|a| a.key.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "@alisaitteke/photoshop-mcp",
                "@openai/codex",
                "corepack",
                "get-shit-done-cc",
                "npm",
                "zsxq-cli",
            ]
        );
        let npm_self = artifacts
            .iter()
            .find(|a| a.key.name == "npm")
            .expect("npm entry");
        assert_eq!(npm_self.version, "12.0.2");
        assert_eq!(npm_self.key.kind, ArtifactKind::Package);
    }

    #[test]
    fn parse_outdated_global_matches_the_recorded_fixture() {
        let json =
            std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/outdated-global.json")
                .expect("read adapters/fixtures/npm/12.0.2/outdated-global.json");
        let candidates = parse_outdated_global(&json, "npm:/opt/homebrew/lib").expect("parse");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "@alisaitteke/photoshop-mcp");
        assert_eq!(candidates[0].current, "1.7.15");
        assert_eq!(candidates[0].target, "1.7.17");
        assert_eq!(candidates[0].channel, UpdateChannel::Native);
    }

    #[test]
    fn parse_outdated_global_of_empty_stdout_is_no_updates() {
        // With nothing outdated npm prints either nothing at all or `{}`,
        // depending on version; both mean "no updates". Neither is committed
        // as a fixture, since there is nothing to record — but the parser
        // must not choke on either.
        let candidates = parse_outdated_global("", "npm:/opt/homebrew/lib").expect("parse");
        assert!(candidates.is_empty());
        assert!(parse_outdated_global("{}", "npm:/opt/homebrew/lib")
            .expect("parse")
            .is_empty());
    }

    #[test]
    fn parse_search_matches_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/search-jq.json")
            .expect("read adapters/fixtures/npm/12.0.2/search-jq.json");
        let hits = parse_search(&json, "npm").expect("parse");
        assert_eq!(hits.len(), 20);
        assert_eq!(hits[0].name, "jq");
        assert_eq!(
            hits[0].description.as_deref(),
            Some("Server-side jQuery wrapper for node.")
        );
        assert!(hits.iter().all(|h| h.adapter_id == "npm"));
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib adapters::npm`
Expected: FAIL to compile — `error[E0425]`/`error[E0433]`: cannot find function `parse_ls_global` (and `parse_outdated_global`, `parse_search`) in this scope, and cannot find `ArtifactKind`/`UpdateChannel`, since `npm.rs` holds only its test module so far. This is a real red state, not "0 tests matched": Step 1 already declared `pub mod npm;`.

- [ ] **Step 3: Implement the three parsers**

Rewrite `crates/banager-core/src/adapters/npm.rs` (keeping the `tests` module from Step 1 unchanged, appended below):

```rust
use crate::model::{
    ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, SearchHit, UpdateCandidate,
    UpdateChannel,
};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct LsGlobalRoot {
    #[serde(default)]
    dependencies: HashMap<String, LsGlobalDependency>,
}

#[derive(Debug, Deserialize)]
struct LsGlobalDependency {
    #[serde(default)]
    version: Option<String>,
}

/// Parses `npm ls -g --depth=0 --json`. The real, committed fixture
/// (`adapters/fixtures/npm/12.0.2/ls-global.json`) shows the top level is a
/// `dependencies` **object** keyed by package name, not an array — a parser
/// expecting an array silently sees zero packages instead of erroring.
/// Sorted by name for deterministic output (a `HashMap`'s own iteration
/// order is not).
fn parse_ls_global(
    json: &str,
    instance_id: &str,
) -> Result<Vec<InstalledArtifact>, crate::adapters::AdapterError> {
    let root: LsGlobalRoot = serde_json::from_str(json)
        .map_err(|e| crate::adapters::AdapterError::Parse(e.to_string()))?;
    let mut out: Vec<InstalledArtifact> = root
        .dependencies
        .into_iter()
        .map(|(name, dep)| InstalledArtifact {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Package,
                name: name.clone(),
            },
            display_name: name,
            version: dep.version.unwrap_or_default(),
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path: None,
            auto_updates: false,
        })
        .collect();
    out.sort_by(|a, b| a.key.name.cmp(&b.key.name));
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct OutdatedEntry {
    current: String,
    latest: String,
}

/// Parses `npm outdated -g --json`. npm exits 1 whenever it finds anything
/// outdated — the caller must still treat that stdout as the real result,
/// not an error (see the per-adapter contract table). Empty stdout (no
/// output at all, not even `{}`) means nothing is outdated.
fn parse_outdated_global(
    json: &str,
    instance_id: &str,
) -> Result<Vec<UpdateCandidate>, crate::adapters::AdapterError> {
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    let root: HashMap<String, OutdatedEntry> = serde_json::from_str(json)
        .map_err(|e| crate::adapters::AdapterError::Parse(e.to_string()))?;
    let mut out: Vec<UpdateCandidate> = root
        .into_iter()
        .map(|(name, entry)| UpdateCandidate {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Package,
                name: name.clone(),
            },
            current: entry.current,
            target: entry.latest,
            channel: UpdateChannel::Native,
            checkable: true,
            warnings: Vec::new(),
        })
        .collect();
    out.sort_by(|a, b| a.key.name.cmp(&b.key.name));
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct SearchEntry {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

/// Parses `npm search --json --searchlimit 20 {query}`.
fn parse_search(
    json: &str,
    adapter_id: &str,
) -> Result<Vec<SearchHit>, crate::adapters::AdapterError> {
    let entries: Vec<SearchEntry> = serde_json::from_str(json)
        .map_err(|e| crate::adapters::AdapterError::Parse(e.to_string()))?;
    Ok(entries
        .into_iter()
        .map(|e| SearchHit {
            adapter_id: adapter_id.to_string(),
            kind: ArtifactKind::Package,
            name: e.name,
            description: e.description,
        })
        .collect())
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib adapters::npm`
Expected: PASS — 4 tests ok (`parse_ls_global_matches_the_recorded_fixture`, `parse_outdated_global_matches_the_recorded_fixture`, `parse_outdated_global_of_empty_stdout_is_no_updates`, `parse_search_matches_the_recorded_fixture`).

- [ ] **Step 5: Commit the parsers**

```bash
git add crates/banager-core/src/adapters/mod.rs crates/banager-core/src/adapters/npm.rs
git commit -m "$(cat <<'EOF'
feat(npm): add fixture-driven parsers for ls/outdated/search

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 6: Write the failing tests for the full `NpmAdapter`**

Create `adapters/meta/npm.toml`:

```toml
schema_version = 1
id = "npm"
name = "npm"
kind = "package_manager"
platforms = ["macos"]
homepage = "https://www.npmjs.com"
verified_versions = ["12.0.2"]
```

Modify `crates/banager-core/src/adapters/npm.rs` — add these tests inside the existing `mod tests` block (after `parse_search_matches_the_recorded_fixture`):

```rust
    use crate::adapters::{Adapter, AdapterError, Capabilities, CheckOptions};
    use crate::events::VecSink;
    use crate::model::{ArtifactKey, CancelPolicy, OpKind, OpRequest, Outcome, Reconciled, ResourceLock, Scope};
    use crate::runner::{CommandOutput, HostEnv, MockRunner};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    fn test_instance() -> ManagerInstance {
        ManagerInstance {
            id: "npm:/opt/homebrew/lib".to_string(),
            adapter_id: "npm".to_string(),
            exe_path: PathBuf::from("/opt/homebrew/bin/npm"),
            prefix: PathBuf::from("/opt/homebrew/lib"),
            scope: Scope::User,
            version: Some("12.0.2".to_string()),
            healthy: true,
            unverified_version: None,
        }
    }

    fn fake_exe(dir: &std::path::Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"#!/bin/sh\n").expect("write fake npm executable");
        path
    }

    #[tokio::test]
    async fn test_detect_finds_npm_on_path_and_resolves_its_global_prefix() {
        let dir = std::env::temp_dir().join(format!(
            "banager-npm-detect-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let npm_path = fake_exe(&dir, "npm");
        let npm_path_str = npm_path.to_str().expect("utf8 path");

        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![npm_path_str, "prefix", "-g"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "/opt/homebrew\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec![npm_path_str, "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "12.0.2\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
        let env = HostEnv {
            path_dirs: vec![dir.clone()],
            home: PathBuf::from("/tmp"),
            euid: 501,
        };
        let instances = adapter.detect(&env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].id, "npm:/opt/homebrew");
        assert_eq!(instances[0].version, Some("12.0.2".to_string()));
        assert!(instances[0].healthy);
        assert!(
            instances[0].unverified_version.is_none(),
            "12.0.2 is verified in adapters/meta/npm.toml"
        );
    }

    #[tokio::test]
    async fn test_detect_flags_an_unverified_version() {
        let dir = std::env::temp_dir().join(format!(
            "banager-npm-detect-unverified-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let npm_path = fake_exe(&dir, "npm");
        let npm_path_str = npm_path.to_str().expect("utf8 path");

        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![npm_path_str, "prefix", "-g"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "/opt/homebrew\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec![npm_path_str, "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "99.9.9\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
        let env = HostEnv {
            path_dirs: vec![dir.clone()],
            home: PathBuf::from("/tmp"),
            euid: 501,
        };
        let instances = adapter.detect(&env).await;
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].unverified_version, Some("99.9.9".to_string()));
    }

    #[tokio::test]
    async fn test_detect_returns_empty_when_npm_is_not_on_path() {
        let runner = Arc::new(MockRunner::new());
        let adapter = NpmAdapter::new(runner.clone());
        let env = HostEnv {
            path_dirs: vec![PathBuf::from("/definitely/not/a/real/path")],
            home: PathBuf::from("/tmp"),
            euid: 501,
        };
        let instances = adapter.detect(&env).await;
        assert!(instances.is_empty());
        assert!(
            runner.calls().is_empty(),
            "no subprocess should run when npm isn't found"
        );
    }

    #[tokio::test]
    async fn test_inventory_accepts_exit_code_1() {
        let runner = Arc::new(MockRunner::new());
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
            .expect("read fixture");
        runner.respond(
            vec!["/opt/homebrew/bin/npm", "ls", "-g", "--depth=0", "--json"],
            CommandOutput {
                exit_code: Some(1),
                stdout: json,
                stderr: "npm warn config global".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
        let artifacts = adapter
            .inventory(&test_instance())
            .await
            .expect("exit 1 must still be parsed");
        assert_eq!(artifacts.len(), 6);
    }

    #[tokio::test]
    async fn test_check_updates_accepts_exit_code_1() {
        let runner = Arc::new(MockRunner::new());
        let json =
            std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/outdated-global.json")
                .expect("read fixture");
        runner.respond(
            vec!["/opt/homebrew/bin/npm", "outdated", "-g", "--json"],
            CommandOutput {
                exit_code: Some(1),
                stdout: json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("exit 1 means updates were found, not a failure");
        assert_eq!(candidates.len(), 1);
    }

    #[tokio::test]
    async fn test_search_matches_the_recorded_fixture_end_to_end() {
        let runner = Arc::new(MockRunner::new());
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/search-jq.json")
            .expect("read fixture");
        runner.respond(
            vec![
                "/opt/homebrew/bin/npm",
                "search",
                "--json",
                "--searchlimit",
                "20",
                "jq",
            ],
            CommandOutput {
                exit_code: Some(0),
                stdout: json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
        let hits = adapter.search(&test_instance(), "jq").await.expect("search");
        assert_eq!(hits.len(), 20);
        assert_eq!(hits[0].name, "jq");
    }

    #[tokio::test]
    async fn test_plan_install() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_writable_fn(|_| true);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.args, vec!["install", "-g", "jq"]);
        assert!(!plan.needs_password);
        assert_eq!(plan.locks, vec![ResourceLock(inst.id.clone())]);
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);
    }

    #[tokio::test]
    async fn test_plan_uninstall() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_writable_fn(|_| true);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.args, vec!["uninstall", "-g", "jq"]);
    }

    #[tokio::test]
    async fn test_plan_upgrade_targets_latest() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_writable_fn(|_| true);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.args, vec!["install", "-g", "jq@latest"]);
    }

    #[tokio::test]
    async fn test_plan_is_refused_when_the_prefix_is_not_writable() {
        let adapter =
            NpmAdapter::new(Arc::new(MockRunner::new())).with_prefix_writable_fn(|_| false);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        let result = adapter.plan(&inst, &req).await;
        match result {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_execute_streams_log_events_and_succeeds() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/npm", "install", "-g", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "added 1 package\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner).with_prefix_writable_fn(|_| true);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Package,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome = adapter
            .execute(&plan, sink.clone(), 1, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(sink.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_and_absent() {
        let runner = Arc::new(MockRunner::new());
        let json = std::fs::read_to_string("../../adapters/fixtures/npm/12.0.2/ls-global.json")
            .expect("read fixture");
        runner.respond(
            vec!["/opt/homebrew/bin/npm", "ls", "-g", "--depth=0", "--json"],
            CommandOutput {
                exit_code: Some(0),
                stdout: json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = NpmAdapter::new(runner);
        let inst = test_instance();
        let present = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Package,
                    name: "npm".to_string(),
                },
            )
            .await
            .expect("reconcile present");
        assert!(present.present);
        assert_eq!(present.version, Some("12.0.2".to_string()));

        let absent = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Package,
                    name: "does-not-exist".to_string(),
                },
            )
            .await
            .expect("reconcile absent");
        assert!(!absent.present);
    }

    #[test]
    fn test_capabilities_report_search_per_item_upgrade_and_uninstall() {
        let adapter = NpmAdapter::new(Arc::new(MockRunner::new()));
        let caps = adapter.capabilities();
        assert!(caps.search);
        assert!(caps.per_item_upgrade);
        assert!(caps.uninstall);
    }
```

Also add, to `crates/banager-core/src/adapters/mod.rs`'s existing `#[cfg(test)] mod tests` block, one test each for the two shared helpers this task introduces (they do not exist yet either, so these fail to compile alongside the npm ones):

```rust
    #[test]
    fn test_second_token_reads_the_version_out_of_a_labelled_version_line() {
        assert_eq!(
            second_token("cargo 1.98.1 (797e8a9bc 2026-08-05)\n"),
            Some("1.98.1".to_string())
        );
        assert_eq!(second_token("uv 0.12.17 (Homebrew)"), Some("0.12.17".to_string()));
        assert_eq!(second_token(""), None);
        assert_eq!(second_token("onlyoneword\n"), None);
    }

    #[tokio::test]
    async fn test_run_plan_maps_a_cancelled_run_to_unconfirmed_and_a_failure_to_the_last_stderr_lines(
    ) {
        use crate::events::VecSink;
        use crate::model::{ArtifactKind, CancelPolicy, OpKind, OpRequest, ResourceLock};
        use crate::runner::{CommandOutput, MockRunner};
        use std::path::PathBuf;
        use tokio_util::sync::CancellationToken;

        fn plan_for(args: Vec<&str>) -> Plan {
            Plan {
                request: OpRequest {
                    kind: OpKind::Install,
                    instance_id: "fake:1".to_string(),
                    artifact_kind: ArtifactKind::Package,
                    name: "jq".to_string(),
                },
                program: PathBuf::from("/bin/fake"),
                args: args.into_iter().map(|a| a.to_string()).collect(),
                env: Vec::new(),
                needs_password: false,
                locks: vec![ResourceLock("fake:1".to_string())],
                cancel_policy: CancelPolicy::KillThenReconcile,
                warnings: Vec::new(),
                affected: Vec::new(),
                timeout_secs: 60,
            }
        }

        let runner_raw = MockRunner::new();
        runner_raw.respond(
            vec!["/bin/fake", "cancelled"],
            CommandOutput {
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: true,
            },
        );
        runner_raw.respond(
            vec!["/bin/fake", "failed"],
            CommandOutput {
                exit_code: Some(2),
                stdout: String::new(),
                stderr: "l1\nl2\nl3\nl4\nl5\nl6\nl7".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let runner: Arc<dyn CommandRunner> = Arc::new(runner_raw);

        let sink = Arc::new(VecSink::new());
        assert_eq!(
            run_plan(
                &runner,
                &plan_for(vec!["cancelled"]),
                sink.clone(),
                1,
                CancellationToken::new()
            )
            .await
            .expect("run_plan"),
            Outcome::Unconfirmed
        );
        assert_eq!(
            run_plan(
                &runner,
                &plan_for(vec!["failed"]),
                sink,
                2,
                CancellationToken::new()
            )
            .await
            .expect("run_plan"),
            Outcome::Failed {
                exit_code: Some(2),
                summary: "l3\nl4\nl5\nl6\nl7".to_string(),
            }
        );
    }
```

- [ ] **Step 7: Run to verify it fails**

Run: `cargo test -p banager-core --lib adapters::`
Expected: FAIL to compile — `error[E0433]`/`error[E0412]`: cannot find struct/function `NpmAdapter` in this scope (referenced throughout the new npm tests; nothing outside the parser functions and their structs exists in `npm.rs` yet), and `error[E0425]`: cannot find function `second_token` / `run_plan` in `adapters/mod.rs`.

- [ ] **Step 8: Implement the two shared helpers, then `NpmAdapter`**

Modify `crates/banager-core/src/adapters/mod.rs` — add the two helpers Tasks 6-10 will reuse, after the `Adapter` trait definition. Their imports go in that file's existing `use` block: `use crate::events::{EventSink, OpId};`, `use crate::model::{Outcome, Plan};`, `use crate::runner::{CommandRunner, CommandSpec, LineCallback};`, `use std::sync::Arc;`, `use std::time::Duration;`, `use tokio_util::sync::CancellationToken;` (add only the ones not already there):

```rust
/// Runs a plan through the runner, streaming each line to the sink, and maps
/// the result the way every adapter must: a clean exit is `Succeeded`, a
/// cancelled or timed-out run is `Unconfirmed` (the operation may or may not
/// have taken effect — only `reconcile` can say), and a non-zero exit is
/// `Failed` carrying the last five stderr lines.
///
/// Every adapter's `execute()` is this function and nothing else. It lives
/// here so the cancelled/timed-out rule and the five-line summary can only
/// ever mean one thing; an earlier draft of this phase had six byte-identical
/// copies of it.
pub async fn run_plan(
    runner: &Arc<dyn CommandRunner>,
    plan: &Plan,
    sink: Arc<dyn EventSink>,
    op_id: OpId,
    cancel: CancellationToken,
) -> Result<Outcome, AdapterError> {
    let sink_for_line = sink.clone();
    let on_line: LineCallback = Arc::new(move |stream, line| {
        sink_for_line.emit(crate::events::OperationEvent::Log { op_id, stream, line });
    });
    let spec = CommandSpec {
        program: plan.program.clone(),
        args: plan.args.clone(),
        env: plan.env.clone(),
        cwd: None,
        timeout: Duration::from_secs(plan.timeout_secs),
    };
    let output = runner.run(spec, Some(on_line), cancel).await?;
    if output.cancelled || output.timed_out {
        return Ok(Outcome::Unconfirmed);
    }
    match output.exit_code {
        Some(0) => Ok(Outcome::Succeeded),
        code => {
            let stderr_lines: Vec<&str> = output.stderr.lines().collect();
            let start = stderr_lines.len().saturating_sub(5);
            Ok(Outcome::Failed {
                exit_code: code,
                summary: stderr_lines[start..].join("\n"),
            })
        }
    }
}

/// `"cargo 1.98.1 (…)"` -> `Some("1.98.1")`. Several tools (cargo, uv, pip)
/// print their version as the second whitespace-separated token of the first
/// line; this is that rule, once. Tools that print it differently — pipx's
/// bare `1.17.3`, Ollama's `ollama version is 0.34.1` — keep their own
/// parser.
pub fn second_token(text: &str) -> Option<String> {
    let mut parts = text.lines().next()?.split_whitespace();
    let _label = parts.next()?;
    parts.next().map(|token| token.to_string())
}
```

Modify `crates/banager-core/src/adapters/npm.rs` — add the adapter above the existing parser functions (keep `parse_ls_global`, `parse_outdated_global`, `parse_search`, and the `tests` module unchanged):

```rust
use crate::adapters::{
    run_plan, validate_package_name, Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions,
};
use crate::events::{EventSink, OpId};
use crate::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstallReason, InstalledArtifact, ManagerInstance,
    OpKind, OpRequest, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate,
    UpdateChannel,
};
use crate::runner::{resolve_exe, CommandOutput, CommandRunner, CommandSpec, HostEnv};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// The real check for `impl NpmAdapter::new`'s `prefix_writable_fn`
/// default: whether the current user can write to `prefix` (the global
/// `node_modules` directory `npm prefix -g` reports). A prefix owned by
/// another user (e.g. a system-wide npm) is read-only for this adapter —
/// see the per-adapter contract table's Notes column.
/// A search box takes free text, not a package name: `validate_package_name`
/// matches `^[A-Za-z0-9@._+/-]+$`, so it rejects any multi-word query
/// ("json parser") as an *invalid name*, which is both wrong and confusing.
/// That function exists to keep a path out of an argv; this one exists to
/// keep a query out of argv's flag namespace and to bound its length. npm
/// itself decides what matches.
fn validate_search_query(query: &str) -> Result<(), AdapterError> {
    let trimmed = query.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') || trimmed.len() > 200 {
        return Err(AdapterError::InvalidName(query.to_string()));
    }
    Ok(())
}

fn real_prefix_is_writable(prefix: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    match std::ffi::CString::new(prefix.as_os_str().as_bytes()) {
        Ok(c_path) => unsafe { libc::access(c_path.as_ptr(), libc::W_OK) == 0 },
        Err(_) => false,
    }
}

pub struct NpmAdapter {
    runner: Arc<dyn CommandRunner>,
    meta: AdapterMeta,
    /// How to decide whether `inst.prefix` is writable by the current user,
    /// gating install/uninstall/upgrade. Production always gets
    /// `real_prefix_is_writable`; tests inject a fixed answer via the
    /// `#[cfg(test)]`-only `with_prefix_writable_fn`, mirroring
    /// `BrewAdapter::with_euid_fn`.
    prefix_writable_fn: fn(&Path) -> bool,
}

impl NpmAdapter {
    pub const ENV: [(&'static str, &'static str); 3] = [
        ("NO_COLOR", "1"),
        ("npm_config_update_notifier", "false"),
        ("npm_config_fund", "false"),
    ];

    pub fn new(runner: Arc<dyn CommandRunner>) -> NpmAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../adapters/meta/npm.toml"))
            .expect("adapters/meta/npm.toml must parse");
        NpmAdapter {
            runner,
            meta,
            prefix_writable_fn: real_prefix_is_writable,
        }
    }

    #[cfg(test)]
    fn with_prefix_writable_fn(mut self, f: fn(&Path) -> bool) -> NpmAdapter {
        self.prefix_writable_fn = f;
        self
    }

    fn env_vec(&self) -> Vec<(String, String)> {
        Self::ENV
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn instance_id_for(prefix: &Path) -> String {
        format!("npm:{}", prefix.display())
    }

    async fn run_npm(
        &self,
        inst: &ManagerInstance,
        args: Vec<String>,
        timeout: Duration,
    ) -> Result<CommandOutput, AdapterError> {
        let spec = CommandSpec {
            program: inst.exe_path.clone(),
            args,
            env: self.env_vec(),
            cwd: None,
            timeout,
        };
        Ok(self
            .runner
            .run(spec, None, CancellationToken::new())
            .await?)
    }

    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let Some(exe_path) = resolve_exe("npm", env) else {
            return Vec::new();
        };
        let prefix_spec = CommandSpec {
            program: exe_path.clone(),
            args: vec!["prefix".to_string(), "-g".to_string()],
            env: self.env_vec(),
            cwd: None,
            timeout: Duration::from_secs(30),
        };
        let prefix_output = self
            .runner
            .run(prefix_spec, None, CancellationToken::new())
            .await;
        let prefix = match &prefix_output {
            Ok(o) if o.exit_code == Some(0) => PathBuf::from(o.stdout.trim()),
            _ => return Vec::new(),
        };
        let version_spec = CommandSpec {
            program: exe_path.clone(),
            args: vec!["--version".to_string()],
            env: self.env_vec(),
            cwd: None,
            timeout: Duration::from_secs(30),
        };
        let version_output = self
            .runner
            .run(version_spec, None, CancellationToken::new())
            .await;
        let version = match version_output {
            Ok(o) if o.exit_code == Some(0) => Some(o.stdout.trim().to_string()),
            _ => None,
        };
        let unverified_version = self.meta.unverified_version(&version);
        vec![ManagerInstance {
            id: Self::instance_id_for(&prefix),
            adapter_id: self.meta.id.clone(),
            exe_path,
            prefix,
            scope: Scope::User,
            healthy: version.is_some(),
            version,
            unverified_version,
        }]
    }

    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let output = self
            .run_npm(
                inst,
                vec![
                    "ls".to_string(),
                    "-g".to_string(),
                    "--depth=0".to_string(),
                    "--json".to_string(),
                ],
                Duration::from_secs(60),
            )
            .await?;
        // `npm ls -g --depth=0 --json` exits 1 for various non-fatal
        // reasons (e.g. peer dependency mismatches); accept 0 or 1 and
        // always parse stdout — see the per-adapter contract table.
        if output.exit_code != Some(0) && output.exit_code != Some(1) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        parse_ls_global(&output.stdout, &inst.id)
    }

    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        let output = self
            .run_npm(
                inst,
                vec![
                    "outdated".to_string(),
                    "-g".to_string(),
                    "--json".to_string(),
                ],
                Duration::from_secs(60),
            )
            .await?;
        // npm exits 1 whenever it finds anything outdated — a result, not a
        // failure. See the per-adapter contract table.
        if output.exit_code != Some(0) && output.exit_code != Some(1) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        parse_outdated_global(&output.stdout, &inst.id)
    }

    pub async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        validate_search_query(query)?;
        let output = self
            .run_npm(
                inst,
                vec![
                    "search".to_string(),
                    "--json".to_string(),
                    "--searchlimit".to_string(),
                    "20".to_string(),
                    query.to_string(),
                ],
                Duration::from_secs(30),
            )
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        parse_search(&output.stdout, &self.meta.id)
    }

    pub async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        if req.instance_id != inst.id {
            return Err(AdapterError::Refused(format!(
                "plan requested for instance {} but given instance {}",
                req.instance_id, inst.id
            )));
        }
        validate_package_name(&req.name)?;
        if !(self.prefix_writable_fn)(&inst.prefix) {
            return Err(AdapterError::Refused(format!(
                "{} is not writable; this npm install is read-only for the current user",
                inst.prefix.display()
            )));
        }
        let lock = ResourceLock(inst.id.clone());
        let args = match req.kind {
            OpKind::Install => vec!["install".to_string(), "-g".to_string(), req.name.clone()],
            OpKind::Uninstall => vec!["uninstall".to_string(), "-g".to_string(), req.name.clone()],
            OpKind::Upgrade => vec![
                "install".to_string(),
                "-g".to_string(),
                format!("{}@latest", req.name),
            ],
        };
        Ok(Plan {
            request: req.clone(),
            program: inst.exe_path.clone(),
            args,
            env: self.env_vec(),
            needs_password: false,
            locks: vec![lock],
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings: Vec::new(),
            affected: Vec::new(),
            timeout_secs: 600,
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
        match artifacts.into_iter().find(|a| a.key.name == key.name) {
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
impl Adapter for NpmAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            search: true,
            per_item_upgrade: true,
            upgrade_all: false,
            uninstall: true,
            background_check: true,
            cancel_safe: true,
        }
    }

    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        NpmAdapter::detect(self, env).await
    }

    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        NpmAdapter::inventory(self, inst).await
    }

    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        NpmAdapter::check_updates(self, inst, opts).await
    }

    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        NpmAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        NpmAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        NpmAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        NpmAdapter::reconcile(self, inst, key).await
    }
}
```

- [ ] **Step 9: Run to verify it passes**

Run: `cargo test -p banager-core --lib adapters::`
Expected: PASS — all 17 tests in `adapters::npm` (4 parser tests + 13 adapter tests), plus the 2 new `adapters::tests` tests for `second_token` and `run_plan`, plus `adapters::tests`' pre-existing tests. Also run `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` — Expected: both clean.

- [ ] **Step 10: Commit**

```bash
git add adapters/meta/npm.toml crates/banager-core/src/adapters/npm.rs crates/banager-core/src/adapters/mod.rs
git commit -m "$(cat <<'EOF'
feat(npm): add the npm global-package adapter and two shared helpers

Inventory, check_updates, search, and install/uninstall/upgrade over
`npm -g`, matching the recorded fixtures at
adapters/fixtures/npm/12.0.2/. Not yet registered with Session (Task
11). Also adds adapters::run_plan and adapters::second_token, which
the five adapters after this one call instead of each carrying their
own copy.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: pipx adapter

**Files:**
- Create: `crates/banager-core/src/adapters/pipx.rs`
- Create: `adapters/meta/pipx.toml`
- Modify: `crates/banager-core/src/adapters/mod.rs` (add `pub mod pipx;` after the existing adapter module declarations — after Task 5 this file reads `pub mod brew;` then `pub mod npm;`; add `pub mod pipx;` as the next line)
- Test: `crates/banager-core/src/adapters/pipx.rs` (inline `#[cfg(test)] mod tests`, matching `adapters/brew/mod.rs`'s convention of tests living beside the code they test)

**Interfaces:**
- Consumes: `crate::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions, validate_package_name}` (Task 1–2, authoritative); `crate::http::{HttpClient, HttpRequest, HttpResponse, MockHttpClient}` (Task 1, authoritative); `crate::model::{ArtifactKey, ArtifactKind, CancelPolicy, InstallReason, InstalledArtifact, ManagerInstance (with `unverified_version: Option<String>`, Task 4), OpKind, OpRequest, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate, UpdateChannel}`; `crate::runner::{resolve_exe, CommandOutput, CommandRunner, CommandSpec, HostEnv, LineCallback, MockRunner}`.
- Produces: `pub struct PipxAdapter`; `impl PipxAdapter { pub fn new(runner: Arc<dyn CommandRunner>, http: Arc<dyn HttpClient>) -> PipxAdapter }`; `impl Adapter for PipxAdapter`. Free functions (private to this module): `fn parse_version(text: &str) -> Option<String>`, `fn parse_list(json: &str, instance_id: &str) -> Result<Vec<InstalledArtifact>, AdapterError>`, `fn parse_outdated(text: &str, instance_id: &str) -> Vec<UpdateCandidate>`, `fn supports_native_outdated(version: &str) -> bool`. Later tasks (Task 11) consume `PipxAdapter::new` to register this adapter in `Session`.
- Reuses, never reimplements: `crate::adapters::run_plan` is this adapter's whole `execute()`, and `AdapterMeta::unverified_version` is its whole unverified-version rule (both from Task 5 / Task 4). `pipx --version` prints a bare version string, not a labelled one, so `parse_version` here is genuinely pipx-specific and does **not** use `crate::adapters::second_token`.
- Deferred, deliberately: `capabilities().upgrade_all` is `false`. `OpKind` is `Install | Uninstall | Upgrade` (`crates/banager-core/src/model.rs`) and has no upgrade-all variant, so there is no request `plan()` could ever receive for it; reporting `true` would advertise a capability nothing can exercise. `pipx upgrade-all` is out of scope for this phase — the per-adapter contract table's mention of it is a deliverable for whichever phase adds an upgrade-all `OpKind`.

- [ ] **Step 1: Write the failing tests for pipx's parsers**

Create `crates/banager-core/src/adapters/pipx.rs` with only its test module for now (the non-test code below does not exist yet, so every test that calls it will fail to compile):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_reads_the_bare_version_string() {
        // pipx's own `pipx --version` fixture is just the version number,
        // no "pipx" label in front (unlike `brew --version`'s "Homebrew
        // 7.0.3") — see adapters/fixtures/pipx/1.17.3/version.txt.
        assert_eq!(parse_version("1.17.3\n"), Some("1.17.3".to_string()));
    }

    #[test]
    fn test_parse_version_of_empty_output_is_none() {
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("\n"), None);
    }

    #[test]
    fn test_supports_native_outdated_thresholds_at_1_16() {
        assert!(supports_native_outdated("1.17.3"));
        assert!(supports_native_outdated("1.16.0"));
        assert!(!supports_native_outdated("1.15.9"));
        assert!(!supports_native_outdated("0.9.0"));
    }

    #[test]
    fn test_parse_list_from_the_recorded_fixture() {
        // cargo runs tests with cwd = crates/banager-core (see
        // adapters/mod.rs's own `test_from_toml_parses_the_committed_brew_meta_file`).
        let json = std::fs::read_to_string("../../adapters/fixtures/pipx/1.17.3/list.json")
            .expect("read pipx list.json fixture");
        let artifacts = parse_list(&json, "pipx").expect("parse pipx list.json");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].key.kind, ArtifactKind::Tool);
        assert_eq!(artifacts[0].key.name, "cowsay");
        assert_eq!(artifacts[0].version, "5.0");
        assert_eq!(artifacts[0].reason, InstallReason::Requested);
    }

    #[test]
    fn test_parse_outdated_from_the_recorded_fixture() {
        let text = std::fs::read_to_string("../../adapters/fixtures/pipx/1.17.3/list-outdated.txt")
            .expect("read pipx list-outdated.txt fixture");
        let candidates = parse_outdated(&text, "pipx");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "cowsay");
        assert_eq!(candidates[0].current, "5.0");
        assert_eq!(candidates[0].target, "6.1");
        assert_eq!(candidates[0].channel, UpdateChannel::Native);
        assert!(candidates[0].checkable);
    }

    #[test]
    fn test_parse_outdated_recognizes_the_no_upgrades_sentence() {
        // "An unmatched line means no updates, never an error" — the literal
        // sentence pipx prints when nothing is outdated (README trap #1).
        let candidates = parse_outdated("pipx found no available upgrades.\n", "pipx");
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_parse_outdated_skips_unmatched_lines_instead_of_erroring() {
        // Edge case the fixture cannot show: pipx sometimes intersperses a
        // warning line above/below the real ones. An unmatched line must be
        // skipped, not treated as an error or a malformed candidate.
        let text = "WARNING: some pipx warning\ncowsay: 5.0 -> 6.1\n";
        let candidates = parse_outdated(text, "pipx");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "cowsay");
    }
}
```

Add `pub mod pipx;` to `crates/banager-core/src/adapters/mod.rs` right after the existing adapter module declarations (`pub mod brew;` and, after Task 5, `pub mod npm;`).

- [ ] **Step 2: Run the tests and confirm they fail to compile**

Run: `cargo test -p banager-core adapters::pipx::`
Expected: FAIL to compile — `cannot find function `parse_version` in module `adapters::pipx`` (and the same for `parse_list`, `parse_outdated`, `supports_native_outdated`, and `ArtifactKind`/`InstallReason`/`UpdateChannel` not yet imported), since only the test module exists so far.

- [ ] **Step 3: Implement pipx's parsers, `PipxAdapter` struct, `detect`, `inventory` and `check_updates`**

Prepend the following to `crates/banager-core/src/adapters/pipx.rs`, above the `#[cfg(test)] mod tests` block already there:

```rust
use crate::adapters::{
    run_plan, validate_package_name, Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions,
};
use crate::events::{EventSink, OpId};
use crate::http::{HttpClient, HttpRequest};
use crate::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstallReason, InstalledArtifact, ManagerInstance,
    OpKind, OpRequest, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate,
    UpdateChannel,
};
use crate::runner::{resolve_exe, CommandOutput, CommandRunner, CommandSpec, HostEnv};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Parses `pipx --version`'s output, which — unlike `brew --version`'s
/// "Homebrew 7.0.3" — is the bare version string with no label
/// (`adapters/fixtures/pipx/1.17.3/version.txt` is exactly `1.17.3\n`).
fn parse_version(text: &str) -> Option<String> {
    let v = text.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// True when `version` is pipx >= 1.16, the floor `pipx list --outdated`
/// needs to exist at all (per this phase's ruling — see
/// `adapters/fixtures/pipx/1.17.3/README.md`). Only major/minor are
/// compared; pipx has never shipped a patch-level `--outdated` gate.
fn supports_native_outdated(version: &str) -> bool {
    let mut parts = version.split('.');
    let major: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor) >= (1, 16)
}

#[derive(Debug, Deserialize)]
struct PipxListRoot {
    venvs: HashMap<String, PipxVenv>,
}

#[derive(Debug, Deserialize)]
struct PipxVenv {
    metadata: PipxMetadata,
}

#[derive(Debug, Deserialize)]
struct PipxMetadata {
    main_package: PipxMainPackage,
}

#[derive(Debug, Deserialize)]
struct PipxMainPackage {
    package: String,
    package_version: String,
}

/// Parses `pipx list --json`. The venv name (the JSON object's key under
/// `venvs`) is the tool's `ArtifactKey.name`; the installed version lives at
/// `venvs.<name>.metadata.main_package.package_version` (this phase's other
/// documented trap for pipx). `venvs` is a `HashMap`, so entries are sorted
/// by name before returning to keep output deterministic.
fn parse_list(json: &str, instance_id: &str) -> Result<Vec<InstalledArtifact>, AdapterError> {
    let root: PipxListRoot =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    let mut out: Vec<InstalledArtifact> = root
        .venvs
        .into_iter()
        .map(|(tool_name, venv)| InstalledArtifact {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Tool,
                name: tool_name,
            },
            display_name: venv.metadata.main_package.package,
            version: venv.metadata.main_package.package_version,
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path: None,
            auto_updates: false,
        })
        .collect();
    out.sort_by(|a, b| a.key.name.cmp(&b.key.name));
    Ok(out)
}

/// Parses `pipx list --outdated`'s prose output: one `name: old -> new`
/// line per outdated tool, and the literal sentence `pipx found no
/// available upgrades.` when there are none. An unmatched line is skipped,
/// never treated as an error (this phase's documented trap for pipx).
fn parse_outdated(text: &str, instance_id: &str) -> Vec<UpdateCandidate> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed == "pipx found no available upgrades." {
        return Vec::new();
    }
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line == "pipx found no available upgrades." {
            continue;
        }
        let Some((name, versions)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        let Some((old, new)) = versions.trim().split_once("->") else {
            continue;
        };
        let (old, new) = (old.trim(), new.trim());
        if name.is_empty() || old.is_empty() || new.is_empty() {
            continue;
        }
        out.push(UpdateCandidate {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Tool,
                name: name.to_string(),
            },
            current: old.to_string(),
            target: new.to_string(),
            channel: UpdateChannel::Native,
            checkable: true,
            warnings: Vec::new(),
        });
    }
    out
}

#[derive(Debug, Deserialize)]
struct PyPiResponse {
    info: PyPiInfo,
}

#[derive(Debug, Deserialize)]
struct PyPiInfo {
    version: String,
}

pub struct PipxAdapter {
    runner: Arc<dyn CommandRunner>,
    http: Arc<dyn HttpClient>,
    meta: AdapterMeta,
}

impl PipxAdapter {
    pub fn new(runner: Arc<dyn CommandRunner>, http: Arc<dyn HttpClient>) -> PipxAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../adapters/meta/pipx.toml"))
            .expect("adapters/meta/pipx.toml must parse");
        PipxAdapter { runner, http, meta }
    }

    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let Some(exe_path) = resolve_exe("pipx", env) else {
            return Vec::new();
        };
        let output = self
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
        let version = match output {
            Ok(o) if o.exit_code == Some(0) => parse_version(&o.stdout),
            _ => None,
        };
        let prefix = exe_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"));
        let unverified_version = self.meta.unverified_version(&version);
        vec![ManagerInstance {
            id: "pipx".to_string(),
            adapter_id: self.meta.id.clone(),
            exe_path,
            prefix,
            scope: Scope::User,
            healthy: version.is_some(),
            version,
            unverified_version,
        }]
    }

    async fn run_pipx(
        &self,
        inst: &ManagerInstance,
        args: Vec<String>,
        timeout: Duration,
    ) -> Result<CommandOutput, AdapterError> {
        let spec = CommandSpec {
            program: inst.exe_path.clone(),
            args,
            env: Vec::new(),
            cwd: None,
            timeout,
        };
        Ok(self
            .runner
            .run(spec, None, CancellationToken::new())
            .await?)
    }

    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let output = self
            .run_pipx(inst, vec!["list".to_string(), "--json".to_string()], Duration::from_secs(60))
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        parse_list(&output.stdout, &inst.id)
    }

    async fn latest_pypi_version(&self, name: &str) -> Result<String, String> {
        let resp = self
            .http
            .send(HttpRequest {
                method: "GET",
                url: format!("https://pypi.org/pypi/{name}/json"),
                headers: Vec::new(),
                timeout: Duration::from_secs(30),
            })
            .await
            .map_err(|e| format!("PyPI request failed: {e}"))?;
        if resp.status != 200 {
            return Err(format!("PyPI returned status {}", resp.status));
        }
        let parsed: PyPiResponse = serde_json::from_str(&resp.body)
            .map_err(|e| format!("could not parse PyPI response: {e}"))?;
        Ok(parsed.info.version)
    }

    /// Below pipx 1.16 there is no `pipx list --outdated`, so each installed
    /// tool is looked up individually on PyPI. A per-package failure (network
    /// down, package removed from PyPI) becomes a `checkable: false`
    /// candidate for just that tool, never a hard error for the whole check.
    async fn check_outdated_via_pypi(&self, installed: &[InstalledArtifact]) -> Vec<UpdateCandidate> {
        let mut out = Vec::new();
        for artifact in installed {
            match self.latest_pypi_version(&artifact.key.name).await {
                Ok(latest) if latest != artifact.version => out.push(UpdateCandidate {
                    key: artifact.key.clone(),
                    current: artifact.version.clone(),
                    target: latest,
                    channel: UpdateChannel::Registry,
                    checkable: true,
                    warnings: Vec::new(),
                }),
                Ok(_) => {}
                Err(reason) => out.push(UpdateCandidate {
                    key: artifact.key.clone(),
                    current: artifact.version.clone(),
                    target: artifact.version.clone(),
                    channel: UpdateChannel::Registry,
                    checkable: false,
                    warnings: vec![reason],
                }),
            }
        }
        out
    }

    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        let native = inst
            .version
            .as_deref()
            .map(supports_native_outdated)
            .unwrap_or(false);
        if native {
            let output = self
                .run_pipx(
                    inst,
                    vec!["list".to_string(), "--outdated".to_string()],
                    Duration::from_secs(60),
                )
                .await?;
            if output.exit_code != Some(0) {
                return Err(AdapterError::CommandFailed {
                    code: output.exit_code,
                    stderr: output.stderr,
                });
            }
            Ok(parse_outdated(&output.stdout, &inst.id))
        } else {
            let installed = self.inventory(inst).await?;
            Ok(self.check_outdated_via_pypi(&installed).await)
        }
    }
}
```

- [ ] **Step 4: Run the tests and confirm the parser tests pass**

Run: `cargo test -p banager-core adapters::pipx::`
Expected: PASS — all 7 tests in `adapters::pipx::tests` pass (the two `HostEnv`/`Adapter`-trait pieces are not exercised yet since `plan`/`execute`/`reconcile`/`Adapter for PipxAdapter` do not exist yet, but nothing calls them from this test module).

- [ ] **Step 5: Write the failing tests for `plan`, `execute`, `reconcile` and the PyPI fallback**

Append to the `#[cfg(test)] mod tests` block in `crates/banager-core/src/adapters/pipx.rs` (inside the existing `mod tests { use super::*; ... }`, after the tests already there):

```rust
    use crate::events::VecSink;
    use crate::http::{HttpResponse, MockHttpClient};
    use crate::runner::MockRunner;

    fn test_instance() -> ManagerInstance {
        ManagerInstance {
            id: "pipx".to_string(),
            adapter_id: "pipx".to_string(),
            exe_path: PathBuf::from("/opt/homebrew/bin/pipx"),
            prefix: PathBuf::from("/opt/homebrew/bin"),
            scope: Scope::User,
            version: Some("1.17.3".to_string()),
            healthy: true,
            unverified_version: None,
        }
    }

    #[tokio::test]
    async fn test_detect_finds_pipx_via_an_isolated_path_dir_and_marks_an_unverified_version() {
        // A dedicated temp directory used only as a fake PATH entry — never
        // a real system path — so this test cannot collide with, depend on,
        // or modify anything actually installed on the machine running it.
        let tmp_dir = std::env::temp_dir().join(format!(
            "banager-pipx-detect-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp_dir).expect("create temp PATH dir");
        let exe_path = tmp_dir.join("pipx");
        std::fs::write(&exe_path, b"#!/bin/sh\n").expect("write fake pipx executable");

        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![exe_path.to_str().expect("utf8 temp path"), "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "9.9.9\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let env = HostEnv {
            path_dirs: vec![tmp_dir.clone()],
            home: PathBuf::from("/tmp"),
            euid: 501,
        };
        let adapter = PipxAdapter::new(runner, Arc::new(MockHttpClient::new()));
        let instances = adapter.detect(&env).await;
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].version, Some("9.9.9".to_string()));
        assert_eq!(instances[0].unverified_version, Some("9.9.9".to_string()));

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_unverified_version_is_none_for_a_verified_version() {
        // The rule itself lives on AdapterMeta (Task 4) and is tested there;
        // this asserts pipx's own meta file pins the version the fixtures
        // were recorded against, so a future meta edit cannot silently start
        // flagging a healthy install.
        let adapter = PipxAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        assert_eq!(
            adapter.meta.unverified_version(&Some("1.17.3".to_string())),
            None
        );
    }

    #[tokio::test]
    async fn test_check_updates_uses_native_list_outdated_when_pipx_is_recent_enough() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--outdated"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "cowsay: 5.0 -> 6.1\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = PipxAdapter::new(runner, Arc::new(MockHttpClient::new()));
        let inst = test_instance(); // version 1.17.3, >= the 1.16 floor
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "cowsay");
        assert_eq!(candidates[0].channel, UpdateChannel::Native);
    }

    #[tokio::test]
    async fn test_check_updates_falls_back_to_pypi_below_pipx_1_16() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--json"],
            CommandOutput {
                exit_code: Some(0),
                stdout: r#"{"venvs":{"cowsay":{"metadata":{"main_package":{"package":"cowsay","package_version":"5.0"}}}}}"#.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "https://pypi.org/pypi/cowsay/json",
            HttpResponse {
                status: 200,
                body: r#"{"info":{"version":"6.1"}}"#.to_string(),
            },
        );
        let adapter = PipxAdapter::new(runner, http.clone());
        let mut inst = test_instance();
        inst.version = Some("1.10.0".to_string()); // below the 1.16 floor
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "cowsay");
        assert_eq!(candidates[0].target, "6.1");
        assert_eq!(candidates[0].channel, UpdateChannel::Registry);
        assert_eq!(http.calls(), vec!["https://pypi.org/pypi/cowsay/json".to_string()]);
    }

    #[tokio::test]
    async fn test_check_updates_via_pypi_marks_a_failed_lookup_as_uncheckable() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--json"],
            CommandOutput {
                exit_code: Some(0),
                stdout: r#"{"venvs":{"cowsay":{"metadata":{"main_package":{"package":"cowsay","package_version":"5.0"}}}}}"#.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let http = Arc::new(MockHttpClient::new());
        http.fail("https://pypi.org/pypi/cowsay/json", "connection refused");
        let adapter = PipxAdapter::new(runner, http);
        let mut inst = test_instance();
        inst.version = Some("1.10.0".to_string());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates should not fail outright on one bad lookup");
        assert_eq!(candidates.len(), 1);
        assert!(!candidates[0].checkable);
        assert_eq!(candidates[0].current, "5.0");
    }

    #[tokio::test]
    async fn test_plan_refuses_when_request_instance_id_does_not_match_given_instance() {
        let runner = Arc::new(MockRunner::new());
        let adapter = PipxAdapter::new(runner.clone(), Arc::new(MockHttpClient::new()));
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "not-pipx".to_string(),
            artifact_kind: ArtifactKind::Tool,
            name: "cowsay".to_string(),
        };
        let result = PipxAdapter::plan(&adapter, &inst, &req).await;
        match result {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_plan_install_uninstall_upgrade_build_the_expected_argv() {
        let adapter = PipxAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance();
        for (kind, expected) in [
            (OpKind::Install, vec!["install", "cowsay"]),
            (OpKind::Uninstall, vec!["uninstall", "cowsay"]),
            (OpKind::Upgrade, vec!["upgrade", "cowsay"]),
        ] {
            let req = OpRequest {
                kind,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Tool,
                name: "cowsay".to_string(),
            };
            let plan = PipxAdapter::plan(&adapter, &inst, &req).await.expect("plan");
            assert_eq!(plan.args, expected);
            assert!(!plan.needs_password);
        }
    }

    #[tokio::test]
    async fn test_execute_streams_log_events_and_succeeds() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "install", "cowsay"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "installed cowsay\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = PipxAdapter::new(runner, Arc::new(MockHttpClient::new()));
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Tool,
            name: "cowsay".to_string(),
        };
        let plan = PipxAdapter::plan(&adapter, &inst, &req).await.expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome = PipxAdapter::execute(&adapter, &plan, sink.clone(), 1, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(sink.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_and_absent() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/pipx", "list", "--json"],
            CommandOutput {
                exit_code: Some(0),
                stdout: r#"{"venvs":{"cowsay":{"metadata":{"main_package":{"package":"cowsay","package_version":"5.0"}}}}}"#.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = PipxAdapter::new(runner, Arc::new(MockHttpClient::new()));
        let inst = test_instance();
        let present = PipxAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey { instance_id: inst.id.clone(), kind: ArtifactKind::Tool, name: "cowsay".to_string() },
        )
        .await
        .expect("reconcile present");
        assert!(present.present);
        assert_eq!(present.version, Some("5.0".to_string()));
        let absent = PipxAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey { instance_id: inst.id.clone(), kind: ArtifactKind::Tool, name: "missing".to_string() },
        )
        .await
        .expect("reconcile absent");
        assert!(!absent.present);
    }

    #[tokio::test]
    async fn test_search_is_unsupported() {
        let adapter = PipxAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance();
        let result = <PipxAdapter as Adapter>::search(&adapter, &inst, "cowsay").await;
        assert!(matches!(result, Err(AdapterError::Unsupported(_))));
    }
```

- [ ] **Step 6: Run the tests and confirm they fail to compile**

Run: `cargo test -p banager-core adapters::pipx::`
Expected: FAIL to compile — `no function or associated item named `plan` found for struct `PipxAdapter`` (and the same for `execute`, `reconcile`, and `<PipxAdapter as Adapter>::search`), since `plan`/`execute`/`reconcile`/`impl Adapter for PipxAdapter` do not exist yet.

- [ ] **Step 7: Implement `plan`, `execute`, `reconcile`, `capabilities`/`search`, the meta file, and the `Adapter` impl**

Append to `crates/banager-core/src/adapters/pipx.rs`, directly below the `check_updates` method inside `impl PipxAdapter { ... }` (before its closing brace):

```rust
    pub async fn search(
        &self,
        _inst: &ManagerInstance,
        _query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        Err(AdapterError::Unsupported(
            "pipx has no search command; browse PyPI directly".to_string(),
        ))
    }

    pub async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        if req.instance_id != inst.id {
            return Err(AdapterError::Refused(format!(
                "plan requested for instance {} but given instance {}",
                req.instance_id, inst.id
            )));
        }
        validate_package_name(&req.name)?;
        let lock = ResourceLock(inst.id.clone());
        let args = match req.kind {
            OpKind::Install => vec!["install".to_string(), req.name.clone()],
            OpKind::Uninstall => vec!["uninstall".to_string(), req.name.clone()],
            OpKind::Upgrade => vec!["upgrade".to_string(), req.name.clone()],
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
            timeout_secs: 600,
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
            Some(a) => Ok(Reconciled { present: true, version: Some(a.version) }),
            None => Ok(Reconciled { present: false, version: None }),
        }
    }
```

Then add the trait forwarding block below `impl PipxAdapter { ... }`:

```rust
#[async_trait]
impl Adapter for PipxAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // No upgrade-all: OpKind has no variant for it, so `plan()`
            // could never receive such a request (see this task's
            // Interfaces block).
            search: false,
            per_item_upgrade: true,
            upgrade_all: false,
            uninstall: true,
            background_check: true,
            cancel_safe: true,
        }
    }

    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        PipxAdapter::detect(self, env).await
    }

    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        PipxAdapter::inventory(self, inst).await
    }

    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        PipxAdapter::check_updates(self, inst, opts).await
    }

    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        PipxAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        PipxAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        PipxAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        PipxAdapter::reconcile(self, inst, key).await
    }
}
```

Create `adapters/meta/pipx.toml`:

```toml
schema_version = 1
id = "pipx"
name = "pipx"
kind = "package_manager"
platforms = ["macos"]
homepage = "https://pipx.pypa.io"
verified_versions = ["1.17.3"]
```

- [ ] **Step 8: Run the tests and confirm they pass**

Run: `cargo test -p banager-core adapters::pipx::`
Expected: PASS — all tests in `adapters::pipx::tests` pass (17 tests total across steps 1 and 5).

- [ ] **Step 9: Run the full workspace gate**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: all three commands exit 0 — no formatting diffs, no clippy warnings, and every test in the workspace (the pre-existing 125+ plus this task's new ones) passes.

- [ ] **Step 10: Commit**

```bash
git add crates/banager-core/src/adapters/pipx.rs crates/banager-core/src/adapters/mod.rs adapters/meta/pipx.toml
git commit -m "$(cat <<'EOF'
feat(adapters): add pipx adapter with pre-1.16 PyPI fallback

pipx list --outdated is prose, not JSON, and only exists from pipx
1.16 onward; below that this checks each installed tool against PyPI
directly through HttpClient, degrading a single failed lookup to a
checkable:false candidate instead of failing the whole check.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: uv adapter

**Files:**
- Create: `crates/banager-core/src/adapters/uv.rs`
- Create: `adapters/meta/uv.toml`
- Modify: `crates/banager-core/src/adapters/mod.rs` (add `pub mod uv;` after `pub mod pipx;`)
- Test: `crates/banager-core/src/adapters/uv.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `crate::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions, validate_package_name}`; `crate::model::{ArtifactKey, ArtifactKind, CancelPolicy, InstallReason, InstalledArtifact, ManagerInstance, OpKind, OpRequest, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate, UpdateChannel}`; `crate::runner::{resolve_exe, CommandOutput, CommandRunner, CommandSpec, HostEnv, LineCallback, MockRunner}`. No `HttpClient` — uv's own subprocess is the only network path this adapter touches.
- Produces: `pub struct UvAdapter`; `impl UvAdapter { pub fn new(runner: Arc<dyn CommandRunner>) -> UvAdapter }`; `impl Adapter for UvAdapter`. Private free functions: `fn parse_tool_list_show_paths(text: &str, instance_id: &str) -> Vec<InstalledArtifact>`, `fn parse_tool_list_outdated(text: &str, instance_id: &str) -> Vec<UpdateCandidate>`.
- Reuses, never reimplements: `crate::adapters::second_token` parses `uv --version`'s "uv X.Y.Z (…)" line (this adapter defines no `parse_version` of its own), `crate::adapters::run_plan` is its whole `execute()`, and `AdapterMeta::unverified_version` is its whole unverified-version rule.
- Deferred, deliberately: `capabilities().upgrade_all` is `false`, for the same reason as pipx — `OpKind` has no upgrade-all variant, so no such request can reach `plan()`.

- [ ] **Step 1: Write the failing tests for uv's text parsers**

Create `crates/banager-core/src/adapters/uv.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_second_token_reads_uvs_recorded_version_line() {
        // adapters/fixtures/uv/0.12.17/version.txt:
        // "uv 0.12.17 (Homebrew 2026-09-18 aarch64-apple-darwin)"
        // The rule is crate::adapters::second_token (Task 5); this pins it
        // against uv's real recorded output.
        assert_eq!(
            second_token("uv 0.12.17 (Homebrew 2026-09-18 aarch64-apple-darwin)\n"),
            Some("0.12.17".to_string())
        );
    }

    #[test]
    fn test_parse_tool_list_show_paths_from_the_recorded_fixture() {
        let text = std::fs::read_to_string(
            "../../adapters/fixtures/uv/0.12.17/tool-list-show-paths.txt",
        )
        .expect("read uv tool-list-show-paths.txt fixture");
        let artifacts = parse_tool_list_show_paths(&text, "uv");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].key.kind, ArtifactKind::Tool);
        assert_eq!(artifacts[0].key.name, "ruff");
        assert_eq!(artifacts[0].version, "0.15.0");
        assert_eq!(
            artifacts[0].path,
            Some(PathBuf::from("/Users/brulek/.local/share/uv/tools/ruff"))
        );
    }

    #[test]
    fn test_parse_tool_list_show_paths_of_no_tools_installed_is_empty() {
        // Not in the recorded fixture (that machine has ruff installed) but
        // documented in adapters/fixtures/uv/0.12.17/README.md.
        assert!(parse_tool_list_show_paths("No tools installed\n", "uv").is_empty());
    }

    #[test]
    fn test_parse_tool_list_outdated_from_the_recorded_fixture() {
        let text = std::fs::read_to_string(
            "../../adapters/fixtures/uv/0.12.17/tool-list-outdated.txt",
        )
        .expect("read uv tool-list-outdated.txt fixture");
        let candidates = parse_tool_list_outdated(&text, "uv");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "ruff");
        assert_eq!(candidates[0].current, "0.15.0");
        assert_eq!(candidates[0].target, "0.16.8");
        assert_eq!(candidates[0].channel, UpdateChannel::Native);
    }

    #[test]
    fn test_parse_tool_list_outdated_of_empty_output_is_empty() {
        // "uv tool list --outdated prints nothing at all" when nothing is
        // outdated — no message, not even a newline (this phase's
        // documented trap for uv).
        assert!(parse_tool_list_outdated("", "uv").is_empty());
    }
}
```

Add `pub mod uv;` to `crates/banager-core/src/adapters/mod.rs` right after `pub mod pipx;`.

- [ ] **Step 2: Run the tests and confirm they fail to compile**

Run: `cargo test -p banager-core adapters::uv::`
Expected: FAIL to compile — `cannot find function `parse_tool_list_show_paths` in module `adapters::uv`` (and the same for `parse_tool_list_outdated`, plus `second_token`/`PathBuf`/`ArtifactKind`/`UpdateChannel` not yet imported).

- [ ] **Step 3: Implement uv's parsers, `UvAdapter` struct, `detect`, `inventory` and `check_updates`**

Prepend to `crates/banager-core/src/adapters/uv.rs`:

```rust
use crate::adapters::{
    run_plan, second_token, validate_package_name, Adapter, AdapterError, AdapterMeta,
    Capabilities, CheckOptions,
};
use crate::events::{EventSink, OpId};
use crate::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstallReason, InstalledArtifact, ManagerInstance,
    OpKind, OpRequest, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate,
    UpdateChannel,
};
use crate::runner::{resolve_exe, CommandOutput, CommandRunner, CommandSpec, HostEnv};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Parses `uv tool list --show-paths`: one `name vX.Y.Z (path)` header line
/// per tool, followed by `- binary (path)` lines that this function skips
/// (the header alone has everything `InstalledArtifact` needs).
fn parse_tool_list_show_paths(text: &str, instance_id: &str) -> Vec<InstalledArtifact> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed == "No tools installed" {
        return Vec::new();
    }
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with("- ") || line.trim().is_empty() {
            continue;
        }
        let Some((name, rest)) = line.split_once(' ') else {
            continue;
        };
        let Some(rest) = rest.strip_prefix('v') else {
            continue;
        };
        let Some((version, path_part)) = rest.split_once(" (") else {
            continue;
        };
        let path = path_part.trim_end_matches(')');
        out.push(InstalledArtifact {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Tool,
                name: name.to_string(),
            },
            display_name: name.to_string(),
            version: version.to_string(),
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path: Some(PathBuf::from(path)),
            auto_updates: false,
        });
    }
    out
}

/// Parses `uv tool list --outdated`: `name vOLD [latest: NEW]` per outdated
/// tool, followed by its `- binary` lines (skipped). With nothing outdated
/// this command prints **nothing at all** — not a message, not a newline —
/// and with no tools installed at all it prints `No tools installed`; both
/// are treated as "no updates", never an error (this phase's documented
/// trap for uv).
fn parse_tool_list_outdated(text: &str, instance_id: &str) -> Vec<UpdateCandidate> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed == "No tools installed" {
        return Vec::new();
    }
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with("- ") || line.trim().is_empty() {
            continue;
        }
        let Some((name, rest)) = line.split_once(' ') else {
            continue;
        };
        let Some(rest) = rest.strip_prefix('v') else {
            continue;
        };
        let Some((old, bracket)) = rest.split_once(" [latest: ") else {
            continue;
        };
        let Some(new) = bracket.strip_suffix(']') else {
            continue;
        };
        out.push(UpdateCandidate {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Tool,
                name: name.to_string(),
            },
            current: old.to_string(),
            target: new.to_string(),
            channel: UpdateChannel::Native,
            checkable: true,
            warnings: Vec::new(),
        });
    }
    out
}

pub struct UvAdapter {
    runner: Arc<dyn CommandRunner>,
    meta: AdapterMeta,
}

impl UvAdapter {
    pub fn new(runner: Arc<dyn CommandRunner>) -> UvAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../adapters/meta/uv.toml"))
            .expect("adapters/meta/uv.toml must parse");
        UvAdapter { runner, meta }
    }

    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let Some(exe_path) = resolve_exe("uv", env) else {
            return Vec::new();
        };
        let output = self
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
        let version = match output {
            // `uv --version` prints "uv X.Y.Z (...)" — the shared
            // second-token rule (crate::adapters::second_token, Task 5).
            Ok(o) if o.exit_code == Some(0) => second_token(&o.stdout),
            _ => None,
        };
        let prefix = exe_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"));
        let unverified_version = self.meta.unverified_version(&version);
        vec![ManagerInstance {
            id: "uv".to_string(),
            adapter_id: self.meta.id.clone(),
            exe_path,
            prefix,
            scope: Scope::User,
            healthy: version.is_some(),
            version,
            unverified_version,
        }]
    }

    async fn run_uv(
        &self,
        inst: &ManagerInstance,
        args: Vec<String>,
        timeout: Duration,
    ) -> Result<CommandOutput, AdapterError> {
        let spec = CommandSpec {
            program: inst.exe_path.clone(),
            args,
            env: Vec::new(),
            cwd: None,
            timeout,
        };
        Ok(self
            .runner
            .run(spec, None, CancellationToken::new())
            .await?)
    }

    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let output = self
            .run_uv(
                inst,
                vec!["tool".to_string(), "list".to_string(), "--show-paths".to_string()],
                Duration::from_secs(60),
            )
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        Ok(parse_tool_list_show_paths(&output.stdout, &inst.id))
    }

    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        let output = self
            .run_uv(
                inst,
                vec!["tool".to_string(), "list".to_string(), "--outdated".to_string()],
                Duration::from_secs(60),
            )
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        Ok(parse_tool_list_outdated(&output.stdout, &inst.id))
    }
}
```

- [ ] **Step 4: Run the tests and confirm the parser tests pass**

Run: `cargo test -p banager-core adapters::uv::`
Expected: PASS — all 5 tests in `adapters::uv::tests` pass.

- [ ] **Step 5: Write the failing tests for `plan`, `execute`, `reconcile`**

Append inside the `#[cfg(test)] mod tests { use super::*; ... }` block in `crates/banager-core/src/adapters/uv.rs`:

```rust
    use crate::events::VecSink;
    use crate::runner::MockRunner;

    fn test_instance() -> ManagerInstance {
        ManagerInstance {
            id: "uv".to_string(),
            adapter_id: "uv".to_string(),
            exe_path: PathBuf::from("/opt/homebrew/bin/uv"),
            prefix: PathBuf::from("/opt/homebrew/bin"),
            scope: Scope::User,
            version: Some("0.12.17".to_string()),
            healthy: true,
            unverified_version: None,
        }
    }

    #[tokio::test]
    async fn test_check_updates_calls_tool_list_outdated_and_parses_the_fixture_output() {
        let text = std::fs::read_to_string(
            "../../adapters/fixtures/uv/0.12.17/tool-list-outdated.txt",
        )
        .expect("read uv tool-list-outdated.txt fixture");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/uv", "tool", "list", "--outdated"],
            CommandOutput {
                exit_code: Some(0),
                stdout: text,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = UvAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("check_updates");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "ruff");
    }

    #[tokio::test]
    async fn test_plan_refuses_when_request_instance_id_does_not_match_given_instance() {
        let adapter = UvAdapter::new(Arc::new(MockRunner::new()));
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "not-uv".to_string(),
            artifact_kind: ArtifactKind::Tool,
            name: "ruff".to_string(),
        };
        let result = UvAdapter::plan(&adapter, &inst, &req).await;
        match result {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_plan_install_uninstall_upgrade_build_the_expected_argv() {
        let adapter = UvAdapter::new(Arc::new(MockRunner::new()));
        let inst = test_instance();
        for (kind, expected) in [
            (OpKind::Install, vec!["tool", "install", "ruff"]),
            (OpKind::Uninstall, vec!["tool", "uninstall", "ruff"]),
            (OpKind::Upgrade, vec!["tool", "upgrade", "ruff"]),
        ] {
            let req = OpRequest {
                kind,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Tool,
                name: "ruff".to_string(),
            };
            let plan = UvAdapter::plan(&adapter, &inst, &req).await.expect("plan");
            assert_eq!(plan.args, expected);
            assert!(!plan.needs_password);
        }
    }

    #[tokio::test]
    async fn test_execute_streams_log_events_and_succeeds() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/uv", "tool", "install", "ruff"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "Installed ruff\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = UvAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Tool,
            name: "ruff".to_string(),
        };
        let plan = UvAdapter::plan(&adapter, &inst, &req).await.expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome = UvAdapter::execute(&adapter, &plan, sink.clone(), 1, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(sink.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_and_absent() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/uv", "tool", "list", "--show-paths"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "ruff v0.15.0 (/Users/brulek/.local/share/uv/tools/ruff)\n- ruff (/Users/brulek/.local/bin/ruff)\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = UvAdapter::new(runner);
        let inst = test_instance();
        let present = UvAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey { instance_id: inst.id.clone(), kind: ArtifactKind::Tool, name: "ruff".to_string() },
        )
        .await
        .expect("reconcile present");
        assert!(present.present);
        assert_eq!(present.version, Some("0.15.0".to_string()));
        let absent = UvAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey { instance_id: inst.id.clone(), kind: ArtifactKind::Tool, name: "missing".to_string() },
        )
        .await
        .expect("reconcile absent");
        assert!(!absent.present);
    }

    #[tokio::test]
    async fn test_search_is_unsupported() {
        let adapter = UvAdapter::new(Arc::new(MockRunner::new()));
        let inst = test_instance();
        let result = <UvAdapter as Adapter>::search(&adapter, &inst, "ruff").await;
        assert!(matches!(result, Err(AdapterError::Unsupported(_))));
    }
```

- [ ] **Step 6: Run the tests and confirm they fail to compile**

Run: `cargo test -p banager-core adapters::uv::`
Expected: FAIL to compile — `no function or associated item named `plan` found for struct `UvAdapter`` (and the same for `execute`/`reconcile`/`<UvAdapter as Adapter>::search`).

- [ ] **Step 7: Implement `plan`, `execute`, `reconcile`, `capabilities`/`search`, the meta file, and the `Adapter` impl**

Append to `impl UvAdapter { ... }` in `crates/banager-core/src/adapters/uv.rs`, below `check_updates`:

```rust
    pub async fn search(
        &self,
        _inst: &ManagerInstance,
        _query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        Err(AdapterError::Unsupported(
            "uv has no tool-search command; browse PyPI directly".to_string(),
        ))
    }

    pub async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        if req.instance_id != inst.id {
            return Err(AdapterError::Refused(format!(
                "plan requested for instance {} but given instance {}",
                req.instance_id, inst.id
            )));
        }
        validate_package_name(&req.name)?;
        let lock = ResourceLock(inst.id.clone());
        let args = match req.kind {
            OpKind::Install => vec!["tool".to_string(), "install".to_string(), req.name.clone()],
            OpKind::Uninstall => vec!["tool".to_string(), "uninstall".to_string(), req.name.clone()],
            OpKind::Upgrade => vec!["tool".to_string(), "upgrade".to_string(), req.name.clone()],
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
            timeout_secs: 600,
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
            Some(a) => Ok(Reconciled { present: true, version: Some(a.version) }),
            None => Ok(Reconciled { present: false, version: None }),
        }
    }
```

Then add the trait forwarding block below `impl UvAdapter { ... }`:

```rust
#[async_trait]
impl Adapter for UvAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // No upgrade-all: OpKind has no variant for it (see this task's
            // Interfaces block).
            search: false,
            per_item_upgrade: true,
            upgrade_all: false,
            uninstall: true,
            background_check: true,
            cancel_safe: true,
        }
    }

    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        UvAdapter::detect(self, env).await
    }

    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        UvAdapter::inventory(self, inst).await
    }

    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        UvAdapter::check_updates(self, inst, opts).await
    }

    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        UvAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        UvAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        UvAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        UvAdapter::reconcile(self, inst, key).await
    }
}
```

Create `adapters/meta/uv.toml`:

```toml
schema_version = 1
id = "uv"
name = "uv"
kind = "package_manager"
platforms = ["macos"]
homepage = "https://docs.astral.sh/uv/"
verified_versions = ["0.12.17"]
```

- [ ] **Step 8: Run the tests and confirm they pass**

Run: `cargo test -p banager-core adapters::uv::`
Expected: PASS — all tests in `adapters::uv::tests` pass (11 tests total across steps 1 and 5).

- [ ] **Step 9: Run the full workspace gate**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: all three commands exit 0.

- [ ] **Step 10: Commit**

```bash
git add crates/banager-core/src/adapters/uv.rs crates/banager-core/src/adapters/mod.rs adapters/meta/uv.toml
git commit -m "$(cat <<'EOF'
feat(adapters): add uv adapter with text-based tool-list parsing

uv tool list --outdated prints nothing at all when nothing is
outdated (no message, no newline) and "No tools installed" with
none installed at all; both are "no updates", never an error.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: pip adapter, read-only

**Files:**
- Create: `crates/banager-core/src/adapters/pip.rs`
- Create: `adapters/meta/pip.toml`
- Modify: `crates/banager-core/src/adapters/mod.rs` (add `pub mod pip;` after `pub mod uv;`)
- Test: `crates/banager-core/src/adapters/pip.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `crate::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions}` (no `validate_package_name` — pip never builds a write argv); `crate::model::{ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, ManagerInstance, OpRequest, Outcome, Plan, Reconciled, Scope, SearchHit, UpdateCandidate, UpdateChannel}`; `crate::runner::{resolve_exe, CommandOutput, CommandRunner, CommandSpec, HostEnv, LineCallback, MockRunner}`.
- Produces: `pub struct PipAdapter`; `impl PipAdapter { pub const CANDIDATE_INTERPRETERS: [&'static str; 7]; pub fn new(runner: Arc<dyn CommandRunner>) -> PipAdapter }`; `impl Adapter for PipAdapter` whose `plan()` always returns `Err(AdapterError::Unsupported(_))` and whose `capabilities()` has `per_item_upgrade: false, upgrade_all: false, uninstall: false`. Those `Capabilities` values are documentation and a backend guard, **not** the UI's signal: `Adapter::capabilities()` has no call site anywhere in the workspace and `Capabilities` never crosses IPC, so Task 12's read-only note is driven by a hard-coded `READ_ONLY_ADAPTER_IDS = new Set(["pip"])` in `InstalledPage.tsx` instead. What this task owes Task 12 is only that no pip artifact can ever produce a runnable `Plan`; Task 12 owns the `SourceNotice.tsx` copy and the decision of which adapter ids show it.
- Reuses, never reimplements: `crate::adapters::second_token` parses `{python} -m pip --version` (this adapter defines no `parse_pip_version` of its own) and `AdapterMeta::unverified_version` is its whole unverified-version rule. It does **not** use `crate::adapters::run_plan`: pip never produces a `Plan`, so its `execute()` is unreachable and refuses (see Step 7).

- [ ] **Step 1: Write the failing tests for pip's parsers**

Create `crates/banager-core/src/adapters/pip.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_second_token_reads_pips_recorded_version_line() {
        // adapters/fixtures/pip/26.2.1/version.txt:
        // "pip 26.2.1 from /opt/homebrew/lib/python3.14/site-packages/pip (python 3.14)"
        // The rule is crate::adapters::second_token (Task 5); this pins it
        // against pip's real recorded output, and documents that the version
        // wanted here is pip's own, not the interpreter's Python version.
        assert_eq!(
            second_token(
                "pip 26.2.1 from /opt/homebrew/lib/python3.14/site-packages/pip (python 3.14)\n"
            ),
            Some("26.2.1".to_string())
        );
    }

    #[test]
    fn test_parse_pip_list_from_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/pip/26.2.1/list.json")
            .expect("read pip list.json fixture");
        let packages = parse_pip_list(&json).expect("parse pip list.json");
        assert_eq!(packages.len(), 7);
        assert!(packages.iter().any(|p| p.name == "PyYAML" && p.version == "6.0.3"));
    }

    #[test]
    fn test_parse_pip_outdated_from_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/pip/26.2.1/list-outdated.json")
            .expect("read pip list-outdated.json fixture");
        let candidates = parse_pip_outdated(&json, "pip:/opt/homebrew/bin/python3.14")
            .expect("parse pip list-outdated.json");
        assert_eq!(candidates.len(), 3);
        let wheel = candidates
            .iter()
            .find(|c| c.key.name == "wheel")
            .expect("wheel candidate");
        assert_eq!(wheel.current, "0.47.0");
        assert_eq!(wheel.target, "0.48.0");
        assert_eq!(wheel.channel, UpdateChannel::Native);
        assert!(wheel.checkable);
    }
}
```

Add `pub mod pip;` to `crates/banager-core/src/adapters/mod.rs` right after `pub mod uv;`.

- [ ] **Step 2: Run the tests and confirm they fail to compile**

Run: `cargo test -p banager-core adapters::pip::`
Expected: FAIL to compile — `cannot find function `parse_pip_list` in module `adapters::pip`` (and the same for `parse_pip_outdated`, plus `second_token`/`UpdateChannel` not yet imported).

- [ ] **Step 3: Implement pip's parsers, `PipAdapter` struct, `detect`, `inventory` and `check_updates`**

Prepend to `crates/banager-core/src/adapters/pip.rs`:

```rust
use crate::adapters::{second_token, Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions};
use crate::events::{EventSink, OpId};
use crate::model::{
    ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, ManagerInstance, OpRequest,
    Outcome, Plan, Reconciled, Scope, SearchHit, UpdateCandidate, UpdateChannel,
};
use crate::runner::{resolve_exe, CommandOutput, CommandRunner, CommandSpec, HostEnv};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
struct PipPackage {
    name: String,
    version: String,
}

fn parse_pip_list(json: &str) -> Result<Vec<PipPackage>, AdapterError> {
    serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))
}

#[derive(Debug, Deserialize)]
struct PipOutdatedPackage {
    name: String,
    version: String,
    latest_version: String,
}

fn parse_pip_outdated(
    json: &str,
    instance_id: &str,
) -> Result<Vec<UpdateCandidate>, AdapterError> {
    let items: Vec<PipOutdatedPackage> =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    Ok(items
        .into_iter()
        .map(|p| UpdateCandidate {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Package,
                name: p.name,
            },
            current: p.version,
            target: p.latest_version,
            channel: UpdateChannel::Native,
            checkable: true,
            warnings: Vec::new(),
        })
        .collect())
}

pub struct PipAdapter {
    runner: Arc<dyn CommandRunner>,
    meta: AdapterMeta,
}

impl PipAdapter {
    /// Interpreter names to probe on `PATH`, most-specific first, so a
    /// `python3` symlink and its versioned target (e.g. `python3.14`)
    /// resolving to the same real file are still only counted once (see
    /// `detect`'s canonicalization-based dedup).
    pub const CANDIDATE_INTERPRETERS: [&'static str; 7] = [
        "python3.14", "python3.13", "python3.12", "python3.11", "python3.10", "python3", "python",
    ];

    pub fn new(runner: Arc<dyn CommandRunner>) -> PipAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../adapters/meta/pip.toml"))
            .expect("adapters/meta/pip.toml must parse");
        PipAdapter { runner, meta }
    }

    /// One `ManagerInstance` per distinct Python interpreter on `PATH` that
    /// has a working `pip` module (contract: "one per interpreter, invoked
    /// as `{python} -m pip`"). Interpreters are deduplicated by their
    /// canonicalized path so `python3` and `python3.14` naming the same
    /// binary do not produce two instances.
    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let mut seen = HashSet::new();
        let mut found = Vec::new();
        for name in Self::CANDIDATE_INTERPRETERS {
            let Some(python_path) = resolve_exe(name, env) else {
                continue;
            };
            let canonical =
                std::fs::canonicalize(&python_path).unwrap_or_else(|_| python_path.clone());
            if !seen.insert(canonical) {
                continue;
            }
            let output = self
                .runner
                .run(
                    CommandSpec {
                        program: python_path.clone(),
                        args: vec!["-m".to_string(), "pip".to_string(), "--version".to_string()],
                        env: Vec::new(),
                        cwd: None,
                        timeout: Duration::from_secs(30),
                    },
                    None,
                    CancellationToken::new(),
                )
                .await;
            let version = match output {
                // "pip 26.2.1 from … (python 3.14)" — the shared
                // second-token rule (crate::adapters::second_token, Task 5)
                // yields pip's own version, which is what
                // `ManagerInstance::version` means here, not the
                // interpreter's Python version.
                Ok(o) if o.exit_code == Some(0) => second_token(&o.stdout),
                _ => None,
            };
            let Some(version) = version else {
                continue; // no pip module for this interpreter
            };
            let prefix = python_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("/"));
            let unverified_version = self.meta.unverified_version(&Some(version.clone()));
            found.push(ManagerInstance {
                id: format!("pip:{}", python_path.display()),
                adapter_id: self.meta.id.clone(),
                exe_path: python_path,
                prefix,
                scope: Scope::User,
                healthy: true,
                version: Some(version),
                unverified_version,
            });
        }
        found
    }

    async fn run_pip_list(
        &self,
        inst: &ManagerInstance,
        extra_args: &[&str],
    ) -> Result<Vec<PipPackage>, AdapterError> {
        let mut args = vec![
            "-m".to_string(),
            "pip".to_string(),
            "list".to_string(),
            "--format=json".to_string(),
        ];
        args.extend(extra_args.iter().map(|s| s.to_string()));
        let output = self
            .runner
            .run(
                CommandSpec {
                    program: inst.exe_path.clone(),
                    args,
                    env: Vec::new(),
                    cwd: None,
                    timeout: Duration::from_secs(60),
                },
                None,
                CancellationToken::new(),
            )
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        parse_pip_list(&output.stdout)
    }

    /// `--not-required` means "nothing else installed depends on this" —
    /// that is not the same as "the user asked for this", so packages in
    /// that set map to `InstallReason::Unknown`, and everything else (something
    /// depends on it) maps to `InstallReason::Dependency`. pip never tells
    /// Banager what the user explicitly typed `pip install` for, so
    /// `InstallReason::Requested` is never used here (this phase's documented
    /// trap for pip).
    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let all = self.run_pip_list(inst, &[]).await?;
        let not_required = self.run_pip_list(inst, &["--not-required"]).await?;
        let leaf_names: HashSet<String> = not_required.into_iter().map(|p| p.name).collect();
        Ok(all
            .into_iter()
            .map(|p| {
                let reason = if leaf_names.contains(&p.name) {
                    InstallReason::Unknown
                } else {
                    InstallReason::Dependency
                };
                InstalledArtifact {
                    key: ArtifactKey {
                        instance_id: inst.id.clone(),
                        kind: ArtifactKind::Package,
                        name: p.name.clone(),
                    },
                    display_name: p.name,
                    version: p.version,
                    reason,
                    description: None,
                    homepage: None,
                    size_bytes: None,
                    installed_at: None,
                    path: None,
                    auto_updates: false,
                }
            })
            .collect())
    }

    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        let args = vec![
            "-m".to_string(),
            "pip".to_string(),
            "list".to_string(),
            "--outdated".to_string(),
            "--format=json".to_string(),
        ];
        let output = self
            .runner
            .run(
                CommandSpec {
                    program: inst.exe_path.clone(),
                    args,
                    env: Vec::new(),
                    cwd: None,
                    timeout: Duration::from_secs(60),
                },
                None,
                CancellationToken::new(),
            )
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        parse_pip_outdated(&output.stdout, &inst.id)
    }
}
```

- [ ] **Step 4: Run the tests and confirm the parser tests pass**

Run: `cargo test -p banager-core adapters::pip::`
Expected: PASS — all 3 tests in `adapters::pip::tests` pass.

- [ ] **Step 5: Write the failing tests for the read-only refusal, `inventory`'s reason mapping, and `reconcile`**

Append inside the `#[cfg(test)] mod tests { use super::*; ... }` block in `crates/banager-core/src/adapters/pip.rs`:

```rust
    use crate::model::OpKind;
    use crate::runner::MockRunner;

    fn test_instance() -> ManagerInstance {
        ManagerInstance {
            id: "pip:/opt/homebrew/bin/python3.14".to_string(),
            adapter_id: "pip".to_string(),
            exe_path: PathBuf::from("/opt/homebrew/bin/python3.14"),
            prefix: PathBuf::from("/opt/homebrew/bin"),
            scope: Scope::User,
            version: Some("26.2.1".to_string()),
            healthy: true,
            unverified_version: None,
        }
    }

    #[tokio::test]
    async fn test_inventory_maps_the_recorded_fixture_pair_to_unknown_reason() {
        // adapters/fixtures/pip/26.2.1/list.json and list-not-required.json
        // are byte-identical on the recorded machine — every installed
        // package there happens to be a leaf nothing depends on, so every
        // one must map to Unknown, never Requested or Dependency.
        let runner = Arc::new(MockRunner::new());
        let list_json = std::fs::read_to_string("../../adapters/fixtures/pip/26.2.1/list.json")
            .expect("read pip list.json fixture");
        let not_required_json =
            std::fs::read_to_string("../../adapters/fixtures/pip/26.2.1/list-not-required.json")
                .expect("read pip list-not-required.json fixture");
        runner.respond(
            vec![
                "/opt/homebrew/bin/python3.14",
                "-m",
                "pip",
                "list",
                "--format=json",
            ],
            CommandOutput {
                exit_code: Some(0),
                stdout: list_json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec![
                "/opt/homebrew/bin/python3.14",
                "-m",
                "pip",
                "list",
                "--format=json",
                "--not-required",
            ],
            CommandOutput {
                exit_code: Some(0),
                stdout: not_required_json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = PipAdapter::new(runner);
        let artifacts = adapter.inventory(&test_instance()).await.expect("inventory");
        assert_eq!(artifacts.len(), 7);
        assert!(artifacts.iter().all(|a| a.reason == InstallReason::Unknown));
    }

    #[test]
    fn test_a_package_required_by_another_maps_to_dependency_reason() {
        // Edge case the recorded fixture pair cannot show (there all
        // packages are leaves): a package present in the full list but
        // absent from --not-required has something depending on it.
        let all = parse_pip_list(r#"[{"name":"six","version":"1.16.0"},{"name":"leaf","version":"1.0.0"}]"#)
            .expect("parse full list");
        let not_required = parse_pip_list(r#"[{"name":"leaf","version":"1.0.0"}]"#)
            .expect("parse not-required list");
        let leaf_names: HashSet<String> = not_required.into_iter().map(|p| p.name).collect();
        let reasons: Vec<InstallReason> = all
            .into_iter()
            .map(|p| {
                if leaf_names.contains(&p.name) {
                    InstallReason::Unknown
                } else {
                    InstallReason::Dependency
                }
            })
            .collect();
        assert_eq!(reasons, vec![InstallReason::Dependency, InstallReason::Unknown]);
    }

    #[tokio::test]
    async fn test_check_updates_calls_list_outdated_and_parses_the_fixture_output() {
        let json = std::fs::read_to_string("../../adapters/fixtures/pip/26.2.1/list-outdated.json")
            .expect("read pip list-outdated.json fixture");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![
                "/opt/homebrew/bin/python3.14",
                "-m",
                "pip",
                "list",
                "--outdated",
                "--format=json",
            ],
            CommandOutput {
                exit_code: Some(0),
                stdout: json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = PipAdapter::new(runner);
        let candidates = adapter
            .check_updates(&test_instance(), &CheckOptions::default())
            .await
            .expect("check_updates");
        assert_eq!(candidates.len(), 3);
    }

    #[tokio::test]
    async fn test_plan_refuses_every_op_kind() {
        let adapter = PipAdapter::new(Arc::new(MockRunner::new()));
        let inst = test_instance();
        for kind in [OpKind::Install, OpKind::Uninstall, OpKind::Upgrade] {
            let req = OpRequest {
                kind,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Package,
                name: "wheel".to_string(),
            };
            let result = PipAdapter::plan(&adapter, &inst, &req).await;
            match result {
                Err(AdapterError::Unsupported(_)) => {}
                other => panic!("expected Unsupported for {kind:?}, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn test_execute_refuses_because_no_pip_plan_can_exist() {
        // Guards the claim in execute()'s doc comment. If plan() ever grows
        // a success path, this test is what says "execute now needs a real
        // body" instead of silently running nothing.
        let adapter = PipAdapter::new(Arc::new(MockRunner::new()));
        let inst = test_instance();
        let plan = Plan {
            request: OpRequest {
                kind: OpKind::Uninstall,
                instance_id: inst.id.clone(),
                artifact_kind: ArtifactKind::Package,
                name: "wheel".to_string(),
            },
            program: inst.exe_path.clone(),
            args: vec!["-m".to_string(), "pip".to_string()],
            env: Vec::new(),
            needs_password: false,
            locks: Vec::new(),
            cancel_policy: crate::model::CancelPolicy::KillThenReconcile,
            warnings: Vec::new(),
            affected: Vec::new(),
            timeout_secs: 60,
        };
        let result = PipAdapter::execute(
            &adapter,
            &plan,
            Arc::new(crate::events::VecSink::new()),
            1,
            CancellationToken::new(),
        )
        .await;
        assert!(matches!(result, Err(AdapterError::Unsupported(_))));
    }

    #[test]
    fn test_capabilities_report_no_write_operations() {
        let adapter = PipAdapter::new(Arc::new(MockRunner::new()));
        let caps = <PipAdapter as Adapter>::capabilities(&adapter);
        assert!(!caps.per_item_upgrade);
        assert!(!caps.upgrade_all);
        assert!(!caps.uninstall);
        assert!(!caps.search);
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_and_absent() {
        let runner = Arc::new(MockRunner::new());
        let list_json = r#"[{"name":"wheel","version":"0.47.0"}]"#.to_string();
        runner.respond(
            vec![
                "/opt/homebrew/bin/python3.14",
                "-m",
                "pip",
                "list",
                "--format=json",
            ],
            CommandOutput {
                exit_code: Some(0),
                stdout: list_json.clone(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec![
                "/opt/homebrew/bin/python3.14",
                "-m",
                "pip",
                "list",
                "--format=json",
                "--not-required",
            ],
            CommandOutput {
                exit_code: Some(0),
                stdout: list_json,
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = PipAdapter::new(runner);
        let inst = test_instance();
        let present = PipAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey { instance_id: inst.id.clone(), kind: ArtifactKind::Package, name: "wheel".to_string() },
        )
        .await
        .expect("reconcile present");
        assert!(present.present);
        let absent = PipAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey { instance_id: inst.id.clone(), kind: ArtifactKind::Package, name: "missing".to_string() },
        )
        .await
        .expect("reconcile absent");
        assert!(!absent.present);
    }
```

- [ ] **Step 6: Run the tests and confirm they fail to compile**

Run: `cargo test -p banager-core adapters::pip::`
Expected: FAIL to compile — `no function or associated item named `plan` found for struct `PipAdapter`` (and the same for `execute` and `reconcile`, and `<PipAdapter as Adapter>::capabilities` since `impl Adapter for PipAdapter` does not exist yet).

- [ ] **Step 7: Implement `plan`, `execute`, `reconcile`, `capabilities`/`search`, the meta file, and the `Adapter` impl**

Append to `impl PipAdapter { ... }` in `crates/banager-core/src/adapters/pip.rs`, below `check_updates`:

```rust
    pub async fn search(
        &self,
        _inst: &ManagerInstance,
        _query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        Err(AdapterError::Unsupported(
            "pip has no search command; browse PyPI directly".to_string(),
        ))
    }

    /// pip is read-only in Banager (contract: "capabilities() returns false
    /// for per_item_upgrade, upgrade_all and uninstall, and plan() must
    /// refuse those kinds with a clear AdapterError::Unsupported rather than
    /// building an argv nobody should run"). Every `OpKind` refuses here,
    /// before any argv is built, so the UI's install/uninstall/upgrade
    /// affordances for a pip-backed artifact never reach a working `Plan`.
    pub async fn plan(&self, _inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        Err(AdapterError::Unsupported(format!(
            "pip is read-only in Banager; use pipx or uv to manage {}",
            req.name
        )))
    }

    /// Unreachable by construction: `plan()` above refuses every `OpKind`,
    /// so no `Plan` for a pip instance can exist and nothing can ever reach
    /// this. The `Adapter` trait requires the method, so it states that
    /// rather than carrying thirty lines of runner plumbing nothing can
    /// call. (If this ever fires, `plan()` has gained a success path and
    /// this needs a real body.)
    pub async fn execute(
        &self,
        _plan: &Plan,
        _sink: Arc<dyn EventSink>,
        _op_id: OpId,
        _cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        Err(AdapterError::Unsupported(
            "pip is read-only in Banager".to_string(),
        ))
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
            Some(a) => Ok(Reconciled { present: true, version: Some(a.version) }),
            None => Ok(Reconciled { present: false, version: None }),
        }
    }
```

Then add the trait forwarding block below `impl PipAdapter { ... }`:

```rust
#[async_trait]
impl Adapter for PipAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            search: false,
            per_item_upgrade: false,
            upgrade_all: false,
            uninstall: false,
            background_check: true,
            cancel_safe: true,
        }
    }

    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        PipAdapter::detect(self, env).await
    }

    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        PipAdapter::inventory(self, inst).await
    }

    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        PipAdapter::check_updates(self, inst, opts).await
    }

    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        PipAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        PipAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        PipAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        PipAdapter::reconcile(self, inst, key).await
    }
}
```

Create `adapters/meta/pip.toml`:

```toml
schema_version = 1
id = "pip"
name = "pip"
kind = "package_manager"
platforms = ["macos"]
homepage = "https://pip.pypa.io"
verified_versions = ["26.2.1"]
```

- [ ] **Step 8: Run the tests and confirm they pass**

Run: `cargo test -p banager-core adapters::pip::`
Expected: PASS — all tests in `adapters::pip::tests` pass (10 tests total across steps 1 and 5).

- [ ] **Step 9: Run the full workspace gate**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: all three commands exit 0.

- [ ] **Step 10: Commit**

```bash
git add crates/banager-core/src/adapters/pip.rs crates/banager-core/src/adapters/mod.rs adapters/meta/pip.toml
git commit -m "$(cat <<'EOF'
feat(adapters): add read-only pip adapter

pip is read-only in Banager: capabilities() reports no write
operations and plan() refuses every OpKind with a clear
AdapterError::Unsupported before any argv is built, pointing at
pipx/uv instead. execute() is therefore unreachable and says so
rather than carrying runner plumbing nothing can call.
--not-required marks a leaf package, which is not the same as "the
user asked for it", so it maps to Unknown, never Requested.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 9: cargo adapter

**Files:**
- Create: `crates/banager-core/src/adapters/cargo.rs`
- Create: `adapters/meta/cargo.toml`
- Modify: `crates/banager-core/src/adapters/mod.rs` (add `pub mod cargo;` after `pub mod pip;`)
- Modify: `crates/banager-core/src/runner/path_env.rs` (`HostEnv` gains `cargo_home` and `ollama_host`; see Step 3)
- Modify: `crates/banager-core/src/adapters/brew/mod.rs`, `crates/banager-core/src/adapters/npm.rs`, `crates/banager-core/src/adapters/pipx.rs`, `crates/banager-core/src/session/mod.rs` (mechanical: the `HostEnv` literals in their test helpers gain the two new fields)
- Test: `crates/banager-core/src/adapters/cargo.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `crate::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions, validate_package_name}`; `crate::http::{HttpClient, HttpRequest, HttpResponse, MockHttpClient}`; `crate::model::{ArtifactKey, ArtifactKind, CancelPolicy, InstallReason, InstalledArtifact, ManagerInstance, OpKind, OpRequest, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate, UpdateChannel}`; `crate::runner::{resolve_exe, CommandOutput, CommandRunner, CommandSpec, HostEnv, LineCallback, MockRunner}`.
- Produces: `pub struct CargoAdapter`; `impl CargoAdapter { pub fn new(runner: Arc<dyn CommandRunner>, http: Arc<dyn HttpClient>) -> CargoAdapter }`; `impl Adapter for CargoAdapter`. Private free functions: `fn parse_install_key(key: &str) -> Option<(String, String, String)>`, `fn parse_crates2_entries(json: &str) -> Result<Vec<(String, String, String)>, AdapterError>`, `fn parse_crates2(json: &str, instance_id: &str) -> Result<Vec<InstalledArtifact>, AdapterError>`, `fn default_binstall_check(env: &HostEnv) -> Option<PathBuf>`.
- Also produces, in `crates/banager-core/src/runner/path_env.rs`: `HostEnv` gains `pub cargo_home: Option<PathBuf>` and `pub ollama_host: Option<String>`, both filled by `HostEnv::discover()`. This is where every other host fact already lives; reading `CARGO_HOME`/`OLLAMA_HOST` from `std::env` inside an adapter would bypass the very indirection `HostEnv` exists for — a Finder-launched app starts with a minimal environment (`crates/banager-core/src/runner/path_env.rs`) — and would make both values untestable. Task 10 consumes `ollama_host`.
- Reuses, never reimplements: `crate::adapters::second_token` parses `cargo --version` (this adapter defines no `parse_version` of its own), `crate::adapters::run_plan` is its whole `execute()`, and `AdapterMeta::unverified_version` is its whole unverified-version rule.

- [ ] **Step 1: Write the failing tests for cargo's `.crates2.json` parsers**

Create `crates/banager-core/src/adapters/cargo.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_second_token_reads_cargos_recorded_version_line() {
        // adapters/fixtures/cargo/1.98.1/version.txt:
        // "cargo 1.98.1 (797e8a9bc 2026-08-05)"
        // The rule is crate::adapters::second_token (Task 5); this pins it
        // against cargo's real recorded output.
        assert_eq!(
            second_token("cargo 1.98.1 (797e8a9bc 2026-08-05)\n"),
            Some("1.98.1".to_string())
        );
    }

    #[test]
    fn test_parse_install_key_splits_name_version_and_source_kind() {
        // ".crates2.json keeps the package name, version and source in the
        // JSON key" (this phase's documented trap for cargo).
        let key = "hexyl 0.17.0 (registry+https://github.com/rust-lang/crates.io-index)";
        assert_eq!(
            parse_install_key(key),
            Some(("hexyl".to_string(), "0.17.0".to_string(), "registry".to_string()))
        );
    }

    #[test]
    fn test_parse_install_key_recognizes_git_and_path_sources() {
        assert_eq!(
            parse_install_key("my-fork 0.1.0 (git+https://github.com/example/my-fork#abc123)"),
            Some(("my-fork".to_string(), "0.1.0".to_string(), "git".to_string()))
        );
        assert_eq!(
            parse_install_key("local-tool 0.1.0 (path+file:///Users/brulek/dev/local-tool)"),
            Some(("local-tool".to_string(), "0.1.0".to_string(), "path".to_string()))
        );
    }

    #[test]
    fn test_parse_crates2_from_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/cargo/1.98.1/crates2.json")
            .expect("read cargo crates2.json fixture");
        let artifacts = parse_crates2(&json, "cargo:/Users/brulek/.cargo").expect("parse crates2.json");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].key.kind, ArtifactKind::Binary);
        assert_eq!(artifacts[0].key.name, "hexyl");
        assert_eq!(artifacts[0].version, "0.17.0");
    }

    #[test]
    fn test_default_binstall_check_resolves_cargo_binstall_through_host_env() {
        // The real resolver must read HostEnv's hydrated PATH, not the
        // process PATH: a Finder-launched app's process PATH is minimal, and
        // an answer taken from it could name a path `plan` then previews but
        // never runs. A dedicated temp directory stands in for a PATH entry,
        // so this cannot depend on whether the machine running it actually
        // has cargo-binstall installed.
        let dir = std::env::temp_dir().join(format!(
            "banager-binstall-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp PATH dir");
        let env = HostEnv {
            path_dirs: vec![dir.clone()],
            home: PathBuf::from("/tmp"),
            euid: 501,
            cargo_home: None,
            ollama_host: None,
        };
        assert_eq!(default_binstall_check(&env), None);

        let exe = dir.join("cargo-binstall");
        std::fs::write(&exe, b"#!/bin/sh\n").expect("write fake cargo-binstall");
        assert_eq!(default_binstall_check(&env), Some(exe));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

Add `pub mod cargo;` to `crates/banager-core/src/adapters/mod.rs` right after `pub mod pip;`.

- [ ] **Step 2: Run the tests and confirm they fail to compile**

Run: `cargo test -p banager-core adapters::cargo::`
Expected: FAIL to compile — `cannot find function `parse_install_key` in module `adapters::cargo`` (and the same for `parse_crates2`/`default_binstall_check`, plus `second_token`/`HostEnv`/`PathBuf`/`ArtifactKind` not yet imported).

- [ ] **Step 3: Widen `HostEnv`, then implement cargo's parsers, `CargoAdapter` struct, `detect`, `inventory` and `check_updates`**

First, modify `crates/banager-core/src/runner/path_env.rs` so the two host facts this adapter and Task 10's need travel through `HostEnv` like every other one:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostEnv {
    pub path_dirs: Vec<PathBuf>,
    pub home: PathBuf,
    pub euid: u32,
    /// `CARGO_HOME` when the host environment sets it; `None` means "use the
    /// default", `home/.cargo`. Carried here rather than read from
    /// `std::env` inside the cargo adapter, for the same reason `path_dirs`
    /// is: a Finder-launched app's process environment is minimal, and an
    /// adapter that reaches around `HostEnv` cannot be tested.
    pub cargo_home: Option<PathBuf>,
    /// `OLLAMA_HOST` when the host environment sets it; `None` means
    /// Ollama's own default, `http://127.0.0.1:11434`. Same reasoning as
    /// `cargo_home`; consumed by Task 10.
    pub ollama_host: Option<String>,
}
```

and its `discover()`, which is the only place these may be read from the process:

```rust
    pub fn discover() -> HostEnv {
        let path_dirs = std::env::var_os("PATH")
            .map(|v| std::env::split_paths(&v).collect())
            .unwrap_or_default();
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let euid = unsafe { libc::geteuid() };
        let cargo_home = std::env::var_os("CARGO_HOME").map(PathBuf::from);
        let ollama_host = std::env::var("OLLAMA_HOST").ok().filter(|h| !h.is_empty());
        HostEnv {
            path_dirs,
            home,
            euid,
            cargo_home,
            ollama_host,
        }
    }
```

Every existing `HostEnv { … }` literal is in a test helper and needs the two new fields. Run:

```bash
sed -i '' -E 's/^([[:space:]]*)euid: (501|0),$/\1euid: \2,\n\1cargo_home: None,\n\1ollama_host: None,/' \
  crates/banager-core/src/runner/path_env.rs \
  crates/banager-core/src/adapters/brew/mod.rs \
  crates/banager-core/src/adapters/npm.rs \
  crates/banager-core/src/adapters/pipx.rs \
  crates/banager-core/src/session/mod.rs
cargo fmt --all
```

Verify with `rg -c 'cargo_home: None,' crates/banager-core/src` — expected 14 (2 in `path_env.rs`'s own tests, 6 in `brew/mod.rs` — 4 pre-existing plus Task 4's 2, 3 in `npm.rs`, 1 in `pipx.rs`, 2 in `session/mod.rs`). The Step 1 test written above already spells both fields out, so it is not matched by this sed and must not be double-edited. If `rg -n 'HostEnv \{' crates src-tauri` shows any literal the sed missed, add the two fields to it by hand.

Then prepend to `crates/banager-core/src/adapters/cargo.rs`:

```rust
use crate::adapters::{
    run_plan, second_token, validate_package_name, Adapter, AdapterError, AdapterMeta,
    Capabilities, CheckOptions,
};
use crate::events::{EventSink, OpId};
use crate::http::{HttpClient, HttpRequest};
use crate::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstallReason, InstalledArtifact, ManagerInstance,
    OpKind, OpRequest, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate,
    UpdateChannel,
};
use crate::runner::{resolve_exe, CommandOutput, CommandRunner, CommandSpec, HostEnv};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// `.crates2.json`'s `installs` object carries the package name, version and
/// source **in the JSON key** — e.g. `"hexyl 0.17.0 (registry+https://
/// github.com/rust-lang/crates.io-index)"` — not in the value, which only
/// has `bins`/`features`/`profile`/`rustc`/`target`/`version_req` (this
/// phase's documented trap for cargo). Splits that key into
/// `(name, version, source_kind)`, where `source_kind` is the part before
/// the first `+` inside the parens (`"registry"`, `"git"` or `"path"`).
fn parse_install_key(key: &str) -> Option<(String, String, String)> {
    let mut parts = key.splitn(3, ' ');
    let name = parts.next()?.to_string();
    let version = parts.next()?.to_string();
    let source = parts.next()?;
    let source = source.strip_prefix('(')?.strip_suffix(')')?;
    let kind = source.split('+').next().unwrap_or(source).to_string();
    Some((name, version, kind))
}

#[derive(Debug, Deserialize)]
struct Crates2Root {
    #[serde(default)]
    installs: HashMap<String, serde_json::Value>,
}

fn parse_crates2_entries(json: &str) -> Result<Vec<(String, String, String)>, AdapterError> {
    let root: Crates2Root =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    let mut entries: Vec<(String, String, String)> =
        root.installs.keys().filter_map(|k| parse_install_key(k)).collect();
    entries.sort();
    Ok(entries)
}

fn parse_crates2(json: &str, instance_id: &str) -> Result<Vec<InstalledArtifact>, AdapterError> {
    let entries = parse_crates2_entries(json)?;
    Ok(entries
        .into_iter()
        .map(|(name, version, _source_kind)| InstalledArtifact {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Binary,
                name: name.clone(),
            },
            display_name: name,
            version,
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: None,
            installed_at: None,
            path: None,
            auto_updates: false,
        })
        .collect())
}

/// Real detection: resolves `cargo-binstall` through `HostEnv`'s hydrated
/// `PATH` — the same list `cargo` itself would search for a
/// `cargo-<subcommand>` plugin, and the same list every other adapter
/// resolves its executable from. Returns the resolved path, not a boolean,
/// so `plan` can preview exactly the program that will run; an earlier draft
/// scanned the *process* `PATH` for a yes/no answer and then previewed a
/// sibling-of-cargo path that the scan had never checked. A plain `fn`
/// pointer (not a closure) so tests can swap in a fixed answer — a test
/// cannot control whether the machine running it has cargo-binstall.
fn default_binstall_check(env: &HostEnv) -> Option<PathBuf> {
    resolve_exe("cargo-binstall", env)
}

pub struct CargoAdapter {
    runner: Arc<dyn CommandRunner>,
    http: Arc<dyn HttpClient>,
    meta: AdapterMeta,
    binstall_check: fn(&HostEnv) -> Option<PathBuf>,
    /// The path `detect` last resolved for cargo-binstall, or `None` when it
    /// is not installed. `plan` has no `HostEnv` of its own — the `Adapter`
    /// trait gives it only an instance — and `Session` always refreshes, and
    /// therefore detects, before it will issue a plan for an instance, so
    /// reading the cached answer here is what makes the previewed
    /// `Plan::program` the exact path that will run.
    binstall: Mutex<Option<PathBuf>>,
}

impl CargoAdapter {
    pub fn new(runner: Arc<dyn CommandRunner>, http: Arc<dyn HttpClient>) -> CargoAdapter {
        let meta = AdapterMeta::from_toml(include_str!("../../../../adapters/meta/cargo.toml"))
            .expect("adapters/meta/cargo.toml must parse");
        CargoAdapter {
            runner,
            http,
            meta,
            binstall_check: default_binstall_check,
            binstall: Mutex::new(None),
        }
    }

    /// Test seam: pin what `plan` will believe about cargo-binstall without
    /// running `detect` (and so without depending on the host machine).
    #[cfg(test)]
    fn with_binstall(self, path: Option<PathBuf>) -> CargoAdapter {
        *self.binstall.lock().unwrap() = path;
        self
    }

    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let Some(exe_path) = resolve_exe("cargo", env) else {
            return Vec::new();
        };
        let cargo_home = env
            .cargo_home
            .clone()
            .unwrap_or_else(|| env.home.join(".cargo"));
        *self.binstall.lock().unwrap() = (self.binstall_check)(env);
        let output = self
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
        let version = match output {
            // "cargo 1.98.1 (hash date)" — the shared second-token rule
            // (crate::adapters::second_token, Task 5).
            Ok(o) if o.exit_code == Some(0) => second_token(&o.stdout),
            _ => None,
        };
        let unverified_version = self.meta.unverified_version(&version);
        vec![ManagerInstance {
            id: format!("cargo:{}", cargo_home.display()),
            adapter_id: self.meta.id.clone(),
            exe_path,
            prefix: cargo_home,
            scope: Scope::User,
            healthy: version.is_some(),
            version,
            unverified_version,
        }]
    }

    /// A Rust toolchain that has never run `cargo install` has no
    /// `.crates2.json` at all, which is "nothing installed", not a failure —
    /// treating it as one would make every refresh on such a machine report
    /// a per-instance error and hold the whole snapshot permanently stale.
    /// Any other IO error (an unreadable or truncated file) is still an
    /// error.
    fn read_crates2(&self, inst: &ManagerInstance) -> Result<String, AdapterError> {
        let path = inst.prefix.join(".crates2.json");
        match std::fs::read_to_string(&path) {
            Ok(json) => Ok(json),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok("{\"installs\":{}}".to_string())
            }
            Err(e) => Err(AdapterError::Parse(format!(
                "reading {}: {e}",
                path.display()
            ))),
        }
    }

    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let json = self.read_crates2(inst)?;
        parse_crates2(&json, &inst.id)
    }

    async fn latest_stable_version(&self, name: &str) -> Result<String, String> {
        #[derive(Deserialize)]
        struct CratesIoResponse {
            #[serde(rename = "crate")]
            krate: CrateInfo,
        }
        #[derive(Deserialize)]
        struct CrateInfo {
            max_stable_version: String,
        }
        let resp = self
            .http
            .send(HttpRequest {
                method: "GET",
                url: format!("https://crates.io/api/v1/crates/{name}"),
                headers: Vec::new(),
                timeout: Duration::from_secs(30),
            })
            .await
            .map_err(|e| format!("crates.io request failed: {e}"))?;
        if resp.status != 200 {
            return Err(format!("crates.io returned status {}", resp.status));
        }
        let parsed: CratesIoResponse = serde_json::from_str(&resp.body)
            .map_err(|e| format!("could not parse crates.io response: {e}"))?;
        Ok(parsed.krate.max_stable_version)
    }

    /// Registry-sourced crates are checked one at a time against crates.io.
    /// Git and path sources are `checkable: false` with a reason
    /// unconditionally — Banager has no way to check those for updates at
    /// all, so every such crate always gets a row explaining why, not just
    /// the ones that happen to be outdated (contract: "git and path sources
    /// are checkable: false with a reason").
    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        let json = self.read_crates2(inst)?;
        let entries = parse_crates2_entries(&json)?;
        let mut out = Vec::new();
        for (name, version, source_kind) in entries {
            let key = ArtifactKey {
                instance_id: inst.id.clone(),
                kind: ArtifactKind::Binary,
                name: name.clone(),
            };
            if source_kind != "registry" {
                out.push(UpdateCandidate {
                    key,
                    current: version.clone(),
                    target: version,
                    channel: UpdateChannel::Registry,
                    checkable: false,
                    warnings: vec![format!(
                        "installed from {source_kind}, cannot check crates.io for updates"
                    )],
                });
                continue;
            }
            match self.latest_stable_version(&name).await {
                Ok(latest) if latest != version => out.push(UpdateCandidate {
                    key,
                    current: version,
                    target: latest,
                    channel: UpdateChannel::Registry,
                    checkable: true,
                    warnings: Vec::new(),
                }),
                Ok(_) => {}
                Err(reason) => out.push(UpdateCandidate {
                    key,
                    current: version.clone(),
                    target: version,
                    channel: UpdateChannel::Registry,
                    checkable: false,
                    warnings: vec![reason],
                }),
            }
        }
        Ok(out)
    }
}
```

- [ ] **Step 4: Run the tests and confirm the parser tests pass**

Run: `cargo test -p banager-core adapters::cargo::`
Expected: PASS — all 5 tests in `adapters::cargo::tests` pass.

- [ ] **Step 5: Write the failing tests for `check_updates`'s network paths, `plan`'s binstall branch, `execute` and `reconcile`**

Append inside the `#[cfg(test)] mod tests { use super::*; ... }` block in `crates/banager-core/src/adapters/cargo.rs`:

```rust
    use crate::events::VecSink;
    use crate::http::{HttpResponse, MockHttpClient};
    use crate::runner::MockRunner;

    fn temp_cargo_home(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "banager-cargo-home-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn test_instance(prefix: PathBuf) -> ManagerInstance {
        ManagerInstance {
            id: format!("cargo:{}", prefix.display()),
            adapter_id: "cargo".to_string(),
            exe_path: PathBuf::from("/Users/brulek/.cargo/bin/cargo"),
            prefix,
            scope: Scope::User,
            version: Some("1.98.1".to_string()),
            healthy: true,
            unverified_version: None,
        }
    }

    #[tokio::test]
    async fn test_check_updates_flags_the_fixture_crate_as_outdated() {
        let json = std::fs::read_to_string("../../adapters/fixtures/cargo/1.98.1/crates2.json")
            .expect("read cargo crates2.json fixture");
        let home = temp_cargo_home("outdated");
        std::fs::create_dir_all(&home).expect("create cargo home");
        std::fs::write(home.join(".crates2.json"), &json).expect("write crates2.json");

        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "https://crates.io/api/v1/crates/hexyl",
            HttpResponse {
                status: 200,
                body: r#"{"crate":{"max_stable_version":"0.18.0"}}"#.to_string(),
            },
        );
        let adapter = CargoAdapter::new(Arc::new(MockRunner::new()), http);
        let inst = test_instance(home.clone());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].key.name, "hexyl");
        assert!(candidates[0].checkable);
        assert_eq!(candidates[0].target, "0.18.0");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_check_updates_reports_nothing_when_the_fixture_crate_is_current() {
        let json = std::fs::read_to_string("../../adapters/fixtures/cargo/1.98.1/crates2.json")
            .expect("read cargo crates2.json fixture");
        let home = temp_cargo_home("current");
        std::fs::create_dir_all(&home).expect("create cargo home");
        std::fs::write(home.join(".crates2.json"), &json).expect("write crates2.json");

        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "https://crates.io/api/v1/crates/hexyl",
            HttpResponse {
                status: 200,
                body: r#"{"crate":{"max_stable_version":"0.17.0"}}"#.to_string(),
            },
        );
        let adapter = CargoAdapter::new(Arc::new(MockRunner::new()), http);
        let inst = test_instance(home.clone());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert!(candidates.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_check_updates_marks_a_git_sourced_crate_as_uncheckable() {
        // Edge case the recorded fixture (a single registry-sourced crate)
        // cannot show: a crate installed from a git repository.
        let json = r#"{"installs":{"my-fork 0.1.0 (git+https://github.com/example/my-fork#abc123)":{"version_req":null,"bins":["my-fork"],"features":[],"all_features":false,"no_default_features":false,"profile":"release","target":"aarch64-apple-darwin","rustc":"rustc 1.98.1\n"}}}"#;
        let home = temp_cargo_home("git-source");
        std::fs::create_dir_all(&home).expect("create cargo home");
        std::fs::write(home.join(".crates2.json"), json).expect("write crates2.json");

        let http = Arc::new(MockHttpClient::new());
        let adapter = CargoAdapter::new(Arc::new(MockRunner::new()), http.clone());
        let inst = test_instance(home.clone());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert_eq!(candidates.len(), 1);
        assert!(!candidates[0].checkable);
        assert!(candidates[0].warnings[0].contains("git"));
        assert!(http.calls().is_empty(), "a git-sourced crate must never reach crates.io");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_inventory_of_a_cargo_home_with_no_crates2_json_is_empty_not_an_error() {
        // A Rust toolchain that has never run `cargo install` has no
        // .crates2.json. That is "nothing installed", not a failed refresh:
        // an error here would push a SourceError and hold the whole snapshot
        // stale on every refresh, forever, on an entirely healthy machine.
        let home = temp_cargo_home("empty");
        std::fs::create_dir_all(&home).expect("create cargo home");
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(home.clone());
        let artifacts = adapter.inventory(&inst).await.expect("inventory");
        assert!(artifacts.is_empty());
        let candidates = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert!(candidates.is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_plan_refuses_when_request_instance_id_does_not_match_given_instance() {
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "not-cargo".to_string(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        };
        let result = CargoAdapter::plan(&adapter, &inst, &req).await;
        match result {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_plan_install_without_binstall_compiles_and_warns() {
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()))
                .with_binstall(None);
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        };
        let plan = CargoAdapter::plan(&adapter, &inst, &req).await.expect("plan");
        assert_eq!(plan.program, PathBuf::from("/Users/brulek/.cargo/bin/cargo"));
        assert_eq!(plan.args, vec!["install", "hexyl"]);
        assert_eq!(plan.warnings, vec!["compiles locally and can take several minutes".to_string()]);
    }

    #[tokio::test]
    async fn test_plan_upgrade_with_binstall_skips_the_compile_warning() {
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()))
                .with_binstall(Some(PathBuf::from("/Users/brulek/.cargo/bin/cargo-binstall")));
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let req = OpRequest {
            kind: OpKind::Upgrade,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        };
        let plan = CargoAdapter::plan(&adapter, &inst, &req).await.expect("plan");
        assert_eq!(
            plan.program,
            PathBuf::from("/Users/brulek/.cargo/bin/cargo-binstall")
        );
        assert_eq!(plan.args, vec!["-y", "--force", "hexyl"]);
        assert!(plan.warnings.is_empty());
    }

    #[tokio::test]
    async fn test_plan_uninstall_never_uses_binstall() {
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()))
                .with_binstall(Some(PathBuf::from("/Users/brulek/.cargo/bin/cargo-binstall")));
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        };
        let plan = CargoAdapter::plan(&adapter, &inst, &req).await.expect("plan");
        assert_eq!(plan.program, PathBuf::from("/Users/brulek/.cargo/bin/cargo"));
        assert_eq!(plan.args, vec!["uninstall", "hexyl"]);
    }

    #[tokio::test]
    async fn test_execute_streams_log_events_and_succeeds() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/Users/brulek/.cargo/bin/cargo", "install", "hexyl"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "Installing hexyl\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter =
            CargoAdapter::new(runner, Arc::new(MockHttpClient::new())).with_binstall(None);
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        };
        let plan = CargoAdapter::plan(&adapter, &inst, &req).await.expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome = CargoAdapter::execute(&adapter, &plan, sink.clone(), 1, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(sink.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_and_absent() {
        let json = std::fs::read_to_string("../../adapters/fixtures/cargo/1.98.1/crates2.json")
            .expect("read cargo crates2.json fixture");
        let home = temp_cargo_home("reconcile");
        std::fs::create_dir_all(&home).expect("create cargo home");
        std::fs::write(home.join(".crates2.json"), &json).expect("write crates2.json");
        let adapter = CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(home.clone());
        let present = CargoAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey { instance_id: inst.id.clone(), kind: ArtifactKind::Binary, name: "hexyl".to_string() },
        )
        .await
        .expect("reconcile present");
        assert!(present.present);
        let absent = CargoAdapter::reconcile(
            &adapter,
            &inst,
            &ArtifactKey { instance_id: inst.id.clone(), kind: ArtifactKind::Binary, name: "missing".to_string() },
        )
        .await
        .expect("reconcile absent");
        assert!(!absent.present);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_search_is_unsupported() {
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(PathBuf::from("/Users/brulek/.cargo"));
        let result = <CargoAdapter as Adapter>::search(&adapter, &inst, "hexyl").await;
        assert!(matches!(result, Err(AdapterError::Unsupported(_))));
    }
```

- [ ] **Step 6: Run the tests and confirm they fail to compile**

Run: `cargo test -p banager-core adapters::cargo::`
Expected: FAIL to compile — `no function or associated item named `plan` found for struct `CargoAdapter`` (and the same for `with_binstall`/`execute`/`reconcile`/`<CargoAdapter as Adapter>::search`).

- [ ] **Step 7: Implement `plan`, `execute`, `reconcile`, `capabilities`/`search`, the meta file, and the `Adapter` impl**

Append to `impl CargoAdapter { ... }` in `crates/banager-core/src/adapters/cargo.rs`, below `check_updates`:

```rust
    pub async fn search(
        &self,
        _inst: &ManagerInstance,
        _query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        Err(AdapterError::Unsupported(
            "cargo has no search command Banager uses; browse crates.io directly".to_string(),
        ))
    }

    pub async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        if req.instance_id != inst.id {
            return Err(AdapterError::Refused(format!(
                "plan requested for instance {} but given instance {}",
                req.instance_id, inst.id
            )));
        }
        validate_package_name(&req.name)?;
        let lock = ResourceLock(inst.id.clone());
        match req.kind {
            OpKind::Install | OpKind::Upgrade => {
                // The path `detect` resolved through HostEnv, not a fresh
                // guess: whatever is previewed here is exactly what runs.
                let binstall = self.binstall.lock().unwrap().clone();
                let mut warnings = Vec::new();
                let (program, mut args) = match binstall {
                    Some(path) => (path, vec!["-y".to_string()]),
                    None => {
                        warnings
                            .push("compiles locally and can take several minutes".to_string());
                        (inst.exe_path.clone(), vec!["install".to_string()])
                    }
                };
                if matches!(req.kind, OpKind::Upgrade) {
                    args.push("--force".to_string());
                }
                args.push(req.name.clone());
                Ok(Plan {
                    request: req.clone(),
                    program,
                    args,
                    env: Vec::new(),
                    needs_password: false,
                    locks: vec![lock],
                    cancel_policy: CancelPolicy::KillThenReconcile,
                    warnings,
                    affected: Vec::new(),
                    timeout_secs: 1800,
                })
            }
            OpKind::Uninstall => Ok(Plan {
                request: req.clone(),
                program: inst.exe_path.clone(),
                args: vec!["uninstall".to_string(), req.name.clone()],
                env: Vec::new(),
                needs_password: false,
                locks: vec![lock],
                cancel_policy: CancelPolicy::KillThenReconcile,
                warnings: Vec::new(),
                affected: Vec::new(),
                timeout_secs: 300,
            }),
        }
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
            Some(a) => Ok(Reconciled { present: true, version: Some(a.version) }),
            None => Ok(Reconciled { present: false, version: None }),
        }
    }
```

Then add the trait forwarding block below `impl CargoAdapter { ... }`:

```rust
#[async_trait]
impl Adapter for CargoAdapter {
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
        CargoAdapter::detect(self, env).await
    }

    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        CargoAdapter::inventory(self, inst).await
    }

    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        CargoAdapter::check_updates(self, inst, opts).await
    }

    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        CargoAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        CargoAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        CargoAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        CargoAdapter::reconcile(self, inst, key).await
    }
}
```

Create `adapters/meta/cargo.toml`:

```toml
schema_version = 1
id = "cargo"
name = "Cargo"
kind = "package_manager"
platforms = ["macos"]
homepage = "https://doc.rust-lang.org/cargo/"
verified_versions = ["1.98.1"]
```

- [ ] **Step 8: Run the tests and confirm they pass**

Run: `cargo test -p banager-core adapters::cargo::`
Expected: PASS — all tests in `adapters::cargo::tests` pass (16 tests total across steps 1 and 5). Then run `cargo test --workspace` once here too: Step 3 widened `HostEnv`, so this is where a missed literal in another crate's test helper shows up.

- [ ] **Step 9: Run the full workspace gate**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: all three commands exit 0.

- [ ] **Step 10: Commit**

```bash
git add crates/banager-core/src/adapters/cargo.rs crates/banager-core/src/adapters/mod.rs crates/banager-core/src/runner/path_env.rs crates/banager-core/src/adapters/brew/mod.rs crates/banager-core/src/adapters/npm.rs crates/banager-core/src/adapters/pipx.rs crates/banager-core/src/session/mod.rs adapters/meta/cargo.toml
git commit -m "$(cat <<'EOF'
feat(adapters): add cargo adapter over .crates2.json and crates.io

.crates2.json carries name/version/source in the installs object's
JSON key, not its value. Registry-sourced crates are checked against
crates.io; git and path sources are always checkable:false with a
reason, since Banager cannot check either for updates. Installing
or upgrading without cargo-binstall present warns that it compiles
locally; when it is present, the path previewed is the one detect
resolved through HostEnv, so the preview cannot name a program the
check never looked at. A missing .crates2.json means nothing is
installed, not a failed refresh. HostEnv gains cargo_home and
ollama_host so no adapter has to read the process environment.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 10: ollama adapter

**Files:**
- Create: `crates/banager-core/src/adapters/ollama/mod.rs`
- Create: `crates/banager-core/src/adapters/ollama/parse.rs`
- Create: `adapters/meta/ollama.toml`
- Modify: `crates/banager-core/src/adapters/mod.rs` (add `pub mod ollama;` after `pub mod cargo;`)
- Test: `crates/banager-core/src/adapters/ollama/parse.rs` and `crates/banager-core/src/adapters/ollama/mod.rs` (inline `#[cfg(test)] mod tests` in each, matching `adapters/brew/parse.rs` + `adapters/brew/mod.rs`'s split)

**Interfaces:**
- Consumes: `crate::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions}`; `crate::http::{HttpClient, HttpRequest, HttpResponse, MockHttpClient}`; `crate::model::{ArtifactKey, CancelPolicy, InstalledArtifact, ManagerInstance, OpKind, OpRequest, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate, UpdateChannel}` (`ArtifactKind::Model`, `InstallReason::Requested` via `parse::parse_tags`); `crate::runner::{resolve_exe, CommandRunner, CommandSpec, HostEnv, LineCallback, MockRunner}`.
- Produces: `pub struct OllamaAdapter`; `impl OllamaAdapter { pub fn new(runner: Arc<dyn CommandRunner>, http: Arc<dyn HttpClient>) -> OllamaAdapter }`; `impl Adapter for OllamaAdapter`. In `parse.rs`: `pub fn parse_version(text: &str) -> Option<String>`, `pub fn parse_tags(json: &str, instance_id: &str) -> Result<Vec<InstalledArtifact>, AdapterError>`, `pub fn split_model_reference(reference: &str) -> (String, String, String)`, `pub fn layer_digests(json: &str) -> Result<HashSet<String>, AdapterError>`, `pub fn config_digest(json: &str) -> Result<Option<String>, AdapterError>`.
- Everything `inventory`/`check_updates` need travels on the instance, because the `Adapter` trait gives them only `inst: &ManagerInstance`: `detect` sets `prefix` to `env.home.join(".ollama")` so the local manifests root can be derived from it, and encodes the daemon URL in the instance id as `ollama:{host}` so `host_of(inst)` can read it back. The host itself comes from `HostEnv::ollama_host` (Task 9 added the field), never from `std::env` — this adapter has no `OLLAMA_HOST` read and no mutable host of its own, so a machine or CI runner that happens to set `OLLAMA_HOST` cannot change what any test mocks.
- Reuses, never reimplements: `crate::adapters::run_plan` is this adapter's whole `execute()`, and `AdapterMeta::unverified_version` is its whole unverified-version rule. `ollama --version` prints `ollama version is X.Y.Z`, so `parse::parse_version` (last token) is genuinely Ollama-specific and does **not** use `crate::adapters::second_token`.

- [ ] **Step 1: Write the failing tests for ollama's response and manifest parsers**

Create `crates/banager-core/src/adapters/ollama/parse.rs`:

```rust
use crate::adapters::AdapterError;
use crate::model::{ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact};
use serde::Deserialize;
use std::collections::HashSet;

/// Parses `ollama --version`'s "ollama version is X.Y.Z" output.
pub fn parse_version(text: &str) -> Option<String> {
    let v = text.split_whitespace().last()?;
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

#[derive(Debug, Deserialize)]
struct TagsRoot {
    #[serde(default)]
    models: Vec<TagsModel>,
}

#[derive(Debug, Deserialize)]
struct TagsModel {
    name: String,
    digest: String,
    #[serde(default)]
    size: u64,
}

/// Parses `GET {host}/api/tags`'s body into the models Ollama currently has
/// pulled. `TagsModel::name` is the full `name:tag` form (e.g.
/// `qwen3.8:27b-mlx`) exactly as Ollama reports it, which is what
/// `split_model_reference` expects. There is no per-model install
/// timestamp in this response, so `installed_at` is always `None`.
pub fn parse_tags(json: &str, instance_id: &str) -> Result<Vec<InstalledArtifact>, AdapterError> {
    let root: TagsRoot =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    Ok(root
        .models
        .into_iter()
        .map(|m| InstalledArtifact {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Model,
                name: m.name.clone(),
            },
            display_name: m.name,
            version: m.digest,
            reason: InstallReason::Requested,
            description: None,
            homepage: None,
            size_bytes: Some(m.size),
            installed_at: None,
            path: None,
            auto_updates: false,
        })
        .collect())
}

/// Splits an Ollama model reference (`name:tag`, e.g. `qwen3.8:27b-mlx`, or
/// `namespace/name:tag`) into `(namespace, name, tag)`. A bare name with no
/// `/` uses Ollama's default namespace, `library`; a reference with no
/// `:tag` uses Ollama's default tag, `latest`.
pub fn split_model_reference(reference: &str) -> (String, String, String) {
    let (name_part, tag) = match reference.split_once(':') {
        Some((n, t)) => (n, t.to_string()),
        None => (reference, "latest".to_string()),
    };
    match name_part.split_once('/') {
        Some((ns, n)) => (ns.to_string(), n.to_string(), tag),
        None => ("library".to_string(), name_part.to_string(), tag),
    }
}

#[derive(Debug, Deserialize)]
struct ManifestLayer {
    digest: String,
}

#[derive(Debug, Deserialize)]
struct ManifestConfig {
    digest: String,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    layers: Vec<ManifestLayer>,
    #[serde(default)]
    config: Option<ManifestConfig>,
}

/// Parses a v2 Docker-distribution manifest (the shape both the local
/// `~/.ollama/models/manifests/...` file and the registry's `GET
/// /v2/{ns}/{name}/manifests/{tag}` response use) into the set of its
/// layer digests. Comparing this set — not the serialized manifest bytes —
/// is what makes the up-to-date check correct: Ollama rewrites the local
/// manifest file on disk, so a byte-for-byte comparison would report a
/// false "outdated" for a model that has not actually changed.
pub fn layer_digests(json: &str) -> Result<HashSet<String>, AdapterError> {
    let manifest: Manifest =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    Ok(manifest.layers.into_iter().map(|l| l.digest).collect())
}

/// The manifest's `config.digest`, the one short, stable string that names
/// *this* build of the model. `UpdateCandidate`'s contract is that `current`
/// and `target` differ, and a model's tag (`27b-mlx`) does not change when
/// the model behind it is republished — so the tag cannot be the target.
/// `None` when the manifest carries no config section.
pub fn config_digest(json: &str) -> Result<Option<String>, AdapterError> {
    let manifest: Manifest =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    Ok(manifest.config.map(|c| c.digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_reads_the_last_token() {
        // adapters/fixtures/ollama/0.34.1/version.txt: "ollama version is 0.34.1"
        assert_eq!(parse_version("ollama version is 0.34.1\n"), Some("0.34.1".to_string()));
    }

    #[test]
    fn test_parse_tags_from_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/ollama/0.34.1/api-tags.json")
            .expect("read ollama api-tags.json fixture");
        let artifacts = parse_tags(&json, "ollama:http://127.0.0.1:11434").expect("parse api-tags.json");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].key.kind, ArtifactKind::Model);
        assert_eq!(artifacts[0].key.name, "qwen3.8:27b-mlx");
        assert_eq!(
            artifacts[0].version,
            "5642e97495e1a088883805981563dcdc4a040c2f53388b7a41d1f24d3622cf7e"
        );
        assert_eq!(artifacts[0].size_bytes, Some(18174721847));
    }

    #[test]
    fn test_split_model_reference_handles_namespace_and_tag_defaults() {
        assert_eq!(
            split_model_reference("qwen3.8:27b-mlx"),
            ("library".to_string(), "qwen3.8".to_string(), "27b-mlx".to_string())
        );
        assert_eq!(
            split_model_reference("someuser/somemodel:sometag"),
            ("someuser".to_string(), "somemodel".to_string(), "sometag".to_string())
        );
        assert_eq!(
            split_model_reference("llama3"),
            ("library".to_string(), "llama3".to_string(), "latest".to_string())
        );
    }

    #[test]
    fn test_config_digest_of_the_recorded_local_and_registry_manifests_is_the_same_string() {
        // The config digest is what check_updates reports as an available
        // update's `target`; the recorded pair is the already-up-to-date
        // case, so the two must agree here too.
        let local = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/local-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read local manifest fixture");
        let registry = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/registry-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read registry manifest fixture");
        let local_config = config_digest(&local).expect("parse local manifest");
        assert!(local_config.is_some(), "the recorded manifest has a config section");
        assert_eq!(local_config, config_digest(&registry).expect("parse registry manifest"));
    }

    #[test]
    fn test_layer_digests_of_the_recorded_local_and_registry_manifests_are_equal() {
        let local = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/local-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read local manifest fixture");
        let registry = std::fs::read_to_string(
            "../../adapters/fixtures/ollama/0.34.1/registry-manifest-qwen3.8-27b-mlx.json",
        )
        .expect("read registry manifest fixture");
        let local_digests = layer_digests(&local).expect("parse local manifest");
        let registry_digests = layer_digests(&registry).expect("parse registry manifest");
        assert_eq!(local_digests.len(), 1209);
        assert_eq!(registry_digests.len(), 1209);
        assert_eq!(
            local_digests, registry_digests,
            "the recorded fixture pair is the already-up-to-date case"
        );
    }
}
```

Add `pub mod ollama;` to `crates/banager-core/src/adapters/mod.rs` right after `pub mod cargo;`.

- [ ] **Step 2: Run the tests and confirm they compile and pass**

Run: `cargo test -p banager-core adapters::ollama::parse::`
Expected: PASS — `parse.rs` is fully self-contained (only depends on `AdapterError`/`ArtifactKey`/`ArtifactKind`/`InstallReason`/`InstalledArtifact`, all of which already exist), so this file compiles and all 5 tests pass on the first run. (`crates/banager-core/src/adapters/ollama/mod.rs` does not exist yet — create it now as an empty file with `pub mod parse;` so `pub mod ollama;` in `adapters/mod.rs` resolves; without it this step's `cargo test` invocation fails with `file not found for module `ollama``.)

Create `crates/banager-core/src/adapters/ollama/mod.rs` with just:

```rust
pub mod parse;
```

- [ ] **Step 3: Write the failing tests for `OllamaAdapter`'s `detect`, `inventory` and `check_updates`**

Append to `crates/banager-core/src/adapters/ollama/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{HttpResponse, MockHttpClient};
    use crate::runner::MockRunner;

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
            HttpResponse { status: 200, body: json },
        );
        let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http);
        let inst = test_instance("http://127.0.0.1:11434", PathBuf::from("/Users/brulek/.ollama"));
        let artifacts = adapter.inventory(&inst).await.expect("inventory");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].key.name, "qwen3.8:27b-mlx");
    }

    #[tokio::test]
    async fn test_inventory_fails_clearly_when_the_daemon_answers_with_an_error_status() {
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "http://127.0.0.1:11434/api/tags",
            HttpResponse { status: 500, body: "boom".to_string() },
        );
        let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http);
        let inst = test_instance("http://127.0.0.1:11434", PathBuf::from("/Users/brulek/.ollama"));
        let result = adapter.inventory(&inst).await;
        match result {
            Err(AdapterError::CommandFailed { code: Some(500), .. }) => {}
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
            "banager-ollama-manifests-{}-{}",
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
            HttpResponse { status: 200, body: registry_json },
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
            registry_request.headers.iter().any(|(name, value)| name == "Accept"
                && value == "application/vnd.docker.distribution.manifest.v2+json"),
            "the registry request must carry the v2 manifest Accept header, got {:?}",
            registry_request.headers
        );

        let _ = std::fs::remove_dir_all(&tmp_root);
    }

    #[tokio::test]
    async fn test_check_updates_reports_nothing_for_the_recorded_up_to_date_fixture() {
        let tags_json = std::fs::read_to_string("../../adapters/fixtures/ollama/0.34.1/api-tags.json")
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
            "banager-ollama-home-{}-{}",
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
            HttpResponse { status: 200, body: tags_json },
        );
        http.respond(
            "https://registry.ollama.ai/v2/library/qwen3.8/manifests/27b-mlx",
            HttpResponse { status: 200, body: registry_json },
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
        // file is absent (e.g. deleted out from under Banager).
        let tags_json = std::fs::read_to_string("../../adapters/fixtures/ollama/0.34.1/api-tags.json")
            .expect("read ollama api-tags.json fixture");
        let home = std::env::temp_dir().join(format!(
            "banager-ollama-missing-manifest-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "http://127.0.0.1:11434/api/tags",
            HttpResponse { status: 200, body: tags_json },
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
}
```

- [ ] **Step 4: Run the tests and confirm they fail to compile**

Run: `cargo test -p banager-core adapters::ollama::`
Expected: FAIL to compile — `cannot find type `OllamaAdapter` in module `adapters::ollama`` (and the same for `ManagerInstance`/`PathBuf`/`Arc`/`MockHttpClient`/`MockRunner`/`HttpResponse`/`CheckOptions`/`AdapterError`/`HostEnv` not yet imported into `mod.rs`, and `compare_digests`/`host_for`/`host_of`/`DEFAULT_HOST` not yet defined).

- [ ] **Step 5: Implement `OllamaAdapter`'s struct, `detect`, `inventory`, `check_updates` and `compare_digests`**

`crates/banager-core/src/adapters/ollama/mod.rs` currently contains only the `pub mod parse;` line from Step 2 (plus the `#[cfg(test)] mod tests { ... }` block from Step 3, further down). Replace just that `pub mod parse;` line with:

```rust
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
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
```

The `#[cfg(test)] mod tests { ... }` block from Step 3 stays where it is, below this. Then append, above that test module:

```rust
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
        let local_json = std::fs::read_to_string(&local_path)
            .map_err(|e| format!("could not read local manifest {}: {e}", local_path.display()))?;
        let local_digests =
            layer_digests(&local_json).map_err(|e| format!("could not parse local manifest: {e}"))?;

        let registry_url = format!("https://registry.ollama.ai/v2/{namespace}/{name}/manifests/{tag}");
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
        match self.compare_digests(manifests_root, &namespace, &name, &tag).await {
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
}
```

- [ ] **Step 6: Run the tests and confirm they pass**

Run: `cargo test -p banager-core adapters::ollama::`
Expected: PASS — the `parse` module's 5 tests and `mod.rs`'s 5 tests all pass (`plan`/`execute`/`reconcile`/`impl Adapter` are not exercised by these tests yet since they do not exist yet, but nothing here calls them).

- [ ] **Step 7: Write the failing tests for `plan`, `execute`, `reconcile` and the write-path validation**

Append inside the `#[cfg(test)] mod tests { use super::*; ... }` block in `crates/banager-core/src/adapters/ollama/mod.rs`, after the tests from Step 3:

```rust
    use crate::events::VecSink;
    use crate::model::ArtifactKind;

    #[tokio::test]
    async fn test_plan_refuses_when_request_instance_id_does_not_match_given_instance() {
        let adapter =
            OllamaAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance("http://127.0.0.1:11434", PathBuf::from("/Users/brulek/.ollama"));
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
        let inst = test_instance("http://127.0.0.1:11434", PathBuf::from("/Users/brulek/.ollama"));
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
            let plan = OllamaAdapter::plan(&adapter, &inst, &req).await.expect("plan");
            assert_eq!(plan.args, expected);
            assert!(!plan.needs_password);
        }
    }

    #[tokio::test]
    async fn test_plan_rejects_a_model_name_with_shell_metacharacters_but_allows_the_colon() {
        let adapter =
            OllamaAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance("http://127.0.0.1:11434", PathBuf::from("/Users/brulek/.ollama"));
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
        assert!(OllamaAdapter::plan(&adapter, &inst, &good_req).await.is_ok());
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
        let inst = test_instance("http://127.0.0.1:11434", PathBuf::from("/Users/brulek/.ollama"));
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Model,
            name: "qwen3.8:27b-mlx".to_string(),
        };
        let plan = OllamaAdapter::plan(&adapter, &inst, &req).await.expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome = OllamaAdapter::execute(&adapter, &plan, sink.clone(), 1, CancellationToken::new())
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
            HttpResponse { status: 200, body: json },
        );
        let adapter = OllamaAdapter::new(Arc::new(MockRunner::new()), http);
        let inst = test_instance("http://127.0.0.1:11434", PathBuf::from("/Users/brulek/.ollama"));
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
        let inst = test_instance("http://127.0.0.1:11434", PathBuf::from("/Users/brulek/.ollama"));
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
```

Also add, to the top of the `use super::*;`-scoped test module (alongside `crate::http::{HttpResponse, MockHttpClient}` and `crate::runner::MockRunner` already imported there in Step 3), the one remaining test-only import `test_execute_streams_log_events_and_succeeds` above needs:

```rust
    use crate::runner::CommandOutput;
```

(`HostEnv` is already in scope through the module's own `use crate::runner::{…, HostEnv};` and `use super::*;`.)

- [ ] **Step 8: Run the tests and confirm they fail to compile**

Run: `cargo test -p banager-core adapters::ollama::`
Expected: FAIL to compile — `no function or associated item named `plan` found for struct `OllamaAdapter`` (and the same for `execute`/`reconcile`/`<OllamaAdapter as Adapter>::search`, and `validate_model_reference` not defined).

- [ ] **Step 9: Implement `plan`, `execute`, `reconcile`, `capabilities`/`search`, the meta file, and the `Adapter` impl**

Append to `impl OllamaAdapter { ... }` in `crates/banager-core/src/adapters/ollama/mod.rs`, below `check_updates`:

```rust
    pub async fn search(
        &self,
        _inst: &ManagerInstance,
        _query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        Err(AdapterError::Unsupported(
            "Ollama has no search command Banager uses; browse the model library directly".to_string(),
        ))
    }

    /// Writes go through the CLI, not the HTTP API (this phase's ruling):
    /// `ollama pull {model}` for Install/Upgrade (pulling an already-present
    /// model re-fetches it in place, which is how Ollama itself upgrades a
    /// model) and `ollama rm {model}` for Uninstall.
    pub async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
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
            Some(a) => Ok(Reconciled { present: true, version: Some(a.version) }),
            None => Ok(Reconciled { present: false, version: None }),
        }
    }
```

Then add the trait forwarding block below `impl OllamaAdapter { ... }`:

```rust
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
```

Create `adapters/meta/ollama.toml`:

```toml
schema_version = 1
id = "ollama"
name = "Ollama"
kind = "model_manager"
platforms = ["macos"]
homepage = "https://ollama.com"
verified_versions = ["0.34.1"]
```

- [ ] **Step 10: Run the tests, then the full workspace gate, then commit**

Run: `cargo test -p banager-core adapters::ollama::`
Expected: PASS — all tests in `adapters::ollama::parse::tests` and `adapters::ollama::tests` pass (5 + 12 = 17 tests total).

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: all three commands exit 0.

```bash
git add crates/banager-core/src/adapters/ollama/mod.rs crates/banager-core/src/adapters/ollama/parse.rs crates/banager-core/src/adapters/mod.rs adapters/meta/ollama.toml
git commit -m "$(cat <<'EOF'
feat(adapters): add ollama adapter with HTTP reads and CLI writes

Reads (detect's health check, inventory, check_updates) go over
HTTP to the local daemon and the public registry, never shelling
out to `ollama list`, which would launch Ollama.app on macOS.
Writes (pull/rm) go through the CLI, per this phase's ruling.
check_updates compares the set of layer digests between the local
and registry manifests rather than the serialized files, since
Ollama rewrites the local manifest on disk, and reports the
registry's config digest as the target so an available update does
not render as `27b-mlx -> 27b-mlx`. The daemon URL comes from
HostEnv and travels on the instance id, so nothing here reads the
process environment. Model references use their own validator
instead of validate_package_name, since a name:tag reference's
colon is not in that function's charset.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 11: Session registers all seven adapters

**Files:**
- Modify: `crates/banager-core/src/session/mod.rs`
- Test: `crates/banager-core/src/session/mod.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `BrewAdapter::new(runner: Arc<dyn CommandRunner>) -> BrewAdapter` (existing). Assumed, matching that shape and the per-adapter contract table's network column (Tasks 5–10, not fixed by the skeleton's Core Interfaces — see `unverified`): `NpmAdapter::new(runner: Arc<dyn CommandRunner>) -> NpmAdapter`, `UvAdapter::new(runner: Arc<dyn CommandRunner>) -> UvAdapter`, `PipAdapter::new(runner: Arc<dyn CommandRunner>) -> PipAdapter`, `PipxAdapter::new(runner: Arc<dyn CommandRunner>, http: Arc<dyn HttpClient>) -> PipxAdapter`, `CargoAdapter::new(runner: Arc<dyn CommandRunner>, http: Arc<dyn HttpClient>) -> CargoAdapter`, `OllamaAdapter::new(runner: Arc<dyn CommandRunner>, http: Arc<dyn HttpClient>) -> OllamaAdapter`. `RealHttpClient::new() -> RealHttpClient`, `HttpClient` trait (Task 1). `RealRunner::new() -> RealRunner`, `CommandRunner` trait (existing).
- Produces: `Session::adapter_ids(&self) -> Vec<AdapterId>` — sorted ids of every registered adapter, for later tasks (and this task's own test) to verify registration without depending on what is actually installed on the test machine.
- Note on signatures: Task 2 already changed `Session::refresh` to `pub async fn refresh(self: &Arc<Self>, env: &HostEnv, opts: &CheckOptions) -> Snapshot` and `Adapter::check_updates` to take `opts: &CheckOptions`, and Task 4 added `ManagerInstance.unverified_version`. Every call site and literal written in this task and Tasks 12-14 uses those shapes; `CheckOptions` is already in `session/mod.rs`'s import list from Task 2.

- [ ] **Step 1: Write the failing test for full registration**

Add to `crates/banager-core/src/session/mod.rs`'s `#[cfg(test)] mod tests`, right after the existing `root_env()` helper:

```rust
    #[test]
    fn test_new_registers_all_seven_adapters() {
        let sink = Arc::new(VecSink::new());
        let session = Session::new(sink, None);
        assert_eq!(
            session.adapter_ids(),
            vec![
                "brew".to_string(),
                "cargo".to_string(),
                "npm".to_string(),
                "ollama".to_string(),
                "pip".to_string(),
                "pipx".to_string(),
                "uv".to_string(),
            ]
        );
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p banager-core session::tests::test_new_registers_all_seven_adapters`
Expected: FAIL to compile — `no method named 'adapter_ids' found for struct 'Session'`.

- [ ] **Step 3: Register all seven adapters and add `adapter_ids()`**

Replace `session/mod.rs`'s top imports with (note `CheckOptions`, which Task 2 added and `refresh`'s signature still needs — dropping it here would break the build):

```rust
use crate::adapters::brew::BrewAdapter;
use crate::adapters::cargo::CargoAdapter;
use crate::adapters::npm::NpmAdapter;
use crate::adapters::ollama::OllamaAdapter;
use crate::adapters::pip::PipAdapter;
use crate::adapters::pipx::PipxAdapter;
use crate::adapters::uv::UvAdapter;
use crate::adapters::{Adapter, AdapterError, CheckOptions};
use crate::events::{EventSink, OpId};
use crate::http::{HttpClient, RealHttpClient};
use crate::model::{
    AdapterId, InstalledArtifact, InstanceId, ManagerInstance, OpRequest, Plan, ResourceLock,
    UpdateCandidate,
};
use crate::ops::{OpSummary, OperationManager};
use crate::runner::{CommandRunner, HostEnv, RealRunner};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
```

Replace the body of `Session::new` with:

```rust
    /// Registers all seven adapters (Task 11) over a shared `RealRunner`
    /// and `RealHttpClient` (network-touching adapters only: pipx, cargo,
    /// ollama, per the per-adapter contract table). `now_fn` exists so
    /// tests can pin `refreshed_at`; production passes `None`.
    pub fn new(sink: Arc<dyn EventSink>, now_fn: Option<fn() -> i64>) -> Arc<Session> {
        let runner: Arc<dyn CommandRunner> = Arc::new(RealRunner::new());
        let http: Arc<dyn HttpClient> = Arc::new(RealHttpClient::new());
        let adapters: Vec<Arc<dyn Adapter>> = vec![
            Arc::new(BrewAdapter::new(runner.clone())),
            Arc::new(NpmAdapter::new(runner.clone())),
            Arc::new(PipxAdapter::new(runner.clone(), http.clone())),
            Arc::new(UvAdapter::new(runner.clone())),
            Arc::new(PipAdapter::new(runner.clone())),
            Arc::new(CargoAdapter::new(runner.clone(), http.clone())),
            Arc::new(OllamaAdapter::new(runner, http)),
        ];
        Session::with_adapters(sink, adapters, now_fn)
    }
```

Add a new method right after `operations()`:

```rust
    /// Sorted ids of every adapter this Session has registered, regardless
    /// of whether that adapter currently detects any instance on the host.
    /// A test seam (Task 11) so registration itself is verifiable without
    /// depending on which tools happen to be installed on the machine
    /// running the test.
    pub fn adapter_ids(&self) -> Vec<AdapterId> {
        let mut ids: Vec<AdapterId> = self.adapters.keys().cloned().collect();
        ids.sort();
        ids
    }
```

- [ ] **Step 4: Run it to verify it passes**

Run: `cargo test -p banager-core session::tests::test_new_registers_all_seven_adapters`
Expected: PASS (1 passed).

- [ ] **Step 5: Write the failing test for concurrent cross-adapter detect**

Add to the same `mod tests`:

```rust
    #[tokio::test]
    async fn test_refresh_detects_across_adapters_concurrently_so_a_slow_source_does_not_block_others(
    ) {
        // Regression guard: detect() used to run in a plain sequential loop
        // over `self.adapters.values()`, so a slow adapter (e.g. Ollama
        // probing an unresponsive daemon) delayed every adapter registered
        // after it. Two adapters each delayed 200ms must finish in well
        // under their sum (400ms) once detect() fans out concurrently.
        let (slow_a, state_a) = FakeAdapter::new("slow-a");
        let (slow_b, state_b) = FakeAdapter::new("slow-b");
        state_a.lock().unwrap().detect_delay = Duration::from_millis(200);
        state_a.lock().unwrap().instances = vec![make_instance("slow-a", "slow-a:1")];
        state_b.lock().unwrap().detect_delay = Duration::from_millis(200);
        state_b.lock().unwrap().instances = vec![make_instance("slow-b", "slow-b:1")];
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![slow_a, slow_b], None);

        let started = Instant::now();
        let snapshot = session.refresh(&non_root_env(), &CheckOptions::default()).await;
        let elapsed = started.elapsed();

        assert!(snapshot.instances.iter().any(|i| i.id == "slow-a:1"));
        assert!(snapshot.instances.iter().any(|i| i.id == "slow-b:1"));
        assert!(
            elapsed < Duration::from_millis(350),
            "two 200ms detects must overlap, not run back to back (took {elapsed:?})"
        );
    }
```

- [ ] **Step 6: Run it to verify it fails**

Run: `cargo test -p banager-core session::tests::test_refresh_detects_across_adapters_concurrently_so_a_slow_source_does_not_block_others`
Expected: FAIL — `two 200ms detects must overlap, not run back to back (took ~400ms)`, since `refresh()`'s detect loop is still sequential.

- [ ] **Step 7: Make detect() run per adapter, concurrently**

In `session/mod.rs`'s `refresh()`, replace:

```rust
        let mut instances = Vec::new();
        for adapter in self.adapters.values() {
            instances.extend(adapter.detect(env).await);
        }
        for inst in &instances {
            self.ops.register_instance(inst.clone());
        }
        let detect = if instances.is_empty() {
            DetectOutcome::Missing
        } else {
            DetectOutcome::Found
        };
```

with:

```rust
        // Task 11: each adapter's detect() runs in its own spawned task, so
        // a slow or failing source (e.g. an Ollama daemon that is not
        // answering) can never delay any other adapter's detection -- only
        // its own instances arrive late, the same guarantee the
        // per-instance inventory/check_updates fetch below already gives
        // each instance.
        let mut detect_handles = Vec::with_capacity(self.adapters.len());
        for adapter in self.adapters.values().cloned() {
            let env = env.clone();
            detect_handles.push((
                adapter.meta().id.clone(),
                tokio::spawn(async move { adapter.detect(&env).await }),
            ));
        }
        let mut instances = Vec::new();
        let mut detect_errors = Vec::new();
        for (adapter_id, handle) in detect_handles {
            match handle.await {
                Ok(found) => instances.extend(found),
                Err(_join_err) => {
                    detect_errors.push(SourceError {
                        instance_id: adapter_id,
                        message: "internal error detecting this source".to_string(),
                    });
                }
            }
        }
        for inst in &instances {
            self.ops.register_instance(inst.clone());
        }
        let detect = if instances.is_empty() {
            DetectOutcome::Missing
        } else {
            DetectOutcome::Found
        };
```

Then, a few lines further down in the same method, replace:

```rust
        let mut artifacts = Vec::new();
        let mut updates = Vec::new();
        let mut errors = Vec::new();
        let mut stale = false;
        for (instance_id, handle) in handles {
```

with:

```rust
        let mut artifacts = Vec::new();
        let mut updates = Vec::new();
        let mut errors = detect_errors;
        let mut stale = !errors.is_empty();
        for (instance_id, handle) in handles {
```

(The rest of that loop, and everything after it, is unchanged: a source that fails to detect contributes exactly one `SourceError` keyed by its adapter id and sets `stale`, exactly like a per-instance inventory/check_updates failure does; it never aborts the refresh, since `detect_handles`'/`handles`' loops both run every entry to completion regardless of any single one's outcome.)

- [ ] **Step 8: Run it to verify it passes**

Run: `cargo test -p banager-core session::tests::test_refresh_detects_across_adapters_concurrently_so_a_slow_source_does_not_block_others`
Expected: PASS (elapsed comfortably under 350ms).

- [ ] **Step 9: Write the failing test for an unhealthy instance**

Add to the same `mod tests`:

```rust
    #[tokio::test]
    async fn test_an_unhealthy_instance_is_a_reported_state_not_a_failed_refresh() {
        // A source that is installed but known not to be running -- Ollama
        // with its daemon down is the case this phase adds, since
        // OllamaAdapter::detect returns an instance with healthy:false rather
        // than no instance at all -- must not be fanned out to. Its
        // inventory() would fail, push a SourceError, set `stale`, and so
        // carry `refreshed_at` forward unchanged: on that machine
        // `refreshed_at` would stay None forever, the stale banner would
        // never clear, and the front end would keep reading "no refresh has
        // ever finished" however many brew/npm/pipx refreshes succeeded --
        // all while Task 12's "Ollama isn't running" notice renders right
        // next to it saying exactly what is going on.
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            let mut down = make_instance("fake", "fake:down");
            down.healthy = false;
            s.instances = vec![make_instance("fake", "fake:up"), down];
            s.artifacts
                .insert("fake:up".to_string(), vec![make_artifact("fake:up", "jq")]);
            // If refresh ever does fan out to the unhealthy instance, this
            // makes it fail loudly rather than pass by accident.
            s.failing.push("fake:down".to_string());
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);

        let snapshot = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;

        assert!(
            snapshot.refreshed_at.is_some(),
            "a refresh whose only complaint is a source known not to be running has completed"
        );
        assert!(!snapshot.stale);
        assert!(snapshot.errors.is_empty());
        assert!(
            snapshot.instances.iter().any(|i| i.id == "fake:down"),
            "the unhealthy instance stays in the snapshot so the UI can offer to start it"
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
```

Run: `cargo test -p banager-core session::tests::test_an_unhealthy_instance_is_a_reported_state_not_a_failed_refresh`
Expected: FAIL — `snapshot.errors` has one entry for `fake:down`, `stale` is true, and `refreshed_at` is `None`, because `refresh()` still fans out to every detected instance regardless of `healthy`.

- [ ] **Step 10: Skip the fan-out for an unhealthy instance**

In `session/mod.rs`'s `refresh()`, replace:

```rust
        let mut handles = Vec::with_capacity(instances.len());
        for inst in instances.clone() {
            let Some(adapter) = self.adapters.get(&inst.adapter_id).cloned() else {
                continue;
            };
```

with:

```rust
        let mut handles = Vec::with_capacity(instances.len());
        for inst in instances.clone() {
            // An instance the adapter reported as `healthy: false` is a
            // *reported state*, not a failed refresh: the adapter already
            // knows the source is not answering and said so. Fanning out to
            // it would fail, push a SourceError, set `stale` -- and therefore
            // carry `refreshed_at` forward instead of stamping it -- leaving
            // the whole snapshot permanently stale on a machine where, say,
            // Ollama is installed but not running. It stays in
            // `snapshot.instances` so the UI can render its notice (Task 12)
            // and offer to start it.
            if !inst.healthy {
                continue;
            }
            let Some(adapter) = self.adapters.get(&inst.adapter_id).cloned() else {
                continue;
            };
```

Run: `cargo test -p banager-core session::`
Expected: PASS — the new test, plus every pre-existing `session::tests::*` test unchanged (they all build instances with `healthy: true`).

- [ ] **Step 11: Run the full gate**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: all clean; the full `banager-core` and `banager` test suites pass, including every pre-existing `session::tests::*` test unchanged.

- [ ] **Step 12: Commit**

```bash
git add crates/banager-core/src/session/mod.rs
git commit -m "$(cat <<'EOF'
feat(session): register all seven adapters and detect concurrently

Session::new now wires npm, pipx, uv, pip, cargo and ollama alongside
brew over a shared RealRunner/RealHttpClient. refresh()'s detect phase
now fans out one task per adapter instead of looping sequentially, so a
slow or unresponsive source (most concretely: an Ollama daemon that is
not running) can never delay detection for any other source. An
instance detected as unhealthy is skipped by the inventory/updates
fan-out instead of being refreshed and failing: it is a reported
state, not a failed refresh, so it no longer holds refreshed_at at
None and the snapshot at stale forever.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 12: Per-source UI affordances

**Files:**
- Modify: `src-tauri/src/ipc.rs` (add `open_ollama_app`)
- Modify: `src-tauri/src/lib.rs`
- Create: `src/components/SourceNotice.tsx`
- Test: `src/components/SourceNotice.test.tsx`
- Modify: `src/components/ArtifactRow.tsx`
- Test: `src/components/ArtifactRow.test.tsx`
- Modify: `src/lib/api.ts`
- Modify: `src/lib/queries.ts`
- Modify: `src/lib/types.ts`
- Modify: `src/pages/InstalledPage.tsx`
- Test: `src/pages/InstalledPage.test.tsx`
- Modify: `src/pages/UpdatesPage.tsx`
- Test: `src/pages/UpdatesPage.test.tsx`
- Modify: `src/pages/SettingsPage.tsx`
- Test: `src/pages/SettingsPage.test.tsx`
- Modify: `src/i18n/en.json`
- Modify: `src/i18n/zh-CN.json`

**Interfaces:**
- Consumes: `ManagerInstance.adapter_id`, `ManagerInstance.healthy` (existing fields, model.rs). `IssuedPlan.plan.warnings: string[]` (existing `Plan.warnings`, previously never rendered by `UpdatesPage`). `useSnapshot`/`useSettings` (existing).
- Also consumes: `UpdateCandidate.checkable` (existing `model.rs` field, produced by brew, npm, cargo, pipx and ollama, and **read by nothing** before this task), `ManagerInstance.unverified_version` and its `installed.unverifiedVersion` badge (Task 4 — this task edits `InstalledPage.tsx` on top of Task 4's version of it and must not revert that badge), and `Settings.include_self_updating` (Task 2's Rust field, which reaches no UI until this task adds it).
- Produces: `SourceNotice` component (`variant: "info" | "warning"`, `title`, `description?`, `action?: { label, onClick }`). `ArtifactRow`'s `primaryActionLabel`/`onPrimaryAction` become optional (a row with neither renders no primary-action button). `openOllamaApp(): Promise<void>` (`src/lib/api.ts`) and `useOpenOllamaApp(): UseMutationResult<void, Error, void>` (`src/lib/queries.ts`), backed by a new `open_ollama_app` Tauri command. `src/lib/types.ts`'s `Settings` gains `include_self_updating: boolean` and `SettingsPage.tsx` gains its toggle — without the TypeScript field, `SettingsPage`'s `persist()` sends a payload with no `include_self_updating`, and the Rust side's `#[serde(default)]` then silently resets it to `false` every time any *other* setting is saved.

- [ ] **Step 1: Write the failing tests for the shared plumbing**

`src-tauri/src/ipc.rs` — add to `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn test_open_ollama_app_argv_is_exactly_open_dash_a_ollama() {
        // The argv is the whole contract of this command: it must launch
        // Ollama.app and nothing else, and it takes no input, so there is
        // nothing a caller could steer. Asserted from a pure builder so the
        // test never starts a process.
        let (program, args) = open_ollama_app_argv();
        assert_eq!(program, std::path::Path::new("/usr/bin/open"));
        assert_eq!(args, vec!["-a".to_string(), "Ollama".to_string()]);
    }

    #[test]
    fn test_open_ollama_app_impl_with_spawns_and_reaps_the_program_it_is_given() {
        // Deliberately /bin/echo, not /usr/bin/open: `cargo test --workspace`
        // is this plan's definition of done for every task, so a test that
        // really ran `open -a Ollama` would launch Ollama.app on the
        // developer's machine and on CI -- exactly the side effect this
        // phase's own constraint ("a background refresh never launches an
        // application") exists to prevent -- while asserting nothing beyond
        // "spawn did not error", which is true of any existing binary.
        open_ollama_app_impl_with(std::path::Path::new("/bin/echo"))
            .expect("spawning an existing program must succeed");
    }

    #[test]
    fn test_open_ollama_app_impl_with_reports_a_missing_program_instead_of_panicking() {
        let err = open_ollama_app_impl_with(std::path::Path::new("/definitely/not/a/program"))
            .expect_err("a missing program must be an Err, not a panic");
        assert!(!err.is_empty());
    }
```

`src/components/SourceNotice.test.tsx` (new file):

```tsx
import { describe, expect, it, vi } from "vitest";
import { fireEvent, waitFor } from "@testing-library/react";
import { renderWithProviders } from "../test/setup";
import { SourceNotice } from "./SourceNotice";

describe("SourceNotice", () => {
  it("renders a title and description with no action button when none is given", () => {
    const { getByText, queryByRole } = renderWithProviders(
      <SourceNotice variant="info" title="Read-only" description="Cannot be changed here." />,
    );
    expect(getByText("Read-only")).toBeInTheDocument();
    expect(getByText("Cannot be changed here.")).toBeInTheDocument();
    expect(queryByRole("button")).not.toBeInTheDocument();
  });

  it("renders and fires the action button when one is given", async () => {
    const onClick = vi.fn();
    const { getByRole } = renderWithProviders(
      <SourceNotice variant="warning" title="Not running" action={{ label: "Open it", onClick }} />,
    );
    fireEvent.click(getByRole("button", { name: "Open it" }));
    await waitFor(() => expect(onClick).toHaveBeenCalledTimes(1));
  });
});
```

`src/components/ArtifactRow.test.tsx` — add:

```tsx
  it("renders no primary action button when the row offers none", () => {
    const { queryByRole } = renderWithProviders(
      <ArtifactRow name="numpy" description="desc" badgeText="Up to date" badgeVariant="neutral" />,
    );
    expect(queryByRole("button")).not.toBeInTheDocument();
  });
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p banager ipc::tests::open_ollama_app`
Expected: FAIL to compile — `cannot find function 'open_ollama_app_argv' in this scope` (and the same for `open_ollama_app_impl_with`).

Run: `pnpm exec vitest run src/components/SourceNotice.test.tsx src/components/ArtifactRow.test.tsx`
Expected: FAIL — `SourceNotice.test.tsx` fails to resolve `./SourceNotice`; the new `ArtifactRow` test fails because a button is still rendered (`queryByRole("button")` finds one).

- [ ] **Step 3: Implement the shared plumbing**

`src-tauri/src/ipc.rs` — add the command, after `subscribe_events`:

```rust
/// The exact program and argv this command runs. A pure builder so a test
/// can assert the contract -- launch Ollama.app, nothing else -- without
/// starting a process.
fn open_ollama_app_argv() -> (&'static std::path::Path, Vec<String>) {
    (
        std::path::Path::new("/usr/bin/open"),
        vec!["-a".to_string(), "Ollama".to_string()],
    )
}

/// Fire-and-forget: launches (or focuses) the Ollama.app the user already
/// has installed, for the "Ollama isn't running" notice's button. Takes no
/// input at all, so there is nothing here for the front end to build an
/// argv from or for a caller to influence -- unlike a package operation,
/// this never goes through Session/Plan because it is not a package
/// management action.
///
/// `program` is a parameter purely so tests can point it at an inert binary:
/// `cargo test --workspace` runs on the developer's machine and on CI, and a
/// test that really ran `open -a Ollama` would launch a GUI app on both.
fn open_ollama_app_impl_with(program: &std::path::Path) -> Result<(), String> {
    let (_default_program, args) = open_ollama_app_argv();
    let mut child = std::process::Command::new(program)
        .args(&args)
        .spawn()
        .map_err(|e| e.to_string())?;
    // Reap on a background thread instead of leaving a zombie: `open` exits
    // almost immediately once it has handed off to (or failed to find)
    // Ollama.app, and this command must return without waiting for that.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

pub(crate) fn open_ollama_app_impl() -> Result<(), String> {
    let (program, _args) = open_ollama_app_argv();
    open_ollama_app_impl_with(program)
}

#[tauri::command]
pub async fn open_ollama_app() -> Result<(), String> {
    open_ollama_app_impl()
}
```

`src-tauri/src/lib.rs` — add `ipc::open_ollama_app,` to the `invoke_handler` list, right after `ipc::subscribe_events,`.

`src/components/SourceNotice.tsx` (new file):

```tsx
export type SourceNoticeVariant = "info" | "warning";

export interface SourceNoticeAction {
  label: string;
  onClick: () => void;
}

export interface SourceNoticeProps {
  variant: SourceNoticeVariant;
  title: string;
  description?: string;
  action?: SourceNoticeAction;
}

const VARIANT_CLASSES: Record<SourceNoticeVariant, string> = {
  info: "bg-[var(--color-hover)] text-[var(--color-foreground)]",
  warning: "bg-[var(--color-danger)]/10 text-[var(--color-danger)]",
};

/**
 * A per-source banner rendered under an Installed-page group header: pip's
 * read-only note, or Ollama's "daemon not running" notice with a button to
 * start it (Task 12). Purely presentational -- callers decide when it
 * applies and what its action does; this component never calls `invoke`.
 */
export function SourceNotice({ variant, title, description, action }: SourceNoticeProps) {
  return (
    <div
      className={`mb-2 mt-1 flex items-center justify-between gap-3 rounded-md px-3 py-2 text-sm ${VARIANT_CLASSES[variant]}`}
    >
      <div className="min-w-0">
        <p className="font-medium">{title}</p>
        {description ? <p className="mt-0.5 text-xs opacity-80">{description}</p> : null}
      </div>
      {action ? (
        <button
          type="button"
          onClick={action.onClick}
          className="shrink-0 rounded-md bg-[var(--color-accent)] px-3 py-1 text-xs font-medium text-[var(--color-accent-foreground)]"
        >
          {action.label}
        </button>
      ) : null}
    </div>
  );
}
```

`src/components/ArtifactRow.tsx` — replace the whole file:

```tsx
import type { ReactNode } from "react";

export type BadgeVariant = "neutral" | "warning" | "info";

export interface ArtifactRowSelectable {
  checked: boolean;
  onToggle: () => void;
  ariaLabel: string;
}

export interface ArtifactRowProps {
  name: string;
  description: string;
  badgeText: string;
  badgeVariant: BadgeVariant;
  /** Omit both this and `onPrimaryAction` for a row with no primary action
   * at all (Task 12: a read-only source, e.g. pip, offers no uninstall). */
  primaryActionLabel?: string;
  onPrimaryAction?: () => void;
  primaryActionDisabled?: boolean;
  selectable?: ArtifactRowSelectable;
  /** Extra inline content between the description and the badge (Task 12 uses this for an Ignore link). */
  secondaryContent?: ReactNode;
}

const BADGE_CLASSES: Record<BadgeVariant, string> = {
  neutral: "bg-[var(--color-hover)] text-[var(--color-muted)]",
  warning: "bg-[var(--color-danger)]/10 text-[var(--color-danger)]",
  info: "bg-[var(--color-accent)]/10 text-[var(--color-accent)]",
};

export function ArtifactRow({
  name,
  description,
  badgeText,
  badgeVariant,
  primaryActionLabel,
  onPrimaryAction,
  primaryActionDisabled,
  selectable,
  secondaryContent,
}: ArtifactRowProps) {
  return (
    <div className="flex items-center gap-3 border-b border-[var(--color-border)] px-4 py-2">
      {selectable ? (
        <input
          type="checkbox"
          aria-label={selectable.ariaLabel}
          checked={selectable.checked}
          onChange={selectable.onToggle}
          className="h-4 w-4 shrink-0"
        />
      ) : null}
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium text-[var(--color-foreground)]">{name}</p>
        <p className="truncate text-xs text-[var(--color-muted)]">{description}</p>
      </div>
      {secondaryContent}
      <span
        className={`shrink-0 rounded-full px-2 py-1 text-xs font-medium ${BADGE_CLASSES[badgeVariant]}`}
      >
        {badgeText}
      </span>
      {primaryActionLabel && onPrimaryAction ? (
        <button
          type="button"
          onClick={onPrimaryAction}
          disabled={primaryActionDisabled}
          className="shrink-0 rounded-md bg-[var(--color-accent)] px-3 py-1 text-sm font-medium text-[var(--color-accent-foreground)] disabled:opacity-50"
        >
          {primaryActionLabel}
        </button>
      ) : null}
    </div>
  );
}
```

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p banager ipc::tests::open_ollama_app`
Expected: PASS (3 tests), and no application launches.

Run: `pnpm exec vitest run src/components/SourceNotice.test.tsx src/components/ArtifactRow.test.tsx`
Expected: PASS (5 passed: 2 new `SourceNotice` tests, 1 new `ArtifactRow` test, plus the 2 pre-existing `ArtifactRow` tests unaffected).

- [ ] **Step 5: Write the failing tests for the three page-level affordances**

`src/lib/api.ts` and `src/lib/queries.ts` do not yet export `openOllamaApp`/`useOpenOllamaApp`, so `InstalledPage.test.tsx`'s new test below fails to compile until Step 7; write it anyway now, alongside `UpdatesPage.test.tsx`'s.

`src/pages/InstalledPage.test.tsx` — add two tests, after the existing `describe("InstalledPage", ...)` block's last test:

```tsx
  it("hides the uninstall button and shows a read-only note for pip rows", async () => {
    const pipSnapshot: Snapshot = {
      generation: 1,
      detect: "Found",
      instances: [
        {
          id: "pip:/usr/bin/python3",
          adapter_id: "pip",
          exe_path: "/usr/bin/python3",
          prefix: "/usr",
          scope: "User",
          version: "26.2.1",
          healthy: true,
          unverified_version: null,
        },
      ],
      artifacts: [
        {
          key: { instance_id: "pip:/usr/bin/python3", kind: "Package", name: "requests" },
          display_name: "requests",
          version: "2.32.3",
          // Unknown, not Requested: pip's `--not-required` marks a leaf
          // package, which is not the same as "the user asked for it", so
          // Task 8's adapter can only ever emit Unknown or Dependency here.
          // A "Requested" fixture would pass against data pip cannot produce.
          reason: "Unknown",
          description: "Python HTTP for Humans.",
          homepage: null,
          size_bytes: null,
          installed_at: null,
          path: null,
          auto_updates: false,
        },
      ],
      updates: [],
      refreshed_at: 1789700000,
      stale: false,
      errors: [],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(pipSnapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      return Promise.resolve(undefined);
    });

    const { findByText, queryByRole } = renderWithProviders(<InstalledPage />);

    await findByText("requests");
    expect(queryByRole("button", { name: "Uninstall" })).not.toBeInTheDocument();
    expect(await findByText("Read-only: pip packages")).toBeInTheDocument();
  });

  it("shows a not-running notice with an Open Ollama button when the instance is unhealthy", async () => {
    const ollamaSnapshot: Snapshot = {
      generation: 1,
      detect: "Found",
      instances: [
        {
          id: "ollama:http://127.0.0.1:11434",
          adapter_id: "ollama",
          exe_path: "/usr/local/bin/ollama",
          prefix: "/usr/local",
          scope: "User",
          version: null,
          healthy: false,
          unverified_version: null,
        },
      ],
      artifacts: [],
      updates: [],
      refreshed_at: 1789700000,
      stale: false,
      errors: [],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(ollamaSnapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      if (cmd === "open_ollama_app") return Promise.resolve(undefined);
      return Promise.resolve(undefined);
    });

    const { findByText, getByRole } = renderWithProviders(<InstalledPage />);

    await findByText("Ollama isn't running");
    fireEvent.click(getByRole("button", { name: "Open Ollama" }));

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("open_ollama_app"));
  });
```

`src/pages/SettingsPage.test.tsx` — add a test for the new toggle, after the last existing test in `describe("SettingsPage", ...)`:

```tsx
  it("round-trips the include-self-updating toggle through set_settings", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings();
      if (cmd === "set_settings") return undefined;
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const toggle = await screen.findByRole("switch", { name: "Include self-updating apps" });
    expect(toggle).not.toBeChecked();
    fireEvent.click(toggle);

    await waitFor(() =>
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("set_settings", {
        settings: expect.objectContaining({ include_self_updating: true }),
      }),
    );
  });
```

and add the new field to the shared `baseSettings` helper at the top of that file, so every other test in it keeps type-checking:

```ts
function baseSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
    include_self_updating: false,
    ...overrides,
  };
}
```

`src/pages/UpdatesPage.test.tsx` — add a `planWarnings` knob next to the existing `needsPassword` one. Change:

```ts
let needsPassword: Set<string>;
```

to:

```ts
let needsPassword: Set<string>;
let planWarnings: Record<string, string[]>;
```

In `beforeEach`, change:

```ts
  needsPassword = new Set();
```

to:

```ts
  needsPassword = new Set();
  planWarnings = {};
```

In `issuedPlanFor`, change:

```ts
      needs_password: needsPassword.has(request.name),
      locks: ["brew:/opt/homebrew"],
      cancel_policy: "KillThenReconcile",
      warnings: [],
```

to:

```ts
      needs_password: needsPassword.has(request.name),
      locks: ["brew:/opt/homebrew"],
      cancel_policy: "KillThenReconcile",
      warnings: planWarnings[request.name] ?? [],
```

Add two new tests in `describe("UpdatesPage", ...)`, after `"warns per item, before the sudo prompt, about the one update that needs a password"`:

```tsx
  it("shows a warning carried on the plan, such as cargo's compile-locally notice", async () => {
    planWarnings.glib = ["This will compile locally and can take several minutes."];
    const { findAllByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    const dialog = await findByRole("dialog");
    await within(dialog).findByText(
      "This will compile locally and can take several minutes.",
    );
  });

  it("offers no Update button and no checkbox for a candidate the adapter could not check", async () => {
    // The whole point of UpdateCandidate.checkable. A git-sourced cargo
    // crate reports checkable:false because crates.io knows nothing about
    // it -- and "Update" on such a row would run `cargo install --force
    // my-fork` against the crates.io crate of the same name, a different
    // package entirely. The same flag covers an Ollama model whose manifest
    // could not be read and a pipx tool whose PyPI lookup failed.
    updates = [
      {
        key: { instance_id: "cargo:/Users/brulek/.cargo", kind: "Binary", name: "my-fork" },
        current: "0.1.0",
        target: "0.1.0",
        channel: "Registry",
        checkable: false,
        warnings: ["installed from git, cannot check crates.io for updates"],
      },
    ];
    const { findByText, queryByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("my-fork");
    expect(queryByRole("button", { name: "Update" })).not.toBeInTheDocument();
    expect(queryByRole("checkbox")).not.toBeInTheDocument();
    expect(
      await findByText("installed from git, cannot check crates.io for updates"),
    ).toBeInTheDocument();
  });
```

(`updates` is this file's existing mutable knob — `beforeEach` sets it to `snapshot.updates` and the `get_snapshot` mock returns `{ ...snapshot, updates }` — so assigning it is how every other test in this file varies the update list.)

- [ ] **Step 6: Run them to verify they fail**

Run: `pnpm exec vitest run src/pages/InstalledPage.test.tsx src/pages/UpdatesPage.test.tsx src/pages/SettingsPage.test.tsx`
Expected: FAIL — the pip test cannot find "Uninstall" absent (a button still renders) nor the text "Read-only: pip packages"; the Ollama test cannot find "Ollama isn't running"; the warnings test cannot find the compile-locally text (nothing renders `plan.warnings` yet); the uncheckable test still finds an "Update" button and a checkbox (nothing reads `checkable` yet); the settings test cannot find a switch named "Include self-updating apps", and `pnpm exec tsc -p tsconfig.json` additionally fails on `include_self_updating` not existing on `Settings`.

- [ ] **Step 7: Wire the three affordances into the pages**

`src/lib/api.ts` — add, after `subscribeEvents`:

```ts
export function openOllamaApp(): Promise<void> {
  return call<void>("open_ollama_app");
}
```

`src/lib/queries.ts` — add `openOllamaApp` to the existing `import { ... } from "./api";` list, and add a new hook after `useCancelOperation`:

```ts
export function useOpenOllamaApp(): UseMutationResult<void, Error, void> {
  return useMutation({ mutationFn: openOllamaApp });
}
```

`src/pages/InstalledPage.tsx` — this file already carries Task 4's unverified-version badge, so **edit it, do not replace it with a pre-Task-4 copy**. The listing below is the post-Task-4 file with this task's changes applied; the changes are exactly: the `ADAPTER_LABEL_KEYS` entries for the six new sources, `READ_ONLY_ADAPTER_IDS`, `adapterId`/`healthy` on the `ListItem` variants, the `needsNotice` guard, the two `SourceNotice` renders, the `reason !== "Dependency"` split, and the conditional `primaryActionLabel`/`onPrimaryAction`. Task 4's `unverifiedVersion` field and its badge render are carried through unchanged — if they are missing after this step, Task 4 has been reverted and its own test (`"shows an unverified-version badge…"`) will say so:

```tsx
import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useSnapshot, useSettings, useOpenOllamaApp } from "../lib/queries";
import { useUiStore, artifactKeyId } from "../store/ui";
import { ArtifactRow } from "../components/ArtifactRow";
import { SourceNotice } from "../components/SourceNotice";
import { UninstallDialog } from "../components/UninstallDialog";
import type { InstalledArtifact, OpRequest } from "../lib/types";

const ADAPTER_LABEL_KEYS: Record<string, string> = {
  brew: "adapters.brew",
  npm: "adapters.npm",
  pipx: "adapters.pipx",
  uv: "adapters.uv",
  pip: "adapters.pip",
  cargo: "adapters.cargo",
  ollama: "adapters.ollama",
};

// pip can only report what is installed; it offers no install/uninstall
// path Banager could safely drive (spec's per-adapter contract table).
// Read-only here is a presentational fact about that one source, not a
// judgement call the UI is making on its own.
const READ_ONLY_ADAPTER_IDS = new Set(["pip"]);

type ListItem =
  | {
      type: "group";
      instanceId: string;
      label: string;
      adapterId: string;
      healthy: boolean;
      // Task 4's unverified-version badge. Kept here deliberately: this
      // task edits Task 4's file rather than replacing it.
      unverifiedVersion: string | null;
    }
  | { type: "artifact"; artifact: InstalledArtifact; adapterId: string }
  | { type: "toggle"; instanceId: string; hiddenCount: number };

export function InstalledPage() {
  const { t } = useTranslation();
  const { data: snapshot, isLoading } = useSnapshot();
  const { data: settings } = useSettings();
  const openOllamaApp = useOpenOllamaApp();
  const query = useUiStore((s) => s.query);
  const setQuery = useUiStore((s) => s.setQuery);
  const showDependencies = useUiStore((s) => s.showDependencies);
  const toggleDependencies = useUiStore((s) => s.toggleDependencies);
  const setFocusedOpId = useUiStore((s) => s.setFocusedOpId);
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  const parentRef = useRef<HTMLDivElement>(null);

  // Uninstall is destructive, so the row's button only *targets* an artifact;
  // UninstallDialog is what plans it, shows the exact command and what would
  // break, and submits (Global Constraints, spec §6).
  const [uninstallTarget, setUninstallTarget] = useState<{
    request: OpRequest;
    displayName: string;
  } | null>(null);

  const updatableIds = useMemo(
    () => new Set((snapshot?.updates ?? []).map((u) => artifactKeyId(u.key))),
    [snapshot],
  );

  const items = useMemo<ListItem[]>(() => {
    if (!snapshot) return [];
    const needle = query.trim().toLowerCase();
    const filtered = needle
      ? snapshot.artifacts.filter((a) => a.display_name.toLowerCase().includes(needle))
      : snapshot.artifacts;
    const byInstance = new Map<string, InstalledArtifact[]>();
    for (const artifact of filtered) {
      const list = byInstance.get(artifact.key.instance_id) ?? [];
      list.push(artifact);
      byInstance.set(artifact.key.instance_id, list);
    }
    const result: ListItem[] = [];
    for (const instance of snapshot.instances) {
      const artifacts = byInstance.get(instance.id) ?? [];
      // A source can need a notice (pip's read-only note, Ollama not
      // running) even with nothing installed to list under it -- most
      // visibly, an unhealthy Ollama daemon that has nothing to report yet.
      const needsNotice =
        READ_ONLY_ADAPTER_IDS.has(instance.adapter_id) ||
        (instance.adapter_id === "ollama" && !instance.healthy);
      if (artifacts.length === 0 && !needsNotice) continue;
      const labelKey = ADAPTER_LABEL_KEYS[instance.adapter_id];
      result.push({
        type: "group",
        instanceId: instance.id,
        label: labelKey ? t(labelKey) : instance.adapter_id,
        adapterId: instance.adapter_id,
        healthy: instance.healthy,
        unverifiedVersion: instance.unverified_version,
      });
      // `!== "Dependency"`, not `=== "Requested"`: pip can only ever report
      // Unknown or Dependency (its `--not-required` marks a leaf, which is
      // not the same as "the user asked for it"), so keying off "Requested"
      // would collapse every pip package behind "Show N dependencies" and
      // render the pip group as a header and a notice with no visible rows.
      const primary = artifacts.filter((a) => a.reason !== "Dependency");
      const dependencies = artifacts.filter((a) => a.reason === "Dependency");
      for (const artifact of primary) {
        result.push({ type: "artifact", artifact, adapterId: instance.adapter_id });
      }
      if (dependencies.length > 0) {
        if (showDependencies) {
          for (const artifact of dependencies) {
            result.push({ type: "artifact", artifact, adapterId: instance.adapter_id });
          }
        } else {
          result.push({ type: "toggle", instanceId: instance.id, hiddenCount: dependencies.length });
        }
      }
    }
    return result;
  }, [snapshot, query, showDependencies, t]);

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 56,
  });

  if (isLoading) {
    return <p className="p-4 text-sm text-[var(--color-muted)]">{t("common.loading")}</p>;
  }
  if (!snapshot) {
    return null;
  }

  return (
    <div className="flex h-full flex-col">
      <div className="p-4">
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t("installed.filterPlaceholder")}
          aria-label={t("installed.filterLabel")}
          className="w-full rounded-md border border-[var(--color-border)] bg-[var(--color-background)] px-3 py-2 text-sm"
        />
      </div>
      <div ref={parentRef} className="flex-1 overflow-y-auto">
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((virtualRow) => {
            const item = items[virtualRow.index];
            return (
              <div
                key={virtualRow.key}
                data-index={virtualRow.index}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  height: `${virtualRow.size}px`,
                  transform: `translateY(${virtualRow.start}px)`,
                }}
              >
                {item.type === "group" ? (
                  <div className="px-4 py-2">
                    <p className="text-xs font-semibold uppercase text-[var(--color-muted)]">
                      {item.label}
                      {item.unverifiedVersion ? (
                        <span className="ml-2 normal-case text-[var(--color-danger)]">
                          {t("installed.unverifiedVersion", { version: item.unverifiedVersion })}
                        </span>
                      ) : null}
                    </p>
                    {READ_ONLY_ADAPTER_IDS.has(item.adapterId) ? (
                      <SourceNotice
                        variant="info"
                        title={t("sourceNotice.pipReadOnly.title")}
                        description={t("sourceNotice.pipReadOnly.description")}
                      />
                    ) : null}
                    {item.adapterId === "ollama" && !item.healthy ? (
                      <SourceNotice
                        variant="warning"
                        title={t("sourceNotice.ollamaNotRunning.title")}
                        description={t("sourceNotice.ollamaNotRunning.description")}
                        action={{
                          label: t("sourceNotice.ollamaNotRunning.action"),
                          onClick: () => openOllamaApp.mutate(),
                        }}
                      />
                    ) : null}
                  </div>
                ) : item.type === "toggle" ? (
                  <button
                    type="button"
                    onClick={toggleDependencies}
                    className="px-4 py-2 text-left text-sm text-[var(--color-accent)]"
                  >
                    {t("installed.showDependencies", { count: item.hiddenCount })}
                  </button>
                ) : (
                  <ArtifactRow
                    name={
                      settings?.show_technical_details
                        ? t("installed.nameWithVersion", {
                            name: item.artifact.display_name,
                            version: item.artifact.version,
                          })
                        : item.artifact.display_name
                    }
                    description={item.artifact.description ?? t("installed.noDescription")}
                    badgeText={
                      updatableIds.has(artifactKeyId(item.artifact.key))
                        ? t("installed.updateAvailable")
                        : t("installed.upToDate")
                    }
                    badgeVariant={
                      updatableIds.has(artifactKeyId(item.artifact.key)) ? "info" : "neutral"
                    }
                    primaryActionLabel={
                      READ_ONLY_ADAPTER_IDS.has(item.adapterId) ? undefined : t("installed.uninstall")
                    }
                    onPrimaryAction={
                      READ_ONLY_ADAPTER_IDS.has(item.adapterId)
                        ? undefined
                        : () =>
                            setUninstallTarget({
                              request: {
                                kind: "Uninstall",
                                instance_id: item.artifact.key.instance_id,
                                artifact_kind: item.artifact.key.kind,
                                name: item.artifact.key.name,
                              },
                              displayName: item.artifact.display_name,
                            })
                    }
                  />
                )}
              </div>
            );
          })}
        </div>
      </div>
      {uninstallTarget ? (
        <UninstallDialog
          open
          onOpenChange={(open) => {
            if (!open) setUninstallTarget(null);
          }}
          request={uninstallTarget.request}
          displayName={uninstallTarget.displayName}
          onSubmitted={(opId) => {
            setUninstallTarget(null);
            setFocusedOpId(opId);
            setDrawerOpen(true);
          }}
        />
      ) : null}
    </div>
  );
}
```

`src/pages/UpdatesPage.tsx` — first, make the update list honour `UpdateCandidate.checkable`. Replace the `<ArtifactRow …>` in the `visibleUpdates.map(...)` block's props from `description` through `selectable`:

```tsx
            description={descriptionFor(candidate)}
            badgeText={
              candidate.warnings.length > 0
                ? t("updates.warnings", { count: candidate.warnings.length })
                : t("updates.available")
            }
            badgeVariant={candidate.warnings.length > 0 ? "warning" : "info"}
            primaryActionLabel={t("updates.update")}
            onPrimaryAction={() => openConfirm([candidate])}
            primaryActionDisabled={dialogOpen}
            selectable={{
              checked: selectedUpdates.includes(artifactKeyId(candidate.key)),
              onToggle: () => toggleUpdate(candidate.key),
              ariaLabel: t("updates.selectRow", { name: candidate.key.name }),
            }}
```

with:

```tsx
            // `checkable: false` means the adapter could not establish what
            // the remote version is -- a cargo crate installed from git or a
            // path, an Ollama model whose manifest could not be read, a pipx
            // tool whose PyPI lookup failed. Such a row must offer no action
            // and no selection: "Update" on a git-sourced crate would run
            // `cargo install --force {name}` against the crates.io crate of
            // the same name, which is a different package. The reason lives
            // in `warnings`, so it becomes the row's description.
            description={
              candidate.checkable
                ? descriptionFor(candidate)
                : candidate.warnings.join(" ")
            }
            badgeText={
              !candidate.checkable
                ? t("updates.cannotCheck")
                : candidate.warnings.length > 0
                  ? t("updates.warnings", { count: candidate.warnings.length })
                  : t("updates.available")
            }
            badgeVariant={
              !candidate.checkable
                ? "neutral"
                : candidate.warnings.length > 0
                  ? "warning"
                  : "info"
            }
            primaryActionLabel={candidate.checkable ? t("updates.update") : undefined}
            onPrimaryAction={
              candidate.checkable ? () => openConfirm([candidate]) : undefined
            }
            primaryActionDisabled={dialogOpen}
            selectable={
              candidate.checkable
                ? {
                    checked: selectedUpdates.includes(artifactKeyId(candidate.key)),
                    onToggle: () => toggleUpdate(candidate.key),
                    ariaLabel: t("updates.selectRow", { name: candidate.key.name }),
                  }
                : undefined
            }
```

(`ArtifactRow`'s `primaryActionLabel`, `onPrimaryAction` and `selectable` all became optional in Step 3, so no component change is needed.)

Then, in the confirm dialog's per-item block, replace:

```tsx
              {item.issued !== null ? (
                <CommandPreview program={item.issued.plan.program} args={item.issued.plan.args} />
              ) : null}
              {item.issued?.plan.needs_password ? (
```

with:

```tsx
              {item.issued !== null ? (
                <CommandPreview program={item.issued.plan.program} args={item.issued.plan.args} />
              ) : null}
              {item.issued && item.issued.plan.warnings.length > 0 ? (
                <ul className="list-disc pl-5 text-sm text-[var(--color-foreground)]">
                  {item.issued.plan.warnings.map((warning) => (
                    <li key={warning}>{warning}</li>
                  ))}
                </ul>
              ) : null}
              {item.issued?.plan.needs_password ? (
```

`src/lib/types.ts` — add the field Task 2 put on the Rust `Settings` but nothing has mirrored yet:

```ts
export interface Settings {
  language: Language;
  show_technical_details: boolean;
  ignored_updates: ArtifactKey[];
  include_self_updating: boolean;
}
```

Without this, `SettingsPage`'s `persist()` sends a `set_settings` payload with no `include_self_updating`, and the Rust side's `#[serde(default)]` resets it to `false` every time any other setting is saved.

`src/pages/SettingsPage.tsx` — add the toggle beside the existing technical-details one, immediately after that `<div className="flex items-center justify-between gap-4"> … </div>` block:

```tsx
      <div className="flex items-center justify-between gap-4">
        <div className="flex flex-col">
          <label htmlFor="settings-include-self-updating">
            {t("settings.includeSelfUpdating.label")}
          </label>
          <p
            id="settings-include-self-updating-desc"
            className="text-sm text-[var(--color-muted-foreground)]"
          >
            {t("settings.includeSelfUpdating.description")}
          </p>
        </div>
        <Switch
          id="settings-include-self-updating"
          aria-describedby="settings-include-self-updating-desc"
          checked={current.include_self_updating}
          onCheckedChange={(checked) =>
            persist({ ...current, include_self_updating: checked })
          }
        />
      </div>
```

`src/i18n/en.json` — add, inside `"adapters": { ... }`, after `"brew": "Homebrew"`:

```json
    "npm": "npm",
    "pipx": "pipx",
    "uv": "uv",
    "pip": "pip",
    "cargo": "Cargo",
    "ollama": "Ollama"
```

Add, inside `"updates": { ... }`, after `"available"`:

```json
    "cannotCheck": "Can't check",
```

Add, inside `"settings": { ... }`, after the `"showTechnicalDetails"` object:

```json
    "includeSelfUpdating": {
      "label": "Include self-updating apps",
      "description": "List apps that update themselves, like Chrome and Docker, as updatable here too."
    },
```

Add a new top-level key, after `"commandPreview": { ... },`:

```json
  "sourceNotice": {
    "pipReadOnly": {
      "title": "Read-only: pip packages",
      "description": "Banager can only show what's installed with pip, not update or uninstall it. Install Python command-line tools with pipx or uv instead to manage them here."
    },
    "ollamaNotRunning": {
      "title": "Ollama isn't running",
      "description": "Start the Ollama app to see its models and check for updates.",
      "action": "Open Ollama"
    }
  },
```

`src/i18n/zh-CN.json` — add, inside `"adapters": { ... }`, after `"brew": "Homebrew"`:

```json
    "npm": "npm",
    "pipx": "pipx",
    "uv": "uv",
    "pip": "pip",
    "cargo": "Cargo",
    "ollama": "Ollama"
```

Add, inside `"updates": { ... }`, after `"available"`:

```json
    "cannotCheck": "无法检查",
```

Add, inside `"settings": { ... }`, after the `"showTechnicalDetails"` object:

```json
    "includeSelfUpdating": {
      "label": "包含自更新的应用",
      "description": "把 Chrome、Docker 这类会自己更新的应用,也列为这里可更新的项目。"
    },
```

Add a new top-level key, after `"commandPreview": { ... },`:

```json
  "sourceNotice": {
    "pipReadOnly": {
      "title": "只读:pip 包",
      "description": "Banager 只能展示通过 pip 安装的内容,无法更新或卸载。请改用 pipx 或 uv 安装 Python 命令行工具,才能在这里管理它们。"
    },
    "ollamaNotRunning": {
      "title": "Ollama 没有运行",
      "description": "启动 Ollama 应用,才能看到它的模型并检查更新。",
      "action": "打开 Ollama"
    }
  },
```

- [ ] **Step 8: Run them to verify they pass**

Run: `pnpm exec vitest run src/pages/InstalledPage.test.tsx src/pages/UpdatesPage.test.tsx src/pages/SettingsPage.test.tsx src/i18n/completeness.test.ts`
Expected: PASS — every `InstalledPage`/`UpdatesPage`/`SettingsPage` test including the five new ones, Task 4's `"shows an unverified-version badge…"` still green (proof this task edited that file rather than reverting it), and the i18n key-parity test green since both locale files gained the same `updates.cannotCheck`, `settings.includeSelfUpdating.*` and `sourceNotice.*` keys.

- [ ] **Step 9: Run the full gate**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && pnpm test && pnpm exec tsc -p tsconfig.json`
Expected: all clean.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/ipc.rs src-tauri/src/lib.rs src/components/SourceNotice.tsx src/components/SourceNotice.test.tsx src/components/ArtifactRow.tsx src/components/ArtifactRow.test.tsx src/lib/api.ts src/lib/queries.ts src/lib/types.ts src/pages/InstalledPage.tsx src/pages/InstalledPage.test.tsx src/pages/UpdatesPage.tsx src/pages/UpdatesPage.test.tsx src/pages/SettingsPage.tsx src/pages/SettingsPage.test.tsx src/i18n/en.json src/i18n/zh-CN.json
git commit -m "$(cat <<'EOF'
feat(ui): per-source affordances for pip, cargo and Ollama

pip rows drop their uninstall button and carry a read-only note
pointing at pipx/uv, and are split on reason != Dependency so pip's
Unknown-reason packages are visible rather than collapsed behind
"show dependencies"; the update confirm dialog now renders any warning
carried on the plan (cargo's compile-locally notice included, with no
adapter-specific logic in the front end); an update the adapter could
not check offers no Update button and no checkbox, so a git-sourced
crate can no longer be "updated" into a same-named crates.io package;
an unhealthy Ollama instance shows a not-running notice with a button
that launches Ollama.app via a new open_ollama_app command. Settings
gains the include-self-updating toggle, which is what finally lets a
user set the value Task 2 threaded into check_updates.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 13: Phase-2 deferrals

**Files:**
- Modify: `crates/banager-core/src/session/mod.rs`
- Modify: `crates/banager-core/src/ops/mod.rs`
- Test: `crates/banager-core/tests/ops_summaries_test.rs`
- Create: `src/lib/queryKeys.ts`
- Modify: `src/lib/queries.ts`
- Modify: `src/lib/events.ts`
- Test: `src/lib/queries.test.ts`
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/ipc.rs`

**Interfaces:**
- Consumes: `Session::issued_plans`, `Session::issue_plan`/`submit` (existing, this task's own prior state). `OperationManager::records`/`submit`/`summaries` (existing). `refresh()` (`src/lib/api.ts`, existing). `AppState.channel_sink`, `Session::refresh` (existing).
- Produces: `OperationManager::with_max_records(self, max: usize) -> OperationManager` — a test seam, mirroring `BrewAdapter::with_update_ttl`'s shape. Production never calls it: `Session::with_adapters` builds `OperationManager::new(sink)`, so `DEFAULT_MAX_RECORDS` is the only value that applies outside tests. `queryKeys` re-exported from a new `src/lib/queryKeys.ts` (still re-exported from `src/lib/queries.ts` for existing importers). `refreshIntoCache(queryClient, why): Promise<void>` (`src/lib/events.ts`) becomes exported and promise-returning (was module-private and `void`-returning). `AppState.last_broadcast_generation: AtomicU64`.

Each of the four deferrals below is its own red/green pair; each pair's "write + run to see it fail" is one step and its "implement + run to see it pass" is the next, so all four fit inside this task's step budget.

- [ ] **Step 1: `issued_plans` sweep — write the test and confirm it fails**

Add to `crates/banager-core/src/session/mod.rs`'s `#[cfg(test)] mod tests`, after `test_submit_rejects_a_plan_issued_more_than_600s_ago`:

```rust
    static SWEEP_TEST_NOW: AtomicI64 = AtomicI64::new(2_000_000_000);

    fn sweep_test_now() -> i64 {
        SWEEP_TEST_NOW.load(Ordering::SeqCst)
    }

    #[tokio::test]
    async fn test_issue_plan_sweeps_previously_expired_entries_so_the_map_does_not_grow_unbounded()
    {
        // Distinguishes "issue_plan proactively sweeps" (this test) from
        // "submit itself checks the age of the one entry it looked up"
        // (test_submit_rejects_a_plan_issued_more_than_600s_ago above): once
        // an expired entry has been swept, submitting its id must come back
        // Unknown (the entry is gone), not Expired (which would mean the
        // entry was still sitting in the map when submit ran).
        let (adapter, state) = FakeAdapter::new("fake");
        state.lock().unwrap().instances = vec![make_instance("fake", "fake:1")];
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], Some(sweep_test_now));
        session.refresh(&non_root_env(), &CheckOptions::default()).await;
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };

        let stale = session.issue_plan(&req).await.expect("issue_plan (stale)");
        SWEEP_TEST_NOW.fetch_add(601, Ordering::SeqCst);
        let _fresh = session
            .issue_plan(&req)
            .await
            .expect("issue_plan (fresh, triggers sweep)");

        assert_eq!(
            session.submit(stale.id),
            Err(SubmitError::Unknown),
            "a swept entry must read back as Unknown, not Expired"
        );
    }
```

Run: `cargo test -p banager-core session::tests::test_issue_plan_sweeps_previously_expired_entries_so_the_map_does_not_grow_unbounded`
Expected: FAIL — `assertion 'left == right' failed ... left: Expired, right: Unknown` (the stale entry is still in the map when `submit` runs, so it is rejected as `Expired`, not `Unknown`).

- [ ] **Step 2: `issued_plans` sweep — implement and confirm it passes**

In `session/mod.rs`, replace the `issued_plans` field's doc comment:

```rust
    /// Plans handed out by `issue_plan` but not yet consumed by `submit`,
    /// keyed by `PlanId`. `submit` removes its entry on use, so each plan
    /// can be submitted at most once; an entry older than 600 seconds is
    /// rejected as expired instead of being proactively swept, since this
    /// only grows by one entry per preview an operator actually looks at.
    issued_plans: Mutex<HashMap<PlanId, IssuedPlan>>,
```

with:

```rust
    /// Plans handed out by `issue_plan` but not yet consumed by `submit`,
    /// keyed by `PlanId`. `submit` removes its entry on use, so each plan
    /// can be submitted at most once. An entry older than 600 seconds is
    /// rejected as expired by `submit` (see its own doc comment) *and*
    /// swept out by `issue_plan` itself on every new preview (Task 13), so
    /// a plan the operator previewed and then walked away from does not sit
    /// in this map forever -- only entries still within the 600 s window
    /// ever accumulate here.
    issued_plans: Mutex<HashMap<PlanId, IssuedPlan>>,
```

Replace the body of `issue_plan`:

```rust
        let plan = adapter.plan(&instance, req).await?;
        let id = self.next_plan_id.fetch_add(1, Ordering::SeqCst);
        let issued = IssuedPlan {
            id,
            plan,
            issued_at: self.now(),
        };
        self.issued_plans.lock().unwrap().insert(id, issued.clone());
        Ok(issued)
```

with:

```rust
        let plan = adapter.plan(&instance, req).await?;
        let id = self.next_plan_id.fetch_add(1, Ordering::SeqCst);
        let issued_at = self.now();
        let issued = IssuedPlan { id, plan, issued_at };
        let mut plans = self.issued_plans.lock().unwrap();
        plans.retain(|_, p| issued_at - p.issued_at <= 600);
        plans.insert(id, issued.clone());
        Ok(issued)
```

Run: `cargo test -p banager-core session::tests::`
Expected: PASS — every `session::tests::*` test, including all pre-existing ones and both new ones from Task 11.

- [ ] **Step 3: `records` cap — write the test and confirm it fails**

Add to `crates/banager-core/tests/ops_summaries_test.rs`, after `test_summaries_are_ordered_newest_first`:

```rust
#[tokio::test]
async fn test_records_are_capped_so_old_finished_operations_do_not_accumulate_forever() {
    let mut manager = OperationManager::new(Arc::new(VecSink::new())).with_max_records(2);
    let adapter = Arc::new(FakeAdapter::new());
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    let inst = make_instance("fake:1");
    manager.register_instance(inst.clone());

    let mut ids = Vec::new();
    for name in ["aaa", "bbb", "ccc"] {
        let plan = adapter
            .plan(&inst, &make_request(name, "fake:1"))
            .await
            .unwrap();
        let id = manager.submit(plan);
        manager.wait(id).await;
        ids.push(id);
    }

    let summaries = manager.summaries();
    assert_eq!(
        summaries.len(),
        2,
        "the cap of 2 must be enforced once a third operation completes"
    );
    assert!(
        summaries.iter().all(|s| s.id != ids[0]),
        "the oldest finished operation must be the one evicted"
    );
    assert!(summaries.iter().any(|s| s.id == ids[2]));
}
```

Run: `cargo test -p banager-core --test ops_summaries_test test_records_are_capped_so_old_finished_operations_do_not_accumulate_forever`
Expected: FAIL to compile — `no method named 'with_max_records' found for struct 'OperationManager'`.

- [ ] **Step 4: `records` cap — implement and confirm it passes**

In `crates/banager-core/src/ops/mod.rs`, add a constant right after the `use` block:

```rust
/// Default cap on how many finished (`Done`) operations `records` keeps at
/// once; an operation still in flight is never evicted regardless of this
/// bound. Bounds a long-running session's memory use -- without it, every
/// operation ever submitted in the process's lifetime stays in `records`
/// (and therefore in `summaries()`) forever.
const DEFAULT_MAX_RECORDS: usize = 200;
```

Add a field to `OperationManager`, right after `done_notify`:

```rust
    /// Caps `records` at this many total entries (Task 13); see
    /// `DEFAULT_MAX_RECORDS`'s doc comment and `with_max_records`.
    max_records: usize,
```

In `OperationManager::new`, add the new field's initializer, right after `done_notify: Arc::new(Notify::new()),`:

```rust
            max_records: DEFAULT_MAX_RECORDS,
```

Add a builder method right after `register_instance`:

```rust
    /// Caps how many finished (`Done`) operations `records` keeps at once;
    /// production uses `DEFAULT_MAX_RECORDS`, tests set a small value to
    /// make eviction observable without submitting hundreds of ops.
    pub fn with_max_records(mut self, max: usize) -> OperationManager {
        self.max_records = max;
        self
    }
```

In `submit`, replace:

```rust
        self.records.lock().unwrap().insert(op_id, record);
        self.sink.emit(OperationEvent::Status {
            op_id,
            status: OpStatus::Queued,
        });
```

with:

```rust
        {
            let mut records = self.records.lock().unwrap();
            records.insert(op_id, record);
            Self::evict_oldest_done_records(&mut records, self.max_records);
        }
        self.sink.emit(OperationEvent::Status {
            op_id,
            status: OpStatus::Queued,
        });
```

Add a free associated function right after `submit`'s closing brace (still inside `impl OperationManager`):

```rust
    /// Evicts the oldest (lowest op id) `Done` records, oldest first, until
    /// either the cap is met or no `Done` record remains. Work that is still
    /// Queued/Running/CancelRequested/Cancelling/Verifying is never evicted,
    /// so `max_records` is a target, not a hard bound: with enough operations
    /// in flight at once, `records` can legitimately sit above it.
    fn evict_oldest_done_records(records: &mut HashMap<OpId, OpInternal>, max_records: usize) {
        if records.len() <= max_records {
            return;
        }
        let mut done_ids: Vec<OpId> = records
            .iter()
            .filter(|(_, r)| r.status == OpStatus::Done)
            .map(|(id, _)| *id)
            .collect();
        done_ids.sort_unstable();
        let mut overflow = records.len() - max_records;
        for id in done_ids {
            if overflow == 0 {
                break;
            }
            records.remove(&id);
            overflow -= 1;
        }
    }
```

Run: `cargo test -p banager-core --test ops_summaries_test`
Expected: PASS — every test in that file, including the new one.

- [ ] **Step 5: `useRefresh` routing — write the test and confirm it fails**

Create `src/lib/queryKeys.ts`:

```ts
export const queryKeys = {
  snapshot: ["snapshot"] as const,
  operations: ["operations"] as const,
  settings: ["settings"] as const,
};
```

`src/lib/queries.test.ts` — add, after the existing `"useRefresh writes its result into the snapshot cache"` test, importing `refreshIntoCache` at the top of the file (add this line to the existing `import` group):

```ts
import { refreshIntoCache } from "./events";
```

```ts
  it("useRefresh coalesces with a refresh already started through refreshIntoCache instead of firing a second one", async () => {
    let resolveFirst: (s: Snapshot) => void = () => {};
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "refresh") {
        return new Promise<Snapshot>((resolve) => {
          resolveFirst = resolve;
        });
      }
      return Promise.resolve(undefined);
    });
    const queryClient = newClient();
    const refreshCalls = () => mockInvoke.mock.calls.filter(([cmd]) => cmd === "refresh").length;

    // Something else (e.g. the Finished-event handler in events.ts)
    // already started a refresh through the shared coordinator.
    const inFlight = refreshIntoCache(queryClient, "test-setup");
    await waitFor(() => expect(refreshCalls()).toBe(1));

    const { result } = renderHook(() => useRefresh(), { wrapper: wrapper(queryClient) });
    result.current.mutate();

    // If useRefresh still called `refresh` directly (bypassing the
    // coordinator), this would now be 2.
    expect(refreshCalls()).toBe(1);

    resolveFirst({ ...snapshot, generation: 5 });
    await inFlight;
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect((queryClient.getQueryData(["snapshot"]) as Snapshot).generation).toBe(5);
  });
```

Run: `pnpm exec vitest run src/lib/queries.test.ts`
Expected: FAIL — `expect(refreshCalls()).toBe(1)` sees `2`, because `useRefresh`'s `mutationFn` still calls `refresh` directly.

- [ ] **Step 6: `useRefresh` routing — implement and confirm it passes**

`src/lib/events.ts` — replace:

```ts
let refreshInFlight: Promise<void> | null = null;
let refreshAgain = false;

function refreshIntoCache(queryClient: QueryClient, why: string): void {
  // refreshIntoCache is a plain function, not a hook, so useUiStore.getState()
  // — rather than the useUiStore() hook — is the correct way to reach the store here.
  if (refreshInFlight) {
    refreshAgain = true;
    return;
  }
  refreshInFlight = refresh()
    .then((snapshot) => {
      queryClient.setQueryData(queryKeys.snapshot, snapshot);
      useUiStore.getState().setStartupRefreshError(null);
    })
    .catch((e: unknown) => {
      console.error(`${why} refresh failed`, e);
      useUiStore.getState().setStartupRefreshError(e instanceof Error ? e.message : String(e));
    })
    .finally(() => {
      refreshInFlight = null;
      if (refreshAgain) {
        refreshAgain = false;
        refreshIntoCache(queryClient, `${why} (follow-up)`);
      }
    });
}
```

with:

```ts
let refreshInFlight: Promise<void> | null = null;
let refreshAgain = false;

/**
 * Exported (Task 13) so `useRefresh` (src/lib/queries.ts) shares this same
 * single-flight coordinator instead of calling `refresh()` directly --
 * previously a manual "Try again" click could run fully concurrently with
 * an in-flight startup/event-driven refresh, exactly the race this module
 * exists to prevent. A call that arrives while one is already in flight
 * gets back *that* run's own promise rather than starting a second one; it
 * still schedules the one-more-follow-up this module has always used to
 * make sure whatever changed after the in-flight run started is not lost.
 * Every internal side effect (cache write, `startupRefreshError`) is
 * unchanged; the only new thing is that a failure is now also re-thrown, so
 * a caller like `useRefresh` can `await` this and see `isError` — existing
 * fire-and-forget callers below append their own `.catch(() => {})`.
 */
export function refreshIntoCache(queryClient: QueryClient, why: string): Promise<void> {
  if (refreshInFlight) {
    refreshAgain = true;
    return refreshInFlight;
  }
  const run: Promise<void> = refresh()
    .then((snapshot) => {
      queryClient.setQueryData(queryKeys.snapshot, snapshot);
      useUiStore.getState().setStartupRefreshError(null);
    })
    .catch((e: unknown) => {
      console.error(`${why} refresh failed`, e);
      useUiStore.getState().setStartupRefreshError(e instanceof Error ? e.message : String(e));
      throw e;
    })
    .finally(() => {
      refreshInFlight = null;
      if (refreshAgain) {
        refreshAgain = false;
        refreshIntoCache(queryClient, `${why} (follow-up)`).catch(() => {});
      }
    });
  refreshInFlight = run;
  return run;
}
```

Change the import at the top of `events.ts` from:

```ts
import { refresh, subscribeEvents } from "./api";
import { queryKeys } from "./queries";
```

to:

```ts
import { refresh, subscribeEvents } from "./api";
import { queryKeys } from "./queryKeys";
```

In `useStartupRefresh`, change:

```ts
  useEffect(() => {
    refreshIntoCache(queryClient, "initial");
  }, [queryClient]);
```

to:

```ts
  useEffect(() => {
    refreshIntoCache(queryClient, "initial").catch(() => {});
  }, [queryClient]);
```

In `useOperationEvents`'s `handle`, change:

```ts
          if ("Finished" in opEvent) {
            refreshIntoCache(queryClient, "post-operation");
          }
```

to:

```ts
          if ("Finished" in opEvent) {
            refreshIntoCache(queryClient, "post-operation").catch(() => {});
          }
```

`src/lib/queries.ts` — change:

```ts
import {
  getSnapshot,
  refresh,
  planOperation,
  submitOperation,
  cancelOperation,
  listOperations,
  getSettings,
  setSettings,
} from "./api";
import type { IssuedPlan, OpRequest, OpSummary, Settings, Snapshot } from "./types";

export const queryKeys = {
  snapshot: ["snapshot"] as const,
  operations: ["operations"] as const,
  settings: ["settings"] as const,
};
```

to:

```ts
import {
  getSnapshot,
  planOperation,
  submitOperation,
  cancelOperation,
  listOperations,
  getSettings,
  setSettings,
  openOllamaApp,
} from "./api";
import { refreshIntoCache } from "./events";
import { queryKeys } from "./queryKeys";
import type { IssuedPlan, OpRequest, OpSummary, Settings, Snapshot } from "./types";

export { queryKeys };
```

(`refresh` is no longer imported directly here — `refreshIntoCache` is the only path to it now. `openOllamaApp` was already added to this same import list in Task 12; keep it.)

Replace `useRefresh`:

```ts
export function useRefresh(): UseMutationResult<Snapshot, Error, void> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: refresh,
    onSuccess: (snapshot) => {
      queryClient.setQueryData(queryKeys.snapshot, snapshot);
    },
  });
}
```

with:

```ts
export function useRefresh(): UseMutationResult<Snapshot, Error, void> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      // refreshIntoCache already writes the result into the cache; reading
      // it back here is what makes a coalesced call (one that arrived while
      // another refresh was already in flight) report the *served* result
      // instead of racing a second, redundant `invoke("refresh")`.
      await refreshIntoCache(queryClient, "manual");
      const result = queryClient.getQueryData<Snapshot>(queryKeys.snapshot);
      if (!result) {
        throw new Error("refresh did not produce a snapshot");
      }
      return result;
    },
  });
}
```

Run: `pnpm exec vitest run src/lib/queries.test.ts src/lib/events.test.ts`
Expected: PASS — every test in both files, including the new coalescing test and every pre-existing `events.test.ts` test (startup-refresh error handling, the Finished-triggered refresh, and the existing coalescing test) unchanged.

- [ ] **Step 7: no double `SnapshotChanged` — write the test and confirm it fails**

`src-tauri/src/ipc.rs` — add to `#[cfg(test)] mod tests`, after `state_with_fake_adapter_and_now`:

```rust
    /// Like `state_with_fake_adapter_and_now`, but the fake adapter's
    /// `detect()` sleeps for `detect_delay` first -- long enough to widen
    /// the race window so two concurrent `refresh_impl` calls reliably
    /// coalesce inside `Session::refresh`'s `refresh_gate`, mirroring
    /// `session::tests::test_concurrent_refresh_calls_are_coalesced`'s own
    /// use of an artificial delay for the same reason.
    fn state_with_slow_fake_adapter(detect_delay: std::time::Duration) -> Arc<AppState> {
        let instance = ManagerInstance {
            id: "fake:1".to_string(),
            adapter_id: "fake".to_string(),
            exe_path: PathBuf::from("/bin/true"),
            prefix: PathBuf::from("/"),
            scope: Scope::User,
            version: Some("1.0".to_string()),
            healthy: true,
            unverified_version: None,
        };
        let meta = AdapterMeta {
            id: "fake".to_string(),
            name: "fake".to_string(),
            kind: "fake".to_string(),
            platforms: vec!["macos".to_string()],
            homepage: "https://example.invalid".to_string(),
            schema_version: 1,
            verified_versions: vec![],
        };
        let adapter: Arc<dyn Adapter> = Arc::new(FakeAdapter {
            meta,
            instance,
            execute_calls: Arc::new(AtomicUsize::new(0)),
            check_options_calls: Arc::new(Mutex::new(Vec::new())),
            detect_delay,
        });
        let sink = ChannelSink::new();
        let session =
            banager_core::session::Session::with_adapters(sink.clone(), vec![adapter], None);
        Arc::new(AppState {
            session,
            settings_path: temp_settings_path("ipc-slow"),
            settings: std::sync::Mutex::new(Settings::default()),
            channel_sink: sink,
            last_broadcast_generation: std::sync::atomic::AtomicU64::new(0),
        })
    }

    #[tokio::test]
    async fn test_refresh_impl_broadcasts_snapshot_changed_exactly_once_when_two_calls_coalesce() {
        // Two refresh_impl calls that coalesce inside Session::refresh (its
        // refresh_gate) both receive the *same* resulting Snapshot. Each
        // independently comparing that result's generation against its own
        // "before" reading used to make both of them decide the generation
        // moved and broadcast -- a spurious duplicate for what was really
        // one refresh.
        let state = state_with_slow_fake_adapter(std::time::Duration::from_millis(100));
        let received: Arc<std::sync::Mutex<Vec<UiEvent>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let r = received.clone();
        let channel: Channel<UiEvent> = Channel::new(move |body| {
            let event: UiEvent = body.deserialize().expect("deserialize UiEvent");
            r.lock().unwrap().push(event);
            Ok(())
        });
        subscribe_events_impl(&state, channel).expect("subscribe_events_impl");

        let state_a = state.clone();
        let state_b = state.clone();
        let (a, b) = tokio::join!(
            tokio::spawn(async move { refresh_impl(&state_a).await }),
            tokio::spawn(async move { refresh_impl(&state_b).await }),
        );
        let snap_a = a.expect("task a").expect("refresh_impl a");
        let snap_b = b.expect("task b").expect("refresh_impl b");
        assert_eq!(
            snap_a.generation, snap_b.generation,
            "precondition: both calls must see the same coalesced result"
        );

        let events = received.lock().unwrap();
        let broadcasts: Vec<u64> = events
            .iter()
            .filter_map(|e| match e {
                UiEvent::SnapshotChanged { generation } => Some(*generation),
                UiEvent::Operation(_) => None,
            })
            .collect();
        assert_eq!(
            broadcasts,
            vec![snap_a.generation],
            "exactly one SnapshotChanged must reach the subscriber even though two refresh_impl calls coalesced, got: {events:?}"
        );
    }
```

Also add a `detect_delay` field to the existing `FakeAdapter` struct and thread it through. After Task 2 Step 6 that struct reads:

```rust
    struct FakeAdapter {
        meta: AdapterMeta,
        instance: ManagerInstance,
        execute_calls: Arc<AtomicUsize>,
        check_options_calls: Arc<Mutex<Vec<CheckOptions>>>,
    }
```

so change it to

```rust
    struct FakeAdapter {
        meta: AdapterMeta,
        instance: ManagerInstance,
        execute_calls: Arc<AtomicUsize>,
        check_options_calls: Arc<Mutex<Vec<CheckOptions>>>,
        detect_delay: std::time::Duration,
    }
```

and its `detect` impl from

```rust
        async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
            vec![self.instance.clone()]
        }
```

to

```rust
        async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
            if !self.detect_delay.is_zero() {
                tokio::time::sleep(self.detect_delay).await;
            }
            vec![self.instance.clone()]
        }
```

and, in `state_with_fake_adapter_and_now`, add the new field to the existing `FakeAdapter { ... }` construction:

```rust
        let adapter: Arc<dyn Adapter> = Arc::new(FakeAdapter {
            meta,
            instance,
            execute_calls: execute_calls.clone(),
            check_options_calls: check_options_calls.clone(),
            detect_delay: std::time::Duration::ZERO,
        });
```

Run: `cargo test -p banager ipc::tests::test_refresh_impl_broadcasts_snapshot_changed_exactly_once_when_two_calls_coalesce`
Expected: FAIL — `broadcasts` is `[N, N]` (two identical broadcasts) instead of `[N]`.

- [ ] **Step 8: no double `SnapshotChanged` — implement and confirm it passes**

`src-tauri/src/state.rs` — add a field to `AppState`:

```rust
pub struct AppState {
    pub session: std::sync::Arc<Session>,
    pub settings_path: PathBuf,
    pub settings: Mutex<Settings>,
    pub channel_sink: std::sync::Arc<ChannelSink>,
    /// The last `Snapshot::generation` this process has ever broadcast as a
    /// `SnapshotChanged` event (Task 13). `refresh_impl` compares against
    /// this with a compare-and-swap instead of each call's own "before"
    /// reading of `session.snapshot()`, so that when two `refresh_impl`
    /// calls coalesce inside `Session::refresh` and both receive the same
    /// resulting Snapshot, only one of them ever wins the swap and
    /// broadcasts -- never both.
    pub last_broadcast_generation: std::sync::atomic::AtomicU64,
}
```

and its constructor:

```rust
    pub fn new(settings_path: PathBuf, channel_sink: std::sync::Arc<ChannelSink>) -> AppState {
        let loaded = settings::load(&settings_path);
        let session = Session::new(channel_sink.clone(), None);
        AppState {
            session,
            settings_path,
            settings: Mutex::new(loaded),
            channel_sink,
            last_broadcast_generation: std::sync::atomic::AtomicU64::new(0),
        }
    }
```

`src-tauri/src/ipc.rs` — add `use std::sync::atomic::Ordering;` to the top-level `use` block (outside `#[cfg(test)]`; this is the only task that adds it, and `compare_exchange` below is what needs it). Replace `refresh_impl` — which after Task 2 Step 8 reads:

```rust
pub(crate) async fn refresh_impl(state: &AppState) -> Result<Snapshot, String> {
    let generation_before = state.session.snapshot().generation;
    let opts = CheckOptions {
        include_self_updating: state.get_settings().include_self_updating,
    };
    let snapshot = state.session.refresh(&HostEnv::discover(), &opts).await;
    if snapshot.generation != generation_before {
        state.channel_sink.broadcast(UiEvent::SnapshotChanged {
            generation: snapshot.generation,
        });
    }
    Ok(snapshot)
}
```

with this — **keeping the `opts` construction exactly as Task 2 left it**; only the broadcast condition changes here. Dropping those three lines would silently unwire `Settings.include_self_updating` from `check_updates` again and break Task 2's two end-to-end tests:

```rust
pub(crate) async fn refresh_impl(state: &AppState) -> Result<Snapshot, String> {
    let opts = CheckOptions {
        include_self_updating: state.get_settings().include_self_updating,
    };
    let snapshot = state.session.refresh(&HostEnv::discover(), &opts).await;
    let generation = snapshot.generation;
    // Two refresh_impl calls that coalesce inside Session::refresh (its
    // refresh_gate) both receive the *same* resulting Snapshot. Comparing
    // each call's own "before" reading against that shared result would let
    // both of them independently decide the generation moved and broadcast
    // -- a spurious duplicate for one refresh (Task 13). A compare-and-swap
    // against the last generation this process has ever broadcast ensures
    // exactly one of any group of callers who see the same new generation
    // wins, however many of them coalesced into the same refresh; a caller
    // that loses the swap has nothing left to do, since whoever won it (or
    // a still-newer generation) already has this one covered.
    let previous = state.last_broadcast_generation.load(Ordering::SeqCst);
    if generation > previous
        && state
            .last_broadcast_generation
            .compare_exchange(previous, generation, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    {
        state
            .channel_sink
            .broadcast(UiEvent::SnapshotChanged { generation });
    }
    Ok(snapshot)
}
```

In `state_with_fake_adapter_and_now`, add the new field to its `AppState { ... }` struct literal:

```rust
        let state = AppState {
            session,
            settings_path: temp_settings_path("appstate"),
            settings: std::sync::Mutex::new(Settings::default()),
            channel_sink: sink,
            last_broadcast_generation: std::sync::atomic::AtomicU64::new(0),
        };
```

Run: `cargo test -p banager ipc::tests::`
Expected: PASS — every `ipc::tests::*` test, including the new one, the two pre-existing `SnapshotChanged` tests (`test_refresh_impl_broadcasts_snapshot_changed_when_the_generation_moves`, `test_refresh_impl_does_not_rebroadcast_when_the_generation_is_unchanged`), and Task 2's two `include_self_updating` end-to-end tests, all unchanged.

- [ ] **Step 9: Run the full gate**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && pnpm test && pnpm exec tsc -p tsconfig.json`
Expected: all clean.

- [ ] **Step 10: Commit**

```bash
git add crates/banager-core/src/session/mod.rs crates/banager-core/src/ops/mod.rs crates/banager-core/tests/ops_summaries_test.rs src/lib/queryKeys.ts src/lib/queries.ts src/lib/events.ts src/lib/queries.test.ts src-tauri/src/state.rs src-tauri/src/ipc.rs
git commit -m "$(cat <<'EOF'
fix: clear four phase-2 deferrals in refresh, ops and settings plumbing

issue_plan now sweeps issued_plans entries older than 600s on every
new preview instead of only lazily rejecting them on submit.
OperationManager::records is capped (with_max_records, default 200),
evicting the oldest finished operations first so a long session's
history does not grow without bound. useRefresh now shares
refreshIntoCache's single-flight coordinator with the startup and
event-driven refreshes instead of calling refresh() directly.
refresh_impl uses a compare-and-swap against the last broadcast
generation so two refresh calls that coalesce inside Session::refresh
never send SnapshotChanged twice for the same transition.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 14: Split `session/mod.rs`, record fixtures, CI

**Files:**
- Modify: `crates/banager-core/src/session/mod.rs`
- Create: `crates/banager-core/src/session/refresh.rs`
- Create: `crates/banager-core/src/session/plans.rs`
- Create: `crates/banager-core/tests/fixtures_layout_test.rs`
- Modify: `adapters/fixtures/npm/12.0.2/README.md`
- Modify: `adapters/fixtures/cargo/1.98.1/README.md`

**Interfaces:**
- Consumes: everything `Session` exposed before this task (its split is required to be behaviour-preserving); `Session::adapter_ids()` (Task 11).
- Produces: no new public API — `Session::refresh`/`commit` move into `session::refresh` (a child module, so they keep access to `Session`'s private fields), `Session::issue_plan`/`submit` move into `session::plans`. `crates/banager-core/tests/fixtures_layout_test.rs` — a new regression guard, not a production interface.

This task is a pure refactor: no behaviour change, proven by every pre-existing test (Tasks 1–13's) passing unchanged after the split.

- [ ] **Step 1: Record the pre-split baseline**

Run: `cargo test -p banager-core session:: 2>&1 | tail -5`
Expected: PASS. Write down whatever count that run reports — this task adds and removes no tests, so Step 5 must report the identical number. Do not compare it against a number written here; the run itself is the baseline.

- [ ] **Step 2: Create `session/plans.rs`**

```rust
//! `Session::issue_plan` and `Session::submit`: preview-then-confirm for a
//! destructive operation. Split out of `session/mod.rs` (Task 14); no
//! behaviour change from what shipped there.

use super::{IssuedPlan, PlanId, Session, SubmitError};
use crate::adapters::AdapterError;
use crate::events::OpId;
use crate::model::OpRequest;
use std::sync::atomic::Ordering;

impl Session {
    /// Resolves `req` to its owning adapter, asks it to plan the operation,
    /// then stores the resulting `Plan` under a fresh `PlanId` and returns
    /// both as an `IssuedPlan`. The caller previews `issued.plan`; nothing
    /// in it is ever accepted back -- `submit` takes only `issued.id`. Every
    /// call also sweeps any entry in `issued_plans` older than 600 seconds
    /// (Task 13), so a plan the operator previewed and then never submitted
    /// does not sit in the map forever.
    pub async fn issue_plan(&self, req: &OpRequest) -> Result<IssuedPlan, AdapterError> {
        let instance = self
            .snapshot
            .lock()
            .unwrap()
            .instances
            .iter()
            .find(|i| i.id == req.instance_id)
            .cloned()
            .ok_or_else(|| {
                AdapterError::Refused(format!("unknown instance {}", req.instance_id))
            })?;
        let adapter = self.adapters.get(&instance.adapter_id).ok_or_else(|| {
            AdapterError::Refused(format!("no adapter registered for {}", instance.adapter_id))
        })?;
        let plan = adapter.plan(&instance, req).await?;
        let id = self.next_plan_id.fetch_add(1, Ordering::SeqCst);
        let issued_at = self.now();
        let issued = IssuedPlan { id, plan, issued_at };
        let mut plans = self.issued_plans.lock().unwrap();
        plans.retain(|_, p| issued_at - p.issued_at <= 600);
        plans.insert(id, issued.clone());
        Ok(issued)
    }

    /// Removes (one-time consumption) the issued plan stored under
    /// `plan_id` and submits exactly that stored `Plan`. Fails with
    /// `SubmitError::Unknown` if `plan_id` was never issued, was already
    /// submitted once, or was already swept out by a later `issue_plan`
    /// call, and `SubmitError::Expired` if it is still present but was
    /// issued more than 600 seconds ago -- the client can never influence
    /// what actually runs, since nothing it sends is used except this
    /// opaque id.
    pub fn submit(self: &std::sync::Arc<Self>, plan_id: PlanId) -> Result<OpId, SubmitError> {
        let issued = {
            let mut plans = self.issued_plans.lock().unwrap();
            plans.remove(&plan_id).ok_or(SubmitError::Unknown)?
        };
        if self.now() - issued.issued_at > 600 {
            return Err(SubmitError::Expired);
        }
        Ok(self.ops.submit(issued.plan))
    }
}

#[cfg(test)]
mod tests {
    use crate::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions};
    use crate::events::{EventSink, OpId, VecSink};
    use crate::model::{
        ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, ManagerInstance, OpKind,
        OpRequest, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate,
    };
    use crate::runner::HostEnv;
    use crate::session::{Session, SubmitError};
    use async_trait::async_trait;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    struct FakeAdapter {
        meta: AdapterMeta,
        instances: Vec<ManagerInstance>,
    }

    impl FakeAdapter {
        fn new(instances: Vec<ManagerInstance>) -> Arc<FakeAdapter> {
            Arc::new(FakeAdapter {
                meta: AdapterMeta {
                    id: "fake".to_string(),
                    name: "fake".to_string(),
                    kind: "fake".to_string(),
                    platforms: vec!["macos".to_string()],
                    homepage: "https://example.invalid".to_string(),
                    schema_version: 1,
                    verified_versions: vec![],
                },
                instances,
            })
        }
    }

    #[async_trait]
    impl Adapter for FakeAdapter {
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

        async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
            self.instances.clone()
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
        ) -> Result<Vec<UpdateCandidate>, AdapterError> {
            Ok(Vec::new())
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
            Ok(Plan {
                request: req.clone(),
                program: inst.exe_path.clone(),
                args: vec!["do".to_string(), req.name.clone()],
                env: vec![],
                needs_password: false,
                locks: vec![ResourceLock(inst.id.clone())],
                cancel_policy: CancelPolicy::KillThenReconcile,
                warnings: vec![],
                affected: vec![],
                timeout_secs: 60,
            })
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
            Ok(Reconciled {
                present: true,
                version: None,
            })
        }
    }

    fn make_instance(id: &str) -> ManagerInstance {
        ManagerInstance {
            id: id.to_string(),
            adapter_id: "fake".to_string(),
            exe_path: PathBuf::from("/bin/true"),
            prefix: PathBuf::from("/"),
            scope: Scope::User,
            version: Some("1.0".to_string()),
            healthy: true,
            unverified_version: None,
        }
    }

    fn non_root_env() -> HostEnv {
        HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/tmp"),
            euid: 501,
        }
    }

    fn install_request(instance_id: &str) -> OpRequest {
        OpRequest {
            kind: OpKind::Install,
            instance_id: instance_id.to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        }
    }

    #[tokio::test]
    async fn test_issue_plan_delegates_to_the_owning_adapter() {
        let adapter = FakeAdapter::new(vec![make_instance("fake:1")]);
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session.refresh(&non_root_env(), &CheckOptions::default()).await;
        let req = install_request("fake:1");
        let issued = session.issue_plan(&req).await.expect("issue_plan");
        assert_eq!(issued.id, 1, "PlanId numbering starts at 1");
        assert_eq!(issued.plan.args, vec!["do".to_string(), "jq".to_string()]);
    }

    #[tokio::test]
    async fn test_issue_plan_for_unknown_instance_is_refused() {
        let adapter = FakeAdapter::new(vec![]);
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let req = install_request("does-not-exist");
        match session.issue_plan(&req).await {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_submit_of_a_never_issued_plan_id_is_unknown_and_runs_nothing() {
        let adapter = FakeAdapter::new(vec![make_instance("fake:1")]);
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session.refresh(&non_root_env(), &CheckOptions::default()).await;

        assert_eq!(session.submit(1), Err(SubmitError::Unknown));
        assert_eq!(session.submit(u64::MAX), Err(SubmitError::Unknown));

        let req = install_request("fake:1");
        let issued = session.issue_plan(&req).await.expect("issue_plan");
        assert_eq!(issued.id, 1);
        assert_eq!(session.submit(0), Err(SubmitError::Unknown));
        assert_eq!(session.submit(2), Err(SubmitError::Unknown));
        assert!(
            session.operations().is_empty(),
            "a rejected submit must never reach the OperationManager"
        );
        session
            .submit(issued.id)
            .expect("the genuinely issued id is still submittable after the forgeries failed");
        assert_eq!(session.operations().len(), 1);
    }

    #[tokio::test]
    async fn test_submit_consumes_the_plan_so_the_same_id_cannot_be_replayed() {
        let adapter = FakeAdapter::new(vec![make_instance("fake:1")]);
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session.refresh(&non_root_env(), &CheckOptions::default()).await;
        let req = install_request("fake:1");
        let issued = session.issue_plan(&req).await.expect("issue_plan");

        let op_id = session
            .submit(issued.id)
            .expect("first submit of a freshly issued plan");
        assert_eq!(
            session.submit(issued.id),
            Err(SubmitError::Unknown),
            "an issued plan is single-use: replaying its id must be rejected"
        );
        assert_eq!(
            session.submit(issued.id),
            Err(SubmitError::Unknown),
            "and it stays rejected however many times it is replayed"
        );
        let ops = session.operations();
        assert_eq!(
            ops.len(),
            1,
            "exactly one operation may result from one issued plan"
        );
        assert_eq!(ops[0].id, op_id);

        let reissued = session.issue_plan(&req).await.expect("issue_plan again");
        assert_ne!(reissued.id, issued.id);
        session
            .submit(reissued.id)
            .expect("a re-issued plan is submittable once");
        assert_eq!(session.submit(reissued.id), Err(SubmitError::Unknown));
        assert_eq!(session.operations().len(), 2);
    }

    static FAKE_NOW: AtomicI64 = AtomicI64::new(0);

    fn fake_now() -> i64 {
        FAKE_NOW.load(Ordering::SeqCst)
    }

    #[tokio::test]
    async fn test_submit_rejects_a_plan_issued_more_than_600s_ago() {
        const T0: i64 = 1_758_000_000;
        FAKE_NOW.store(T0, Ordering::SeqCst);
        let adapter = FakeAdapter::new(vec![make_instance("fake:1")]);
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], Some(fake_now));
        session.refresh(&non_root_env(), &CheckOptions::default()).await;
        let req = install_request("fake:1");
        let on_time = session.issue_plan(&req).await.expect("issue_plan");
        let too_late = session.issue_plan(&req).await.expect("issue_plan");
        assert_eq!(
            on_time.issued_at, T0,
            "issued_at must come from the injected clock"
        );
        assert_eq!(too_late.issued_at, T0);

        FAKE_NOW.store(T0 + 600, Ordering::SeqCst);
        session
            .submit(on_time.id)
            .expect("a plan exactly 600 s old is still submittable");
        assert_eq!(session.operations().len(), 1);

        FAKE_NOW.store(T0 + 601, Ordering::SeqCst);
        assert_eq!(session.submit(too_late.id), Err(SubmitError::Expired));
        assert_eq!(
            session.operations().len(),
            1,
            "an expired plan must never reach the OperationManager"
        );

        FAKE_NOW.store(T0, Ordering::SeqCst);
        assert_eq!(session.submit(too_late.id), Err(SubmitError::Unknown));
        assert_eq!(session.operations().len(), 1);
        let fresh = session.issue_plan(&req).await.expect("issue_plan again");
        assert_ne!(fresh.id, too_late.id);
        session
            .submit(fresh.id)
            .expect("a freshly issued plan is submittable");
        assert_eq!(session.operations().len(), 2);
    }

    static SWEEP_TEST_NOW: AtomicI64 = AtomicI64::new(2_000_000_000);

    fn sweep_test_now() -> i64 {
        SWEEP_TEST_NOW.load(Ordering::SeqCst)
    }

    #[tokio::test]
    async fn test_issue_plan_sweeps_previously_expired_entries_so_the_map_does_not_grow_unbounded()
    {
        let adapter = FakeAdapter::new(vec![make_instance("fake:1")]);
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], Some(sweep_test_now));
        session.refresh(&non_root_env(), &CheckOptions::default()).await;
        let req = install_request("fake:1");

        let stale = session.issue_plan(&req).await.expect("issue_plan (stale)");
        SWEEP_TEST_NOW.fetch_add(601, Ordering::SeqCst);
        let _fresh = session
            .issue_plan(&req)
            .await
            .expect("issue_plan (fresh, triggers sweep)");

        assert_eq!(
            session.submit(stale.id),
            Err(SubmitError::Unknown),
            "a swept entry must read back as Unknown, not Expired"
        );
    }
}
```

- [ ] **Step 3: Create `session/refresh.rs`**

```rust
//! `Session::refresh`: detect every registered adapter's instances, then
//! fetch inventory + updates per instance, merging failures into `errors`
//! and `stale` without ever aborting the whole refresh. Split out of
//! `session/mod.rs` (Task 14); no behaviour change from what shipped there.

use super::{DetectOutcome, Session, Snapshot, SourceError};
use crate::adapters::brew::BrewAdapter;
use crate::adapters::CheckOptions;
use crate::model::ResourceLock;
use crate::runner::HostEnv;
use std::sync::atomic::Ordering;

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
    /// Concurrent calls are serialised: a call that starts while another is
    /// already running waits for it, then returns the snapshot that other
    /// call produced instead of running a second, redundant refresh -- see
    /// `refresh_seq` on `Session` for why that check cannot use
    /// `generation`. An instance the adapter reported as `healthy: false` is
    /// skipped by the per-instance fetch: that is a *reported state*, not a
    /// failed refresh (Task 11).
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

        if BrewAdapter::refuses_as_root(env) {
            let refused = Snapshot {
                generation: previous.generation,
                detect: DetectOutcome::RefusedAsRoot,
                instances: Vec::new(),
                artifacts: Vec::new(),
                updates: Vec::new(),
                refreshed_at: Some(self.now()),
                stale: previous.stale,
                errors: Vec::new(),
            };
            return self.commit(previous, refused);
        }

        let mut detect_handles = Vec::with_capacity(self.adapters.len());
        for adapter in self.adapters.values().cloned() {
            let env = env.clone();
            detect_handles.push((
                adapter.meta().id.clone(),
                tokio::spawn(async move { adapter.detect(&env).await }),
            ));
        }
        let mut instances = Vec::new();
        let mut detect_errors = Vec::new();
        for (adapter_id, handle) in detect_handles {
            match handle.await {
                Ok(found) => instances.extend(found),
                Err(_join_err) => {
                    detect_errors.push(SourceError {
                        instance_id: adapter_id,
                        message: "internal error detecting this source".to_string(),
                    });
                }
            }
        }
        for inst in &instances {
            self.ops.register_instance(inst.clone());
        }
        let detect = if instances.is_empty() {
            DetectOutcome::Missing
        } else {
            DetectOutcome::Found
        };

        let mut handles = Vec::with_capacity(instances.len());
        for inst in instances.clone() {
            // Task 11: a source the adapter already reported as not running
            // is a reported state, not a failed refresh. Fanning out to it
            // would push a SourceError and set `stale`, which carries
            // `refreshed_at` forward instead of stamping it -- leaving the
            // snapshot permanently stale on a machine where, say, Ollama is
            // installed but not running. It stays in `snapshot.instances` so
            // the UI can render its notice and offer to start it.
            if !inst.healthy {
                continue;
            }
            let Some(adapter) = self.adapters.get(&inst.adapter_id).cloned() else {
                continue;
            };
            let ops = self.ops.clone();
            let previous = previous.clone();
            let opts = opts;
            handles.push((
                inst.id.clone(),
                tokio::spawn(async move {
                    let _lock = ops
                        .acquire_resource_lock(ResourceLock(inst.id.clone()))
                        .await;
                    let mut artifacts = Vec::new();
                    let mut updates = Vec::new();
                    let mut errors = Vec::new();
                    let mut stale = false;
                    match adapter.inventory(&inst).await {
                        Ok(items) => artifacts.extend(items),
                        Err(e) => {
                            errors.push(SourceError {
                                instance_id: inst.id.clone(),
                                message: e.to_string(),
                            });
                            stale = true;
                            artifacts.extend(
                                previous
                                    .artifacts
                                    .iter()
                                    .filter(|a| a.key.instance_id == inst.id)
                                    .cloned(),
                            );
                        }
                    }
                    match adapter.check_updates(&inst, &opts).await {
                        Ok(items) => updates.extend(items),
                        Err(e) => {
                            errors.push(SourceError {
                                instance_id: inst.id.clone(),
                                message: e.to_string(),
                            });
                            stale = true;
                            updates.extend(
                                previous
                                    .updates
                                    .iter()
                                    .filter(|u| u.key.instance_id == inst.id)
                                    .cloned(),
                            );
                        }
                    }
                    (artifacts, updates, errors, stale)
                }),
            ));
        }

        let mut artifacts = Vec::new();
        let mut updates = Vec::new();
        let mut errors = detect_errors;
        let mut stale = !errors.is_empty();
        for (instance_id, handle) in handles {
            match handle.await {
                Ok((a, u, e, s)) => {
                    artifacts.extend(a);
                    updates.extend(u);
                    errors.extend(e);
                    stale = stale || s;
                }
                Err(_join_err) => {
                    errors.push(SourceError {
                        instance_id,
                        message: "internal error refreshing this instance".to_string(),
                    });
                    stale = true;
                }
            }
        }

        let refreshed_at = if stale {
            previous.refreshed_at
        } else {
            Some(self.now())
        };
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

#[cfg(test)]
mod tests {
    use crate::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions};
    use crate::events::{EventSink, OpId, VecSink};
    use crate::model::{
        ArtifactKind, CancelPolicy, InstallReason, InstalledArtifact, InstanceId, ManagerInstance,
        OpKind, OpRequest, OpStatus, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit,
        UpdateCandidate,
    };
    use crate::runner::HostEnv;
    use crate::session::{DetectOutcome, Session};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;

    struct FakeState {
        instances: Vec<ManagerInstance>,
        artifacts: HashMap<InstanceId, Vec<InstalledArtifact>>,
        updates: HashMap<InstanceId, Vec<UpdateCandidate>>,
        failing: Vec<InstanceId>,
        detect_delay: Duration,
        detect_calls: usize,
        block_execute: bool,
        inventory_calls: Vec<InstanceId>,
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
                failing: Vec::new(),
                detect_delay: Duration::from_millis(0),
                detect_calls: 0,
                block_execute: false,
                inventory_calls: Vec::new(),
            }));
            let adapter = Arc::new(FakeAdapter {
                meta: AdapterMeta {
                    id: id.to_string(),
                    name: id.to_string(),
                    kind: "fake".to_string(),
                    platforms: vec!["macos".to_string()],
                    homepage: "https://example.invalid".to_string(),
                    schema_version: 1,
                    verified_versions: vec![],
                },
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

        async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
            let delay = {
                let mut s = self.state.lock().unwrap();
                s.detect_calls += 1;
                s.detect_delay
            };
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            self.state.lock().unwrap().instances.clone()
        }

        async fn inventory(
            &self,
            inst: &ManagerInstance,
        ) -> Result<Vec<InstalledArtifact>, AdapterError> {
            let mut s = self.state.lock().unwrap();
            s.inventory_calls.push(inst.id.clone());
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
        ) -> Result<Vec<UpdateCandidate>, AdapterError> {
            let s = self.state.lock().unwrap();
            Ok(s.updates.get(&inst.id).cloned().unwrap_or_default())
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
            Ok(Plan {
                request: req.clone(),
                program: inst.exe_path.clone(),
                args: vec!["do".to_string(), req.name.clone()],
                env: vec![],
                needs_password: false,
                locks: vec![ResourceLock(inst.id.clone())],
                cancel_policy: CancelPolicy::KillThenReconcile,
                warnings: vec![],
                affected: vec![],
                timeout_secs: 60,
            })
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
            Ok(Reconciled {
                present: true,
                version: None,
            })
        }
    }

    fn make_instance(adapter_id: &str, id: &str) -> ManagerInstance {
        ManagerInstance {
            id: id.to_string(),
            adapter_id: adapter_id.to_string(),
            exe_path: PathBuf::from("/bin/true"),
            prefix: PathBuf::from("/"),
            scope: Scope::User,
            version: Some("1.0".to_string()),
            healthy: true,
            unverified_version: None,
        }
    }

    fn make_artifact(instance_id: &str, name: &str) -> InstalledArtifact {
        InstalledArtifact {
            key: crate::model::ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Formula,
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
        }
    }

    fn non_root_env() -> HostEnv {
        HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/tmp"),
            euid: 501,
        }
    }

    fn root_env() -> HostEnv {
        HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/var/root"),
            euid: 0,
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
        let snapshot = session.refresh(&non_root_env(), &CheckOptions::default()).await;
        assert_eq!(snapshot.detect, DetectOutcome::Found);
        assert_eq!(snapshot.instances.len(), 1);
        assert_eq!(snapshot.artifacts.len(), 1);
        assert_eq!(snapshot.artifacts[0].display_name, "jq");
        assert!(!snapshot.stale);
        assert!(snapshot.errors.is_empty());
        assert!(snapshot.refreshed_at.is_some());
        assert_eq!(snapshot.generation, 1);
    }

    #[tokio::test]
    async fn test_refresh_as_root_refuses_without_calling_adapters() {
        let (adapter, state) = FakeAdapter::new("fake");
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let snapshot = session.refresh(&root_env(), &CheckOptions::default()).await;
        assert_eq!(snapshot.detect, DetectOutcome::RefusedAsRoot);
        assert!(snapshot.instances.is_empty());
        assert_eq!(
            state.lock().unwrap().detect_calls,
            0,
            "no adapter should be probed while running as root"
        );
    }

    #[tokio::test]
    async fn test_refresh_as_root_stamps_refreshed_at() {
        let (adapter, _state) = FakeAdapter::new("fake");
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let snapshot = session.refresh(&root_env(), &CheckOptions::default()).await;
        assert_eq!(snapshot.detect, DetectOutcome::RefusedAsRoot);
        assert!(
            snapshot.refreshed_at.is_some(),
            "a root refusal is a completed refresh and must set refreshed_at"
        );
    }

    #[tokio::test]
    async fn test_refresh_with_no_instances_yields_missing() {
        let (adapter, _state) = FakeAdapter::new("fake");
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let snapshot = session.refresh(&non_root_env(), &CheckOptions::default()).await;
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
        let session = Session::with_adapters(sink, vec![adapter], None);
        let first = session.refresh(&non_root_env(), &CheckOptions::default()).await;
        assert_eq!(first.artifacts.len(), 2);
        assert!(!first.stale);
        let first_refreshed_at = first.refreshed_at;

        state.lock().unwrap().failing.push("fake:1".to_string());
        let second = session.refresh(&non_root_env(), &CheckOptions::default()).await;
        assert!(second.stale);
        assert_eq!(second.errors.len(), 1);
        assert_eq!(second.errors[0].instance_id, "fake:1");
        assert!(second.artifacts.iter().any(|a| a.key.name == "jq"));
        assert!(second.artifacts.iter().any(|a| a.key.name == "wget"));
        assert_eq!(
            second.refreshed_at, first_refreshed_at,
            "a refresh with a per-instance failure must not claim a new successful timestamp"
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
        let first = session.refresh(&non_root_env(), &CheckOptions::default()).await;
        let second = session.refresh(&non_root_env(), &CheckOptions::default()).await;
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
        let third = session.refresh(&non_root_env(), &CheckOptions::default()).await;
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
            tokio::spawn(async move { session_a.refresh(&non_root_env(), &CheckOptions::default()).await }),
            tokio::spawn(async move { session_b.refresh(&non_root_env(), &CheckOptions::default()).await }),
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
        session.refresh(&non_root_env(), &CheckOptions::default()).await;
        let calls_before = state.lock().unwrap().detect_calls;

        state.lock().unwrap().detect_delay = Duration::from_millis(100);
        let session_a = session.clone();
        let session_b = session.clone();
        let (a, b) = tokio::join!(
            tokio::spawn(async move { session_a.refresh(&non_root_env(), &CheckOptions::default()).await }),
            tokio::spawn(async move { session_b.refresh(&non_root_env(), &CheckOptions::default()).await }),
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
        let refreshed = session.refresh(&non_root_env(), &CheckOptions::default()).await;
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
        let snapshot = session.refresh(&non_root_env(), &CheckOptions::default()).await;
        let elapsed = started.elapsed();

        assert!(snapshot.instances.iter().any(|i| i.id == "slow-a:1"));
        assert!(snapshot.instances.iter().any(|i| i.id == "slow-b:1"));
        assert!(
            elapsed < Duration::from_millis(350),
            "two 200ms detects must overlap, not run back to back (took {elapsed:?})"
        );
    }

    #[tokio::test]
    async fn test_an_unhealthy_instance_is_a_reported_state_not_a_failed_refresh() {
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            let mut down = make_instance("fake", "fake:down");
            down.healthy = false;
            s.instances = vec![make_instance("fake", "fake:up"), down];
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
            "a refresh whose only complaint is a source known not to be running has completed"
        );
        assert!(!snapshot.stale);
        assert!(snapshot.errors.is_empty());
        assert!(
            snapshot.instances.iter().any(|i| i.id == "fake:down"),
            "the unhealthy instance stays in the snapshot so the UI can offer to start it"
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
        session.refresh(&non_root_env(), &CheckOptions::default()).await;
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
        let refresh_task =
            tokio::spawn(async move { session_for_refresh.refresh(&non_root_env(), &CheckOptions::default()).await });

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
}
```

- [ ] **Step 4: Rewrite `session/mod.rs` down to the facade**

Replace the entire file with:

```rust
//! `Session`: the facade `banager-core` exposes to a host shell (the Tauri
//! app in this repo, or a test harness). It owns the registered adapters,
//! the last known set of instances, and an in-memory, generation-numbered
//! `Snapshot`; it forwards operation lifecycle calls to an internal
//! `OperationManager`. Split (Task 14) into this facade, `refresh.rs`
//! (detection and the inventory/updates fetch) and `plans.rs`
//! (preview-then-confirm); no behaviour changed in the split itself.

mod plans;
mod refresh;

// `CheckOptions` is deliberately absent from this list: `refresh` -- the
// only thing that took it -- now lives in `refresh.rs`, which imports it
// itself. Leaving it here would be an unused import under `-D warnings`.
use crate::adapters::brew::BrewAdapter;
use crate::adapters::cargo::CargoAdapter;
use crate::adapters::npm::NpmAdapter;
use crate::adapters::ollama::OllamaAdapter;
use crate::adapters::pip::PipAdapter;
use crate::adapters::pipx::PipxAdapter;
use crate::adapters::uv::UvAdapter;
use crate::adapters::{Adapter, AdapterError};
use crate::events::{EventSink, OpId};
use crate::http::{HttpClient, RealHttpClient};
use crate::model::{
    AdapterId, InstalledArtifact, InstanceId, ManagerInstance, Plan, UpdateCandidate,
};
use crate::ops::{OpSummary, OperationManager};
use crate::runner::{CommandRunner, RealRunner};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceError {
    pub instance_id: InstanceId,
    pub message: String,
}

/// Why an adapter reported no usable instance. `Missing` is the ordinary
/// "Homebrew is not installed" case; `RefusedAsRoot` must be surfaced
/// differently in the UI (spec §7 empty states).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectOutcome {
    Found,
    Missing,
    RefusedAsRoot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub generation: u64,
    pub detect: DetectOutcome,
    pub instances: Vec<ManagerInstance>,
    pub artifacts: Vec<InstalledArtifact>,
    pub updates: Vec<UpdateCandidate>,
    /// Unix seconds of the last fully successful refresh, if any.
    pub refreshed_at: Option<i64>,
    /// True when the newest refresh attempt failed and this data is older
    /// than it looks (spec §3: keep old data, mark it possibly stale).
    pub stale: bool,
    pub errors: Vec<SourceError>,
}

impl Snapshot {
    fn empty() -> Snapshot {
        Snapshot {
            generation: 0,
            detect: DetectOutcome::Missing,
            instances: Vec::new(),
            artifacts: Vec::new(),
            updates: Vec::new(),
            refreshed_at: None,
            stale: false,
            errors: Vec::new(),
        }
    }

    /// Whether `self` and `other` carry the same *data* -- every field
    /// except `generation`, `refreshed_at` and `stale`, which describe the
    /// refresh attempt rather than the fetched data itself.
    fn same_content(&self, other: &Snapshot) -> bool {
        self.detect == other.detect
            && self.instances == other.instances
            && self.artifacts == other.artifacts
            && self.updates == other.updates
            && self.errors == other.errors
    }
}

/// Opaque handle to a plan `Session` has issued and is holding server-side.
/// The front end never constructs one; it only ever echoes back the `id` it
/// was given.
pub type PlanId = u64;

/// A `Plan` the server has already computed and stored, returned to the
/// caller for preview. Submitting requires only the `id`; the `plan` field
/// is for display (exact command preview, spec §6) and is never accepted
/// back from the client (see `Session::submit`, in `plans.rs`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssuedPlan {
    pub id: PlanId,
    pub plan: Plan,
    /// Unix seconds when this plan was issued, used to decide expiry.
    pub issued_at: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SubmitError {
    #[error("no such plan, or it was already submitted")]
    Unknown,
    #[error("this plan is older than 10 minutes; preview it again")]
    Expired,
}

pub struct Session {
    adapters: HashMap<AdapterId, Arc<dyn Adapter>>,
    ops: Arc<OperationManager>,
    /// Serialises `refresh()` (in `refresh.rs`): whichever caller acquires
    /// this first does the real work; anyone already waiting when it
    /// releases just re-reads `snapshot`.
    refresh_gate: tokio::sync::Mutex<()>,
    snapshot: Mutex<Snapshot>,
    /// Bumped every time a refresh actually completes, regardless of
    /// whether its content -- and therefore `generation` -- changed. See
    /// `refresh.rs`'s doc comment for why a waiter needs this instead of
    /// `generation` alone.
    refresh_seq: AtomicU64,
    /// Plans handed out by `issue_plan` (in `plans.rs`) but not yet
    /// consumed by `submit`, keyed by `PlanId`.
    issued_plans: Mutex<HashMap<PlanId, IssuedPlan>>,
    next_plan_id: AtomicU64,
    now_fn: Option<fn() -> i64>,
}

impl Session {
    /// Registers all seven adapters over a shared `RealRunner` and
    /// `RealHttpClient` (network-touching adapters only: pipx, cargo,
    /// ollama). `now_fn` exists so tests can pin `refreshed_at`; production
    /// passes `None`.
    pub fn new(sink: Arc<dyn EventSink>, now_fn: Option<fn() -> i64>) -> Arc<Session> {
        let runner: Arc<dyn CommandRunner> = Arc::new(RealRunner::new());
        let http: Arc<dyn HttpClient> = Arc::new(RealHttpClient::new());
        let adapters: Vec<Arc<dyn Adapter>> = vec![
            Arc::new(BrewAdapter::new(runner.clone())),
            Arc::new(NpmAdapter::new(runner.clone())),
            Arc::new(PipxAdapter::new(runner.clone(), http.clone())),
            Arc::new(UvAdapter::new(runner.clone())),
            Arc::new(PipAdapter::new(runner.clone())),
            Arc::new(CargoAdapter::new(runner.clone(), http.clone())),
            Arc::new(OllamaAdapter::new(runner, http)),
        ];
        Session::with_adapters(sink, adapters, now_fn)
    }

    /// Test seam: build a Session over arbitrary adapters.
    pub fn with_adapters(
        sink: Arc<dyn EventSink>,
        adapters: Vec<Arc<dyn Adapter>>,
        now_fn: Option<fn() -> i64>,
    ) -> Arc<Session> {
        let mut ops = OperationManager::new(sink);
        let mut by_id = HashMap::new();
        for adapter in adapters {
            ops.register_adapter(adapter.clone());
            by_id.insert(adapter.meta().id.clone(), adapter);
        }
        Arc::new(Session {
            adapters: by_id,
            ops: Arc::new(ops),
            refresh_gate: tokio::sync::Mutex::new(()),
            snapshot: Mutex::new(Snapshot::empty()),
            refresh_seq: AtomicU64::new(0),
            issued_plans: Mutex::new(HashMap::new()),
            next_plan_id: AtomicU64::new(1),
            now_fn,
        })
    }

    fn now(&self) -> i64 {
        match self.now_fn {
            Some(f) => f(),
            None => std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap().clone()
    }

    pub fn cancel(&self, op_id: OpId) {
        self.ops.cancel(op_id)
    }

    pub fn operations(&self) -> Vec<OpSummary> {
        self.ops.summaries()
    }

    /// Sorted ids of every adapter this Session has registered, regardless
    /// of whether that adapter currently detects any instance on the host.
    /// A test seam (Task 11) so registration itself is verifiable without
    /// depending on which tools happen to be installed on the machine
    /// running the test.
    pub fn adapter_ids(&self) -> Vec<AdapterId> {
        let mut ids: Vec<AdapterId> = self.adapters.keys().cloned().collect();
        ids.sort();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities, CheckOptions};
    use crate::events::{EventSink, OpId, VecSink};
    use crate::model::{
        ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, OpKind, OpRequest, OpStatus,
        Outcome, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate,
    };
    use crate::runner::HostEnv;
    use async_trait::async_trait;
    use std::path::PathBuf;
    use std::sync::Mutex as StdMutex;
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;

    struct FakeState {
        instances: Vec<ManagerInstance>,
        block_execute: bool,
    }

    struct FakeAdapter {
        meta: AdapterMeta,
        state: Arc<StdMutex<FakeState>>,
    }

    impl FakeAdapter {
        fn new() -> (Arc<FakeAdapter>, Arc<StdMutex<FakeState>>) {
            let state = Arc::new(StdMutex::new(FakeState {
                instances: Vec::new(),
                block_execute: false,
            }));
            let adapter = Arc::new(FakeAdapter {
                meta: AdapterMeta {
                    id: "fake".to_string(),
                    name: "fake".to_string(),
                    kind: "fake".to_string(),
                    platforms: vec!["macos".to_string()],
                    homepage: "https://example.invalid".to_string(),
                    schema_version: 1,
                    verified_versions: vec![],
                },
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

        async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
            self.state.lock().unwrap().instances.clone()
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
        ) -> Result<Vec<UpdateCandidate>, AdapterError> {
            Ok(Vec::new())
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
            Ok(Plan {
                request: req.clone(),
                program: inst.exe_path.clone(),
                args: vec!["do".to_string(), req.name.clone()],
                env: vec![],
                needs_password: false,
                locks: vec![ResourceLock(inst.id.clone())],
                cancel_policy: CancelPolicy::KillThenReconcile,
                warnings: vec![],
                affected: vec![],
                timeout_secs: 60,
            })
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
            _key: &ArtifactKey,
        ) -> Result<Reconciled, AdapterError> {
            Ok(Reconciled {
                present: true,
                version: None,
            })
        }
    }

    fn make_instance(id: &str) -> ManagerInstance {
        ManagerInstance {
            id: id.to_string(),
            adapter_id: "fake".to_string(),
            exe_path: PathBuf::from("/bin/true"),
            prefix: PathBuf::from("/"),
            scope: Scope::User,
            version: Some("1.0".to_string()),
            healthy: true,
            unverified_version: None,
        }
    }

    fn non_root_env() -> HostEnv {
        HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/tmp"),
            euid: 501,
        }
    }

    #[test]
    fn test_new_registers_all_seven_adapters() {
        let sink = Arc::new(VecSink::new());
        let session = Session::new(sink, None);
        assert_eq!(
            session.adapter_ids(),
            vec![
                "brew".to_string(),
                "cargo".to_string(),
                "npm".to_string(),
                "ollama".to_string(),
                "pip".to_string(),
                "pipx".to_string(),
                "uv".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_submit_cancel_and_operations_forward_to_the_operation_manager() {
        let (adapter, state) = FakeAdapter::new();
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake:1")];
            s.block_execute = true;
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session.refresh(&non_root_env(), &CheckOptions::default()).await;
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
        assert_eq!(session.operations().len(), 1);

        session.cancel(op_id);

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(summary) = session.operations().into_iter().find(|o| o.id == op_id) {
                if summary.status == OpStatus::Done {
                    assert_eq!(summary.outcome, Some(Outcome::Succeeded));
                    break;
                }
            }
            assert!(Instant::now() < deadline, "operation never finished");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
```

- [ ] **Step 5: Run the split's own proof**

Run: `cargo test -p banager-core session:: 2>&1 | tail -5`
Expected: PASS, with the exact same test count as Step 1's baseline (every test now lives in `session::tests`, `session::refresh::tests` or `session::plans::tests`, but none was added, removed or changed).

- [ ] **Step 6: Fix up formatting and lints**

Run: `cargo fmt --all --check`
Expected: after `cargo fmt --all` (no `--check`) is run once to apply formatting to the three files, a second `cargo fmt --all --check` is clean.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean (no unused-import or dead-code warnings across `mod.rs`/`refresh.rs`/`plans.rs`).

- [ ] **Step 7: Account for the two unparsed fixtures, then write the fixture-layout regression guard**

Two committed fixtures are read by no parser in any task, and nothing currently says so — the next reader will treat them as a missing parser rather than a deliberate recording. Say so where they live.

Append to `adapters/fixtures/npm/12.0.2/README.md`:

```markdown
`view-jq-description.json` is recorded from `npm view jq description --json`.
No parser reads it: this phase's npm adapter runs only `ls`, `outdated`,
`search` and the install/uninstall/upgrade commands, per the per-adapter
contract table. It is kept for whichever later phase populates
`SearchHit.description` / `InstalledArtifact.description`.
```

Append to `adapters/fixtures/cargo/1.98.1/README.md`:

```markdown
`install-list.txt` is recorded from `cargo install --list`. The adapter reads
`.crates2.json` instead — the text output begins with two unrelated
workspace-profile warning lines, which is exactly why. This file is kept as
corroboration that the `.crates2.json` parse agrees with what cargo itself
reports, not as a parser input.
```

Then `crates/banager-core/tests/fixtures_layout_test.rs` (new file):

```rust
use banager_core::events::VecSink;
use banager_core::session::Session;
use std::path::Path;
use std::sync::Arc;

/// Regression guard: every adapter id `Session::new` registers must have a
/// recorded fixture directory under `adapters/fixtures/`, and every fixture
/// directory that exists must be for a real, registered adapter -- so a
/// fixture directory can never go stale (renamed adapter, removed source)
/// without this test catching it, and a newly added source can never ship
/// without at least one recorded, README'd fixture version.
#[test]
fn test_every_registered_adapter_has_a_documented_fixture_directory() {
    let sink = Arc::new(VecSink::new());
    let session = Session::new(sink, None);
    let adapter_ids = session.adapter_ids();

    // cargo runs tests with cwd = the package manifest directory
    // (crates/banager-core), matching every other fixture/meta path in this
    // crate.
    let fixtures_root = Path::new("../../adapters/fixtures");
    let mut fixture_ids: Vec<String> = std::fs::read_dir(fixtures_root)
        .expect("read adapters/fixtures")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    fixture_ids.sort();

    assert_eq!(
        fixture_ids, adapter_ids,
        "adapters/fixtures/* must have exactly one directory per registered adapter id"
    );

    for id in &adapter_ids {
        let source_dir = fixtures_root.join(id);
        let mut versions: Vec<_> = std::fs::read_dir(&source_dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", source_dir.display()))
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .collect();
        assert!(
            !versions.is_empty(),
            "{} has no recorded version directory",
            source_dir.display()
        );
        versions.sort_by_key(|e| e.file_name());
        for version_dir in versions {
            let readme = version_dir.path().join("README.md");
            assert!(
                readme.is_file(),
                "{} is missing a README.md naming its commands and traps",
                version_dir.path().display()
            );
        }
    }
}
```

- [ ] **Step 8: Run it to verify it passes**

Run: `cargo test -p banager-core --test fixtures_layout_test`
Expected: PASS. This is a verification of already-recorded, already-correct static data plus Task 11's already-correct `adapter_ids()` (both landed in earlier tasks), so there is no red state to drive through here — its value is as a permanent guard against a future fixture directory silently going stale or a new source shipping undocumented, not as a red/green cycle.

- [ ] **Step 9: Run the full gate**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && pnpm test && pnpm exec tsc -p tsconfig.json`
Expected: all clean.

- [ ] **Step 10: Commit**

```bash
git add crates/banager-core/src/session/mod.rs crates/banager-core/src/session/refresh.rs crates/banager-core/src/session/plans.rs crates/banager-core/tests/fixtures_layout_test.rs adapters/fixtures/npm/12.0.2/README.md adapters/fixtures/cargo/1.98.1/README.md
git commit -m "$(cat <<'EOF'
refactor(session): split mod.rs into facade, refresh and plans

session/mod.rs had grown to over a thousand lines covering three
distinct responsibilities. refresh() and its commit() helper move to
session/refresh.rs; issue_plan() and submit() move to session/plans.rs
as sibling impl blocks (both reach Session's private fields as child
modules of session). No behaviour changed -- every pre-existing test
passes unchanged, split only by which file it now lives in. Also adds
a regression guard tying Session::new's registered adapter ids to the
recorded adapters/fixtures/* directories, so CI catches a fixture
directory going stale or a new source shipping undocumented.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```
