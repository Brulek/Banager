# Banager Phase 0–1 Implementation Plan: Signed Skeleton + Core + Homebrew Adapter

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a signed, notarized macOS Tauri app shell built by CI, plus a Tauri-independent `banager-core` crate that can detect Homebrew, inventory formulae and casks with install reasons, check for updates, search, and run install/uninstall/upgrade operations with streaming events, cancellation and post-execution reconciliation, all verified by fixture-driven tests recorded on a real Mac.

**Architecture:** Cargo workspace with two crates: `crates/banager-core` (pure Rust: model, event sink, command runner, adapter trait, Homebrew adapter, operation manager) and `src-tauri` (Tauri 2 shell, for now only a hello window that hydrates PATH at startup). Frontend is a Vite + React 19 + TypeScript + Tailwind v4 app scaffolded by `create-tauri-app`. Adapters are typed Rust implementations; TOML carries metadata only and is compiled in with `include_str!`.

**Tech Stack:** Rust stable (edition 2021), Tauri 2 (≥ 2.11.1), tokio, tokio-util (CancellationToken), async-trait, serde / serde_json / toml, thiserror, insta (snapshot tests), fix-path-env, libc; pnpm, Vite, React 19, TypeScript, Tailwind v4; GitHub Actions with `tauri-apps/tauri-action`.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-09-17-banager-design.md`. Every task inherits it.
- Platform: macOS only for v1; universal build (`--target universal-apple-darwin`); minimum macOS 13.3.
- Tauri version floor: `tauri = "2.11.1"` or newer (CVE-2026-42184). Bundle identifier: `com.brulek.banager`. Product name: `Banager`.
- Commands are always argv arrays with an absolute program path; never a shell string. Package names validated against `^[A-Za-z0-9@._+/-]+$` and must not start with `-`.
- Homebrew environment for every brew invocation: `HOMEBREW_NO_AUTO_UPDATE=1`, `HOMEBREW_NO_ENV_HINTS=1`, `HOMEBREW_NO_INSTALL_CLEANUP=1`, `NO_COLOR=1`. Refuse to run brew when euid is 0.
- Never pass `--ignore-dependencies` to `brew uninstall`. Never run bare `brew upgrade`; upgrade is always per item.
- Timeouts: detect 30 s, inventory/check 120 s, install/upgrade/uninstall 1800 s.
- Fixtures are recorded from a real Mac only (`adapters/fixtures/brew/<brew-version>/`); never hand-written or AI-generated.
- Core crate must not depend on `tauri`. Core never creates a tokio runtime; tests use `#[tokio::test]`.
- Secrets (Apple certificate, notarization credentials, updater private key) are set by the user with `gh secret set`; the plan never contains or echoes their values.
- Commit messages end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`.
- Language of code comments and docs: English identifiers, Chinese or English prose both acceptable.

---

## File Structure

```
Banager/
├── Cargo.toml                          workspace: members = ["src-tauri", "crates/banager-core"]
├── package.json / pnpm-lock.yaml / vite.config.ts / index.html / tsconfig.json
├── src/                                React shell (hello window only in this plan)
│   ├── main.tsx  App.tsx  index.css (Tailwind v4 import)
├── src-tauri/
│   ├── Cargo.toml                      crate "banager" (bin), depends on banager-core, fix-path-env
│   ├── tauri.conf.json                 identifier com.brulek.banager, macOS minimumSystemVersion 13.3, updater + bundle config
│   ├── capabilities/default.json
│   └── src/main.rs, src/lib.rs         run(): fix_path_env::fix() then tauri::Builder
├── crates/banager-core/
│   ├── Cargo.toml
│   ├── src/lib.rs                      pub mod model, events, runner, adapters, ops; pub use of common types
│   ├── src/model.rs                    ManagerInstance, ArtifactKey, InstalledArtifact, UpdateCandidate, SearchHit, OpRequest, Plan, Outcome, OpStatus, Reconciled …
│   ├── src/events.rs                   OpId, OperationEvent, EventSink trait, VecSink (test sink)
│   ├── src/runner/mod.rs               CommandSpec, CommandOutput, CommandRunner trait, RunnerError
│   ├── src/runner/real.rs              RealRunner (tokio::process, process group, timeout, cancel, line streaming)
│   ├── src/runner/mock.rs              MockRunner (argv → canned output, records calls)
│   ├── src/runner/path_env.rs          HostEnv::discover(), resolve_exe()
│   ├── src/adapters/mod.rs             Capabilities, AdapterMeta (+ TOML loading), AdapterError, Adapter trait, validate_package_name()
│   ├── src/adapters/brew/mod.rs        BrewAdapter (detect / inventory / check_updates / search / plan / execute / reconcile)
│   ├── src/adapters/brew/parse.rs      serde types for brew JSON v2 + parse_info_installed(), parse_outdated(), parse_search()
│   ├── src/ops/mod.rs                  OperationManager (queue, resource locks, state machine, cancel, verifying)
│   ├── examples/brew_smoke.rs          manual end-to-end check on the developer's Mac (read-only)
│   ├── tests/brew_fixtures.rs          fixture-driven parser + adapter tests (insta snapshots)
│   └── tests/brew_live.rs              CI-only live smoke test (install/inventory/uninstall hello, gated by BANAGER_LIVE=1)
├── adapters/meta/brew.toml             adapter metadata (compiled in via include_str!)
├── adapters/fixtures/brew/<version>/   info-installed.json, outdated.json, search-jq.txt, search-desc-jq.txt, uses-jq.txt, version.txt
├── scripts/banager-askpass.sh          SUDO_ASKPASS helper spike (osascript password dialog)
├── docs/spikes/2026-09-askpass.md      spike result
├── docs/what-we-run.md                 every command the app can execute, per adapter
└── .github/workflows/ci.yml, release.yml
```

## Core Interfaces (authoritative; later tasks must use these exact names)

```rust
// crates/banager-core/src/model.rs
use std::path::PathBuf;
use serde::{Serialize, Deserialize};

pub type InstanceId = String;   // "brew:/opt/homebrew"
pub type AdapterId = String;    // "brew"

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope { User, System }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagerInstance {
    pub id: InstanceId,
    pub adapter_id: AdapterId,
    pub exe_path: PathBuf,
    pub prefix: PathBuf,
    pub scope: Scope,
    pub version: Option<String>,
    pub healthy: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArtifactKind { Formula, Cask, Package, Tool, Model, Binary }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallReason { Requested, Dependency, Unknown }

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArtifactKey { pub instance_id: InstanceId, pub kind: ArtifactKind, pub name: String }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledArtifact {
    pub key: ArtifactKey,
    pub display_name: String,
    pub version: String,
    pub reason: InstallReason,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub size_bytes: Option<u64>,
    pub installed_at: Option<i64>,      // unix seconds
    pub path: Option<PathBuf>,
    pub auto_updates: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateChannel { Native, Registry, Digest }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateCandidate {
    pub key: ArtifactKey,
    pub current: String,
    pub target: String,
    pub channel: UpdateChannel,
    pub checkable: bool,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit { pub adapter_id: AdapterId, pub kind: ArtifactKind, pub name: String, pub description: Option<String> }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpKind { Install, Uninstall, Upgrade }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpRequest { pub kind: OpKind, pub instance_id: InstanceId, pub artifact_kind: ArtifactKind, pub name: String }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CancelPolicy { SafeKill, KillThenReconcile, NoCancel }

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceLock(pub String);   // "brew:/opt/homebrew"

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub request: OpRequest,
    pub program: PathBuf,
    pub args: Vec<String>,              // argv without program; preview = program + args
    pub env: Vec<(String, String)>,
    pub needs_password: bool,
    pub locks: Vec<ResourceLock>,
    pub cancel_policy: CancelPolicy,
    pub warnings: Vec<String>,
    pub affected: Vec<String>,          // dependents that would break on uninstall
    pub timeout_secs: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Succeeded,
    NoChange,
    PartialSuccess,
    NeedsAttention(String),
    Failed { exit_code: Option<i32>, summary: String },
    Unconfirmed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpStatus { Queued, Running, CancelRequested, Cancelling, Verifying, Done }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reconciled { pub present: bool, pub version: Option<String> }
```

```rust
// crates/banager-core/src/events.rs
pub type OpId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stream { Stdout, Stderr }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationEvent {
    Status { op_id: OpId, status: OpStatus },
    Log { op_id: OpId, stream: Stream, line: String },
    Finished { op_id: OpId, outcome: Outcome },
}

pub trait EventSink: Send + Sync {
    fn emit(&self, event: OperationEvent);
}

pub struct VecSink { pub events: std::sync::Mutex<Vec<OperationEvent>> }   // impl EventSink; VecSink::new(); .snapshot() -> Vec<OperationEvent>
```

```rust
// crates/banager-core/src/runner/mod.rs
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: Option<PathBuf>,
    pub timeout: std::time::Duration,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOutput { pub exit_code: Option<i32>, pub stdout: String, pub stderr: String, pub timed_out: bool, pub cancelled: bool }

pub type LineCallback = std::sync::Arc<dyn Fn(Stream, String) + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("program not found: {0}")] NotFound(PathBuf),
    #[error("spawn failed: {0}")] Spawn(#[from] std::io::Error),
    #[error("no canned response for {0:?}")] NoMock(Vec<String>),
}

#[async_trait::async_trait]
pub trait CommandRunner: Send + Sync {
    async fn run(&self, spec: CommandSpec, on_line: Option<LineCallback>, cancel: tokio_util::sync::CancellationToken) -> Result<CommandOutput, RunnerError>;
}
// runner/real.rs:  pub struct RealRunner;  (process_group(0) on unix, kills the whole group on cancel/timeout, reads stdout+stderr concurrently, splits on '\n' and '\r')
// runner/mock.rs:  pub struct MockRunner { .. }  MockRunner::new(); .respond(argv: Vec<&str>, output: CommandOutput); .calls() -> Vec<Vec<String>>  (argv includes program path as first element; lookup by exact argv)
// runner/path_env.rs: pub struct HostEnv { pub path_dirs: Vec<PathBuf>, pub home: PathBuf, pub euid: u32 }
//                     impl HostEnv { pub fn discover() -> HostEnv }  (uses std::env PATH after fix_path_env::fix() was called by the host; libc::geteuid)
//                     pub fn resolve_exe(name: &str, env: &HostEnv) -> Option<PathBuf>
```

```rust
// crates/banager-core/src/adapters/mod.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities { pub search: bool, pub per_item_upgrade: bool, pub upgrade_all: bool, pub uninstall: bool, pub background_check: bool, pub cancel_safe: bool }

#[derive(Clone, Debug, Deserialize)]
pub struct AdapterMeta { pub id: String, pub name: String, pub kind: String, pub platforms: Vec<String>, pub homepage: String, pub schema_version: u32, pub verified_versions: Vec<String> }
impl AdapterMeta { pub fn from_toml(s: &str) -> Result<AdapterMeta, toml::de::Error> }

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("runner: {0}")] Runner(#[from] crate::runner::RunnerError),
    #[error("parse: {0}")] Parse(String),
    #[error("command failed (exit {code:?}): {stderr}")] CommandFailed { code: Option<i32>, stderr: String },
    #[error("refused: {0}")] Refused(String),
    #[error("invalid name: {0}")] InvalidName(String),
    #[error("unsupported: {0}")] Unsupported(String),
}

pub fn validate_package_name(name: &str) -> Result<(), AdapterError>;   // ^[A-Za-z0-9@._+/-]+$ and not starting with '-'

#[async_trait::async_trait]
pub trait Adapter: Send + Sync {
    fn meta(&self) -> &AdapterMeta;
    fn capabilities(&self) -> Capabilities;
    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance>;
    async fn inventory(&self, inst: &ManagerInstance) -> Result<Vec<InstalledArtifact>, AdapterError>;
    async fn check_updates(&self, inst: &ManagerInstance) -> Result<Vec<UpdateCandidate>, AdapterError>;
    async fn search(&self, inst: &ManagerInstance, query: &str) -> Result<Vec<SearchHit>, AdapterError>;
    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError>;
    async fn execute(&self, plan: &Plan, sink: Arc<dyn EventSink>, op_id: OpId, cancel: CancellationToken) -> Result<Outcome, AdapterError>;
    async fn reconcile(&self, inst: &ManagerInstance, key: &ArtifactKey) -> Result<Reconciled, AdapterError>;
}
```

```rust
// crates/banager-core/src/adapters/brew/mod.rs
pub struct BrewAdapter { /* runner: Arc<dyn CommandRunner>, meta: AdapterMeta, last_update: Mutex<Option<Instant>>, update_ttl: Duration */ }
impl BrewAdapter {
    pub fn new(runner: Arc<dyn CommandRunner>) -> BrewAdapter;           // update_ttl = 6h
    pub fn with_update_ttl(self, ttl: Duration) -> BrewAdapter;
    pub const ENV: [(&'static str, &'static str); 4];                     // the four HOMEBREW_/NO_COLOR vars
    pub const CANDIDATE_PATHS: [&'static str; 3];                         // /opt/homebrew/bin/brew, /usr/local/bin/brew, /home/linuxbrew/.linuxbrew/bin/brew
}
// brew/parse.rs
pub fn parse_info_installed(json: &str, instance_id: &str) -> Result<Vec<InstalledArtifact>, AdapterError>;
pub fn parse_outdated(json: &str, instance_id: &str) -> Result<Vec<UpdateCandidate>, AdapterError>;
pub fn parse_search(text: &str, adapter_id: &str) -> Vec<SearchHit>;      // sections "==> Formulae" / "==> Casks"; lines "name" or "name: desc"
pub fn parse_uses(text: &str) -> Vec<String>;                             // whitespace separated names
pub fn parse_version(text: &str) -> Option<String>;                       // "Homebrew 7.0.3" -> "7.0.3"
```

```rust
// crates/banager-core/src/ops/mod.rs
pub struct OperationManager { /* adapters: HashMap<AdapterId, Arc<dyn Adapter>>, instances: Mutex<HashMap<InstanceId, ManagerInstance>>, sink: Arc<dyn EventSink>, held: Arc<Mutex<HashSet<ResourceLock>>>, records: Arc<Mutex<HashMap<OpId, OpRecord>>>, next_id: AtomicU64 */ }
pub struct OpRecord { pub id: OpId, pub plan: Plan, pub status: OpStatus, pub outcome: Option<Outcome>, pub cancel: CancellationToken }
impl OperationManager {
    pub fn new(sink: Arc<dyn EventSink>) -> OperationManager;
    pub fn register_adapter(&mut self, adapter: Arc<dyn Adapter>);
    pub fn register_instance(&self, inst: ManagerInstance);
    pub fn submit(self: &Arc<Self>, plan: Plan) -> OpId;      // spawns a tokio task; waits for locks (poll every 50 ms), Running → execute → Verifying (reconcile) → Done; emits Status/Finished
    pub fn cancel(&self, op_id: OpId);                         // sets CancelRequested, cancels the token
    pub fn record(&self, op_id: OpId) -> Option<OpRecord>;
    pub async fn wait(&self, op_id: OpId) -> Option<Outcome>;  // polls until Done
}
```

## Task List (expanded below)

| # | Task | Deliverable |
|---|---|---|
| 1 | Scaffold Tauri app + Cargo workspace + Tailwind | `pnpm tauri build` produces `Banager.app` locally (unsigned) |
| 2 | CI workflow | `ci.yml` green on macOS runner: fmt, clippy, cargo test, pnpm build, tauri build |
| 3 | Release workflow with signing, notarization, updater | tag `v0.0.1` → notarized universal `.dmg` + `latest.json` (needs user-provided secrets) |
| 4 | `banager-core` crate: model + events | types compile, serde round-trip tests, `VecSink` |
| 5 | Runner: trait, `MockRunner`, `RealRunner` | streaming lines, exit codes, timeout kills process group, cancel works |
| 6 | PATH hydration + `HostEnv` + `resolve_exe`; call `fix_path_env::fix()` in Tauri startup | `resolve_exe("sh")` finds `/bin/sh`; shell logs the discovered PATH |
| 7 | Adapter trait, `Capabilities`, `AdapterMeta` from `adapters/meta/brew.toml`, `validate_package_name` | meta parses; name validation rejects `-rf`, `a;b` |
| 8 | Record Homebrew fixtures from the real Mac | files under `adapters/fixtures/brew/<version>/` committed |
| 9 | Brew parsers with insta snapshot tests over fixtures | `parse_info_installed` handles formulae + casks, reasons, linked version; `parse_outdated`; `parse_search` sections; `parse_uses`; `parse_version` |
| 10 | `BrewAdapter`: detect / inventory / check_updates (with `brew update` TTL) / search, tested with `MockRunner` | euid 0 refused; TTL prevents second `brew update`; casks marked `Cask` |
| 11 | `BrewAdapter`: plan / execute / reconcile for install, uninstall (with `brew uses --installed` guard), upgrade; `SUDO_ASKPASS` passthrough | argv previews exact; uninstall with dependents lists `affected` and warning; execute streams `Log` events; reconcile re-inventories |
| 12 | `OperationManager` with resource locks, state machine, cancel, verifying | two plans on the same lock run serially; cancel → `Cancelling` → reconcile → `Unconfirmed`/`Succeeded` |
| 13 | `SUDO_ASKPASS` spike script + result doc | `scripts/banager-askpass.sh`; `docs/spikes/2026-09-askpass.md` records whether `sudo -A` works without a TTY |
| 14 | `examples/brew_smoke.rs` + `docs/what-we-run.md` | read-only end-to-end run prints instance, counts, outdated list; doc lists every brew command |
| 15 | CI live Homebrew smoke test | `tests/brew_live.rs` (ignored + `BANAGER_LIVE=1`) installs, inventories and removes `hello`; runs on every CI push |

---

### Task 1: Scaffold Tauri app + Cargo workspace + Tailwind

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/banager-core/Cargo.toml`
- Create: `crates/banager-core/src/lib.rs`
- Create: `package.json`, `index.html`, `tsconfig.json`, `tsconfig.node.json`, `vite.config.ts`
- Create: `src/main.tsx`, `src/App.tsx`, `src/App.css`, `src/index.css`, `src/vite-env.d.ts`, `src/assets/react.svg`
- Create: `public/tauri.svg`, `public/vite.svg`
- Create: `.vscode/extensions.json`
- Create: `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`, `src-tauri/icons/*`, `src-tauri/.gitignore`
- Create: `pnpm-lock.yaml` (generated by `pnpm install` in Step 8)
- Modify: `.gitignore`

**Interfaces:**
- Consumes: nothing (first task).
- Produces: a Cargo workspace with members `["src-tauri", "crates/banager-core"]`; an empty `banager-core` library crate that later tasks fill in; a `banager` Tauri binary crate that depends on `banager-core` (path dependency) and `fix-path-env` (git dependency); a working `pnpm build` / `pnpm tauri build` pipeline that later CI and release tasks reuse verbatim.

This task is environment setup and scaffolding, not TDD — there is no behavior to test-drive yet. Its "test" is the build itself succeeding at the end.

- [ ] **Step 1: Install the Rust toolchain and macOS targets**

This machine has Node 22 (with corepack), Homebrew and Xcode Command Line Tools, but **neither Rust nor pnpm** yet (verified 2026-09-17: `cargo` and `pnpm` are absent; `corepack` 0.34 is present). Install Rust with both macOS targets needed for the universal build, and activate pnpm through corepack:

Run:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustup target add aarch64-apple-darwin x86_64-apple-darwin
corepack enable
corepack prepare pnpm@latest --activate
```

Run: `rustc --version && cargo --version && pnpm --version`
Expected: three version banners (any current stable Rust works; edition 2021 needs ≥ 1.56; pnpm 10.x). Add `source "$HOME/.cargo/env"` to your shell profile (`~/.zshrc`) so future shells pick up `cargo`/`rustc` without re-sourcing. If `corepack enable` fails with a permissions error on `/opt/homebrew/bin`, run `npm install -g pnpm` instead.

Portability note for the whole plan: `crates/banager-core/src/runner/real.rs` uses `process_group(0)` and `libc::killpg`, which are Unix-only. That is acceptable for this macOS-only plan; the Windows Job Object variant and the weekly `canary.yml` cross-platform compile check from spec §10 are deferred to the Windows plan, and `real.rs` must carry a `#![cfg(unix)]`-style note in its header comment so nobody expects it to build on Windows.

- [ ] **Step 2: Scaffold a fresh Tauri + React + TypeScript app in a scratch directory**

The repo root (`/Users/brulek/dev/Banager`) already has `.git`, `README.md`, `.gitignore` and `docs/` committed, so `create-tauri-app` cannot target it directly (it refuses non-empty directories, and `--force` silently overwrites `README.md`). Scaffold into `/tmp` instead, using project name `banager` so the generated `package.json` name, `Cargo.toml` package name, and lib name (`banager_lib`) all come out clean:

Run:
```bash
cd /tmp && rm -rf banager-scaffold && npx --yes create-tauri-app@latest banager-scaffold -m pnpm -t react-ts --identifier com.brulek.banager -y
```

Expected: prints "Template created!" and a tree containing `package.json`, `index.html`, `src/`, `src-tauri/`, `public/`, `.vscode/`, `.gitignore`, `README.md`. Verify with:

Run: `grep -E '"name"|"productName"' /tmp/banager-scaffold/package.json /tmp/banager-scaffold/src-tauri/tauri.conf.json`
Expected: `"name": "banager"` and `"productName": "banager"`.

- [ ] **Step 3: Copy the generated files into the repo, keeping the existing README.md**

Run:
```bash
cd /Users/brulek/dev/Banager
cp -R /tmp/banager-scaffold/.vscode .
cp /tmp/banager-scaffold/index.html .
cp /tmp/banager-scaffold/package.json .
cp -R /tmp/banager-scaffold/public .
cp -R /tmp/banager-scaffold/src .
cp -R /tmp/banager-scaffold/src-tauri .
cp /tmp/banager-scaffold/tsconfig.json .
cp /tmp/banager-scaffold/tsconfig.node.json .
cp /tmp/banager-scaffold/vite.config.ts .
```

Do **not** copy `/tmp/banager-scaffold/README.md` or `/tmp/banager-scaffold/.gitignore` — the repo's own versions are kept (the `.gitignore` is merged by hand in the next step).

Run: `find . -maxdepth 1 -not -path './.git' -not -path '.' | sort`
Expected: `.gitignore`, `.vscode`, `README.md`, `docs`, `index.html`, `package.json`, `public`, `src`, `src-tauri`, `tsconfig.json`, `tsconfig.node.json`, `vite.config.ts`.

- [ ] **Step 4: Merge `.gitignore`**

The existing root `.gitignore` already covers `node_modules/`, `target/`, `dist/`, `.DS_Store`, `*.log`; add the two entries the scaffold's own `.gitignore` has that ours doesn't (`dist-ssr` for stale Vite SSR builds, `*.local` for Vite's local env override files):

```diff
 node_modules/
 target/
 dist/
+dist-ssr/
 .DS_Store
 *.log
+*.local
```

`src-tauri/.gitignore` (copied in Step 3) already ignores `/target/` and `/gen/schemas` for the Rust crate, so it needs no edit.

- [ ] **Step 5: Set up the Cargo workspace and the `banager-core` stub crate**

Create `Cargo.toml` at the repo root:

```toml
[workspace]
resolver = "2"
members = ["src-tauri", "crates/banager-core"]
```

Create `crates/banager-core/Cargo.toml`:

```toml
[package]
name = "banager-core"
version = "0.1.0"
edition = "2021"
description = "Pure-Rust core library for Banager: model, adapters, runner, operation engine. Must never depend on tauri."

[dependencies]

[dev-dependencies]
```

Create `crates/banager-core/src/lib.rs`:

```rust
//! banager-core: pure Rust library with the Homebrew adapter and operation
//! engine. This crate must never depend on `tauri` — see
//! `docs/superpowers/specs/2026-09-17-banager-design.md` section 3.
```

- [ ] **Step 6: Wire `src-tauri` to depend on `banager-core` and `fix-path-env`, and pin the Tauri version floor**

Edit `src-tauri/Cargo.toml` (as generated by Step 2/3, then edited):

```toml
[package]
name = "banager"
version = "0.1.0"
description = "A friendly manager for everything you installed from the terminal"
authors = ["Brulek"]
edition = "2021"

# See more keys and their definitions at https://doc.rust-lang.org/cargo/reference/manifest.html

[lib]
# The `_lib` suffix may seem redundant but it is necessary
# to make the lib name unique and wouldn't conflict with the bin name.
# This seems to be only an issue on Windows, see https://github.com/rust-lang/cargo/issues/8519
name = "banager_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2.11.1", features = [] }
tauri-plugin-opener = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
banager-core = { path = "../crates/banager-core" }
fix-path-env = { git = "https://github.com/tauri-apps/fix-path-env-rs" }


# Read the optimization guideline for more details: https://tauri.app/concept/size/#cargo-configuration
[profile.release]
codegen-units = 1
lto = true
opt-level = 3
panic = "abort"
strip = true
```

`tauri = "2.11.1"` is a floor, not a pin: Cargo will resolve to the newest `2.x` release compatible with it (`2.11.5` was the newest published version as of 2026-09-17; verified via `curl -s https://crates.io/api/v1/crates/tauri`). This satisfies CVE-2026-42184 (fixed in 2.11.1+) per the Global Constraints. `fix-path-env` has no crates.io release (confirmed: `curl -s "https://crates.io/api/v1/crates?q=fix-path-env"` returns zero results), so it must stay a git dependency.

- [ ] **Step 7: Rewrite `tauri.conf.json`, `package.json`, and `vite.config.ts`; add Tailwind v4**

Replace `src-tauri/tauri.conf.json` in full:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Banager",
  "version": "0.1.0",
  "identifier": "com.brulek.banager",
  "build": {
    "beforeDevCommand": "pnpm dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "pnpm build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "title": "Banager",
        "width": 800,
        "height": 600
      }
    ],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": ["app", "dmg"],
    "macOS": {
      "minimumSystemVersion": "13.3"
    },
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

Replace `package.json` in full:

```json
{
  "name": "banager",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "packageManager": "pnpm@12.4.2",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "tauri": "tauri"
  },
  "dependencies": {
    "react": "^19.1.0",
    "react-dom": "^19.1.0",
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-opener": "^2"
  },
  "devDependencies": {
    "@types/react": "^19.1.8",
    "@types/react-dom": "^19.1.6",
    "@vitejs/plugin-react": "^6.0.2",
    "typescript": "~6.0.3",
    "vite": "^8.0.16",
    "@tauri-apps/cli": "^2",
    "tailwindcss": "^4.3.3",
    "@tailwindcss/vite": "^4.3.3"
  }
}
```

Replace `vite.config.ts` in full:

```ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
// @ts-expect-error type error without @types/node package
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [react(), tailwindcss()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
```

Create `src/index.css` (new file — the scaffold only has `src/App.css`):

```css
@import "tailwindcss";
```

Edit `src/main.tsx` to import it:

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

Edit `src/App.tsx`'s heading to prove Tailwind utility classes are live, and to stop calling the app "Tauri + React":

```diff
-      <h1>Welcome to Tauri + React</h1>
+      <h1 className="text-2xl font-bold">Banager</h1>
```

- [ ] **Step 8: Install JS dependencies and verify the frontend builds**

Run: `pnpm install`
Expected: exits 0, creates `pnpm-lock.yaml` and `node_modules/`.

Run: `pnpm build`
Expected: `tsc` reports no type errors, `vite build` exits 0 and writes `dist/index.html` plus a hashed CSS bundle under `dist/assets/` that contains Tailwind's reset (`*,:after,:before{box-sizing:border-box` or similar) — confirming `@tailwindcss/vite` actually ran.

- [ ] **Step 9: Verify the full unsigned Tauri build**

Run: `pnpm tauri build`
Expected: cargo compiles `banager-core` and `banager` in release mode (first run takes several minutes), then bundles; final lines mention `Banager.app` and `Banager_0.1.0_aarch64.dmg` (or `universal`, depending on your Mac's default target) under `src-tauri/target/release/bundle/macos/` and `.../dmg/`. The app is ad-hoc signed (no `APPLE_SIGNING_IDENTITY` set), which is expected and fine for local verification — Task 3 adds real signing in CI.

Run: `open src-tauri/target/release/bundle/macos/Banager.app`
Expected: a window titled "Banager" opens showing the (still-default) greet demo styled with the Tailwind heading.

- [ ] **Step 10: Commit**

```bash
git add Cargo.toml .gitignore package.json index.html tsconfig.json tsconfig.node.json vite.config.ts .vscode src public src-tauri crates/banager-core
git commit -m "$(cat <<'EOF'
feat: scaffold Tauri + React + Tailwind shell and banager-core stub crate

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

Do not commit `node_modules/`, `dist/`, `target/`, or `pnpm-lock.yaml`'s generated caches — the `.gitignore` from Step 4 already excludes `node_modules/`, `target/`, and `dist/`. `pnpm-lock.yaml` itself **should** be committed (it isn't ignored); add it explicitly:

```bash
git add pnpm-lock.yaml
git commit --amend --no-edit
```

---

### Task 2: CI workflow

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: the `pnpm build` / `cargo test --workspace` / `pnpm tauri build --target universal-apple-darwin --no-bundle` commands established in Task 1.
- Produces: a green `ci.yml` that every later task's commits run against once pushed.

This is a configuration task, not TDD; its "test" is running the same commands locally first, then confirming the Actions run is green.

- [ ] **Step 1: Write `.github/workflows/ci.yml`**

```yaml
name: CI

on:
  pull_request:
  push:
    branches: [main]

jobs:
  build-and-test:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4

      - uses: pnpm/action-setup@v4

      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: pnpm

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: aarch64-apple-darwin,x86_64-apple-darwin
          components: rustfmt, clippy

      - uses: Swatinem/rust-cache@v2

      - name: cargo fmt
        run: cargo fmt --all --check

      - name: cargo clippy
        run: cargo clippy --workspace --all-targets -- -D warnings

      - name: cargo test
        run: cargo test --workspace

      - name: pnpm install
        run: pnpm install --frozen-lockfile

      - name: pnpm build
        run: pnpm build

      - name: tauri build (unsigned, no bundle)
        run: pnpm tauri build --target universal-apple-darwin --no-bundle
```

`pnpm/action-setup@v4` needs no `version:` input because Task 1's `package.json` already sets `"packageManager": "pnpm@12.4.2"`, which the action reads automatically.

- [ ] **Step 2: Validate the YAML parses**

Run: `python3 -c "import yaml, sys; yaml.safe_load(open('.github/workflows/ci.yml')); print('ok')"`
Expected: prints `ok` with exit 0. (If `pyyaml` isn't installed, run `python3 -m pip install --user pyyaml` first.)

- [ ] **Step 3: Run every job step locally before trusting CI**

Run, from the repo root, in order:
```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm install --frozen-lockfile
pnpm build
pnpm tauri build --target universal-apple-darwin --no-bundle
```
Expected: every command exits 0. `--no-bundle` stops after producing the compiled binaries, skipping the `.app`/`.dmg` bundling step (faster, and it's what CI runs on every PR).

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "$(cat <<'EOF'
ci: add macOS GitHub Actions workflow for fmt, clippy, tests, and an unsigned build

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 5: Push and confirm the Actions run is green**

Run:
```bash
git push origin main
gh run watch
```
Expected: the `CI` workflow appears in `gh run watch` and finishes with a green checkmark. If it fails, read the failing step's log (`gh run view --log-failed`) and fix before moving to Task 3.

---

### Task 3: Release workflow with signing, notarization, updater

**Files:**
- Create: `.github/workflows/release.yml`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/capabilities/default.json`

**Interfaces:**
- Consumes: `src-tauri/tauri.conf.json`'s `bundle` block from Task 1; `tauri::Builder` chain from Task 1's `src-tauri/src/lib.rs`.
- Produces: `tauri_plugin_updater::Builder` registered in the app, and a `plugins.updater` config block that a future UI phase reads to check for updates. Nothing later in *this* plan consumes the release workflow directly — it's the deployment path for whatever `banager-core` accumulates.

This task is also configuration, not TDD. The very first two steps are **credentials the agent must never handle** — they are the user's own one-time setup, done in their own terminal/GUI, not something to run on their behalf.

- [ ] **Step 1 (User action — credentials, the agent must not do this): create signing material and secrets**

Tell the user to run these themselves, in order, and to never paste the actual secret values into chat or into a file the agent reads:

1. Xcode → Settings → Accounts → Manage Certificates → "+" → "Developer ID Application". This creates a code-signing certificate in the login keychain.
2. Open Keychain Access, find the new "Developer ID Application: ..." certificate, right-click → Export, save as `cert.p12` with a password you'll remember.
3. `base64 -i cert.p12 | pbcopy` — this is the value for the `APPLE_CERTIFICATE` secret (base64-encoded .p12).
4. At https://appleid.apple.com → Sign-In and Security → App-Specific Passwords, generate one — this is `APPLE_PASSWORD`.
5. At https://developer.apple.com/account → Membership, copy the Team ID — this is `APPLE_TEAM_ID`.
6. `pnpm tauri signer generate -w ~/.tauri/banager.key` — prints a public key; copy it into `plugins.updater.pubkey` in Step 3 below. The private key file `~/.tauri/banager.key` and the passphrase it asks for are `TAURI_SIGNING_PRIVATE_KEY` (contents of the file) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
7. For each secret, run (one at a time, entering the value at the interactive prompt — never `gh secret set NAME --body "value"` inline):
   ```
   gh secret set APPLE_CERTIFICATE
   gh secret set APPLE_CERTIFICATE_PASSWORD
   gh secret set APPLE_SIGNING_IDENTITY
   gh secret set APPLE_ID
   gh secret set APPLE_PASSWORD
   gh secret set APPLE_TEAM_ID
   gh secret set TAURI_SIGNING_PRIVATE_KEY
   gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD
   ```
   `APPLE_SIGNING_IDENTITY` is the certificate's common name, e.g. `Developer ID Application: Your Name (TEAMID)` (find it with `security find-identity -v -p codesigning`). `APPLE_ID` is the Apple ID email used for notarization.

The agent's job resumes at Step 2 below, once these secrets exist.

- [ ] **Step 2: Add `tauri-plugin-updater` and register it**

Edit `src-tauri/Cargo.toml`, adding one line under `[dependencies]`:

```diff
 tauri = { version = "2.11.1", features = [] }
 tauri-plugin-opener = "2"
+tauri-plugin-updater = "2.11.0"
 serde = { version = "1", features = ["derive"] }
 serde_json = "1"
 banager-core = { path = "../crates/banager-core" }
 fix-path-env = { git = "https://github.com/tauri-apps/fix-path-env-rs" }
```

Edit `src-tauri/src/lib.rs`'s `run()` (as written by Task 6, which adds the `fix_path_env::fix()` call and `HostEnv` logging) to also register the updater plugin:

```diff
     tauri::Builder::default()
         .plugin(tauri_plugin_opener::init())
+        .plugin(tauri_plugin_updater::Builder::new().build())
         .invoke_handler(tauri::generate_handler![greet])
         .run(tauri::generate_context!())
         .expect("error while running tauri application");
```

Edit `src-tauri/capabilities/default.json` to allow the updater's commands:

```diff
   "permissions": [
     "core:default",
-    "opener:default"
+    "opener:default",
+    "updater:default"
   ]
```

- [ ] **Step 3: Add the updater config block to `tauri.conf.json`**

```diff
   "bundle": {
     "active": true,
     "targets": ["app", "dmg"],
     "macOS": {
       "minimumSystemVersion": "13.3"
     },
+    "createUpdaterArtifacts": true,
     "icon": [
       "icons/32x32.png",
       "icons/128x128.png",
       "icons/128x128@2x.png",
       "icons/icon.icns",
       "icons/icon.ico"
     ]
-  }
+  },
+  "plugins": {
+    "updater": {
+      "pubkey": "REPLACE_WITH_PUBLIC_KEY_FROM_pnpm_tauri_signer_generate",
+      "endpoints": [
+        "https://github.com/Brulek/Banager/releases/latest/download/latest.json"
+      ]
+    }
+  }
 }
```

Replace `REPLACE_WITH_PUBLIC_KEY_FROM_pnpm_tauri_signer_generate` with the actual public key the user's Step 1.6 printed.

- [ ] **Step 4: Write `.github/workflows/release.yml`**

```yaml
name: Release

on:
  push:
    tags:
      - "v*"

jobs:
  release:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4

      - uses: pnpm/action-setup@v4

      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: pnpm

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: aarch64-apple-darwin,x86_64-apple-darwin

      - uses: Swatinem/rust-cache@v2

      - name: pnpm install
        run: pnpm install --frozen-lockfile

      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
          APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
          APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
          APPLE_ID: ${{ secrets.APPLE_ID }}
          APPLE_PASSWORD: ${{ secrets.APPLE_PASSWORD }}
          APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
        with:
          tagName: ${{ github.ref_name }}
          releaseName: "Banager ${{ github.ref_name }}"
          releaseDraft: true
          includeUpdaterJson: true
          args: --target universal-apple-darwin
```

- [ ] **Step 5: Validate YAML and build locally**

Run: `python3 -c "import yaml, sys; yaml.safe_load(open('.github/workflows/release.yml')); print('ok')"`
Expected: prints `ok`.

Run: `cargo build -p banager --release` (from repo root; confirms `tauri-plugin-updater` compiles and the plugin registration in `lib.rs` type-checks)
Expected: exits 0.

- [ ] **Step 6: Commit**

```bash
git add .github/workflows/release.yml src-tauri/Cargo.toml src-tauri/src/lib.rs src-tauri/capabilities/default.json src-tauri/tauri.conf.json
git commit -m "$(cat <<'EOF'
feat: add signed, notarized release workflow with Tauri updater

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 7: Verify end to end**

Run:
```bash
git tag v0.0.1
git push origin v0.0.1
gh run watch
```
Expected: the `Release` workflow runs, produces a draft GitHub Release with a universal `.dmg`, a `.app.tar.gz`, and `latest.json`.

Download the `.dmg` from the release, mount it, then run:
```bash
spctl -a -vv -t install /Volumes/Banager/Banager.app
```
Expected: `accepted` and `source=Notarized Developer ID`.

Run: `xcrun stapler validate /Volumes/Banager/Banager.app`
Expected: "The validate action worked!" (the notarization ticket is stapled, so the app opens offline without a Gatekeeper network check).
### Task 4: `banager-core` crate: model + events

**Files:**
- Modify: `crates/banager-core/src/lib.rs`
- Create: `crates/banager-core/src/model.rs`
- Create: `crates/banager-core/src/events.rs`
- Modify: `crates/banager-core/Cargo.toml`

**Interfaces:**
- Consumes: nothing beyond the empty crate from Task 1.
- Produces: every type in the skeleton's "Core Interfaces" `model.rs` and `events.rs` blocks, exactly as named there — `InstanceId`, `AdapterId`, `Scope`, `ManagerInstance`, `ArtifactKind`, `InstallReason`, `ArtifactKey`, `InstalledArtifact`, `UpdateChannel`, `UpdateCandidate`, `SearchHit`, `OpKind`, `OpRequest`, `CancelPolicy`, `ResourceLock`, `Plan`, `Outcome`, `OpStatus`, `Reconciled`, `OpId`, `Stream`, `OperationEvent`, `EventSink`, `VecSink`. Every later task imports these from `banager_core::model` / `banager_core::events` (re-exported at the crate root too).

- [ ] **Step 1: Declare the new modules (red)**

Edit `crates/banager-core/src/lib.rs`:

```rust
//! banager-core: pure Rust library with the Homebrew adapter and operation
//! engine. This crate must never depend on `tauri` — see
//! `docs/superpowers/specs/2026-09-17-banager-design.md` section 3.

pub mod model;
pub mod events;

pub use model::*;
pub use events::*;
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo build -p banager-core`
Expected: FAIL — `error[E0583]: file not found for module \`model\`` (and the same for `events`).

- [ ] **Step 3: Write `model.rs`**

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub type InstanceId = String; // "brew:/opt/homebrew"
pub type AdapterId = String; // "brew"

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    User,
    System,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagerInstance {
    pub id: InstanceId,
    pub adapter_id: AdapterId,
    pub exe_path: PathBuf,
    pub prefix: PathBuf,
    pub scope: Scope,
    pub version: Option<String>,
    pub healthy: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArtifactKind {
    Formula,
    Cask,
    Package,
    Tool,
    Model,
    Binary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallReason {
    Requested,
    Dependency,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArtifactKey {
    pub instance_id: InstanceId,
    pub kind: ArtifactKind,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledArtifact {
    pub key: ArtifactKey,
    pub display_name: String,
    pub version: String,
    pub reason: InstallReason,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub size_bytes: Option<u64>,
    pub installed_at: Option<i64>, // unix seconds
    pub path: Option<PathBuf>,
    pub auto_updates: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateChannel {
    Native,
    Registry,
    Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateCandidate {
    pub key: ArtifactKey,
    pub current: String,
    pub target: String,
    pub channel: UpdateChannel,
    pub checkable: bool,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    pub adapter_id: AdapterId,
    pub kind: ArtifactKind,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpKind {
    Install,
    Uninstall,
    Upgrade,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpRequest {
    pub kind: OpKind,
    pub instance_id: InstanceId,
    pub artifact_kind: ArtifactKind,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CancelPolicy {
    SafeKill,
    KillThenReconcile,
    NoCancel,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceLock(pub String); // "brew:/opt/homebrew"

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub request: OpRequest,
    pub program: PathBuf,
    pub args: Vec<String>, // argv without program; preview = program + args
    pub env: Vec<(String, String)>,
    pub needs_password: bool,
    pub locks: Vec<ResourceLock>,
    pub cancel_policy: CancelPolicy,
    pub warnings: Vec<String>,
    pub affected: Vec<String>, // dependents that would break on uninstall
    pub timeout_secs: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Succeeded,
    NoChange,
    PartialSuccess,
    NeedsAttention(String),
    Failed { exit_code: Option<i32>, summary: String },
    Unconfirmed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpStatus {
    Queued,
    Running,
    CancelRequested,
    Cancelling,
    Verifying,
    Done,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reconciled {
    pub present: bool,
    pub version: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

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
        };
        let json = serde_json::to_string(&instance).expect("serialize");
        let back: ManagerInstance = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(instance, back);
    }

    #[test]
    fn test_outcome_failed_round_trips_through_json() {
        let outcome = Outcome::Failed {
            exit_code: Some(1),
            summary: "boom".to_string(),
        };
        let json = serde_json::to_string(&outcome).expect("serialize");
        let back: Outcome = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(outcome, back);
    }

    #[test]
    fn test_plan_round_trips_through_json() {
        let plan = Plan {
            request: OpRequest {
                kind: OpKind::Install,
                instance_id: "brew:/opt/homebrew".to_string(),
                artifact_kind: ArtifactKind::Formula,
                name: "jq".to_string(),
            },
            program: PathBuf::from("/opt/homebrew/bin/brew"),
            args: vec![
                "install".to_string(),
                "--formula".to_string(),
                "jq".to_string(),
            ],
            env: vec![("HOMEBREW_NO_AUTO_UPDATE".to_string(), "1".to_string())],
            needs_password: false,
            locks: vec![ResourceLock("brew:/opt/homebrew".to_string())],
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings: vec![],
            affected: vec![],
            timeout_secs: 1800,
        };
        let json = serde_json::to_string(&plan).expect("serialize");
        let back: Plan = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(plan, back);
    }
}
```

- [ ] **Step 4: Write `events.rs`**

```rust
use crate::model::{OpStatus, Outcome};
use serde::{Deserialize, Serialize};

pub type OpId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationEvent {
    Status { op_id: OpId, status: OpStatus },
    Log { op_id: OpId, stream: Stream, line: String },
    Finished { op_id: OpId, outcome: Outcome },
}

pub trait EventSink: Send + Sync {
    fn emit(&self, event: OperationEvent);
}

pub struct VecSink {
    pub events: std::sync::Mutex<Vec<OperationEvent>>,
}

impl VecSink {
    pub fn new() -> VecSink {
        VecSink {
            events: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn snapshot(&self) -> Vec<OperationEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl Default for VecSink {
    fn default() -> Self {
        VecSink::new()
    }
}

impl EventSink for VecSink {
    fn emit(&self, event: OperationEvent) {
        self.events.lock().unwrap().push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_event_round_trips_through_json() {
        let event = OperationEvent::Log {
            op_id: 42,
            stream: Stream::Stdout,
            line: "hello".to_string(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        let back: OperationEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event, back);
    }

    #[test]
    fn test_vec_sink_records_events_in_order() {
        let sink = VecSink::new();
        sink.emit(OperationEvent::Status {
            op_id: 1,
            status: OpStatus::Queued,
        });
        sink.emit(OperationEvent::Finished {
            op_id: 1,
            outcome: Outcome::Succeeded,
        });
        let events = sink.snapshot();
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0],
            OperationEvent::Status {
                op_id: 1,
                status: OpStatus::Queued
            }
        );
        assert_eq!(
            events[1],
            OperationEvent::Finished {
                op_id: 1,
                outcome: Outcome::Succeeded
            }
        );
    }
}
```

- [ ] **Step 5: Add `serde` and `serde_json` to `Cargo.toml`**

```diff
 [dependencies]
+serde = { version = "1", features = ["derive"] }
+serde_json = "1"

 [dev-dependencies]
```

- [ ] **Step 6: Run to see it pass**

Run: `cargo test -p banager-core`
Expected: PASS — 5 tests (`model::tests::test_manager_instance_round_trips_through_json`, `model::tests::test_outcome_failed_round_trips_through_json`, `model::tests::test_plan_round_trips_through_json`, `events::tests::test_operation_event_round_trips_through_json`, `events::tests::test_vec_sink_records_events_in_order`).

- [ ] **Step 7: Commit**

```bash
git add crates/banager-core/Cargo.toml crates/banager-core/src/lib.rs crates/banager-core/src/model.rs crates/banager-core/src/events.rs
git commit -m "$(cat <<'EOF'
feat(core): add model and events modules with serde round-trip tests

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: Runner: trait, `MockRunner`, `RealRunner`

**Files:**
- Modify: `crates/banager-core/src/lib.rs`
- Create: `crates/banager-core/src/runner/mod.rs`
- Create: `crates/banager-core/src/runner/real.rs`
- Create: `crates/banager-core/src/runner/mock.rs`
- Modify: `crates/banager-core/Cargo.toml`

**Interfaces:**
- Consumes: `crate::events::Stream` (Task 4).
- Produces: `CommandSpec`, `CommandOutput`, `LineCallback`, `RunnerError`, the `CommandRunner` trait, `RealRunner`, `MockRunner` — all used by every adapter task from Task 7 onward.

- [ ] **Step 1: Declare the module (red)**

Edit `crates/banager-core/src/lib.rs`:

```diff
 pub mod model;
 pub mod events;
+pub mod runner;

 pub use model::*;
 pub use events::*;
```

Run: `cargo build -p banager-core`
Expected: FAIL — `error[E0583]: file not found for module \`runner\``.

- [ ] **Step 2: Write `runner/mod.rs`**

```rust
use crate::events::Stream;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub mod mock;
pub mod real;

pub use mock::MockRunner;
pub use real::RealRunner;

#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: Option<PathBuf>,
    pub timeout: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub cancelled: bool,
}

pub type LineCallback = Arc<dyn Fn(Stream, String) + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("program not found: {0}")]
    NotFound(PathBuf),
    #[error("spawn failed: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("no canned response for {0:?}")]
    NoMock(Vec<String>),
}

#[async_trait::async_trait]
pub trait CommandRunner: Send + Sync {
    async fn run(
        &self,
        spec: CommandSpec,
        on_line: Option<LineCallback>,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<CommandOutput, RunnerError>;
}
```

- [ ] **Step 3: Write `runner/mock.rs`**

```rust
use super::{CommandOutput, CommandRunner, CommandSpec, LineCallback, RunnerError};
use crate::events::Stream;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub struct MockRunner {
    responses: Mutex<HashMap<Vec<String>, CommandOutput>>,
    calls: Mutex<Vec<Vec<String>>>,
}

impl MockRunner {
    pub fn new() -> MockRunner {
        MockRunner {
            responses: Mutex::new(HashMap::new()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_runner_returns_canned_output_and_records_argv() {
        let runner = MockRunner::new();
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

        let spec = CommandSpec {
            program: std::path::PathBuf::from("/opt/homebrew/bin/brew"),
            args: vec!["--version".to_string()],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(5),
        };
        let output = runner
            .run(spec, None, CancellationToken::new())
            .await
            .expect("mocked call");

        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout, "Homebrew 7.0.3\n");
        assert_eq!(
            runner.calls(),
            vec![vec![
                "/opt/homebrew/bin/brew".to_string(),
                "--version".to_string()
            ]]
        );
    }

    #[tokio::test]
    async fn test_mock_runner_errors_on_unconfigured_argv() {
        let runner = MockRunner::new();
        let spec = CommandSpec {
            program: std::path::PathBuf::from("/opt/homebrew/bin/brew"),
            args: vec!["update".to_string()],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(5),
        };
        let result = runner.run(spec, None, CancellationToken::new()).await;
        assert!(matches!(result, Err(RunnerError::NoMock(_))));
    }
}
```

- [ ] **Step 4: Write `runner/real.rs`**

```rust
use super::{CommandOutput, CommandRunner, CommandSpec, LineCallback, RunnerError};
use crate::events::Stream;
use async_trait::async_trait;
use std::os::unix::process::CommandExt;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

pub struct RealRunner;

impl RealRunner {
    pub fn new() -> RealRunner {
        RealRunner
    }
}

impl Default for RealRunner {
    fn default() -> Self {
        RealRunner::new()
    }
}

/// Drains complete lines (split on both `\n` and `\r`, so `brew`'s
/// carriage-return progress updates are treated as line boundaries too) from
/// the front of `buf`, leaving any trailing partial line buffered.
fn drain_lines(buf: &mut Vec<u8>) -> Vec<String> {
    let mut lines = Vec::new();
    let mut start = 0;
    for i in 0..buf.len() {
        if buf[i] == b'\n' || buf[i] == b'\r' {
            if i > start {
                lines.push(String::from_utf8_lossy(&buf[start..i]).to_string());
            }
            start = i + 1;
        }
    }
    buf.drain(0..start);
    lines
}

#[async_trait]
impl CommandRunner for RealRunner {
    async fn run(
        &self,
        spec: CommandSpec,
        on_line: Option<LineCallback>,
        cancel: CancellationToken,
    ) -> Result<CommandOutput, RunnerError> {
        if !spec.program.exists() {
            return Err(RunnerError::NotFound(spec.program.clone()));
        }

        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args);
        cmd.envs(spec.env.iter().cloned());
        if let Some(cwd) = &spec.cwd {
            cmd.current_dir(cwd);
        }
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.process_group(0);

        let mut child = cmd.spawn()?;
        let pid = child.id().map(|p| p as libc::pid_t);
        let mut stdout = child.stdout.take().expect("stdout was piped");
        let mut stderr = child.stderr.take().expect("stderr was piped");

        let mut stdout_buf: Vec<u8> = Vec::new();
        let mut stderr_buf: Vec<u8> = Vec::new();
        let mut stdout_all = String::new();
        let mut stderr_all = String::new();
        let mut read_buf = [0u8; 4096];

        let mut stdout_done = false;
        let mut stderr_done = false;
        let mut timed_out = false;
        let mut cancelled = false;

        let sleep = tokio::time::sleep(spec.timeout);
        tokio::pin!(sleep);

        while !(stdout_done && stderr_done) && !timed_out && !cancelled {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    cancelled = true;
                    if let Some(pid) = pid {
                        unsafe { libc::killpg(pid, libc::SIGKILL); }
                    }
                }
                _ = &mut sleep => {
                    timed_out = true;
                    if let Some(pid) = pid {
                        unsafe { libc::killpg(pid, libc::SIGKILL); }
                    }
                }
                res = stdout.read(&mut read_buf), if !stdout_done => {
                    match res {
                        Ok(0) => stdout_done = true,
                        Ok(n) => {
                            stdout_buf.extend_from_slice(&read_buf[..n]);
                            stdout_all.push_str(&String::from_utf8_lossy(&read_buf[..n]));
                            for line in drain_lines(&mut stdout_buf) {
                                if let Some(cb) = &on_line {
                                    cb(Stream::Stdout, line);
                                }
                            }
                        }
                        Err(_) => stdout_done = true,
                    }
                }
                res = stderr.read(&mut read_buf), if !stderr_done => {
                    match res {
                        Ok(0) => stderr_done = true,
                        Ok(n) => {
                            stderr_buf.extend_from_slice(&read_buf[..n]);
                            stderr_all.push_str(&String::from_utf8_lossy(&read_buf[..n]));
                            for line in drain_lines(&mut stderr_buf) {
                                if let Some(cb) = &on_line {
                                    cb(Stream::Stderr, line);
                                }
                            }
                        }
                        Err(_) => stderr_done = true,
                    }
                }
            }
        }

        let exit_code = if timed_out || cancelled {
            let _ = child.wait().await;
            None
        } else {
            child.wait().await.ok().and_then(|status| status.code())
        };

        Ok(CommandOutput {
            exit_code,
            stdout: stdout_all,
            stderr: stderr_all,
            timed_out,
            cancelled,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn sh() -> std::path::PathBuf {
        std::path::PathBuf::from("/bin/sh")
    }

    #[tokio::test]
    async fn test_streams_stdout_lines_and_reports_exit_code() {
        let runner = RealRunner::new();
        let lines: Arc<Mutex<Vec<(Stream, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let lines_cb = lines.clone();
        let on_line: LineCallback = Arc::new(move |stream, line| {
            lines_cb.lock().unwrap().push((stream, line));
        });

        let spec = CommandSpec {
            program: sh(),
            args: vec!["-c".to_string(), "printf 'a\\nb\\n'".to_string()],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(5),
        };
        let output = runner
            .run(spec, Some(on_line), CancellationToken::new())
            .await
            .expect("spawn /bin/sh");

        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout, "a\nb\n");
        assert!(!output.timed_out);
        assert!(!output.cancelled);
        assert_eq!(
            *lines.lock().unwrap(),
            vec![
                (Stream::Stdout, "a".to_string()),
                (Stream::Stdout, "b".to_string())
            ]
        );
    }

    #[tokio::test]
    async fn test_reports_nonzero_exit_code() {
        let runner = RealRunner::new();
        let spec = CommandSpec {
            program: sh(),
            args: vec!["-c".to_string(), "exit 3".to_string()],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(5),
        };
        let output = runner
            .run(spec, None, CancellationToken::new())
            .await
            .expect("spawn /bin/sh");
        assert_eq!(output.exit_code, Some(3));
    }

    #[tokio::test]
    async fn test_timeout_kills_process_group() {
        let runner = RealRunner::new();
        let spec = CommandSpec {
            program: sh(),
            args: vec!["-c".to_string(), "sleep 5".to_string()],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_millis(200),
        };
        let output = runner
            .run(spec, None, CancellationToken::new())
            .await
            .expect("spawn /bin/sh");
        assert!(output.timed_out);
        assert_eq!(output.exit_code, None);
    }

    #[tokio::test]
    async fn test_cancel_kills_process_group() {
        let runner = RealRunner::new();
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            cancel_clone.cancel();
        });

        let spec = CommandSpec {
            program: sh(),
            args: vec!["-c".to_string(), "sleep 5".to_string()],
            env: vec![],
            cwd: None,
            timeout: std::time::Duration::from_secs(5),
        };
        let output = runner
            .run(spec, None, cancel)
            .await
            .expect("spawn /bin/sh");
        assert!(output.cancelled);
        assert_eq!(output.exit_code, None);
    }
}
```

- [ ] **Step 5: Add runner dependencies to `Cargo.toml`**

```diff
 [dependencies]
 serde = { version = "1", features = ["derive"] }
 serde_json = "1"
+async-trait = "0.1"
+thiserror = "2"
+tokio = { version = "1", features = ["rt-multi-thread", "macros", "process", "io-util", "time", "sync"] }
+tokio-util = { version = "0.7", features = ["rt"] }
+libc = "0.2"

 [dev-dependencies]
```

- [ ] **Step 6: Run to see it pass**

Run: `cargo test -p banager-core`
Expected: PASS — the 5 tests from Task 4 plus 2 `MockRunner` tests and 4 `RealRunner` tests (11 total). The timeout and cancel tests each take a little over 100–200 ms, not the full 5 s `sleep`, since the process is killed early.

- [ ] **Step 7: Commit**

```bash
git add crates/banager-core/Cargo.toml crates/banager-core/src/lib.rs crates/banager-core/src/runner
git commit -m "$(cat <<'EOF'
feat(core): add CommandRunner trait with RealRunner and MockRunner

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: PATH hydration + `HostEnv` + `resolve_exe`; call `fix_path_env::fix()` in Tauri startup

**Files:**
- Modify: `crates/banager-core/src/runner/mod.rs`
- Create: `crates/banager-core/src/runner/path_env.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: nothing new from earlier tasks (uses `libc`, already a dependency from Task 5).
- Produces: `HostEnv { path_dirs, home, euid }` and `HostEnv::discover()` — consumed by the `Adapter` trait (Task 7) and `BrewAdapter::detect` (Task 10), which checks `BrewAdapter::CANDIDATE_PATHS` directly rather than doing a generic `PATH` search. `resolve_exe(name, env)` is part of this crate's public surface for future adapters that *do* need a generic `PATH` search (e.g. finding `npm`), and is exercised by this task's own tests, but nothing later in this plan calls it.

- [ ] **Step 1: Declare the module (red)**

Edit `crates/banager-core/src/runner/mod.rs`:

```diff
 pub mod mock;
+pub mod path_env;
 pub mod real;

 pub use mock::MockRunner;
+pub use path_env::{resolve_exe, HostEnv};
 pub use real::RealRunner;
```

Run: `cargo build -p banager-core`
Expected: FAIL — `error[E0583]: file not found for module \`path_env\``.

- [ ] **Step 2: Write `runner/path_env.rs`**

```rust
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostEnv {
    pub path_dirs: Vec<PathBuf>,
    pub home: PathBuf,
    pub euid: u32,
}

impl HostEnv {
    /// Reads `PATH`/`HOME` from the process environment. Call this only
    /// after `fix_path_env::fix()` has already run (in the Tauri shell's
    /// `run()`), since apps launched from Finder start with a minimal
    /// default `PATH` that doesn't include Homebrew's `bin` directories.
    pub fn discover() -> HostEnv {
        let path_dirs = std::env::var_os("PATH")
            .map(|v| std::env::split_paths(&v).collect())
            .unwrap_or_default();
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let euid = unsafe { libc::geteuid() };
        HostEnv {
            path_dirs,
            home,
            euid,
        }
    }
}

pub fn resolve_exe(name: &str, env: &HostEnv) -> Option<PathBuf> {
    for dir in &env.path_dirs {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_reads_a_nonempty_path() {
        let env = HostEnv::discover();
        assert!(!env.path_dirs.is_empty());
    }

    #[test]
    fn test_resolve_exe_finds_sh_on_a_real_mac() {
        let env = HostEnv {
            path_dirs: vec![PathBuf::from("/bin"), PathBuf::from("/usr/bin")],
            home: PathBuf::from("/tmp"),
            euid: 501,
        };
        assert_eq!(resolve_exe("sh", &env), Some(PathBuf::from("/bin/sh")));
    }

    #[test]
    fn test_resolve_exe_returns_none_for_a_missing_binary() {
        let env = HostEnv {
            path_dirs: vec![PathBuf::from("/bin")],
            home: PathBuf::from("/tmp"),
            euid: 501,
        };
        assert_eq!(
            resolve_exe("definitely-not-a-real-binary-xyz", &env),
            None
        );
    }
}
```

- [ ] **Step 3: Run to see it pass**

Run: `cargo test -p banager-core`
Expected: PASS — 14 tests total (11 from Task 5 plus 3 new `path_env` tests).

- [ ] **Step 4: Wire `fix_path_env::fix()` and log the discovered PATH from the Tauri shell**

Edit `src-tauri/src/lib.rs` in full:

```rust
// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if fix_path_env::fix().is_err() {
        eprintln!("[banager] failed to fix PATH; falling back to the process's default PATH");
    }
    let host_env = banager_core::runner::HostEnv::discover();
    println!("[banager] discovered PATH dirs: {:?}", host_env.path_dirs);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 5: Verify the shell logs the discovered PATH**

Run: `cargo run -p banager --bin banager 2>&1 | head -5`
(Or, if that binary target name doesn't match your Cargo.toml, run `pnpm tauri dev` and watch the terminal it starts in.)
Expected: a line like `[banager] discovered PATH dirs: ["/opt/homebrew/bin", "/usr/local/bin", ..., "/usr/bin", "/bin", ...]` printed to stdout before the window opens. Quit the app (Cmd+Q) once you've seen the line.

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/runner/mod.rs crates/banager-core/src/runner/path_env.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(core): add HostEnv PATH discovery and wire fix-path-env into Tauri startup

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: Adapter trait, `Capabilities`, `AdapterMeta` from `adapters/meta/brew.toml`, `validate_package_name`

**Files:**
- Modify: `crates/banager-core/src/lib.rs`
- Create: `crates/banager-core/src/adapters/mod.rs`
- Create: `adapters/meta/brew.toml`
- Modify: `crates/banager-core/Cargo.toml`

**Interfaces:**
- Consumes: `crate::events::{EventSink, OpId}`, `crate::model::{ArtifactKey, InstalledArtifact, ManagerInstance, OpRequest, Outcome, Plan, Reconciled, SearchHit, UpdateCandidate}` (Task 4), `crate::runner::HostEnv` (Task 6).
- Produces: `Capabilities`, `AdapterMeta` (+ `AdapterMeta::from_toml`), `AdapterError`, `validate_package_name`, the `Adapter` trait — the contract `BrewAdapter` implements starting Task 10.

- [ ] **Step 1: Declare the module (red)**

Edit `crates/banager-core/src/lib.rs`:

```diff
 pub mod model;
 pub mod events;
 pub mod runner;
+pub mod adapters;

 pub use model::*;
 pub use events::*;
```

Run: `cargo build -p banager-core`
Expected: FAIL — `error[E0583]: file not found for module \`adapters\``.

- [ ] **Step 2: Write `adapters/mod.rs`**

```rust
use crate::events::{EventSink, OpId};
use crate::model::{
    ArtifactKey, InstalledArtifact, ManagerInstance, OpRequest, Outcome, Plan, Reconciled,
    SearchHit, UpdateCandidate,
};
use crate::runner::HostEnv;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub search: bool,
    pub per_item_upgrade: bool,
    pub upgrade_all: bool,
    pub uninstall: bool,
    pub background_check: bool,
    pub cancel_safe: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AdapterMeta {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub platforms: Vec<String>,
    pub homepage: String,
    pub schema_version: u32,
    pub verified_versions: Vec<String>,
}

impl AdapterMeta {
    pub fn from_toml(s: &str) -> Result<AdapterMeta, toml::de::Error> {
        toml::from_str(s)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("runner: {0}")]
    Runner(#[from] crate::runner::RunnerError),
    #[error("parse: {0}")]
    Parse(String),
    #[error("command failed (exit {code:?}): {stderr}")]
    CommandFailed { code: Option<i32>, stderr: String },
    #[error("refused: {0}")]
    Refused(String),
    #[error("invalid name: {0}")]
    InvalidName(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
}

/// Matches `^[A-Za-z0-9@._+/-]+$` and rejects names starting with `-`
/// (implemented by hand instead of pulling in the `regex` crate, since this
/// is the only place in the crate that needs pattern matching).
pub fn validate_package_name(name: &str) -> Result<(), AdapterError> {
    if name.is_empty() || name.starts_with('-') {
        return Err(AdapterError::InvalidName(name.to_string()));
    }
    let valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '+' | '/' | '-'));
    if !valid {
        return Err(AdapterError::InvalidName(name.to_string()));
    }
    Ok(())
}

#[async_trait]
pub trait Adapter: Send + Sync {
    fn meta(&self) -> &AdapterMeta;
    fn capabilities(&self) -> Capabilities;
    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance>;
    async fn inventory(&self, inst: &ManagerInstance) -> Result<Vec<InstalledArtifact>, AdapterError>;
    async fn check_updates(&self, inst: &ManagerInstance) -> Result<Vec<UpdateCandidate>, AdapterError>;
    async fn search(&self, inst: &ManagerInstance, query: &str) -> Result<Vec<SearchHit>, AdapterError>;
    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError>;
    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError>;
    async fn reconcile(&self, inst: &ManagerInstance, key: &ArtifactKey) -> Result<Reconciled, AdapterError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_toml_parses_the_committed_brew_meta_file() {
        // cargo runs tests with cwd = the package manifest directory
        // (crates/banager-core), so this reaches the repo-root file.
        let s = std::fs::read_to_string("../../adapters/meta/brew.toml")
            .expect("read adapters/meta/brew.toml");
        let meta = AdapterMeta::from_toml(&s).expect("parse brew.toml");
        assert_eq!(meta.id, "brew");
        assert_eq!(meta.name, "Homebrew");
        assert_eq!(meta.platforms, vec!["macos".to_string()]);
    }

    #[test]
    fn test_validate_package_name_accepts_normal_names() {
        assert!(validate_package_name("jq").is_ok());
        assert!(validate_package_name("node@20").is_ok());
        assert!(validate_package_name("some.tool_v2+beta").is_ok());
    }

    #[test]
    fn test_validate_package_name_rejects_shell_metacharacters() {
        assert!(validate_package_name("-rf").is_err());
        assert!(validate_package_name("a;b").is_err());
        assert!(validate_package_name("").is_err());
    }
}
```

- [ ] **Step 3: Write `adapters/meta/brew.toml`**

```toml
schema_version = 1
id = "brew"
name = "Homebrew"
kind = "package_manager"
platforms = ["macos"]
homepage = "https://brew.sh"
verified_versions = ["7.0.3"]
```

`7.0.3` is the version installed on the reference Mac as of 2026-09-17 (`brew --version`). Task 8 records fixtures against whatever version is actually installed when fixtures are recorded; if it differs, add that version to `verified_versions` too rather than replacing this one.

- [ ] **Step 4: Add `toml` to `Cargo.toml`**

```diff
 tokio-util = { version = "0.7", features = ["rt"] }
 libc = "0.2"
+toml = "1"
```

- [ ] **Step 5: Run to see it pass**

Run: `cargo test -p banager-core`
Expected: PASS — 17 tests total (14 from Task 6 plus 3 new `adapters::tests`).

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/Cargo.toml crates/banager-core/src/lib.rs crates/banager-core/src/adapters/mod.rs adapters/meta/brew.toml
git commit -m "$(cat <<'EOF'
feat(core): add Adapter trait, AdapterMeta TOML loading, and package-name validation

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```
### Task 8: Record Homebrew fixtures from the real Mac

**Files:**
- Create: `adapters/fixtures/brew/<version>/info-installed.json`
- Create: `adapters/fixtures/brew/<version>/outdated.json`
- Create: `adapters/fixtures/brew/<version>/search-jq.txt`
- Create: `adapters/fixtures/brew/<version>/search-desc-jq.txt`
- Create: `adapters/fixtures/brew/<version>/uses-jq.txt`
- Create: `adapters/fixtures/brew/<version>/version.txt`
- Create: `adapters/fixtures/brew/<version>/README.md`

**Interfaces:**
- Consumes: nothing (raw `brew` CLI output).
- Produces: the fixture files Task 9's snapshot tests read. `<version>` is whatever `brew --version` prints on the machine recording them — every command below is read-only.

This is a manual-command task, not TDD: there's no function to red/green here, only real command output to capture verbatim per the Global Constraint "fixtures are recorded from a real Mac only... never hand-written or AI-generated."

- [ ] **Step 1: Determine the fixture directory from your installed Homebrew version**

Run:
```bash
BREW_VERSION=$(brew --version | head -1 | awk '{print $2}')
echo "$BREW_VERSION"
mkdir -p "adapters/fixtures/brew/$BREW_VERSION"
```
Expected: prints a version like `7.0.3`. Use this exact value (call it `$BREW_VERSION` for the rest of this task) — on the reference Mac used while writing this plan it was `7.0.3`, which is also what `adapters/meta/brew.toml`'s `verified_versions` (Task 7) already lists. If yours differs, also add it to that list.

- [ ] **Step 2: Record `brew info --installed --json=v2`**

Run: `brew info --installed --json=v2 > "adapters/fixtures/brew/$BREW_VERSION/info-installed.json"`
Expected: exits 0; the file contains a JSON object with top-level `"formulae"` and `"casks"` arrays. Sanity-check: `python3 -c "import json; d=json.load(open('adapters/fixtures/brew/$BREW_VERSION/info-installed.json')); print(len(d['formulae']), len(d['casks']))"` prints two integers.

- [ ] **Step 3: Record `brew outdated --json=v2`**

Run: `brew outdated --json=v2 > "adapters/fixtures/brew/$BREW_VERSION/outdated.json"`
Expected: exits 0 (even with zero outdated packages, this prints `{"formulae":[],"casks":[]}` or similar — that's a valid fixture).

- [ ] **Step 4: Record both `brew search` variants for `jq`**

Run:
```bash
brew search jq > "adapters/fixtures/brew/$BREW_VERSION/search-jq.txt"
brew search --desc jq > "adapters/fixtures/brew/$BREW_VERSION/search-desc-jq.txt"
```
Expected: both exit 0; each file has an `==> Formulae` section (and possibly `==> Casks`) followed by matching names.

- [ ] **Step 5: Record `brew uses --installed jq` and `brew --version`**

Run:
```bash
brew uses --installed jq > "adapters/fixtures/brew/$BREW_VERSION/uses-jq.txt"
brew --version > "adapters/fixtures/brew/$BREW_VERSION/version.txt"
```
Expected: `uses-jq.txt` may be empty (no installed formula depends on `jq`) or list whitespace-separated formula names — both are valid; `version.txt` contains a line like `Homebrew 7.0.3`.

- [ ] **Step 6: Write the fixture README**

Create `adapters/fixtures/brew/$BREW_VERSION/README.md` (substitute your actual machine name, date, and `$BREW_VERSION`):

```markdown
# Homebrew 7.0.3 fixtures

Recorded on: BrulekMBA (Apple Silicon, macOS 27.0)
Date: 2026-09-17
Homebrew version: 7.0.3 (`brew --version`)

These files are unedited, real output from the commands named after each
file, captured verbatim on the machine and date above:

- `info-installed.json` — `brew info --installed --json=v2`
- `outdated.json` — `brew outdated --json=v2`
- `search-jq.txt` — `brew search jq`
- `search-desc-jq.txt` — `brew search --desc jq`
- `uses-jq.txt` — `brew uses --installed jq`
- `version.txt` — `brew --version`

`info-installed.json` and `outdated.json` reflect this machine's actual
installed package list. That is expected and acceptable — it contains no
secrets, only formula/cask names, versions, and installation metadata.
```

- [ ] **Step 7: Commit**

```bash
git add adapters/fixtures
git commit -m "$(cat <<'EOF'
test: record real Homebrew 7.0.3 fixtures for the brew adapter

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

---

### Task 9: Brew parsers with insta snapshot tests over fixtures

**Files:**
- Modify: `crates/banager-core/src/adapters/mod.rs`
- Create: `crates/banager-core/src/adapters/brew/mod.rs`
- Create: `crates/banager-core/src/adapters/brew/parse.rs`
- Create: `crates/banager-core/tests/brew_fixtures.rs`
- Modify: `crates/banager-core/Cargo.toml`

**Interfaces:**
- Consumes: `crate::model::{ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, SearchHit, UpdateCandidate, UpdateChannel}` (Task 4), `crate::adapters::AdapterError` (Task 7), the fixture files from Task 8.
- Produces: `parse_info_installed`, `parse_outdated`, `parse_search`, `parse_uses`, `parse_version` at `banager_core::adapters::brew::parse::*` — consumed by `BrewAdapter` starting Task 10.

- [ ] **Step 1: Declare the module (red)**

Edit `crates/banager-core/src/adapters/mod.rs`, adding the submodule declaration near the top (after the `use` block, before `Capabilities`):

```diff
 use tokio_util::sync::CancellationToken;

+pub mod brew;
+
 #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
 pub struct Capabilities {
```

Create `crates/banager-core/tests/brew_fixtures.rs` (the full test file, referencing functions that don't exist yet):

```rust
use banager_core::adapters::brew::parse::{
    parse_info_installed, parse_outdated, parse_search, parse_uses, parse_version,
};

// Substitute this if `brew --version` on your machine differs from the one
// recorded in Task 8.
const FIXTURE_DIR: &str = "../../adapters/fixtures/brew/7.0.3";

fn read_fixture(name: &str) -> String {
    let path = format!("{}/{}", FIXTURE_DIR, name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {}", path, e))
}

#[test]
fn test_parse_info_installed_snapshot() {
    let json = read_fixture("info-installed.json");
    let result = parse_info_installed(&json, "brew:/opt/homebrew").expect("parse");
    insta::assert_json_snapshot!(result);
}

#[test]
fn test_parse_outdated_snapshot() {
    let json = read_fixture("outdated.json");
    let result = parse_outdated(&json, "brew:/opt/homebrew").expect("parse");
    insta::assert_json_snapshot!(result);
}

#[test]
fn test_parse_search_snapshot() {
    let text = read_fixture("search-jq.txt");
    let result = parse_search(&text, "brew");
    insta::assert_json_snapshot!(result);
}

#[test]
fn test_parse_search_desc_snapshot() {
    let text = read_fixture("search-desc-jq.txt");
    let result = parse_search(&text, "brew");
    insta::assert_json_snapshot!(result);
}

#[test]
fn test_parse_uses_snapshot() {
    let text = read_fixture("uses-jq.txt");
    let result = parse_uses(&text);
    insta::assert_json_snapshot!(result);
}

#[test]
fn test_parse_version_reads_the_recorded_version() {
    let text = read_fixture("version.txt");
    assert_eq!(parse_version(&text), Some("7.0.3".to_string()));
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p banager-core --test brew_fixtures`
Expected: FAIL to compile — `error[E0433]: failed to resolve: could not find \`brew\` in \`adapters\`` (the `pub mod brew;` line was added, but `src/adapters/brew/mod.rs` doesn't exist yet).

- [ ] **Step 3: Write `adapters/brew/parse.rs`**

```rust
use crate::adapters::AdapterError;
use crate::model::{
    ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, SearchHit, UpdateCandidate,
    UpdateChannel,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct InfoInstalledRoot {
    #[serde(default)]
    formulae: Vec<FormulaInfo>,
    #[serde(default)]
    casks: Vec<CaskInfo>,
}

#[derive(Debug, Deserialize)]
struct FormulaInfo {
    name: String,
    #[serde(default)]
    desc: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    linked_keg: Option<String>,
    #[serde(default)]
    installed: Vec<FormulaInstalledEntry>,
}

#[derive(Debug, Deserialize)]
struct FormulaInstalledEntry {
    version: String,
    #[serde(default)]
    installed_on_request: bool,
    #[serde(default)]
    installed_as_dependency: bool,
    #[serde(default)]
    time: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CaskInfo {
    token: String,
    #[serde(default)]
    name: Vec<String>,
    #[serde(default)]
    desc: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    installed: Option<String>,
    #[serde(default)]
    auto_updates: Option<bool>,
}

/// Parses `brew info --installed --json=v2`. For each formula, picks the
/// `installed` entry whose `version` matches `linked_keg` (falling back to
/// the last entry in the chronological array if no match, e.g. an unlinked
/// keg-only formula); `installed_on_request` -> `Requested`,
/// `installed_as_dependency` -> `Dependency`, else `Unknown`. Casks have no
/// install-reason field in brew's JSON, so they are always `Requested`.
pub fn parse_info_installed(
    json: &str,
    instance_id: &str,
) -> Result<Vec<InstalledArtifact>, AdapterError> {
    let root: InfoInstalledRoot =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;

    let mut out = Vec::new();

    for f in root.formulae {
        let picked = f
            .installed
            .iter()
            .find(|entry| Some(&entry.version) == f.linked_keg.as_ref())
            .or_else(|| f.installed.last());

        let (version, reason, installed_at) = match picked {
            Some(entry) => {
                let reason = if entry.installed_on_request {
                    InstallReason::Requested
                } else if entry.installed_as_dependency {
                    InstallReason::Dependency
                } else {
                    InstallReason::Unknown
                };
                (entry.version.clone(), reason, entry.time)
            }
            None => (String::new(), InstallReason::Unknown, None),
        };

        out.push(InstalledArtifact {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Formula,
                name: f.name.clone(),
            },
            display_name: f.name,
            version,
            reason,
            description: f.desc,
            homepage: f.homepage,
            size_bytes: None,
            installed_at,
            path: None,
            auto_updates: false,
        });
    }

    for c in root.casks {
        let display_name = c.name.into_iter().next().unwrap_or_else(|| c.token.clone());
        out.push(InstalledArtifact {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind: ArtifactKind::Cask,
                name: c.token,
            },
            display_name,
            version: c.installed.unwrap_or_default(),
            reason: InstallReason::Requested,
            description: c.desc,
            homepage: c.homepage,
            size_bytes: None,
            installed_at: None,
            path: None,
            auto_updates: c.auto_updates.unwrap_or(false),
        });
    }

    Ok(out)
}

#[derive(Debug, Deserialize)]
struct OutdatedRoot {
    #[serde(default)]
    formulae: Vec<OutdatedItem>,
    #[serde(default)]
    casks: Vec<OutdatedItem>,
}

#[derive(Debug, Deserialize)]
struct OutdatedItem {
    name: String,
    #[serde(default)]
    installed_versions: Vec<String>,
    current_version: String,
    #[serde(default)]
    pinned: bool,
    #[serde(default)]
    #[allow(dead_code)]
    pinned_version: Option<String>,
}

/// Parses `brew outdated --json=v2`. Pinned items get a `"pinned"` warning
/// (Banager can still show them, but shouldn't silently upgrade past a pin).
pub fn parse_outdated(
    json: &str,
    instance_id: &str,
) -> Result<Vec<UpdateCandidate>, AdapterError> {
    let root: OutdatedRoot =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;

    let mut out = Vec::new();

    let items = root
        .formulae
        .into_iter()
        .map(|i| (i, ArtifactKind::Formula))
        .chain(root.casks.into_iter().map(|i| (i, ArtifactKind::Cask)));

    for (item, kind) in items {
        let current = item.installed_versions.last().cloned().unwrap_or_default();
        let mut warnings = Vec::new();
        if item.pinned {
            warnings.push("pinned".to_string());
        }
        out.push(UpdateCandidate {
            key: ArtifactKey {
                instance_id: instance_id.to_string(),
                kind,
                name: item.name,
            },
            current,
            target: item.current_version,
            channel: UpdateChannel::Native,
            checkable: true,
            warnings,
        });
    }

    Ok(out)
}

/// Parses `brew search` / `brew search --desc` output: optional
/// `==> Formulae` / `==> Casks` section headers, entries as either `name` or
/// `name: description`, blank lines, and `If you meant` / `Error:` lines are
/// all ignored outside/around the sections.
pub fn parse_search(text: &str, adapter_id: &str) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    let mut current_kind: Option<ArtifactKind> = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("If you meant") || line.starts_with("Error:") {
            continue;
        }
        if line == "==> Formulae" {
            current_kind = Some(ArtifactKind::Formula);
            continue;
        }
        if line == "==> Casks" {
            current_kind = Some(ArtifactKind::Cask);
            continue;
        }
        let Some(kind) = current_kind else {
            continue;
        };
        let (name, description) = match line.split_once(':') {
            Some((n, d)) => (n.trim().to_string(), Some(d.trim().to_string())),
            None => (line.to_string(), None),
        };
        hits.push(SearchHit {
            adapter_id: adapter_id.to_string(),
            kind,
            name,
            description,
        });
    }

    hits
}

/// Parses `brew uses --installed {name}`: one whitespace-separated formula
/// name per token (brew prints one per line, but splitting on all whitespace
/// is robust to either layout).
pub fn parse_uses(text: &str) -> Vec<String> {
    text.split_whitespace().map(|s| s.to_string()).collect()
}

/// Parses `brew --version`'s first line, e.g. "Homebrew 7.0.3", returning
/// just the version.
pub fn parse_version(text: &str) -> Option<String> {
    let first_line = text.lines().next()?;
    let mut parts = first_line.split_whitespace();
    let _label = parts.next()?; // "Homebrew"
    let version = parts.next()?;
    Some(version.to_string())
}
```

- [ ] **Step 4: Write `adapters/brew/mod.rs`**

```rust
pub mod parse;
```

- [ ] **Step 5: Add `insta` to `Cargo.toml`**

```diff
 [dev-dependencies]
+insta = { version = "1", features = ["json"] }
```

(`serde_json` is already a dependency from Task 4.)

- [ ] **Step 6: Run with `INSTA_UPDATE=always` to create the snapshots**

Run: `INSTA_UPDATE=always cargo test -p banager-core --test brew_fixtures`
Expected: PASS — all 6 tests pass, and insta writes new files under `crates/banager-core/tests/snapshots/` (one per `assert_json_snapshot!` call: `brew_fixtures__parse_info_installed_snapshot.snap`, `brew_fixtures__parse_outdated_snapshot.snap`, `brew_fixtures__parse_search_snapshot.snap`, `brew_fixtures__parse_search_desc_snapshot.snap`, `brew_fixtures__parse_uses_snapshot.snap`).

- [ ] **Step 7: Review the snapshots**

Run: `cat crates/banager-core/tests/snapshots/brew_fixtures__parse_info_installed_snapshot.snap`
Expected: a YAML-fronted snapshot file whose body is the JSON array of `InstalledArtifact` values — read through it and confirm formula entries have plausible `reason` values (`Requested`/`Dependency`) and cask entries have `"kind": "Cask"`. If anything looks wrong, fix `parse.rs` and re-run Step 6 (`INSTA_UPDATE=always`) rather than hand-editing the `.snap` file.

- [ ] **Step 8: Run again without `INSTA_UPDATE` to confirm stability**

Run: `cargo test -p banager-core --test brew_fixtures`
Expected: PASS — the freshly generated snapshots now match on a normal run.

- [ ] **Step 9: Commit**

```bash
git add crates/banager-core/Cargo.toml crates/banager-core/src/adapters/mod.rs crates/banager-core/src/adapters/brew crates/banager-core/tests/brew_fixtures.rs crates/banager-core/tests/snapshots
git commit -m "$(cat <<'EOF'
feat(core): add Homebrew JSON/text parsers with fixture-driven snapshot tests

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```
### Task 10: `BrewAdapter`: detect / inventory / check_updates / search, tested with `MockRunner`

**Files:**
- Modify: `crates/banager-core/src/adapters/brew/mod.rs`

**Interfaces:**
- Consumes: `crate::adapters::{AdapterError, AdapterMeta, validate_package_name}` (Task 7), `crate::model::{InstalledArtifact, ManagerInstance, Scope, SearchHit, UpdateCandidate}` (Task 4), `crate::runner::{CommandOutput, CommandRunner, CommandSpec, HostEnv, MockRunner}` (Tasks 5–6), `parse::{parse_info_installed, parse_outdated, parse_search, parse_version}` (Task 9).
- Produces: `BrewAdapter` with `new`, `with_update_ttl`, `ENV`, `CANDIDATE_PATHS`, and **inherent** (not yet trait) async methods `detect`, `inventory`, `check_updates`, `search`. Task 11 adds `plan`/`execute`/`reconcile` and the `Adapter` trait impl — until then, `BrewAdapter` is used and tested only as a concrete type.

- [ ] **Step 1: Write the failing test (red)**

Append to the end of `crates/banager-core/src/adapters/brew/mod.rs` (which currently contains only `pub mod parse;`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ArtifactKind;
    use crate::runner::MockRunner;
    use std::sync::Arc;

    fn test_instance() -> ManagerInstance {
        ManagerInstance {
            id: "brew:/opt/homebrew".to_string(),
            adapter_id: "brew".to_string(),
            exe_path: PathBuf::from("/opt/homebrew/bin/brew"),
            prefix: PathBuf::from("/opt/homebrew"),
            scope: Scope::User,
            version: Some("7.0.3".to_string()),
            healthy: true,
        }
    }

    #[tokio::test]
    async fn test_detect_refuses_root() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner);
        let env = HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/var/root"),
            euid: 0,
        };
        let instances = adapter.detect(&env).await;
        assert!(instances.is_empty());
    }

    #[tokio::test]
    async fn test_detect_finds_opt_homebrew_on_this_apple_silicon_mac() {
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
        // Assumes Homebrew is installed at /opt/homebrew, true for Banager's
        // target (Apple Silicon Macs, per the design spec) and for CI's
        // macos-latest runners. /usr/local/bin/brew and the Linux path do
        // not exist on this machine, so exactly one instance is found.
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].id, "brew:/opt/homebrew");
        assert_eq!(instances[0].version, Some("7.0.3".to_string()));
        assert!(instances[0].healthy);
    }

    #[tokio::test]
    async fn test_inventory_parses_formula_and_cask() {
        let runner = Arc::new(MockRunner::new());
        let json = r#"{
            "formulae": [
                {"name":"jq","desc":"JSON processor","homepage":"https://jqlang.org","linked_keg":"1.7.1","installed":[{"version":"1.7.1","installed_on_request":true,"installed_as_dependency":false,"time":1700000000}]}
            ],
            "casks": [
                {"token":"claudebar","name":["ClaudeBar"],"desc":"Menu bar app","homepage":"https://example.invalid","installed":"1.0.0","auto_updates":false}
            ]
        }"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "info", "--installed", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: json.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let artifacts = adapter.inventory(&test_instance()).await.expect("inventory");
        assert_eq!(artifacts.len(), 2);
        assert_eq!(artifacts[0].key.kind, ArtifactKind::Formula);
        assert_eq!(artifacts[1].key.kind, ArtifactKind::Cask);
    }

    #[tokio::test]
    async fn test_check_updates_respects_ttl() {
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
        let mock_ref = runner.clone();
        let adapter = BrewAdapter::new(runner).with_update_ttl(Duration::from_secs(3600));
        let inst = test_instance();

        let first = adapter.check_updates(&inst).await.expect("first check_updates");
        assert_eq!(first.len(), 1);
        let second = adapter.check_updates(&inst).await.expect("second check_updates");
        assert_eq!(second.len(), 1);

        let calls = mock_ref.calls();
        let update_calls = calls.iter().filter(|c| c.get(1).map(String::as_str) == Some("update")).count();
        let outdated_calls = calls.iter().filter(|c| c.get(1).map(String::as_str) == Some("outdated")).count();
        assert_eq!(update_calls, 1, "brew update should run once within the TTL window");
        assert_eq!(outdated_calls, 2);
    }

    #[tokio::test]
    async fn test_search_calls_both_search_variants() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "search", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "==> Formulae\njq\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "search", "--desc", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "==> Formulae\njq: Command-line JSON processor\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let mock_ref = runner.clone();
        let adapter = BrewAdapter::new(runner);
        let hits = adapter.search(&test_instance(), "jq").await.expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "jq");
        assert_eq!(hits[0].description.as_deref(), Some("Command-line JSON processor"));
        assert_eq!(
            mock_ref.calls(),
            vec![
                vec!["/opt/homebrew/bin/brew".to_string(), "search".to_string(), "jq".to_string()],
                vec![
                    "/opt/homebrew/bin/brew".to_string(),
                    "search".to_string(),
                    "--desc".to_string(),
                    "jq".to_string()
                ],
            ]
        );
    }
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p banager-core --lib adapters::brew`
Expected: FAIL to compile — `error[E0433]: failed to resolve: use of undeclared type \`BrewAdapter\`` (and similar for `HostEnv`, `CommandOutput` not yet imported at the top of the file).

- [ ] **Step 3: Write the implementation**

Insert this **above** the `#[cfg(test)]` block, so the full file becomes: `pub mod parse;`, then this, then the test module from Step 1.

```rust
pub mod parse;

use crate::adapters::{validate_package_name, AdapterError, AdapterMeta};
use crate::model::{InstalledArtifact, ManagerInstance, Scope, SearchHit, UpdateCandidate};
use crate::runner::{CommandOutput, CommandRunner, CommandSpec, HostEnv};
use parse::{parse_info_installed, parse_outdated, parse_search, parse_version};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

pub struct BrewAdapter {
    runner: Arc<dyn CommandRunner>,
    meta: AdapterMeta,
    last_update: Mutex<Option<Instant>>,
    update_ttl: Duration,
}

impl BrewAdapter {
    pub const ENV: [(&'static str, &'static str); 4] = [
        ("HOMEBREW_NO_AUTO_UPDATE", "1"),
        ("HOMEBREW_NO_ENV_HINTS", "1"),
        ("HOMEBREW_NO_INSTALL_CLEANUP", "1"),
        ("NO_COLOR", "1"),
    ];

    pub const CANDIDATE_PATHS: [&'static str; 3] = [
        "/opt/homebrew/bin/brew",
        "/usr/local/bin/brew",
        "/home/linuxbrew/.linuxbrew/bin/brew",
    ];

    pub fn new(runner: Arc<dyn CommandRunner>) -> BrewAdapter {
        let meta =
            AdapterMeta::from_toml(include_str!("../../../../../adapters/meta/brew.toml"))
                .expect("adapters/meta/brew.toml must parse");
        BrewAdapter {
            runner,
            meta,
            last_update: Mutex::new(None),
            update_ttl: Duration::from_secs(6 * 3600),
        }
    }

    pub fn with_update_ttl(mut self, ttl: Duration) -> BrewAdapter {
        self.update_ttl = ttl;
        self
    }

    fn env_vec(&self) -> Vec<(String, String)> {
        Self::ENV
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn prefix_for(exe_path: &Path) -> PathBuf {
        exe_path
            .parent()
            .and_then(|bin| bin.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    fn instance_id_for(prefix: &Path) -> String {
        format!("brew:{}", prefix.display())
    }

    async fn run_brew(
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
        Ok(self.runner.run(spec, None, CancellationToken::new()).await?)
    }

    async fn maybe_update(&self, inst: &ManagerInstance) -> Result<(), AdapterError> {
        let needs_update = {
            let last = self.last_update.lock().unwrap();
            match *last {
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
        *self.last_update.lock().unwrap() = Some(Instant::now());
        Ok(())
    }

    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        if env.euid == 0 {
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
            let prefix = Self::prefix_for(&path);
            found.push(ManagerInstance {
                id: Self::instance_id_for(&prefix),
                adapter_id: self.meta.id.clone(),
                exe_path: path,
                prefix,
                scope: Scope::User,
                healthy: version.is_some(),
                version,
            });
        }
        found
    }

    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let output = self
            .run_brew(
                inst,
                vec![
                    "info".to_string(),
                    "--installed".to_string(),
                    "--json=v2".to_string(),
                ],
                Duration::from_secs(120),
            )
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        parse_info_installed(&output.stdout, &inst.id)
    }

    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<UpdateCandidate>, AdapterError> {
        self.maybe_update(inst).await?;
        let output = self
            .run_brew(
                inst,
                vec!["outdated".to_string(), "--json=v2".to_string()],
                Duration::from_secs(120),
            )
            .await?;
        if output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: output.exit_code,
                stderr: output.stderr,
            });
        }
        parse_outdated(&output.stdout, &inst.id)
    }

    pub async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        validate_package_name(query)?;
        let names_output = self
            .run_brew(
                inst,
                vec!["search".to_string(), query.to_string()],
                Duration::from_secs(30),
            )
            .await?;
        if names_output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: names_output.exit_code,
                stderr: names_output.stderr,
            });
        }
        let desc_output = self
            .run_brew(
                inst,
                vec![
                    "search".to_string(),
                    "--desc".to_string(),
                    query.to_string(),
                ],
                Duration::from_secs(30),
            )
            .await?;
        if desc_output.exit_code != Some(0) {
            return Err(AdapterError::CommandFailed {
                code: desc_output.exit_code,
                stderr: desc_output.stderr,
            });
        }
        let mut hits = parse_search(&desc_output.stdout, &self.meta.id);
        if hits.is_empty() {
            hits = parse_search(&names_output.stdout, &self.meta.id);
        }
        Ok(hits)
    }
}
```

`include_str!("../../../../../adapters/meta/brew.toml")` has five `../` because `include_str!` resolves relative to this source file's own directory (`crates/banager-core/src/adapters/brew/`), not the crate root — verified with `python3 -c "import os; print(os.path.relpath('adapters/meta/brew.toml', 'crates/banager-core/src/adapters/brew'))"`, which prints exactly that path.

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p banager-core --lib adapters::brew`
Expected: PASS — 5 tests (`test_detect_refuses_root`, `test_detect_finds_opt_homebrew_on_this_apple_silicon_mac`, `test_inventory_parses_formula_and_cask`, `test_check_updates_respects_ttl`, `test_search_calls_both_search_variants`).

Run: `cargo test -p banager-core`
Expected: PASS — all tests across the crate (28 total: 3 model + 2 events + 2+4 runner + 3 path_env + 3 adapters + 6 brew_fixtures (integration test) + 5 new brew tests).

- [ ] **Step 5: Commit**

```bash
git add crates/banager-core/src/adapters/brew/mod.rs
git commit -m "$(cat <<'EOF'
feat(core): implement BrewAdapter detect/inventory/check_updates/search

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

---

### Task 11: `BrewAdapter`: plan / execute / reconcile for install, uninstall, upgrade; `SUDO_ASKPASS` passthrough

**Files:**
- Modify: `crates/banager-core/src/adapters/brew/mod.rs`

**Interfaces:**
- Consumes: `crate::adapters::Adapter` trait, `crate::model::{ArtifactKind, CancelPolicy, OpKind, OpRequest, ResourceLock, Plan, ArtifactKey, Outcome, Reconciled}` (Task 4/7), `crate::events::{EventSink, OpId, OperationEvent}` (Task 4), `crate::runner::LineCallback` (Task 5), `parse::parse_uses` (Task 9), and `BrewAdapter`'s private helpers `run_brew`/`env_vec` from Task 10.
- Produces: `BrewAdapter::plan`, `BrewAdapter::execute`, `BrewAdapter::reconcile`, and the full `impl Adapter for BrewAdapter` — the first adapter later tasks (`OperationManager` in Task 12, `examples/brew_smoke.rs` in Task 14) can treat as a trait object.

- [ ] **Step 1: Write the failing tests (red)**

Append a new test module to the end of `crates/banager-core/src/adapters/brew/mod.rs` (after the existing `mod tests` block from Task 10):

```rust
#[cfg(test)]
mod plan_execute_tests {
    use super::*;
    use crate::events::VecSink;
    use crate::runner::MockRunner;

    fn test_instance() -> ManagerInstance {
        ManagerInstance {
            id: "brew:/opt/homebrew".to_string(),
            adapter_id: "brew".to_string(),
            exe_path: PathBuf::from("/opt/homebrew/bin/brew"),
            prefix: PathBuf::from("/opt/homebrew"),
            scope: Scope::User,
            version: Some("7.0.3".to_string()),
            healthy: true,
        }
    }

    #[tokio::test]
    async fn test_plan_install_formula() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.args, vec!["install", "--formula", "jq"]);
        assert!(!plan.needs_password);
        assert_eq!(
            plan.locks,
            vec![ResourceLock("brew:/opt/homebrew".to_string())]
        );
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);
    }

    #[tokio::test]
    async fn test_plan_install_cask_needs_password() {
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Cask,
            name: "claudebar".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.args, vec!["install", "--cask", "claudebar"]);
        assert!(plan.needs_password);
    }

    #[tokio::test]
    async fn test_plan_uninstall_with_dependents_warns() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "python@3.13\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert_eq!(plan.affected, vec!["python@3.13".to_string()]);
        assert_eq!(
            plan.warnings,
            vec!["Removing jq will break: python@3.13".to_string()]
        );
    }

    #[tokio::test]
    async fn test_plan_uninstall_without_dependents_has_no_warning() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "uses", "--installed", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Uninstall,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        assert!(plan.affected.is_empty());
        assert!(plan.warnings.is_empty());
    }

    #[tokio::test]
    async fn test_execute_streams_log_events_and_succeeds() {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "install", "--formula", "jq"],
            CommandOutput {
                exit_code: Some(0),
                stdout: "Installing jq\nDone\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome = adapter
            .execute(&plan, sink.clone(), 1, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(sink.snapshot().len(), 2);
    }

    #[tokio::test]
    async fn test_execute_failure_summary_is_last_five_stderr_lines() {
        let runner = Arc::new(MockRunner::new());
        let stderr = (1..=8)
            .map(|n| format!("line{n}"))
            .collect::<Vec<_>>()
            .join("\n");
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "install", "--formula", "jq"],
            CommandOutput {
                exit_code: Some(1),
                stdout: String::new(),
                stderr,
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome = adapter
            .execute(&plan, sink, 1, CancellationToken::new())
            .await
            .expect("execute");
        match outcome {
            Outcome::Failed { exit_code, summary } => {
                assert_eq!(exit_code, Some(1));
                assert_eq!(summary, "line4\nline5\nline6\nline7\nline8");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_and_absent() {
        let runner = Arc::new(MockRunner::new());
        let json = r#"{"formulae":[{"name":"jq","desc":null,"homepage":null,"linked_keg":"1.7.1","installed":[{"version":"1.7.1","installed_on_request":true,"installed_as_dependency":false,"time":null}]}],"casks":[]}"#;
        runner.respond(
            vec!["/opt/homebrew/bin/brew", "info", "--installed", "--json=v2"],
            CommandOutput {
                exit_code: Some(0),
                stdout: json.to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();

        let present = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Formula,
                    name: "jq".to_string(),
                },
            )
            .await
            .expect("reconcile present");
        assert_eq!(
            present,
            Reconciled {
                present: true,
                version: Some("1.7.1".to_string())
            }
        );

        let absent = adapter
            .reconcile(
                &inst,
                &ArtifactKey {
                    instance_id: inst.id.clone(),
                    kind: ArtifactKind::Formula,
                    name: "missing".to_string(),
                },
            )
            .await
            .expect("reconcile absent");
        assert_eq!(
            absent,
            Reconciled {
                present: false,
                version: None
            }
        );
    }

    #[tokio::test]
    async fn test_plan_passes_through_sudo_askpass_for_cask_install() {
        std::env::set_var("SUDO_ASKPASS", "/tmp/fake-askpass.sh");
        let runner = Arc::new(MockRunner::new());
        let adapter = BrewAdapter::new(runner);
        let inst = test_instance();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: inst.id.clone(),
            artifact_kind: ArtifactKind::Cask,
            name: "claudebar".to_string(),
        };
        let plan = adapter.plan(&inst, &req).await.expect("plan");
        std::env::remove_var("SUDO_ASKPASS");
        assert!(plan
            .env
            .contains(&("SUDO_ASKPASS".to_string(), "/tmp/fake-askpass.sh".to_string())));
    }
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p banager-core --lib adapters::brew::plan_execute_tests`
Expected: FAIL to compile — `error[E0599]: no method named \`plan\` found for struct \`BrewAdapter\`` (it only has `detect`/`inventory`/`check_updates`/`search` so far).

- [ ] **Step 3: Add the missing imports and write `plan`/`execute`/`reconcile`**

Edit the `use` block at the top of `crates/banager-core/src/adapters/brew/mod.rs`:

```diff
 pub mod parse;

-use crate::adapters::{validate_package_name, AdapterError, AdapterMeta};
-use crate::model::{InstalledArtifact, ManagerInstance, Scope, SearchHit, UpdateCandidate};
-use crate::runner::{CommandOutput, CommandRunner, CommandSpec, HostEnv};
-use parse::{parse_info_installed, parse_outdated, parse_search, parse_version};
+use crate::adapters::{validate_package_name, Adapter, AdapterError, AdapterMeta, Capabilities};
+use crate::events::{EventSink, OpId};
+use crate::model::{
+    ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, ManagerInstance, OpKind,
+    OpRequest, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate,
+};
+use crate::runner::{CommandOutput, CommandRunner, CommandSpec, HostEnv, LineCallback};
+use async_trait::async_trait;
+use parse::{parse_info_installed, parse_outdated, parse_search, parse_uses, parse_version};
 use std::path::{Path, PathBuf};
 use std::sync::{Arc, Mutex};
 use std::time::{Duration, Instant};
 use tokio_util::sync::CancellationToken;
```

Append a second `impl BrewAdapter` block after the one from Task 10 (Rust allows multiple inherent `impl` blocks for the same type), and before the `#[cfg(test)] mod tests` block:

```rust
impl BrewAdapter {
    pub async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        validate_package_name(&req.name)?;
        let lock = ResourceLock(inst.id.clone());
        match req.kind {
            OpKind::Install => {
                let flag = match req.artifact_kind {
                    ArtifactKind::Cask => "--cask",
                    _ => "--formula",
                };
                let needs_password = matches!(req.artifact_kind, ArtifactKind::Cask);
                let mut env = self.env_vec();
                if let Ok(askpass) = std::env::var("SUDO_ASKPASS") {
                    env.push(("SUDO_ASKPASS".to_string(), askpass));
                }
                Ok(Plan {
                    request: req.clone(),
                    program: inst.exe_path.clone(),
                    args: vec!["install".to_string(), flag.to_string(), req.name.clone()],
                    env,
                    needs_password,
                    locks: vec![lock],
                    cancel_policy: CancelPolicy::KillThenReconcile,
                    warnings: Vec::new(),
                    affected: Vec::new(),
                    timeout_secs: 1800,
                })
            }
            OpKind::Uninstall => {
                let uses_output = self
                    .run_brew(
                        inst,
                        vec![
                            "uses".to_string(),
                            "--installed".to_string(),
                            req.name.clone(),
                        ],
                        Duration::from_secs(120),
                    )
                    .await?;
                let affected = if uses_output.exit_code == Some(0) {
                    parse_uses(&uses_output.stdout)
                } else {
                    Vec::new()
                };
                let mut warnings = Vec::new();
                if !affected.is_empty() {
                    warnings.push(format!(
                        "Removing {} will break: {}",
                        req.name,
                        affected.join(", ")
                    ));
                }
                Ok(Plan {
                    request: req.clone(),
                    program: inst.exe_path.clone(),
                    args: vec!["uninstall".to_string(), req.name.clone()],
                    env: self.env_vec(),
                    needs_password: false,
                    locks: vec![lock],
                    cancel_policy: CancelPolicy::KillThenReconcile,
                    warnings,
                    affected,
                    timeout_secs: 1800,
                })
            }
            OpKind::Upgrade => {
                let needs_password = matches!(req.artifact_kind, ArtifactKind::Cask);
                let mut env = self.env_vec();
                if let Ok(askpass) = std::env::var("SUDO_ASKPASS") {
                    env.push(("SUDO_ASKPASS".to_string(), askpass));
                }
                Ok(Plan {
                    request: req.clone(),
                    program: inst.exe_path.clone(),
                    args: vec!["upgrade".to_string(), req.name.clone()],
                    env,
                    needs_password,
                    locks: vec![lock],
                    cancel_policy: CancelPolicy::KillThenReconcile,
                    warnings: Vec::new(),
                    affected: Vec::new(),
                    timeout_secs: 1800,
                })
            }
        }
    }

    pub async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        let sink_for_line = sink.clone();
        let on_line: LineCallback = Arc::new(move |stream, line| {
            sink_for_line.emit(crate::events::OperationEvent::Log {
                op_id,
                stream,
                line,
            });
        });
        let spec = CommandSpec {
            program: plan.program.clone(),
            args: plan.args.clone(),
            env: plan.env.clone(),
            cwd: None,
            timeout: Duration::from_secs(plan.timeout_secs),
        };
        let output = self.runner.run(spec, Some(on_line), cancel).await?;
        if output.cancelled || output.timed_out {
            return Ok(Outcome::Unconfirmed);
        }
        match output.exit_code {
            Some(0) => Ok(Outcome::Succeeded),
            code => {
                let stderr_lines: Vec<&str> = output.stderr.lines().collect();
                let start = stderr_lines.len().saturating_sub(5);
                let summary = stderr_lines[start..].join("\n");
                Ok(Outcome::Failed {
                    exit_code: code,
                    summary,
                })
            }
        }
    }

    pub async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        let artifacts = self.inventory(inst).await?;
        match artifacts
            .into_iter()
            .find(|a| a.key.name == key.name && a.key.kind == key.kind)
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
```

- [ ] **Step 4: Add the `Adapter` trait impl**

Append after the `impl BrewAdapter` block from Step 3 (still before the test modules):

```rust
// Adapter is implemented by forwarding to the inherent methods above via
// fully-qualified `BrewAdapter::method(self, ...)` calls. This is
// unambiguous even though the method names match: `BrewAdapter::detect`
// can only resolve to the inherent impl (a trait method would be spelled
// `<BrewAdapter as Adapter>::detect`), so there is no risk of the trait
// method accidentally calling itself.
#[async_trait]
impl Adapter for BrewAdapter {
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
        BrewAdapter::detect(self, env).await
    }

    async fn inventory(&self, inst: &ManagerInstance) -> Result<Vec<InstalledArtifact>, AdapterError> {
        BrewAdapter::inventory(self, inst).await
    }

    async fn check_updates(&self, inst: &ManagerInstance) -> Result<Vec<UpdateCandidate>, AdapterError> {
        BrewAdapter::check_updates(self, inst).await
    }

    async fn search(&self, inst: &ManagerInstance, query: &str) -> Result<Vec<SearchHit>, AdapterError> {
        BrewAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        BrewAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        BrewAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(&self, inst: &ManagerInstance, key: &ArtifactKey) -> Result<Reconciled, AdapterError> {
        BrewAdapter::reconcile(self, inst, key).await
    }
}
```

- [ ] **Step 5: Run to see it pass**

Run: `cargo test -p banager-core --lib adapters::brew`
Expected: PASS — 13 tests (5 from Task 10's `mod tests` plus 8 new `mod plan_execute_tests`).

Run: `cargo build -p banager-core`
Expected: exits 0, confirming `BrewAdapter` satisfies the full `Adapter` trait (this is the first type to implement it, so any signature mismatch against Task 7's trait definition would fail here).

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/adapters/brew/mod.rs
git commit -m "$(cat <<'EOF'
feat(core): implement BrewAdapter plan/execute/reconcile and the Adapter trait

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

---

### Task 12: `OperationManager` with resource locks, state machine, cancel, verifying

**Files:**
- Modify: `crates/banager-core/src/lib.rs`
- Create: `crates/banager-core/src/ops/mod.rs`
- Create: `crates/banager-core/tests/ops_lock_test.rs`

**Interfaces:**
- Consumes: `crate::adapters::Adapter` (Task 7), `crate::events::{EventSink, OpId, OperationEvent}` (Task 4), `crate::model::{AdapterId, ArtifactKey, InstanceId, ManagerInstance, OpKind, OpStatus, Outcome, Plan, ResourceLock}` (Task 4).
- Produces: `OperationManager` and `OpRecord`, exactly as named in the skeleton — the type a future Tauri command layer (outside this plan's scope) will wrap to expose install/uninstall/upgrade to the UI.

- [ ] **Step 1: Declare the module (red)**

Edit `crates/banager-core/src/lib.rs`:

```diff
 pub mod model;
 pub mod events;
 pub mod runner;
 pub mod adapters;
+pub mod ops;

 pub use model::*;
 pub use events::*;
```

Run: `cargo build -p banager-core`
Expected: FAIL — `error[E0583]: file not found for module \`ops\``.

- [ ] **Step 2: Write `ops/mod.rs`**

```rust
use crate::adapters::Adapter;
use crate::events::{EventSink, OpId, OperationEvent};
use crate::model::{
    AdapterId, ArtifactKey, InstanceId, ManagerInstance, OpKind, OpStatus, Outcome, Plan,
    ResourceLock,
};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

pub struct OpRecord {
    pub id: OpId,
    pub plan: Plan,
    pub status: OpStatus,
    pub outcome: Option<Outcome>,
    pub cancel: CancellationToken,
}

pub struct OperationManager {
    adapters: HashMap<AdapterId, Arc<dyn Adapter>>,
    instances: Mutex<HashMap<InstanceId, ManagerInstance>>,
    sink: Arc<dyn EventSink>,
    held: Arc<Mutex<HashSet<ResourceLock>>>,
    records: Arc<Mutex<HashMap<OpId, OpRecord>>>,
    next_id: AtomicU64,
}

impl OperationManager {
    pub fn new(sink: Arc<dyn EventSink>) -> OperationManager {
        OperationManager {
            adapters: HashMap::new(),
            instances: Mutex::new(HashMap::new()),
            sink,
            held: Arc::new(Mutex::new(HashSet::new())),
            records: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(1),
        }
    }

    /// Adapters are registered once, before the manager is shared behind an
    /// `Arc` (see the tests below) — that's why this takes `&mut self`
    /// while every other method takes `&self`.
    pub fn register_adapter(&mut self, adapter: Arc<dyn Adapter>) {
        let id = adapter.meta().id.clone();
        self.adapters.insert(id, adapter);
    }

    pub fn register_instance(&self, inst: ManagerInstance) {
        self.instances.lock().unwrap().insert(inst.id.clone(), inst);
    }

    pub fn record(&self, op_id: OpId) -> Option<OpRecord> {
        let records = self.records.lock().unwrap();
        records.get(&op_id).map(|r| OpRecord {
            id: r.id,
            plan: r.plan.clone(),
            status: r.status,
            outcome: r.outcome.clone(),
            cancel: r.cancel.clone(),
        })
    }

    pub fn cancel(&self, op_id: OpId) {
        let mut records = self.records.lock().unwrap();
        if let Some(r) = records.get_mut(&op_id) {
            r.status = OpStatus::CancelRequested;
            r.cancel.cancel();
            drop(records);
            self.sink.emit(OperationEvent::Status {
                op_id,
                status: OpStatus::CancelRequested,
            });
        }
    }

    pub async fn wait(&self, op_id: OpId) -> Option<Outcome> {
        loop {
            {
                let records = self.records.lock().unwrap();
                match records.get(&op_id) {
                    Some(r) if r.status == OpStatus::Done => return r.outcome.clone(),
                    None => return None,
                    _ => {}
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }

    pub fn submit(self: &Arc<Self>, plan: Plan) -> OpId {
        let op_id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let cancel = CancellationToken::new();
        let record = OpRecord {
            id: op_id,
            plan: plan.clone(),
            status: OpStatus::Queued,
            outcome: None,
            cancel: cancel.clone(),
        };
        self.records.lock().unwrap().insert(op_id, record);
        self.sink.emit(OperationEvent::Status {
            op_id,
            status: OpStatus::Queued,
        });

        let manager = Arc::clone(self);
        tokio::spawn(async move {
            manager.run_operation(op_id, plan, cancel).await;
        });

        op_id
    }

    async fn run_operation(self: Arc<Self>, op_id: OpId, plan: Plan, cancel: CancellationToken) {
        // Wait for every lock this plan needs, polling every 50 ms. Only
        // mark `acquired` once every lock in `plan.locks` was free and has
        // now been inserted into `held` — otherwise a later step could
        // release a lock this op never actually took.
        let mut acquired = false;
        loop {
            {
                let mut held = self.held.lock().unwrap();
                if plan.locks.iter().all(|l| !held.contains(l)) {
                    for l in &plan.locks {
                        held.insert(l.clone());
                    }
                    acquired = true;
                }
            }
            if acquired {
                break;
            }
            if cancel.is_cancelled() {
                self.finish(op_id, Outcome::Unconfirmed, false);
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        self.set_status(op_id, OpStatus::Running);

        let instance = {
            let instances = self.instances.lock().unwrap();
            instances.get(&plan.request.instance_id).cloned()
        };
        let instance = match instance {
            Some(i) => i,
            None => {
                self.finish(
                    op_id,
                    Outcome::Failed {
                        exit_code: None,
                        summary: format!("unknown instance {}", plan.request.instance_id),
                    },
                    true,
                );
                return;
            }
        };

        let adapter = self.adapters.get(&instance.adapter_id).cloned();
        let adapter = match adapter {
            Some(a) => a,
            None => {
                self.finish(
                    op_id,
                    Outcome::Failed {
                        exit_code: None,
                        summary: format!("no adapter registered for {}", instance.adapter_id),
                    },
                    true,
                );
                return;
            }
        };

        let exec_result = adapter
            .execute(&plan, self.sink.clone(), op_id, cancel.clone())
            .await;

        if cancel.is_cancelled() {
            self.set_status(op_id, OpStatus::Cancelling);
        }
        self.set_status(op_id, OpStatus::Verifying);

        let key = ArtifactKey {
            instance_id: plan.request.instance_id.clone(),
            kind: plan.request.artifact_kind,
            name: plan.request.name.clone(),
        };
        let reconciled = adapter.reconcile(&instance, &key).await;

        let final_outcome = match exec_result {
            Ok(Outcome::Unconfirmed) => match reconciled {
                Ok(r) => {
                    let present_means_success = plan.request.kind != OpKind::Uninstall;
                    if r.present == present_means_success {
                        Outcome::Succeeded
                    } else {
                        Outcome::Unconfirmed
                    }
                }
                Err(_) => Outcome::Unconfirmed,
            },
            Ok(other) => other,
            Err(e) => Outcome::Failed {
                exit_code: None,
                summary: e.to_string(),
            },
        };

        self.finish(op_id, final_outcome, true);
    }

    fn set_status(&self, op_id: OpId, status: OpStatus) {
        if let Some(r) = self.records.lock().unwrap().get_mut(&op_id) {
            r.status = status;
        }
        self.sink.emit(OperationEvent::Status { op_id, status });
    }

    /// `release_locks` must be `false` when this op never actually acquired
    /// its locks (cancelled while still waiting for them) — otherwise this
    /// would release locks a *different*, still-running op is holding.
    fn finish(&self, op_id: OpId, outcome: Outcome, release_locks: bool) {
        {
            let mut records = self.records.lock().unwrap();
            if let Some(r) = records.get_mut(&op_id) {
                r.status = OpStatus::Done;
                r.outcome = Some(outcome.clone());
                if release_locks {
                    let mut held = self.held.lock().unwrap();
                    for l in &r.plan.locks {
                        held.remove(l);
                    }
                }
            }
        }
        self.sink.emit(OperationEvent::Finished { op_id, outcome });
    }
}
```

- [ ] **Step 3: Run to see it compiles**

Run: `cargo build -p banager-core`
Expected: exits 0 (no tests exercise `ops` yet — that's Step 4).

- [ ] **Step 4: Write the lock-serialization integration test with a `FakeAdapter`**

Create `crates/banager-core/tests/ops_lock_test.rs`:

```rust
use async_trait::async_trait;
use banager_core::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities};
use banager_core::events::{EventSink, OpId, VecSink};
use banager_core::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, ManagerInstance, OpKind,
    OpRequest, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate,
};
use banager_core::ops::OperationManager;
use banager_core::runner::HostEnv;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// A minimal `Adapter` whose `execute` just sleeps and records when it ran,
/// so the tests below can prove same-lock plans never overlap while
/// different-lock plans do.
struct FakeAdapter {
    meta: AdapterMeta,
    log: Arc<Mutex<Vec<(String, Instant, Instant)>>>,
}

impl FakeAdapter {
    fn new(id: &str, log: Arc<Mutex<Vec<(String, Instant, Instant)>>>) -> FakeAdapter {
        FakeAdapter {
            meta: AdapterMeta {
                id: id.to_string(),
                name: id.to_string(),
                kind: "fake".to_string(),
                platforms: vec!["macos".to_string()],
                homepage: "https://example.invalid".to_string(),
                schema_version: 1,
                verified_versions: vec![],
            },
            log,
        }
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
            background_check: false,
            cancel_safe: true,
        }
    }

    async fn detect(&self, _env: &HostEnv) -> Vec<ManagerInstance> {
        Vec::new()
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

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        Ok(Plan {
            request: req.clone(),
            program: inst.exe_path.clone(),
            args: vec![],
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
        plan: &Plan,
        _sink: Arc<dyn EventSink>,
        _op_id: OpId,
        _cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        let start = Instant::now();
        tokio::time::sleep(Duration::from_millis(200)).await;
        let end = Instant::now();
        self.log
            .lock()
            .unwrap()
            .push((plan.request.name.clone(), start, end));
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

fn make_instance(id: &str, prefix: &str) -> ManagerInstance {
    ManagerInstance {
        id: id.to_string(),
        adapter_id: "fake".to_string(),
        exe_path: PathBuf::from("/bin/true"),
        prefix: PathBuf::from(prefix),
        scope: Scope::User,
        version: None,
        healthy: true,
    }
}

fn overlaps(a_start: &Instant, a_end: &Instant, b_start: &Instant, b_end: &Instant) -> bool {
    a_start < b_end && b_start < a_end
}

#[tokio::test]
async fn test_same_lock_runs_serially() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(VecSink::new());
    let mut manager = OperationManager::new(sink);
    let adapter = Arc::new(FakeAdapter::new("fake", log.clone()));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);

    let inst = make_instance("fake:/shared", "/shared");
    manager.register_instance(inst.clone());

    let req_a = OpRequest {
        kind: OpKind::Install,
        instance_id: inst.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: "a".to_string(),
    };
    let req_b = OpRequest {
        kind: OpKind::Install,
        instance_id: inst.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: "b".to_string(),
    };

    let plan_a = adapter.plan(&inst, &req_a).await.expect("plan a");
    let plan_b = adapter.plan(&inst, &req_b).await.expect("plan b");

    let id_a = manager.submit(plan_a);
    let id_b = manager.submit(plan_b);

    manager.wait(id_a).await;
    manager.wait(id_b).await;

    let entries = log.lock().unwrap().clone();
    assert_eq!(entries.len(), 2);
    let (_, start_a, end_a) = &entries[0];
    let (_, start_b, end_b) = &entries[1];
    assert!(
        !overlaps(start_a, end_a, start_b, end_b),
        "same-lock operations overlapped: {:?}",
        entries
    );
}

#[tokio::test]
async fn test_different_locks_run_concurrently() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(VecSink::new());
    let mut manager = OperationManager::new(sink);
    let adapter = Arc::new(FakeAdapter::new("fake", log.clone()));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);

    let inst_a = make_instance("fake:/a", "/a");
    let inst_b = make_instance("fake:/b", "/b");
    manager.register_instance(inst_a.clone());
    manager.register_instance(inst_b.clone());

    let req_a = OpRequest {
        kind: OpKind::Install,
        instance_id: inst_a.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: "a".to_string(),
    };
    let req_b = OpRequest {
        kind: OpKind::Install,
        instance_id: inst_b.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: "b".to_string(),
    };

    let plan_a = adapter.plan(&inst_a, &req_a).await.expect("plan a");
    let plan_b = adapter.plan(&inst_b, &req_b).await.expect("plan b");

    let id_a = manager.submit(plan_a);
    let id_b = manager.submit(plan_b);

    manager.wait(id_a).await;
    manager.wait(id_b).await;

    let entries = log.lock().unwrap().clone();
    assert_eq!(entries.len(), 2);
    let (_, start_a, end_a) = &entries[0];
    let (_, start_b, end_b) = &entries[1];
    assert!(
        overlaps(start_a, end_a, start_b, end_b),
        "different-lock operations did not overlap: {:?}",
        entries
    );
}
```

- [ ] **Step 5: Run to see it pass**

Run: `cargo test -p banager-core --test ops_lock_test -- --nocapture`
Expected: PASS — 2 tests. `test_same_lock_runs_serially` takes a little over 400 ms (200 ms + up to ~50 ms poll latency, twice); `test_different_locks_run_concurrently` takes a little over 200 ms (both run at once).

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/lib.rs crates/banager-core/src/ops crates/banager-core/tests/ops_lock_test.rs
git commit -m "$(cat <<'EOF'
feat(core): add OperationManager with resource locks, cancellation, and reconcile-on-verify

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```
### Task 13: `SUDO_ASKPASS` spike script + result doc

**Files:**
- Create: `scripts/banager-askpass.sh`
- Create: `docs/spikes/2026-09-askpass.md`

**Interfaces:**
- Consumes: nothing.
- Produces: a shell script that Task 11's `SUDO_ASKPASS` passthrough is meant to eventually point `sudo` at (wiring it into the app's actual cask-install flow is out of scope for this plan — a later UI phase will invoke it), and a written record of whether it actually works without a TTY.

This is a manual, interactive spike: it requires a real GUI session and your actual admin password, so none of its steps can be scripted end-to-end or run by an agent unattended. Do not run any `brew install` as part of this — it is scoped to proving the password dialog mechanism alone.

- [ ] **Step 1: Write the askpass script**

Create `scripts/banager-askpass.sh`:

```sh
#!/bin/sh
osascript -e 'text returned of (display dialog "Banager needs your password to finish this step." default answer "" with hidden answer with title "Banager")'
```

- [ ] **Step 2: Make it executable**

Run: `chmod +x scripts/banager-askpass.sh`
Expected: `ls -l scripts/banager-askpass.sh` shows the `x` bits set (`-rwxr-xr-x`).

- [ ] **Step 3: Run the correct-password scenario**

Run, from a real Terminal window (not over SSH — `osascript`'s dialog needs a GUI session attached to your login):
```bash
SUDO_ASKPASS="$PWD/scripts/banager-askpass.sh" sudo -A -k true < /dev/null > /tmp/askpass.log 2>&1; echo exit=$?
```
Expected: a native macOS dialog titled "Banager" appears asking for your password. Type your real admin password and click OK. The command should print `exit=0`. `-k` forces `sudo` to ignore any cached credential, so the dialog is guaranteed to appear; `-A` tells `sudo` to use `$SUDO_ASKPASS` instead of a terminal prompt.

- [ ] **Step 4: Run the cancel scenario**

Run the same command again:
```bash
SUDO_ASKPASS="$PWD/scripts/banager-askpass.sh" sudo -A -k true < /dev/null > /tmp/askpass.log 2>&1; echo exit=$?
```
This time click "Cancel" in the dialog instead of entering a password.
Expected: `exit=1` (or another nonzero code — record the exact value you see).

- [ ] **Step 5: Run the wrong-password scenario**

Run it a third time, entering an intentionally wrong password and clicking OK.
Expected: either `sudo` re-prompts (the dialog reappears) or fails with a nonzero exit — record whichever actually happens; both are valid outcomes for a spike, the point is to know which one it is.

- [ ] **Step 6: Write the results doc**

Create `docs/spikes/2026-09-askpass.md`, filling in the "Observed" column with what Steps 3–5 actually showed (do not leave the template's own text in that column — replace it with your real observation for each row):

```markdown
# Spike: SUDO_ASKPASS for cask installs without a TTY

Date: 2026-09-17
Command under test: `SUDO_ASKPASS="$PWD/scripts/banager-askpass.sh" sudo -A -k true < /dev/null`

| Scenario | Expected | Observed |
|---|---|---|
| Correct password entered | Dialog appears; `sudo` succeeds; exit code 0 | _fill in after Step 3_ |
| Cancel clicked in the dialog | `sudo` fails cleanly with a nonzero exit code, no hang | _fill in after Step 4_ |
| Wrong password entered | Either `sudo` re-prompts via the dialog again, or fails with a nonzero exit code | _fill in after Step 5_ |

## Conclusion

_State plainly here whether `SUDO_ASKPASS` + this `osascript` dialog is
viable for Banager's cask-install flow without a Terminal window open. If
any scenario hung, required a TTY, or silently did nothing, say so — that
determines whether Task 11's `SUDO_ASKPASS` passthrough is usable as-is or
whether cask installs needing `sudo` must fall back to "open Terminal and
run this command" (see spec section 14, risk row 1)._
```

- [ ] **Step 7: Commit**

```bash
git add scripts/banager-askpass.sh docs/spikes/2026-09-askpass.md
git commit -m "$(cat <<'EOF'
docs: record SUDO_ASKPASS spike for cask installs and add the askpass helper script

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

---

### Task 14: `examples/brew_smoke.rs` + `docs/what-we-run.md`

**Files:**
- Create: `crates/banager-core/examples/brew_smoke.rs`
- Create: `docs/what-we-run.md`

**Interfaces:**
- Consumes: `RealRunner` (Task 5), `HostEnv::discover()` (Task 6), `BrewAdapter::{new, detect, inventory, check_updates, with_update_ttl}` (Tasks 10–11).
- Produces: a manually-run smoke check and a trust document; nothing later in this plan depends on either.

- [ ] **Step 1: Write the example**

Create `crates/banager-core/examples/brew_smoke.rs`:

```rust
use banager_core::adapters::brew::BrewAdapter;
use banager_core::runner::{HostEnv, RealRunner};
use std::sync::Arc;
use std::time::Duration;

/// Read-only smoke check: detects a real Homebrew install, runs `brew
/// update` unconditionally (via `with_update_ttl(Duration::from_secs(0))`),
/// then lists installed and outdated counts. Never installs, uninstalls, or
/// upgrades anything.
#[tokio::main]
async fn main() {
    let env = HostEnv::discover();
    let runner: Arc<dyn banager_core::runner::CommandRunner> = Arc::new(RealRunner::new());
    let adapter = BrewAdapter::new(runner).with_update_ttl(Duration::from_secs(0));

    let instances = adapter.detect(&env).await;
    if instances.is_empty() {
        println!("No Homebrew instance detected on this machine.");
        return;
    }

    for inst in &instances {
        println!(
            "Found instance: {} (brew {})",
            inst.id,
            inst.version.as_deref().unwrap_or("unknown")
        );

        let artifacts = adapter.inventory(inst).await.expect("inventory failed");
        println!("  {} installed artifacts", artifacts.len());

        let outdated = adapter.check_updates(inst).await.expect("check_updates failed");
        println!("  {} outdated artifacts", outdated.len());
        for candidate in outdated.iter().take(10) {
            println!(
                "    {} {} -> {}",
                candidate.key.name, candidate.current, candidate.target
            );
        }
    }
}
```

- [ ] **Step 2: Run it**

Run: `cargo run -p banager-core --example brew_smoke`
Expected: on a Mac with Homebrew at `/opt/homebrew`, prints `Found instance: brew:/opt/homebrew (brew 7.0.3)` (or your installed version), then the installed-artifact count, the outdated count, and up to 10 `name current -> target` lines; exits 0. On a machine without Homebrew, prints `No Homebrew instance detected on this machine.` and exits 0. This genuinely runs `brew update` against your real Homebrew installation (read-only — it only refreshes brew's local package index, it does not install or upgrade anything) — expect it to take several seconds.

- [ ] **Step 3: Write `docs/what-we-run.md`**

Create `docs/what-we-run.md`:

```markdown
# What Banager Runs (Phase 0–1: Homebrew only)

Banager never invokes a shell. Every command below is a fixed argv array
run directly against the resolved Homebrew binary (one of
`BrewAdapter::CANDIDATE_PATHS`). The only user-controlled input in any of
these commands is a single validated argument — a formula/cask name or a
search query, checked by `validate_package_name` against
`^[A-Za-z0-9@._+/-]+$`, and never allowed to start with `-`.

## Environment applied to every invocation

    HOMEBREW_NO_AUTO_UPDATE=1
    HOMEBREW_NO_ENV_HINTS=1
    HOMEBREW_NO_INSTALL_CLEANUP=1
    NO_COLOR=1

Cask install/upgrade additionally passes through `SUDO_ASKPASS` when that
variable is already set in Banager's own process environment — Banager
never sets it on its own behalf.

Banager refuses to run any `brew` command at all when the current
process's effective user ID is 0 (root).

## Read-only commands (background checks; never require a password)

| Purpose | Argv | Timeout |
|---|---|---|
| Detect a Homebrew install | `<brew> --version` | 30 s |
| List installed formulae + casks | `<brew> info --installed --json=v2` | 120 s |
| Refresh Homebrew's local package index (TTL: 6 hours) | `<brew> update` | 120 s |
| List outdated formulae + casks | `<brew> outdated --json=v2` | 120 s |
| Search by name | `<brew> search {query}` | 30 s |
| Search by name + description | `<brew> search --desc {query}` | 30 s |
| List installed formulae depending on a formula (uninstall-safety check) | `<brew> uses --installed {name}` | 120 s |

## Write commands (only run after the user reviews and confirms a plan preview)

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install a formula | `<brew> install --formula {name}` | 1800 s | No |
| Install a cask | `<brew> install --cask {name}` | 1800 s | Sometimes — some cask installers invoke `sudo`; `SUDO_ASKPASS` is passed through when set |
| Uninstall a formula or cask | `<brew> uninstall {name}` | 1800 s | No |
| Upgrade one formula or cask | `<brew> upgrade {name}` | 1800 s | Sometimes (casks only) |

Banager never passes `--ignore-dependencies` to `brew uninstall`, and never
runs a bare `brew upgrade` — upgrades are always one invocation per
confirmed artifact, never "upgrade everything" in a single command.

`<brew>` above is always the absolute path `BrewAdapter::detect` found on
disk (one of `/opt/homebrew/bin/brew`, `/usr/local/bin/brew`,
`/home/linuxbrew/.linuxbrew/bin/brew`), never a bare `brew` resolved
through a shell `PATH` lookup.
```

- [ ] **Step 4: Cross-check the doc against the actual code**

Run: `grep -n '"install"\|"uninstall"\|"upgrade"\|"search"\|"uses"\|"outdated"\|"info"\|"update"\|"--version"' crates/banager-core/src/adapters/brew/mod.rs`
Expected: every argv literal this prints (from `run_brew`, `detect`, `plan`) has a corresponding row in the tables you just wrote in `docs/what-we-run.md` — there should be exactly seven distinct commands in total (7 read-only + 4 write, per the tables), and no argv in the code that the doc doesn't mention, or vice versa. If you find a mismatch, fix `docs/what-we-run.md` (not the code — Tasks 10–11 already have their own passing tests).

- [ ] **Step 5: Commit**

```bash
git add crates/banager-core/examples/brew_smoke.rs docs/what-we-run.md
git commit -m "$(cat <<'EOF'
docs: add read-only brew smoke example and the what-we-run command inventory

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

---

### Task 15: CI live Homebrew smoke test (spec §10 / §11.3)

**Files:**
- Create: `crates/banager-core/tests/brew_live.rs`
- Modify: `.github/workflows/ci.yml` (add one step after `cargo test`)

**Interfaces:**
- Consumes: `RealRunner`, `HostEnv::discover()` (Tasks 5–6); `BrewAdapter::{new, with_update_ttl}` (Task 10); the `Adapter` trait methods `detect`, `plan`, `execute`, `reconcile`, `inventory` on `BrewAdapter` (Tasks 10–11); `VecSink` (Task 4); `Outcome`, `OpRequest`, `OpKind`, `ArtifactKind`, `ArtifactKey` (Task 4).
- Produces: nothing later depends on it; it is the release gate that proves the install → inventory → uninstall path against a real Homebrew.

The test is `#[ignore]` and additionally gated on `BANAGER_LIVE=1`, so `cargo test --workspace` never installs anything on a developer machine by accident. GNU `hello` is the smallest well-known formula (one binary, no dependencies), which is why it is the probe package.

- [ ] **Step 1: Write the live test**

Create `crates/banager-core/tests/brew_live.rs`:

```rust
//! Live Homebrew smoke test: installs, inventories and removes the tiny GNU
//! `hello` formula through the real adapter. Runs only when
//! `BANAGER_LIVE=1` is set AND the test is invoked with `--ignored`, so a
//! plain `cargo test` never touches the machine.

use banager_core::adapters::brew::BrewAdapter;
use banager_core::adapters::Adapter;
use banager_core::events::VecSink;
use banager_core::model::{ArtifactKey, ArtifactKind, OpKind, OpRequest, Outcome};
use banager_core::runner::{HostEnv, RealRunner};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::test]
#[ignore = "installs and removes the `hello` formula; run with BANAGER_LIVE=1 cargo test -p banager-core --test brew_live -- --ignored"]
async fn live_install_inventory_uninstall_hello() {
    if std::env::var("BANAGER_LIVE").as_deref() != Ok("1") {
        eprintln!("BANAGER_LIVE is not 1; skipping live smoke test");
        return;
    }

    let runner = Arc::new(RealRunner);
    // A huge TTL means `brew update` is not run here; CI runners already ship
    // a fresh Homebrew and the install path does not need the newest index.
    let adapter = BrewAdapter::new(runner).with_update_ttl(Duration::from_secs(60 * 60 * 24 * 365));
    let env = HostEnv::discover();
    let instances = Adapter::detect(&adapter, &env).await;
    let inst = instances
        .first()
        .cloned()
        .expect("a Homebrew instance must be detected on the CI runner");
    let sink = Arc::new(VecSink::new());
    let key = ArtifactKey {
        instance_id: inst.id.clone(),
        kind: ArtifactKind::Formula,
        name: "hello".to_string(),
    };

    // Install.
    let install_req = OpRequest {
        kind: OpKind::Install,
        instance_id: inst.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: "hello".to_string(),
    };
    let install_plan = Adapter::plan(&adapter, &inst, &install_req).await.expect("install plan");
    assert_eq!(
        install_plan.args,
        vec!["install".to_string(), "--formula".to_string(), "hello".to_string()],
        "install argv preview must be exactly `brew install --formula hello`"
    );
    let outcome = Adapter::execute(&adapter, &install_plan, sink.clone(), 1, CancellationToken::new())
        .await
        .expect("install execute");
    assert_eq!(outcome, Outcome::Succeeded, "install must succeed; log: {:?}", sink.snapshot());

    // Inventory + reconcile see it.
    let reconciled = Adapter::reconcile(&adapter, &inst, &key).await.expect("reconcile after install");
    assert!(reconciled.present, "hello must be present after install");
    assert!(reconciled.version.is_some());
    let inventory = Adapter::inventory(&adapter, &inst).await.expect("inventory");
    assert!(inventory.iter().any(|a| a.key == key), "inventory must list hello");

    // Uninstall.
    let uninstall_req = OpRequest {
        kind: OpKind::Uninstall,
        instance_id: inst.id.clone(),
        artifact_kind: ArtifactKind::Formula,
        name: "hello".to_string(),
    };
    let uninstall_plan = Adapter::plan(&adapter, &inst, &uninstall_req).await.expect("uninstall plan");
    assert!(uninstall_plan.affected.is_empty(), "nothing installed depends on hello");
    assert_eq!(
        uninstall_plan.args,
        vec!["uninstall".to_string(), "hello".to_string()],
        "uninstall argv preview must be exactly `brew uninstall hello`"
    );
    let outcome = Adapter::execute(&adapter, &uninstall_plan, sink.clone(), 2, CancellationToken::new())
        .await
        .expect("uninstall execute");
    assert_eq!(outcome, Outcome::Succeeded, "uninstall must succeed; log: {:?}", sink.snapshot());

    let reconciled = Adapter::reconcile(&adapter, &inst, &key).await.expect("reconcile after uninstall");
    assert!(!reconciled.present, "hello must be gone after uninstall");
}
```

- [ ] **Step 2: Confirm the plain test run skips it**

Run: `cargo test -p banager-core --test brew_live`
Expected: `test live_install_inventory_uninstall_hello ... ignored` and `test result: ok. 0 passed; 0 failed; 1 ignored`.

- [ ] **Step 3: Run it for real on this Mac once**

This installs and then removes GNU `hello` via your own Homebrew (about 2 MB, no dependencies). It is the same thing CI will do on every push.

Run: `BANAGER_LIVE=1 cargo test -p banager-core --test brew_live -- --ignored --nocapture`
Expected: brew's own install/uninstall output streams through (`==> Fetching hello`, `🍺  /opt/homebrew/Cellar/hello/...`, `Uninstalling /opt/homebrew/Cellar/hello/...`) and finally `test result: ok. 1 passed`. Afterwards `brew list --formula | grep -x hello` prints nothing (exit 1), proving cleanup.

- [ ] **Step 4: Add the CI step**

Edit `.github/workflows/ci.yml`: insert the following step directly after the existing `cargo test` step (the one whose `run:` is `cargo test --workspace`), keeping the same indentation as its neighbours:

```yaml
      - name: live homebrew smoke (install/inventory/uninstall hello)
        env:
          BANAGER_LIVE: "1"
        run: cargo test -p banager-core --test brew_live -- --ignored --nocapture
```

Run: `python3 -c "import yaml; d=yaml.safe_load(open('.github/workflows/ci.yml')); steps=[s['name'] for s in d['jobs'][list(d['jobs'])[0]]['steps'] if 'name' in s]; print(steps)"`
Expected: the printed list contains `'cargo test'` immediately followed by `'live homebrew smoke (install/inventory/uninstall hello)'`.

- [ ] **Step 5: Commit and watch CI**

```bash
git add crates/banager-core/tests/brew_live.rs .github/workflows/ci.yml
git commit -m "$(cat <<'EOF'
test(core): live Homebrew smoke test (install/inventory/uninstall hello) wired into CI

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
git push
gh run watch --exit-status
```

Expected: the CI run finishes green, and the live step's log shows `test result: ok. 1 passed`.
