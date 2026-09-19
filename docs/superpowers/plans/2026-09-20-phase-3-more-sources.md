# Canager Phase 3 Implementation Plan: The Other Sources

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Canager see everything a neglected Mac actually accumulated, not just Homebrew: global npm packages, pipx and uv tools, pip packages (read-only), cargo binaries, and Ollama models — each with the same guarantees Homebrew already has (an exact command preview before anything destructive, a live log, a working cancel, and a post-execution check that the machine really changed).

**Architecture:** Six new adapters behind the existing `Adapter` trait. Two of them need the network, so this phase adds an `HttpClient` trait with a real and a mock implementation, mirroring how `CommandRunner` already makes subprocess work testable. The trait gains a `CheckOptions` parameter so a setting can reach `check_updates` (the change `greedy_casks` needed). Everything destructive still flows through a subprocess `Plan`, including Ollama — see the ruling below.

**Tech Stack:** Rust (canager-core, tauri 2.11.x), reqwest with rustls, tokio; React 19, TanStack Query v5, Zustand, i18next, vitest.

## Ruling: Ollama writes go through the CLI, not the HTTP API

Spec §4.2 specifies `POST /api/pull` and `DELETE /api/delete` for Ollama's destructive operations. This plan uses `ollama pull {model}` and `ollama rm {model}` instead, and keeps HTTP for reads only (`GET /api/tags`, and the registry manifest comparison).

Why: every guarantee the product makes about a destructive action is built on the subprocess `Plan` — the preview shows `program` plus `args`, cancellation kills the process group, the log drawer streams the child's output, and `reconcile` re-checks afterwards. An HTTP write would need a second, parallel version of all four, and the preview would have to show something that is not a command while spec §6 promises "the exact command that will run". The CLI is present whenever the daemon is (they ship together), `ollama pull` streams progress on stdout, and `ollama rm` is instant. Reads stay on HTTP because `ollama list` shells out to the same daemon anyway and, on macOS, launches Ollama.app as a side effect — which a background refresh must never do.

Recorded as a deliberate deviation. If a later phase needs byte-level progress bars, revisit it then; spec §4.2's own fallback ("last pulled + pull again") already tolerates less precision.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-09-17-canager-design.md`. §3 (data flow), §4.1–4.2 (adapters, per-source commands), §5 (data model), §6 (execution and safety), §7 (UI), §11 (testing) bind this phase.
- Backlog: `docs/superpowers/backlog.md`. Everything under "阶段 3（其余来源）/ 存储与刷新层" is implemented here and must not be deferred again. The phase-2 deferrals named in Task 13 are also in scope.
- macOS only; minimum macOS 13.3; Tauri ≥ 2.11.1. `canager-core` never depends on `tauri` and never creates a tokio runtime.
- Commands are argv arrays with an absolute program path, never a shell string. Package names pass `validate_package_name` before they reach any argv.
- Every destructive action previews its exact command before running. Operations that need a password say so first. The front end never builds an argv, never decides what is safe to remove, and never guesses an outcome.
- Network access is confined to the `HttpClient` trait. No adapter calls reqwest directly, so every network path has a mock in tests. Requests carry a `canager/{version}` User-Agent and a 30-second timeout.
- A background refresh never launches an application, never prompts for a password, and never writes to the machine.
- No business logic in TypeScript. Components never call `invoke`; only `src/lib/api.ts` does. No hard-coded user-visible strings — every one goes through `t()`, with matching keys in `en.json` and `zh-CN.json` (the parity test enforces this).
- Colours use `bg-[var(--color-x)]` arbitrary values only; never bare semantic classes, never hex.
- Interactions that assert on DOM changes use `fireEvent` plus `findBy*`/`waitFor`, never a naked `.click()` followed by a synchronous assertion.
- Fixtures come from real machines only — record them, never hand-write them. Inline JSON in a unit test is fine and is not a fixture.
- Definition of done for every task: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm test`, `pnpm exec tsc -p tsconfig.json` — all clean, no new warnings.
- Commit messages end with a blank line then `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>`.

## What already exists (do not rebuild)

`canager-core` has: `model.rs` (ManagerInstance, InstalledArtifact, UpdateCandidate, ArtifactKey, Plan, OpRequest, Outcome, …), `events.rs` (EventSink, OperationEvent), `runner/` (CommandRunner with RealRunner and MockRunner, HostEnv PATH hydration, process-group kill, timeouts), `adapters/` (the Adapter trait, AdapterMeta from TOML, validate_package_name, and the Homebrew adapter), `ops/` (OperationManager with resource locks, a 3-permit semaphore, cancel semantics and post-execution reconcile), `session/` (Session facade, generation-numbered Snapshot, refresh coalescing, server-issued single-use IssuedPlan), `settings.rs`.

`src-tauri` has nine IPC commands and a Channel event bridge. The React app has the installed, updates and settings pages, an operation bar, a log drawer, an uninstall dialog, empty and error states, and en + zh-CN.

125 Rust tests and 124 front-end tests pass on main; CI is green including a live Homebrew install/uninstall smoke test.

---

## File Structure

```
crates/canager-core/src/
├── http/mod.rs             NEW  HttpClient trait, HttpRequest/HttpResponse, HttpError
├── http/real.rs            NEW  RealHttpClient (reqwest, rustls, 30s timeout, canager UA)
├── http/mock.rs            NEW  MockHttpClient (url -> canned response, records calls)
├── adapters/mod.rs         MOD  CheckOptions; Adapter::check_updates gains it; Capabilities gains `needs_network`
├── adapters/brew/mod.rs    MOD  honour CheckOptions.include_self_updating (--greedy); degrade a failed `brew update` to a warning instead of failing the whole check; serialise maybe_update per instance
├── adapters/npm.rs         NEW
├── adapters/pipx.rs        NEW
├── adapters/uv.rs          NEW
├── adapters/pip.rs         NEW  read-only
├── adapters/cargo.rs       NEW  ~/.cargo/.crates2.json + crates.io
├── adapters/ollama/mod.rs  NEW  HTTP reads, CLI writes
├── adapters/ollama/parse.rs NEW /api/tags and registry manifest shapes
├── session/mod.rs          SPLIT into session/mod.rs (facade) + session/refresh.rs + session/plans.rs
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
// crates/canager-core/src/http/mod.rs
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
impl RealHttpClient { pub fn new() -> RealHttpClient; }   // rustls, 30s default, UA "canager/{CARGO_PKG_VERSION}"

pub struct MockHttpClient { /* private */ }
impl MockHttpClient {
    pub fn new() -> MockHttpClient;
    pub fn respond(&self, url: &str, response: HttpResponse);
    pub fn fail(&self, url: &str, error: &str);
    pub fn calls(&self) -> Vec<String>;                    // urls, in order
}
```

```rust
// crates/canager-core/src/adapters/mod.rs  (changed)
/// Options a caller passes down to `check_updates`. Adapters ignore fields
/// that do not apply to them; a new field must never change behaviour for an
/// adapter that does not read it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckOptions {
    /// Homebrew only: include casks that update themselves (`brew outdated --greedy`).
    pub include_self_updating: bool,
}

pub struct Capabilities {
    pub search: bool,
    pub per_item_upgrade: bool,
    pub upgrade_all: bool,
    pub uninstall: bool,
    pub background_check: bool,
    pub cancel_safe: bool,
    /// True when `check_updates` reaches the network. The UI marks these as
    /// unavailable offline rather than showing them as "up to date".
    pub needs_network: bool,
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
// crates/canager-core/src/model.rs  (added field, additive only)
pub struct ManagerInstance {
    // … existing fields …
    /// None when the adapter's metadata lists no verified versions, or when
    /// the detected version is among them. Some(detected) when it is not, so
    /// the UI can mark the source as running an unverified version (spec §4.1).
    pub unverified_version: Option<String>,
}
```

```rust
// crates/canager-core/src/settings.rs  (added field)
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
| `pipx` | the `pipx` executable | `pipx list --json`, version at `venvs.<name>.metadata.main_package.package_version` | `pipx list --outdated` when `pipx --version` ≥ 1.16, else compare PyPI JSON per package through `HttpClient` | `pipx install/uninstall/upgrade {id}`; upgrade-all | none | |
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
