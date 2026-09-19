# Canager Phase 2 Implementation Plan: UI Shell (Installed / Updates / Operations / Settings)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the working Homebrew core into an app a non-programmer can actually use: a macOS-native-feeling window that lists what is installed, shows what can be updated, runs install/update/uninstall with a live log and a working Cancel button, explains what an uninstall would break, and remembers a few settings — in English and Simplified Chinese.

**Architecture:** `canager-core` gains a `Session` facade that owns the adapters, the detected instances and an in-memory `Snapshot` (generation-numbered), plus file-backed `Settings`. The Tauri shell exposes that facade over `#[tauri::command]`s and streams operation events through a Tauri `Channel`. The React front end is a thin view over those commands: TanStack Query owns server state, Zustand owns view state, and no business logic lives in TypeScript. Persistent caching (SQLite) stays out of this phase — it belongs to the phase 3 storage layer per `docs/superpowers/backlog.md`.

**Tech Stack:** Rust (canager-core, tauri 2.11.x), tauri Channel IPC; pnpm, Vite, React 19, TypeScript, Tailwind CSS v4, Radix UI primitives, TanStack Query v5, Zustand, i18next + react-i18next, @tanstack/react-virtual, vitest + @testing-library/react.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-09-17-canager-design.md`. §3 (data flow), §5 (data model), §6 (execution and safety), §7 (UI), §9 (i18n), §11 (testing) bind this phase. Read §7 in full before Task 6.
- Backlog: `docs/superpowers/backlog.md`. Every item under "阶段 2（界面 / IPC）之前必须处理" is implemented by Task 1 or Task 4 of this plan and must not be deferred again.
- macOS only; minimum macOS 13.3; Tauri ≥ 2.11.1; universal build unchanged.
- `canager-core` must never depend on `tauri` and must never create a tokio runtime.
- No business logic in TypeScript. The front end never builds an argv, never decides whether something is safe to remove, and never guesses an outcome: it renders what the commands return.
- IPC only accepts known operations and server-issued object IDs (spec §6). A command's `program`/`args`/`env`/`locks` are constructed by the Rust side alone and are never accepted as values from the front end — see `plan_operation`/`submit_operation`'s server-issued, single-use `IssuedPlan`/`PlanId` in Core Interfaces below.
- Every destructive action shows the exact command that will run before it runs (spec §6). Uninstall additionally shows what would break.
- The app never asks for a password in the background; nothing in this phase triggers a privileged operation on a timer.
- UI copy ships in `en` and `zh-CN`. No user-visible string is hard-coded in a component; every one comes from the i18n resources. English is the default; the language follows the system unless overridden in Settings.
- Default view hides version numbers, paths and argv. The Settings toggle "Show technical details" reveals them. There is one layout, not two (spec §7: one UI, not a simple/advanced split).
- Visual rules (spec §7): system font stack, follows system light/dark, 8 pt spacing grid, macOS-style sidebar, virtualized long lists.
- Serde representation is fixed by the existing core types, which carry **no** `rename_all` attributes and whose insta snapshots depend on that. Do not add serde attributes to `crates/canager-core/src/model.rs` or `events.rs`. TypeScript types mirror the default representation exactly (see Core Interfaces below).
- Colors always use the `var(--color-*)` arbitrary-value syntax (e.g. `bg-[var(--color-muted)]`, `text-[var(--color-muted-foreground)]`); never a bare semantic utility class such as `bg-muted` or `text-foreground`, and never a hard-coded color literal in component code.
- Commit messages end with a blank line then `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>`.
- Definition of done for every task: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and (from Task 6 onward) `pnpm lint` where configured plus `pnpm test` all clean, with no new warnings.

---

## File Structure

```
crates/canager-core/src/
├── session/mod.rs        NEW  Session facade: adapters + instances + Snapshot(generation) + ops passthrough
├── settings.rs           NEW  Settings struct, atomic JSON load/save, defaults
├── ops/mod.rs            MOD  add OpSummary + OperationManager::summaries(); remove OpRecord.cancel
├── adapters/mod.rs       MOD  harden validate_package_name (reject path-like names)
├── adapters/brew/mod.rs  MOD  detect() distinguishes "refused because root" from "not installed"
└── lib.rs                MOD  pub mod session; pub mod settings;

src-tauri/src/
├── lib.rs                MOD  build AppState, register commands, set up the event bridge, drop `greet`
├── state.rs              NEW  AppState { session, settings_path, settings, channel registry }
├── ipc.rs                NEW  every #[tauri::command]
└── events.rs             NEW  ChannelSink: EventSink -> Tauri Channel; UiEvent enum

src-tauri/tauri.conf.json MOD  CSP, window title/size, no template leftovers

src/
├── main.tsx              MOD  QueryClientProvider + i18n bootstrap
├── App.tsx               REWRITE  app shell: sidebar + routed page + operation bar + log drawer
├── index.css             MOD  Tailwind import + design tokens (light/dark)
├── lib/
│   ├── types.ts          NEW  TS mirrors of the Rust DTOs (hand-written, exact)
│   ├── api.ts            NEW  typed invoke() wrappers, one function per command
│   ├── events.ts         NEW  useOperationEvents(): Channel subscription -> Zustand
│   ├── queries.ts        NEW  TanStack Query keys + hooks (useSnapshot, useSettings, ...)
│   └── format.ts         NEW  pure display helpers (size, relative time, badge text keys)
├── store/
│   └── ui.ts             NEW  Zustand: current page, selection, drawer open, live op logs
├── components/
│   ├── Sidebar.tsx       NEW  sections + counts
│   ├── ArtifactRow.tsx   NEW  one installed/updatable row
│   ├── OperationBar.tsx  NEW  bottom bar: running op, progress, cancel, open drawer
│   ├── LogDrawer.tsx     NEW  streaming log lines for the selected op
│   ├── UninstallDialog.tsx NEW  affected list + command preview + confirm
│   ├── CommandPreview.tsx  NEW  argv rendering (shown when technical details on, always in dialogs)
│   ├── EmptyState.tsx    NEW  no-brew / nothing-installed / refresh-failed
│   └── ui/               NEW  thin Radix wrappers: Dialog, Switch, Tabs, Tooltip, ScrollArea
├── pages/
│   ├── InstalledPage.tsx NEW
│   ├── UpdatesPage.tsx   NEW
│   └── SettingsPage.tsx  NEW
├── i18n/
│   ├── index.ts          NEW  i18next init, system language detection
│   ├── en.json           NEW
│   └── zh-CN.json        NEW
└── test/
    ├── setup.ts          NEW  vitest + jsdom + Testing Library setup, invoke mock
    └── *.test.tsx        NEW  per-component tests colocated by the tasks that add them
```

## Core Interfaces (authoritative — later tasks use these names and shapes verbatim)

### Rust: new core types

```rust
// crates/canager-core/src/settings.rs
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language { System, En, ZhCn }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub language: Language,
    pub show_technical_details: bool,
    pub ignored_updates: Vec<crate::model::ArtifactKey>,
}
impl Default for Settings {
    // Language::System, show_technical_details: false, ignored_updates: vec![]
}
/// Missing file, unreadable file or malformed JSON all yield `Settings::default()`
/// — settings are a convenience, never a reason to fail startup.
pub fn load(path: &Path) -> Settings;
/// Writes to `<path>.tmp` then renames, so a crash mid-write cannot corrupt it.
pub fn save(path: &Path, settings: &Settings) -> std::io::Result<()>;
```

```rust
// crates/canager-core/src/session/mod.rs
use crate::adapters::Adapter;
use crate::events::EventSink;
use crate::model::*;
use crate::ops::{OperationManager, OpSummary};
use crate::runner::HostEnv;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceError { pub instance_id: InstanceId, pub message: String }

/// Why an adapter reported no usable instance. `Missing` is the ordinary
/// "Homebrew is not installed" case; `RefusedAsRoot` must be surfaced
/// differently in the UI (spec §7 empty states).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectOutcome { Found, Missing, RefusedAsRoot }

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

/// Opaque handle to a plan `Session` has issued and is holding server-side.
/// The front end never constructs one; it only ever echoes back the `id` it
/// was given.
pub type PlanId = u64;

/// A `Plan` the server has already computed and stored, returned to the
/// caller for preview. Submitting requires only the `id`; the `plan` field
/// is for display (exact command preview, spec §6) and is never accepted
/// back from the client (see `Session::submit`).
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

pub struct Session { /* private */ }

impl Session {
    /// Registers the Homebrew adapter with a `RealRunner`. `now_fn` exists so
    /// tests can pin `refreshed_at`; production passes `None`.
    pub fn new(sink: Arc<dyn EventSink>, now_fn: Option<fn() -> i64>) -> Arc<Session>;
    /// Test seam: build a Session over arbitrary adapters.
    pub fn with_adapters(sink: Arc<dyn EventSink>, adapters: Vec<Arc<dyn Adapter>>, now_fn: Option<fn() -> i64>) -> Arc<Session>;

    /// Detect instances, then inventory + check updates for each. Bumps
    /// `generation` on every call that changes anything. Per-instance
    /// failures land in `errors` and set `stale`; they never abort the whole
    /// refresh. Concurrent calls are serialised; the second returns the
    /// snapshot produced by the first.
    pub async fn refresh(self: &Arc<Self>, env: &HostEnv) -> Snapshot;
    pub fn snapshot(&self) -> Snapshot;

    /// Resolves `req` to its owning adapter, asks it to plan the operation,
    /// then stores the resulting `Plan` server-side under a fresh `PlanId`
    /// and returns both as an `IssuedPlan` for the caller to preview. IPC
    /// accepts only known operations and server-issued object IDs (spec
    /// §6): nothing in the returned `Plan` is ever accepted back from the
    /// client — `submit` takes only the `PlanId`.
    pub async fn issue_plan(&self, req: &OpRequest) -> Result<IssuedPlan, crate::adapters::AdapterError>;
    /// Consumes (removes) the issued plan stored under `plan_id` and submits
    /// exactly that stored `Plan` — never one reconstructed from anything
    /// the caller supplied. Fails with `SubmitError::Unknown` if `plan_id`
    /// is not currently issued (never issued, or already submitted once),
    /// and `SubmitError::Expired` if it was issued more than 600 seconds
    /// ago (the caller must re-`issue_plan` to get a fresh preview).
    pub fn submit(self: &Arc<Self>, plan_id: PlanId) -> Result<OpId, SubmitError>;
    pub fn cancel(&self, op_id: OpId);
    pub fn operations(&self) -> Vec<OpSummary>;
}
```

```rust
// crates/canager-core/src/ops/mod.rs  (added)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpSummary {
    pub id: OpId,
    pub kind: OpKind,
    pub instance_id: InstanceId,
    pub artifact_kind: ArtifactKind,
    pub name: String,
    pub status: OpStatus,
    pub outcome: Option<Outcome>,
    pub argv_preview: Vec<String>,   // program followed by args
}
impl OperationManager {
    /// Newest first (descending op id).
    pub fn summaries(&self) -> Vec<OpSummary>;
}
```

### Rust: Tauri shell

```rust
// src-tauri/src/events.rs
// Deserialize too (not just Serialize): Task 6's tests decode a Channel's
// received body back into a UiEvent to assert on it. This is the shell's
// own type, not a core one, so it is exempt from the "no serde attributes
// on canager-core types" constraint above.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum UiEvent {
    Operation(canager_core::events::OperationEvent),
    SnapshotChanged { generation: u64 },
}
/// Fans every core event out to all registered Channels; a Channel whose
/// send fails (window closed) is dropped from the registry.
pub struct ChannelSink { /* private */ }
impl ChannelSink {
    pub fn new() -> Arc<ChannelSink>;
    pub fn register(&self, channel: tauri::ipc::Channel<UiEvent>);
    pub fn broadcast(&self, event: UiEvent);
}
impl canager_core::events::EventSink for ChannelSink { /* emit -> broadcast(UiEvent::Operation(..)) */ }
```

```rust
// src-tauri/src/ipc.rs — every command. All are async and take State<AppState>.
#[tauri::command] async fn get_snapshot(state: State<'_, AppState>) -> Result<Snapshot, String>;
#[tauri::command] async fn refresh(state: State<'_, AppState>) -> Result<Snapshot, String>;
#[tauri::command] async fn plan_operation(state: State<'_, AppState>, request: OpRequest) -> Result<IssuedPlan, String>;
#[tauri::command] async fn submit_operation(state: State<'_, AppState>, plan_id: u64) -> Result<u64, String>;
#[tauri::command] async fn cancel_operation(state: State<'_, AppState>, op_id: u64) -> Result<(), String>;
#[tauri::command] async fn list_operations(state: State<'_, AppState>) -> Result<Vec<OpSummary>, String>;
#[tauri::command] async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String>;
#[tauri::command] async fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String>;
#[tauri::command] async fn subscribe_events(state: State<'_, AppState>, channel: Channel<UiEvent>) -> Result<(), String>;
```

Errors cross IPC as `String` (the `Display` of `AdapterError` or `io::Error`). The front end shows them verbatim inside an error surface; it never parses them.

### JSON wire format (exact — TypeScript must match)

Core types carry no serde renames, so: **struct fields are snake_case**, **unit enum variants are bare strings**, **data-carrying variants are externally tagged objects**.

| Rust | JSON |
|---|---|
| `ArtifactKind::Formula` | `"Formula"` (also `"Cask" \| "Package" \| "Tool" \| "Model" \| "Binary"`) |
| `InstallReason::Requested` | `"Requested" \| "Dependency" \| "Unknown"` |
| `Scope::User` | `"User" \| "System"` |
| `OpKind::Install` | `"Install" \| "Uninstall" \| "Upgrade"` |
| `OpStatus::Running` | `"Queued" \| "Running" \| "CancelRequested" \| "Cancelling" \| "Verifying" \| "Done"` |
| `UpdateChannel::Native` | `"Native" \| "Registry" \| "Digest"` |
| `CancelPolicy::SafeKill` | `"SafeKill" \| "KillThenReconcile" \| "NoCancel"` |
| `Outcome::Succeeded` | `"Succeeded"`; also `"NoChange" \| "PartialSuccess" \| "Unconfirmed"` |
| `Outcome::NeedsAttention(String)` | `{"NeedsAttention": "…"}` |
| `Outcome::Failed { exit_code, summary }` | `{"Failed": {"exit_code": 1 \| null, "summary": "…"}}` |
| `ResourceLock(String)` | `"brew:/opt/homebrew"` (newtype = inner value) |
| `ArtifactKey` | `{"instance_id": "…", "kind": "Formula", "name": "…"}` |
| `IssuedPlan` | `{"id": 1, "plan": <Plan>, "issued_at": 1758000000}` |
| `OperationEvent::Status { op_id, status }` | `{"Status": {"op_id": 1, "status": "Running"}}` |
| `OperationEvent::Log { op_id, stream, line }` | `{"Log": {"op_id": 1, "stream": "Stdout", "line": "…"}}` |
| `OperationEvent::Finished { op_id, outcome }` | `{"Finished": {"op_id": 1, "outcome": "Succeeded"}}` |
| `UiEvent::Operation(e)` | `{"Operation": <OperationEvent>}` |
| `UiEvent::SnapshotChanged { generation }` | `{"SnapshotChanged": {"generation": 7}}` |
| `Language::ZhCn` | `"ZhCn"` (also `"System" \| "En"`) |
| `DetectOutcome::Found` | `"Found" \| "Missing" \| "RefusedAsRoot"` |

Any task that finds a mismatch between this table and the real serialization must fix the **TypeScript**, not the Rust, and say so in its report.

### TypeScript

```ts
// src/lib/types.ts — hand-written mirrors; no codegen in this phase.
export type ArtifactKind = "Formula" | "Cask" | "Package" | "Tool" | "Model" | "Binary";
export type InstallReason = "Requested" | "Dependency" | "Unknown";
export type OpKind = "Install" | "Uninstall" | "Upgrade";
export type OpStatus = "Queued" | "Running" | "CancelRequested" | "Cancelling" | "Verifying" | "Done";
export type DetectOutcome = "Found" | "Missing" | "RefusedAsRoot";
export type Outcome =
  | "Succeeded" | "NoChange" | "PartialSuccess" | "Unconfirmed"
  | { NeedsAttention: string }
  | { Failed: { exit_code: number | null; summary: string } };
export interface ArtifactKey { instance_id: string; kind: ArtifactKind; name: string }
export interface InstalledArtifact {
  key: ArtifactKey; display_name: string; version: string; reason: InstallReason;
  description: string | null; homepage: string | null; size_bytes: number | null;
  installed_at: number | null; path: string | null; auto_updates: boolean;
}
export interface UpdateCandidate {
  key: ArtifactKey; current: string; target: string;
  channel: "Native" | "Registry" | "Digest"; checkable: boolean; warnings: string[];
}
export interface ManagerInstance {
  id: string; adapter_id: string; exe_path: string; prefix: string;
  scope: "User" | "System"; version: string | null; healthy: boolean;
}
export interface Plan {
  request: OpRequest; program: string; args: string[]; env: [string, string][];
  needs_password: boolean; locks: string[];
  cancel_policy: "SafeKill" | "KillThenReconcile" | "NoCancel";
  warnings: string[]; affected: string[]; timeout_secs: number;
}
export interface OpRequest { kind: OpKind; instance_id: string; artifact_kind: ArtifactKind; name: string }
export interface IssuedPlan { id: number; plan: Plan; issued_at: number }
export interface OpSummary {
  id: number; kind: OpKind; instance_id: string; artifact_kind: ArtifactKind;
  name: string; status: OpStatus; outcome: Outcome | null; argv_preview: string[];
}
export interface SourceError { instance_id: string; message: string }
export interface Snapshot {
  generation: number; detect: DetectOutcome; instances: ManagerInstance[];
  artifacts: InstalledArtifact[]; updates: UpdateCandidate[];
  refreshed_at: number | null; stale: boolean; errors: SourceError[];
}
export type Language = "System" | "En" | "ZhCn";
export interface Settings {
  language: Language; show_technical_details: boolean;
  ignored_updates: ArtifactKey[];
}
export type OperationEvent =
  | { Status: { op_id: number; status: OpStatus } }
  | { Log: { op_id: number; stream: "Stdout" | "Stderr"; line: string } }
  | { Finished: { op_id: number; outcome: Outcome } };
export type UiEvent = { Operation: OperationEvent } | { SnapshotChanged: { generation: number } };
```

```ts
// src/lib/api.ts — one function per command, no other invoke() call sites anywhere.
export function getSnapshot(): Promise<Snapshot>;
export function refresh(): Promise<Snapshot>;
export function planOperation(request: OpRequest): Promise<IssuedPlan>;
export function submitOperation(planId: number): Promise<number>;
export function cancelOperation(opId: number): Promise<void>;
export function listOperations(): Promise<OpSummary[]>;
export function getSettings(): Promise<Settings>;
export function setSettings(settings: Settings): Promise<void>;
export function subscribeEvents(onEvent: (e: UiEvent) => void): Promise<() => void>;
```

```ts
// src/lib/queries.ts — TanStack Query is the only owner of server state.
export const queryKeys = {
  snapshot: ["snapshot"] as const,
  operations: ["operations"] as const,
  settings: ["settings"] as const,
};
export function useSnapshot(): UseQueryResult<Snapshot>;
export function useSettings(): UseQueryResult<Settings>;
export function useOperations(): UseQueryResult<OpSummary[]>;
export function useRefresh(): UseMutationResult<Snapshot, Error, void>;
export function useSaveSettings(): UseMutationResult<void, Error, Settings>;
/** Plans, shows nothing itself; callers render the IssuedPlan's Plan then submit its id. */
export function usePlanOperation(): UseMutationResult<IssuedPlan, Error, OpRequest>;
export function useSubmitOperation(): UseMutationResult<number, Error, number>;
export function useCancelOperation(): UseMutationResult<void, Error, number>;
```

```ts
// src/store/ui.ts — view state only. Never caches server data.
export type Page = "installed" | "updates" | "settings";
export interface LogLine { opId: number; stream: "Stdout" | "Stderr"; line: string; seq: number }
export interface UiState {
  page: Page; setPage(p: Page): void;
  query: string; setQuery(q: string): void;            // installed-page filter
  showDependencies: boolean; toggleDependencies(): void;
  drawerOpen: boolean; setDrawerOpen(open: boolean): void;
  focusedOpId: number | null; setFocusedOpId(id: number | null): void;
  /** Ring buffer, max 2000 lines total; oldest dropped first. */
  logs: LogLine[]; appendLog(l: Omit<LogLine, "seq">): void; clearLogs(opId: number): void;
  selectedUpdates: string[];                            // `${instance_id}|${kind}|${name}`
  toggleUpdate(key: ArtifactKey): void; clearSelectedUpdates(): void;
}
export const useUiStore: UseBoundStore<StoreApi<UiState>>;
export function artifactKeyId(key: ArtifactKey): string;  // `${instance_id}|${kind}|${name}`
```

## Task List

| # | Task | Deliverable |
|---|---|---|
| 1 | Harden `validate_package_name` | rejects path-like names: absolute paths, a leading `.`, a `..` segment anywhere, a trailing `.rb` |
| 2 | Distinguish root refusal from missing Homebrew | `BrewAdapter::refuses_as_root`; `detect()` uses it instead of re-deriving the euid-0 rule |
| 3 | `OpSummary` + `OperationManager::summaries()`; `OpRecord.cancel` removed | newest-first op list with argv preview; cancelling now only possible via `OperationManager::cancel` |
| 4 | Core `Settings` | atomic JSON load/save, corrupt file falls back to defaults |
| 5 | Core `Session` facade | `Snapshot` with generation/stale/errors; refresh serialised, per-instance resource-locked; issue_plan/submit (server-issued, single-use `PlanId`)/cancel/operations passthrough |
| 6 | `ChannelSink` and `UiEvent` | fans core `OperationEvent`s out to every registered Tauri `Channel`; drops a channel whose send fails |
| 7 | Tauri shell wiring (`AppState`, CSP, window config, remove template `greet`) | `AppState` built and managed, real CSP, template `greet` command and its front-end caller removed |
| 8 | Tauri IPC commands | all nine commands, each with a Rust test over `Session` |
| 9 | Front-end foundation | deps, Tailwind v4 tokens + dark mode (including `--color-muted-foreground`), app shell + sidebar, **i18next bootstrap with `en.json`** (so every later component uses `t()` from its first line), shared `Dialog`/`Switch` Radix wrappers, template files deleted, vitest harness |
| 10 | TS types + API client + event bridge | `types.ts`, `api.ts`, `events.ts`, `queries.ts`, `store/ui.ts`, with tests against a mocked `invoke` |
| 11 | Installed page | grouped by instance, dependencies collapsed, virtualized, filter box; `Sidebar` switched to the shared `Page` type |
| 12 | Updates page | per-item and multi-select update, ignore, "up to date" state, shared `CommandPreview` |
| 13 | Operation bar + log drawer | live status, streamed lines, working Cancel |
| 14 | Uninstall dialog | affected list, command preview, confirm disabled when something would break; wired into the Installed page's uninstall button so it is actually reachable |
| 15 | Settings page | technical details, language, greedy casks; persisted through IPC; wired into the app shell |
| 16 | i18n completeness | complete `zh-CN.json` covering every namespace through Task 15, system-language detection, Settings override wired, automated check that every `t()` key exists in both files and no component holds a literal user-visible string |
| 17 | Empty and error states | no Homebrew, root refusal, refresh failed (stale banner), nothing installed |
| 18 | Front-end tests in CI | `pnpm test` wired into `ci.yml`, green |

---

### Task 1: Harden `validate_package_name`

**Files:**
- Modify: `crates/canager-core/src/adapters/mod.rs:57-71` (harden `validate_package_name`), `:106-135` (add tests to the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `AdapterError::InvalidName` (existing, unchanged); the `Adapter` trait (unchanged — this task adds **no** new trait method).
- Produces: `validate_package_name(name: &str) -> Result<(), AdapterError>` — same signature, stricter body. All existing call sites (`BrewAdapter::search`, `BrewAdapter::plan`) are unaffected.

- [ ] **Step 1: Write the failing tests for hardened `validate_package_name`**

Add these five tests inside the existing `#[cfg(test)] mod tests` block at the bottom of `crates/canager-core/src/adapters/mod.rs` (after `test_validate_package_name_rejects_shell_metacharacters`):

```rust
    #[test]
    fn test_validate_package_name_rejects_an_absolute_path() {
        assert!(validate_package_name("/tmp/evil.rb").is_err());
    }

    #[test]
    fn test_validate_package_name_rejects_a_leading_dot() {
        assert!(validate_package_name(".hidden").is_err());
    }

    #[test]
    fn test_validate_package_name_rejects_a_dotdot_segment() {
        assert!(validate_package_name("foo/../evil").is_err());
        assert!(validate_package_name("../evil").is_err());
    }

    #[test]
    fn test_validate_package_name_rejects_an_rb_suffix() {
        assert!(validate_package_name("evil.rb").is_err());
        assert!(validate_package_name("some/tap/evil.rb").is_err());
    }

    #[test]
    fn test_validate_package_name_still_accepts_a_tap_qualified_cask_name() {
        assert!(validate_package_name("gautham-v/tap/claudebar").is_ok());
    }
```

- [ ] **Step 2: Run the new tests and confirm they fail**

Run: `cargo test -p canager-core --lib validate_package_name -- --nocapture`
Expected: FAIL — `test_validate_package_name_rejects_an_absolute_path`, `test_validate_package_name_rejects_a_leading_dot`, `test_validate_package_name_rejects_a_dotdot_segment` and `test_validate_package_name_rejects_an_rb_suffix` all fail with `assertion failed: validate_package_name(...).is_err()` (the current implementation accepts all four inputs); `test_validate_package_name_still_accepts_a_tap_qualified_cask_name` passes already.

- [ ] **Step 3: Harden `validate_package_name`**

Replace the function at `crates/canager-core/src/adapters/mod.rs:57-71` with:

```rust
/// Matches `^[A-Za-z0-9@._+/-]+$`, rejects names starting with `-`, `/` or
/// `.`, rejects a `..` path segment anywhere, and rejects a trailing `.rb`
/// (implemented by hand instead of pulling in the `regex` crate, since this
/// is the only place in the crate that needs pattern matching). The `/`,
/// leading-`.`, `..`-segment and `.rb`-suffix rules exist specifically so
/// `brew install --formula {name}` can never be handed a path: without them
/// `validate_package_name("/tmp/evil.rb")` — or a tap-relative
/// `"../../tmp/evil.rb"` — would pass, and Homebrew treats a `.rb`-suffixed
/// argument as a local formula file to load and run, not a formula name to
/// look up.
pub fn validate_package_name(name: &str) -> Result<(), AdapterError> {
    if name.is_empty()
        || name.starts_with('-')
        || name.starts_with('/')
        || name.starts_with('.')
        || name.ends_with(".rb")
        || name.split('/').any(|segment| segment == "..")
    {
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
```

- [ ] **Step 4: Run the tests again and confirm they pass**

Run: `cargo test -p canager-core --lib validate_package_name`
Expected: `test result: ok. 7 passed; 0 failed; ...` (the 2 pre-existing `validate_package_name` tests plus the 5 new ones)

- [ ] **Step 5: Commit**

```bash
git add crates/canager-core/src/adapters/mod.rs
git commit -m "$(cat <<'EOF'
fix(core): reject path-like package names in validate_package_name

brew install --formula /tmp/evil.rb (or a ..-relative equivalent) could
execute an arbitrary local formula file before this change; names must
now not start with / or ., not contain a .. segment, and not end in .rb.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Distinguish root refusal from missing Homebrew

**Files:**
- Modify: `crates/canager-core/src/adapters/brew/mod.rs:158-192` (extract `refuses_as_root`, use it in `detect()`), `:539-846` (add a test to the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `HostEnv { path_dirs, home, euid }` (existing, unchanged).
- Produces: `BrewAdapter::refuses_as_root(env: &HostEnv) -> bool` — new **inherent** function (not a trait method), the implementation choice for the backlog's "`detect()` distinguishes root refusal from missing" item. The `Adapter` trait gains nothing new and stays object-safe/stable. Task 5's `Session::refresh` cannot reach this method through a `dyn Adapter` (it's inherent to `BrewAdapter`, not virtual, so the trait object has no way to call it); instead `Session` calls `BrewAdapter::refuses_as_root(env)` directly, by its concrete type, wherever it needs to tell "refused as root" apart from "not installed". This function exists so the euid-0 rule is asserted and unit-tested exactly once, at the one adapter that currently enforces it, and both `detect()` and `Session::refresh` call this same function rather than each re-deriving the rule.

- [ ] **Step 1: Write the failing test for `BrewAdapter::refuses_as_root`**

Add this test inside the first `#[cfg(test)] mod tests` block in `crates/canager-core/src/adapters/brew/mod.rs` (the one starting at line 539, right after `test_detect_refuses_root`):

```rust
    #[test]
    fn test_refuses_as_root_is_true_only_for_euid_zero() {
        let root = HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/var/root"),
            euid: 0,
        };
        let user = HostEnv {
            path_dirs: vec![],
            home: PathBuf::from("/tmp"),
            euid: 501,
        };
        assert!(BrewAdapter::refuses_as_root(&root));
        assert!(!BrewAdapter::refuses_as_root(&user));
    }
```

- [ ] **Step 2: Run the test and confirm it fails to compile**

Run: `cargo test -p canager-core --lib refuses_as_root`
Expected: FAIL to compile — `error[E0599]: no function or associated item named `refuses_as_root` found for struct `BrewAdapter``

- [ ] **Step 3: Extract `refuses_as_root` and use it in `detect()`**

Replace the start of `detect` at `crates/canager-core/src/adapters/brew/mod.rs:158-161` (`pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> { if env.euid == 0 { return Vec::new(); }`) with:

```rust
    /// True when `env`'s effective UID means every brew invocation this
    /// adapter makes (`detect` included) will refuse to run. Callers use
    /// this — instead of re-deriving "euid 0 means root" themselves — to
    /// tell "Homebrew refused to run as root" apart from "Homebrew is not
    /// installed" when `detect()`'s returned `Vec` is empty either way;
    /// keeping the rule in exactly one place means it can never drift
    /// between call sites (this method and `Session::refresh`, added in a
    /// later plan, both call this function directly).
    pub fn refuses_as_root(env: &HostEnv) -> bool {
        env.euid == 0
    }

    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        if Self::refuses_as_root(env) {
            return Vec::new();
        }
```

- [ ] **Step 4: Run the test and confirm it passes, then confirm no regression**

Run: `cargo test -p canager-core --lib brew::`
Expected: `test result: ok. 27 passed; 0 failed; ...` (this filter matches both the `brew::tests::` module and the separate `brew::plan_execute_tests::` module in the same file — 26 pre-existing across the two, plus the new `test_refuses_as_root_is_true_only_for_euid_zero`), including `test_detect_refuses_root` and `test_detect_finds_opt_homebrew_on_this_apple_silicon_mac` still passing unchanged.

- [ ] **Step 5: Commit**

```bash
git add crates/canager-core/src/adapters/brew/mod.rs
git commit -m "$(cat <<'EOF'
refactor(core): extract BrewAdapter::refuses_as_root from detect()

Gives the euid-0 root-refusal rule a name and a direct unit test,
and is the seam the upcoming Session facade documents as the source
of truth for "refused as root" vs "not installed".

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: `OpSummary` + `OperationManager::summaries()`; `OpRecord.cancel` removed

**Files:**
- Modify: `crates/canager-core/src/ops/mod.rs:1-11` (imports), `:13-19` (delete `OpRecord.cancel` — the cancellation token stays on the private `OpInternal` only), `:82-94` (new `done_notify` field), `:96-107` (`new()`), `:121-129` (`record()` drops the now-removed field from the `OpRecord { ... }` it builds), `:154-166` (`wait()` rewritten on `Notify`), `:458-472` (`finish()` notifies waiters), plus a new `OpSummary` struct and `OperationManager::summaries()`
- Test: `crates/canager-core/tests/ops_summaries_test.rs` (new)

**Interfaces:**
- Consumes: `OperationManager::{new, register_adapter, register_instance, submit, wait}` (existing, unchanged).
- Produces:
  - `OpSummary { id: OpId, kind: OpKind, instance_id: InstanceId, artifact_kind: ArtifactKind, name: String, status: OpStatus, outcome: Option<Outcome>, argv_preview: Vec<String> }` and `OperationManager::summaries(&self) -> Vec<OpSummary>` (newest first) — used verbatim by Task 5's `Session::operations()` and Task 8's `list_operations` IPC command.
  - `OpRecord` no longer has a `cancel` field at all: the token it used to expose stays solely on the private `OpInternal`, which `record()`'s public `OpRecord` is built from. (Merely making the field private is not enough: `record()` would still construct it and nothing would ever read it, which is exactly the shape of a `dead_code` warning `-D warnings` rejects — removing the field is what actually fixes that.) External code (including the future IPC layer) must call `OperationManager::cancel(op_id)`; there is no longer any way to reach a token directly and cancel it out from under the status bookkeeping, which used to be able to skip the `CancelRequested` status update and event.

- [ ] **Step 1: Write the failing tests for `OperationManager::summaries()` and the `Notify`-based `wait()`**

Create `crates/canager-core/tests/ops_summaries_test.rs`:

```rust
use async_trait::async_trait;
use canager_core::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities};
use canager_core::events::{EventSink, OpId, VecSink};
use canager_core::model::{
    ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, ManagerInstance, OpKind, OpRequest,
    OpStatus, Outcome, Plan, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate,
};
use canager_core::ops::OperationManager;
use canager_core::runner::HostEnv;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

struct FakeAdapter {
    meta: AdapterMeta,
    /// When `gated` is set, `execute()` waits for `release.notified()`
    /// before returning instead of completing immediately. Lets a test hold
    /// an operation in `Running` for as long as it likes — with no reliance
    /// on real time — so it can register several `wait()` callers before
    /// choosing the exact moment the operation finishes. Unset by default,
    /// so every test that does not opt in still completes immediately.
    gated: std::sync::atomic::AtomicBool,
    release: Arc<tokio::sync::Notify>,
}

impl FakeAdapter {
    fn new() -> FakeAdapter {
        FakeAdapter {
            meta: AdapterMeta {
                id: "fake".to_string(),
                name: "fake".to_string(),
                kind: "fake".to_string(),
                platforms: vec!["macos".to_string()],
                homepage: "https://example.invalid".to_string(),
                schema_version: 1,
                verified_versions: vec![],
            },
            gated: std::sync::atomic::AtomicBool::new(false),
            release: Arc::new(tokio::sync::Notify::new()),
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
            args: vec!["install".to_string(), req.name.clone()],
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
        if self.gated.load(std::sync::atomic::Ordering::SeqCst) {
            self.release.notified().await;
        }
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
        version: None,
        healthy: true,
    }
}

fn make_request(name: &str, instance_id: &str) -> OpRequest {
    OpRequest {
        kind: OpKind::Install,
        instance_id: instance_id.to_string(),
        artifact_kind: ArtifactKind::Formula,
        name: name.to_string(),
    }
}

#[tokio::test]
async fn test_summaries_is_empty_before_anything_is_submitted() {
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    manager.register_adapter(Arc::new(FakeAdapter::new()));
    let manager = Arc::new(manager);
    assert!(manager.summaries().is_empty());
}

#[tokio::test]
async fn test_summaries_reflects_a_submitted_operation_and_its_argv_preview() {
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    let adapter = Arc::new(FakeAdapter::new());
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    let inst = make_instance("fake:1");
    manager.register_instance(inst.clone());

    let req = make_request("jq", "fake:1");
    let plan = adapter.plan(&inst, &req).await.expect("plan");
    let op_id = manager.submit(plan);
    let outcome = manager.wait(op_id).await;
    assert_eq!(outcome, Some(Outcome::Succeeded));

    let summaries = manager.summaries();
    assert_eq!(summaries.len(), 1);
    let summary = &summaries[0];
    assert_eq!(summary.id, op_id);
    assert_eq!(summary.kind, OpKind::Install);
    assert_eq!(summary.instance_id, "fake:1");
    assert_eq!(summary.artifact_kind, ArtifactKind::Formula);
    assert_eq!(summary.name, "jq");
    assert_eq!(summary.status, OpStatus::Done);
    assert_eq!(summary.outcome, Some(Outcome::Succeeded));
    assert_eq!(
        summary.argv_preview,
        vec![
            "/bin/true".to_string(),
            "install".to_string(),
            "jq".to_string()
        ]
    );
}

#[tokio::test]
async fn test_summaries_are_ordered_newest_first() {
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    let adapter = Arc::new(FakeAdapter::new());
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    let inst = make_instance("fake:1");
    manager.register_instance(inst.clone());

    let plan_a = adapter
        .plan(&inst, &make_request("aaa", "fake:1"))
        .await
        .unwrap();
    let id_a = manager.submit(plan_a);
    manager.wait(id_a).await;

    let plan_b = adapter
        .plan(&inst, &make_request("bbb", "fake:1"))
        .await
        .unwrap();
    let id_b = manager.submit(plan_b);
    manager.wait(id_b).await;

    let summaries = manager.summaries();
    assert_eq!(summaries.len(), 2);
    assert_eq!(
        summaries[0].id, id_b,
        "the more recently submitted op must come first"
    );
    assert_eq!(summaries[1].id, id_a);
}

#[tokio::test]
async fn test_wait_does_not_hang_after_finish() {
    // This only guards against an unbounded stall (e.g. a regression to a
    // dropped notification that never wakes `wait()` at all). It does
    // *not* prove the 20ms poll loop is gone — the old poll-based
    // implementation would pass this same assertion, just slower — so it
    // must not be read as a performance regression test. That property is
    // covered by `test_multiple_waiters_all_wake_once_the_op_finishes`
    // below, which uses a controlled synchronization point instead of a
    // wall-clock bound and so cannot flake under CI load the way tightening
    // this timeout would.
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    let adapter = Arc::new(FakeAdapter::new());
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    let inst = make_instance("fake:1");
    manager.register_instance(inst.clone());

    let plan = adapter
        .plan(&inst, &make_request("jq", "fake:1"))
        .await
        .unwrap();
    let op_id = manager.submit(plan);
    let outcome = tokio::time::timeout(Duration::from_millis(200), manager.wait(op_id))
        .await
        .expect("wait() should not hang");
    assert_eq!(outcome, Some(Outcome::Succeeded));
}

#[tokio::test]
async fn test_multiple_waiters_all_wake_once_the_op_finishes() {
    // Exercises the actual race `wait()`'s "create `notified()` before
    // checking status" ordering exists to prevent, with several concurrent
    // waiters instead of one. The synchronization is entirely deterministic
    // — a gate on `execute()` plus cooperative yielding, no sleeps or
    // timing thresholds — so this cannot be flaky under CI load the way a
    // tightened wall-clock bound would be.
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    let adapter = Arc::new(FakeAdapter::new());
    adapter.gated.store(true, std::sync::atomic::Ordering::SeqCst);
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    let inst = make_instance("fake:1");
    manager.register_instance(inst.clone());

    let plan = adapter
        .plan(&inst, &make_request("jq", "fake:1"))
        .await
        .unwrap();
    let op_id = manager.submit(plan);

    // Wait until `execute()` has actually been entered and is blocked on
    // the gate (status == Running), so there is no window in which the op
    // could finish before any waiter is spawned.
    loop {
        if manager.record(op_id).map(|r| r.status) == Some(OpStatus::Running) {
            break;
        }
        tokio::task::yield_now().await;
    }

    let waiters: Vec<_> = (0..5)
        .map(|_| {
            let manager = manager.clone();
            tokio::spawn(async move { manager.wait(op_id).await })
        })
        .collect();

    // Give every spawned waiter a chance to run up to its `notified().await`
    // point before the op is allowed to finish, so this test actually
    // exercises concurrent registered waiters rather than each one simply
    // observing an already-`Done` status.
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
    // `notify_one`, not `notify_waiters`: exactly one `execute()` call is
    // ever blocked on this gate, and `notify_one` (unlike `notify_waiters`)
    // stores its permit if `execute()` has not reached the await yet, so
    // this can never race the gate itself.
    adapter.release.notify_one();

    for w in waiters {
        assert_eq!(
            w.await.expect("waiter task panicked"),
            Some(Outcome::Succeeded)
        );
    }
}
```

- [ ] **Step 2: Run the new test file and confirm it fails to compile**

Run: `cargo test -p canager-core --test ops_summaries_test`
Expected: FAIL to compile — `error[E0599]: no method named `summaries` found for struct `OperationManager` in the current scope`

- [ ] **Step 3: Implement `OpSummary`, `OperationManager::summaries()`, remove `OpRecord.cancel`, and `Notify`-based `wait()`**

In `crates/canager-core/src/ops/mod.rs`, replace the imports at lines 1-11 with:

```rust
use crate::adapters::Adapter;
use crate::events::{EventSink, OpId, OperationEvent};
use crate::model::{
    AdapterId, ArtifactKey, ArtifactKind, InstanceId, ManagerInstance, OpKind, OpStatus, Outcome,
    Plan, ResourceLock,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;
```

Replace the `OpRecord` struct at lines 13-19 with:

```rust
pub struct OpRecord {
    pub id: OpId,
    pub plan: Plan,
    pub status: OpStatus,
    pub outcome: Option<Outcome>,
}
```

`record()` (lines 121-129) must be updated to match — drop the field it no longer builds:

```rust
    pub fn record(&self, op_id: OpId) -> Option<OpRecord> {
        let records = self.records.lock().unwrap();
        records.get(&op_id).map(|r| OpRecord {
            id: r.id,
            plan: r.plan.clone(),
            status: r.status,
            outcome: r.outcome.clone(),
        })
    }
```

Add this new, separate struct (it did not exist before this task):

```rust
/// A read-only view of one operation for a UI, independent of the
/// operation's own lifetime bookkeeping (`OpRecord`/`OpInternal`). Carries
/// exactly what a list of "current and recent operations" needs to render.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpSummary {
    pub id: OpId,
    pub kind: OpKind,
    pub instance_id: InstanceId,
    pub artifact_kind: ArtifactKind,
    pub name: String,
    pub status: OpStatus,
    pub outcome: Option<Outcome>,
    pub argv_preview: Vec<String>, // program followed by args
}
```

In the `OperationManager` struct (lines 82-94), add a field after `semaphore`:

```rust
    semaphore: Arc<Semaphore>,
    /// Fired every time any operation reaches `Done`, so `wait()` can react
    /// immediately instead of polling `records` every 20 ms. A single
    /// instance shared by every operation: `wait(op_id)` re-checks its own
    /// `op_id`'s status after every wake, so a notification meant for a
    /// different op just costs one extra, harmless status check.
    done_notify: Arc<Notify>,
```

In `OperationManager::new` (lines 96-107), add the field to the constructed value, after `semaphore: Arc::new(Semaphore::new(3)),`:

```rust
            semaphore: Arc::new(Semaphore::new(3)),
            done_notify: Arc::new(Notify::new()),
```

Replace `wait()` (lines 154-166) with:

```rust
    pub async fn wait(&self, op_id: OpId) -> Option<Outcome> {
        loop {
            // Register interest in the next notification *before* checking
            // `records`: `Notify::notified()`'s returned future remembers a
            // notification that lands between this line and the `.await`
            // below, so a `finish()` racing with this check can never be
            // missed — the lost-wakeup a naive "check, then await" would
            // have. This replaces the previous 20ms-poll implementation.
            let notified = self.done_notify.notified();
            {
                let records = self.records.lock().unwrap();
                match records.get(&op_id) {
                    Some(r) if r.status == OpStatus::Done => return r.outcome.clone(),
                    None => return None,
                    _ => {}
                }
            }
            notified.await;
        }
    }
```

Add `summaries()` to `impl OperationManager`, right after the `record()` method (which ends at line 130):

```rust
    /// Newest first (descending op id).
    pub fn summaries(&self) -> Vec<OpSummary> {
        let records = self.records.lock().unwrap();
        let mut summaries: Vec<OpSummary> = records
            .values()
            .map(|r| {
                let mut argv_preview = vec![r.plan.program.to_string_lossy().to_string()];
                argv_preview.extend(r.plan.args.iter().cloned());
                OpSummary {
                    id: r.id,
                    kind: r.plan.request.kind,
                    instance_id: r.plan.request.instance_id.clone(),
                    artifact_kind: r.plan.request.artifact_kind,
                    name: r.plan.request.name.clone(),
                    status: r.status,
                    outcome: r.outcome.clone(),
                    argv_preview,
                }
            })
            .collect();
        summaries.sort_by_key(|s| std::cmp::Reverse(s.id));
        summaries
    }
```

Finally, in `finish()` (lines 458-472), notify waiters right after the lock scope that marks the record `Done`:

```rust
    fn finish(&self, op_id: OpId, outcome: Outcome, release_locks: bool) {
        {
            let mut records = self.records.lock().unwrap();
            if let Some(r) = records.get_mut(&op_id) {
                r.status = OpStatus::Done;
                r.outcome = Some(outcome.clone());
                if release_locks {
                    if let Some(lr) = &r.lock_release {
                        lr.release_once();
                    }
                }
            }
        }
        self.done_notify.notify_waiters();
        self.sink.emit(OperationEvent::Finished { op_id, outcome });
    }
```

- [ ] **Step 4: Run the new test file and confirm it passes, then run the whole ops test surface**

Run: `cargo test -p canager-core --test ops_summaries_test`
Expected: `test result: ok. 5 passed; 0 failed; ...`

Run: `cargo test -p canager-core --test ops_cancel_test --test ops_lock_test --test ops_outcome_test --test ops_panic_test --test ops_semaphore_test`
Expected: every suite reports `test result: ok.` with the same pass counts as before this task (0 regressions) — `ops_outcome_test` `9 passed`, `ops_panic_test` `1 passed`, `ops_semaphore_test` `2 passed`, and `ops_cancel_test`/`ops_lock_test` with 0 failures across their scenarios.

- [ ] **Step 5: Commit**

```bash
git add crates/canager-core/src/ops/mod.rs crates/canager-core/tests/ops_summaries_test.rs
git commit -m "$(cat <<'EOF'
feat(core): add OperationManager::summaries, remove OpRecord.cancel

summaries() gives the future UI layer a Serialize-able list of current
and recent operations (status, outcome, argv preview), newest first.
OpRecord no longer carries a cancel field at all — the token stays on
the private OpInternal only — closing the bypass around cancel()'s
status bookkeeping, and wait() now wakes via tokio::sync::Notify
instead of a 20ms poll loop.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: Core `Settings`

**Files:**
- Create: `crates/canager-core/src/settings.rs`
- Modify: `crates/canager-core/src/lib.rs` (add `pub mod settings;`)
- Test: `crates/canager-core/src/settings.rs` (inline `#[cfg(test)] mod tests`, matching the crate's existing convention in `model.rs`/`events.rs`/`adapters/mod.rs`)

**Interfaces:**
- Consumes: `crate::model::ArtifactKey` (existing, unchanged).
- Produces (fixed verbatim by `docs/superpowers/plans/2026-09-19-phase-2-ui-shell.md`'s Core Interfaces section):
  - `Language { System, En, ZhCn }`
  - `Settings { language: Language, show_technical_details: bool, ignored_updates: Vec<ArtifactKey> }`, with `impl Default for Settings`
  - `load(path: &Path) -> Settings` — missing file, unreadable file, or malformed JSON all yield `Settings::default()`
  - `save(path: &Path, settings: &Settings) -> std::io::Result<()>` — writes to a `<path>.tmp.<n>` staging file, `n` a process-local auto-incrementing counter (so two concurrent `save()` calls to the same path can never write the same staging file out from under each other), then renames it over `path`

  These are consumed verbatim by Task 7's `AppState` and Task 8's `get_settings`/`set_settings` commands.

- [ ] **Step 1: Write the failing tests**

Create `crates/canager-core/src/settings.rs` with the module's `use` statements and its full test module, and nothing else yet (the types and functions the tests reference do not exist yet):

```rust
use crate::model::ArtifactKey;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ArtifactKind;
    use std::path::PathBuf;

    fn temp_settings_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "canager-settings-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn test_default_settings_are_the_documented_safe_defaults() {
        let settings = Settings::default();
        assert_eq!(settings.language, Language::System);
        assert!(!settings.show_technical_details);
        assert!(settings.ignored_updates.is_empty());
    }

    #[test]
    fn test_default_settings_serialize_with_no_renames() {
        // Guards the JSON wire-format contract the TypeScript mirror in
        // docs/superpowers/plans/2026-09-19-phase-2-ui-shell.md depends on:
        // plain snake_case field names, bare-string unit variants. A
        // `#[serde(rename_all = ...)]` added later would still round-trip
        // inside Rust but would silently break the front end.
        let json = serde_json::to_string(&Settings::default()).expect("serialize");
        assert!(json.contains("\"language\":\"System\""));
        assert!(json.contains("\"show_technical_details\":false"));
        assert!(json.contains("\"ignored_updates\":[]"));
    }

    #[test]
    fn test_load_of_a_missing_file_returns_defaults() {
        let path = temp_settings_path("missing");
        let _ = std::fs::remove_file(&path);
        assert_eq!(load(&path), Settings::default());
    }

    #[test]
    fn test_load_of_malformed_json_returns_defaults() {
        let path = temp_settings_path("malformed");
        std::fs::write(&path, b"{ not json").expect("write garbage");
        assert_eq!(load(&path), Settings::default());
        let _ = std::fs::remove_file(&path);
    }

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
        };
        save(&path, &settings).expect("save");
        let loaded = load(&path);
        assert_eq!(loaded, settings);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_save_writes_atomically_and_leaves_no_tmp_file_behind() {
        let path = temp_settings_path("atomic");
        save(&path, &Settings::default()).expect("save");
        // The staging file is named `<path>.tmp.<n>` (`n` a process-local
        // counter, so concurrent saves never collide on one fixed name) —
        // scan for any leftover `<file-name>.tmp.*` sibling rather than
        // checking one fixed `.tmp` path.
        let dir = path.parent().expect("path has a parent");
        let file_name = path.file_name().unwrap().to_string_lossy().into_owned();
        let leftover = std::fs::read_dir(dir)
            .expect("read temp dir")
            .filter_map(|e| e.ok())
            .any(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.starts_with(&format!("{file_name}.tmp."))
            });
        assert!(
            !leftover,
            "no <path>.tmp.<n> staging file may be left behind"
        );
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_save_creates_missing_parent_directory() {
        let dir = temp_settings_path("parent-dir");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("settings.json");
        save(&path, &Settings::default()).expect("save should create the parent dir");
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

Then add `pub mod settings;` to `crates/canager-core/src/lib.rs`, after `pub mod runner;`:

```rust
pub mod adapters;
pub mod events;
pub mod model;
pub mod ops;
pub mod runner;
pub mod settings;

pub use events::*;
pub use model::*;
```

- [ ] **Step 2: Run the tests and confirm they fail to compile**

Run: `cargo test -p canager-core --lib settings::`
Expected: FAIL to compile — multiple `error[E0412]: cannot find type `Settings` in this scope` / `cannot find type `Language` in this scope` / `error[E0425]: cannot find function `load`/`save` in this scope` (the module has tests but no implementation yet).

- [ ] **Step 3: Implement `Language`, `Settings`, `load`, and `save`**

Insert the implementation into `crates/canager-core/src/settings.rs`, between the `use` block and the `#[cfg(test)]` module:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    System,
    En,
    ZhCn,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub language: Language,
    pub show_technical_details: bool,
    pub ignored_updates: Vec<ArtifactKey>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            language: Language::System,
            show_technical_details: false,
            ignored_updates: Vec::new(),
        }
    }
}

/// Missing file, unreadable file or malformed JSON all yield
/// `Settings::default()` — settings are a convenience, never a reason to
/// fail startup.
pub fn load(path: &Path) -> Settings {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

/// Per-process counter for `save()`'s staging file name. A fixed `<path>.tmp`
/// would let two concurrent `save()` calls to the same path clobber each
/// other's staging file (one call's `write` landing in the middle of
/// another's, or one `rename` picking up the wrong writer's bytes); suffixing
/// each call's staging file with its own counter value makes that
/// impossible, regardless of how many callers race.
static SAVE_TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Writes to a `<path>.tmp.<n>` staging file, `n` unique to this call within
/// this process, then renames it over `path`, so a crash mid-write can never
/// leave a half-written, corrupt settings file in `path`'s place, and two
/// concurrent calls can never collide on the same staging file.
pub fn save(path: &Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let seq = SAVE_TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let mut tmp_os = path.as_os_str().to_os_string();
    tmp_os.push(format!(".tmp.{seq}"));
    let tmp_path = std::path::PathBuf::from(tmp_os);
    std::fs::write(&tmp_path, json)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p canager-core --lib settings::`
Expected: `test result: ok. 7 passed; 0 failed; ...`

- [ ] **Step 5: Run the full workspace definition-of-done check**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: `cargo fmt --all --check` prints nothing and exits 0; clippy ends with `Finished` and no warnings; the test run ends with every suite reporting `test result: ok.` (all previously-passing tests plus the new `settings::` tests).

- [ ] **Step 6: Commit**

```bash
git add crates/canager-core/src/settings.rs crates/canager-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(core): add file-backed Settings with atomic save and safe defaults

Settings::load never fails startup — a missing or corrupt settings
file just yields Settings::default() — and save() writes via a
<path>.tmp.<n> staging file (n a process-local counter) plus rename,
so a crash mid-write cannot corrupt it and concurrent saves can never
collide on one fixed staging file.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: Core `Session` facade

**Files:**
- Create: `crates/canager-core/src/session/mod.rs`
- Modify: `crates/canager-core/src/lib.rs` (insert `pub mod session;` before the `pub mod settings;` line Task 4 added, keeping the module list alphabetical)
- Modify: `crates/canager-core/src/ops/mod.rs` (add `OperationManager::acquire_resource_lock` + `ResourceLockGuard`, so `refresh` can share the real per-instance resource lock with in-flight operations — M8 in the design review)
- Test: `crates/canager-core/src/session/mod.rs` (inline `#[cfg(test)] mod tests`, with a self-contained `FakeAdapter` in the style of `crates/canager-core/tests/ops_cancel_test.rs`)

**Interfaces:**
- Consumes: the `Adapter` trait and `AdapterError` (`crate::adapters`, unchanged); `BrewAdapter::refuses_as_root` (`crate::adapters::brew`, Task 2) — called directly, by its concrete type, wherever `refresh` needs to tell "refused as root" apart from "not installed" (Task 2's Interfaces note now says this explicitly, closing N5 in the design review); `EventSink`, `OpId` (`crate::events`, unchanged); `AdapterId, InstalledArtifact, InstanceId, ManagerInstance, OpRequest, Plan, ResourceLock, UpdateCandidate` (`crate::model`, unchanged); `HostEnv` (`crate::runner`, unchanged); `OperationManager::{new, register_adapter, register_instance, submit, cancel}` (existing) and `OperationManager::summaries` / `OpSummary` (Task 3); `BrewAdapter::new` and `RealRunner::new` (existing, used only by `Session::new`'s production path).
- Produces (fixed verbatim by the skeleton's Core Interfaces section):
  - `SourceError { instance_id: InstanceId, message: String }`
  - `DetectOutcome { Found, Missing, RefusedAsRoot }`
  - `Snapshot { generation: u64, detect: DetectOutcome, instances: Vec<ManagerInstance>, artifacts: Vec<InstalledArtifact>, updates: Vec<UpdateCandidate>, refreshed_at: Option<i64>, stale: bool, errors: Vec<SourceError> }`
  - `PlanId` (= `u64`), `IssuedPlan { id: PlanId, plan: Plan, issued_at: i64 }`, `SubmitError { Unknown, Expired }` — the server-issued, single-use, expiring plan handle spec §6 requires (F1 in the design review): IPC must accept only known operations and server-issued object IDs, never a client-supplied `Plan`.
  - `Session::{new, with_adapters, refresh, snapshot, issue_plan, submit, cancel, operations}` with exactly the signatures in the skeleton. `issue_plan` is the only way a `Plan` is ever produced for a caller to see; `submit` accepts nothing but the opaque `PlanId` `issue_plan` handed out, looks up and removes the matching stored `Plan`, and submits exactly that — never anything reconstructed from caller-supplied data.
  - Also modifies `crates/canager-core/src/ops/mod.rs` (touched by Task 3, extended here): a new `OperationManager::acquire_resource_lock(self: &Arc<Self>, lock: ResourceLock) -> ResourceLockGuard` and a `pub struct ResourceLockGuard` (RAII; releases the lock on drop) that share the same `held` set `run_operation` already uses. This lets `refresh` (M8 in the design review) hold the *same* resource lock as an in-flight install/upgrade/uninstall for a given instance, so the two can never interleave on that instance while still running freely across different instances.
  - Not fixed by the skeleton (this task's own design decisions, needed to implement the documented contracts): a private `Snapshot::same_content` comparison used to decide whether `generation` bumps; a private `Session::commit` helper; a private `refresh_seq: AtomicU64` counter on `Session`, bumped every time a refresh completes regardless of whether its content changed (M5 in the design review — see Step 3's doc comment for why this must be separate from `generation`); and a private `issued_plans: Mutex<HashMap<PlanId, IssuedPlan>>` plus `next_plan_id: AtomicU64` on `Session`, backing `issue_plan`/`submit`. None of these are used outside this file.

- [ ] **Step 1: Write the failing tests**

Create `crates/canager-core/src/session/mod.rs` with its full production `use` block plus the complete test module (the types the tests reference — `Session`, `Snapshot`, `DetectOutcome`, `SourceError` — do not exist yet):

```rust
//! `Session`: the facade `canager-core` exposes to a host shell (the Tauri
//! app in this repo, or a test harness). It owns the registered adapters,
//! the last known set of instances, and an in-memory, generation-numbered
//! `Snapshot`; it forwards operation lifecycle calls to an internal
//! `OperationManager`. See
//! `docs/superpowers/plans/2026-09-19-phase-2-ui-shell.md`'s Core
//! Interfaces section — every name and shape here is fixed by that
//! document.

use crate::adapters::brew::BrewAdapter;
use crate::adapters::{Adapter, AdapterError};
use crate::events::{EventSink, OpId};
use crate::model::{
    AdapterId, InstalledArtifact, InstanceId, ManagerInstance, OpRequest, Plan, ResourceLock,
    UpdateCandidate,
};
use crate::ops::{OpSummary, OperationManager};
use crate::runner::HostEnv;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities};
    use crate::events::{EventSink, OpId, VecSink};
    use crate::model::{
        ArtifactKind, CancelPolicy, InstallReason, OpKind, OpStatus, Outcome, Reconciled,
        ResourceLock, Scope, SearchHit,
    };
    use async_trait::async_trait;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};
    use tokio_util::sync::CancellationToken;

    /// Controls one `FakeAdapter`'s behaviour so a single test can flip a
    /// source from healthy to failing mid-run, add an artificial `detect()`
    /// delay to observe refresh coalescing, or block `execute()` on
    /// cancellation. Not shared with any other test file's `FakeAdapter`.
    struct FakeState {
        instances: Vec<ManagerInstance>,
        artifacts: HashMap<InstanceId, Vec<InstalledArtifact>>,
        updates: HashMap<InstanceId, Vec<UpdateCandidate>>,
        /// Instance ids whose `inventory` should fail on the *next* call
        /// only (consumed on use).
        failing: Vec<InstanceId>,
        detect_delay: Duration,
        detect_calls: usize,
        block_execute: bool,
        /// Every instance id `inventory()` was actually called for, in
        /// call order — used by `test_refresh_is_mutually_exclusive_...`
        /// to observe that one instance's fetch proceeded while another's
        /// was still blocked on a resource lock (M8 in the design review).
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
        let snapshot = session.refresh(&non_root_env()).await;
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
        let snapshot = session.refresh(&root_env()).await;
        assert_eq!(snapshot.detect, DetectOutcome::RefusedAsRoot);
        assert!(snapshot.instances.is_empty());
        assert_eq!(
            state.lock().unwrap().detect_calls,
            0,
            "no adapter should be probed while running as root"
        );
    }

    #[tokio::test]
    async fn test_refresh_with_no_instances_yields_missing() {
        let (adapter, _state) = FakeAdapter::new("fake");
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let snapshot = session.refresh(&non_root_env()).await;
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
        let first = session.refresh(&non_root_env()).await;
        assert_eq!(first.artifacts.len(), 2);
        assert!(!first.stale);
        let first_refreshed_at = first.refreshed_at;

        state.lock().unwrap().failing.push("fake:1".to_string());
        let second = session.refresh(&non_root_env()).await;
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
        let first = session.refresh(&non_root_env()).await;
        let second = session.refresh(&non_root_env()).await;
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
        let third = session.refresh(&non_root_env()).await;
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
            tokio::spawn(async move { session_a.refresh(&non_root_env()).await }),
            tokio::spawn(async move { session_b.refresh(&non_root_env()).await }),
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
        // Regression guard for M5 in the design review: `generation` only
        // advances when content actually changes, so by itself it cannot
        // tell "another refresh already completed while I waited for the
        // gate" apart from "no refresh has run since I last checked" — two
        // refreshes back to back that both see identical data must still
        // coalesce into one `detect()` call, not run the adapters twice.
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.artifacts
                .insert("fake:1".to_string(), vec![make_artifact("fake:1", "jq")]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        // Establish a steady-state snapshot first, outside any concurrency.
        session.refresh(&non_root_env()).await;
        let calls_before = state.lock().unwrap().detect_calls;

        // Every refresh from here on sees exactly the same data as above,
        // so `generation` will not advance no matter how many times it
        // runs — that must not be mistaken for "no refresh has happened".
        state.lock().unwrap().detect_delay = Duration::from_millis(100);
        let session_a = session.clone();
        let session_b = session.clone();
        let (a, b) = tokio::join!(
            tokio::spawn(async move { session_a.refresh(&non_root_env()).await }),
            tokio::spawn(async move { session_b.refresh(&non_root_env()).await }),
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
        let refreshed = session.refresh(&non_root_env()).await;
        let after = session.snapshot();
        assert_eq!(after, refreshed);
    }

}
```

Then add `pub mod session;` to `crates/canager-core/src/lib.rs`, before the `pub mod settings;` line Task 4 added (keeping the module list alphabetical):

```rust
pub mod adapters;
pub mod events;
pub mod model;
pub mod ops;
pub mod runner;
pub mod session;
pub mod settings;

pub use events::*;
pub use model::*;
```

This has to happen now, not later: `session/mod.rs` must actually be part of the crate for Step 2's compile failure to be the real "these types don't exist" error, rather than the test filter silently matching zero tests because the file was never compiled at all (M4 in the design review).

Deliberately **not yet included above**: any test that calls `issue_plan`, `submit`, `cancel` or `operations`. Once `mod session;` makes this file part of the crate, a single missing method anywhere in the test module fails the whole compile unit, so no test in it — including these already-correct refresh/snapshot ones — could report a pass if such a call were present now. They are written in Step 6, after this half is fully green and committed.

- [ ] **Step 2: Run the tests and confirm they fail to compile**

Run: `cargo test -p canager-core --lib session::`
Expected: FAIL to compile — `error[E0412]: cannot find type `Session`/`Snapshot`/`DetectOutcome`/`SourceError` in this scope` (the module is now part of the crate via `pub mod session;`, so this is a real compile failure — the types simply do not exist yet; Step 3 implements them).

- [ ] **Step 3: Implement `SourceError`, `DetectOutcome`, `Snapshot`, `PlanId`, `IssuedPlan`, `SubmitError`, `Session::{new, with_adapters, refresh, snapshot}`, and `OperationManager::acquire_resource_lock`**

First, add this to `crates/canager-core/src/ops/mod.rs` (extending Task 3's edits to this file) — a way for a caller other than `run_operation` to hold one of the same resource locks an operation holds, so `refresh` (below) can never interleave with an install/upgrade/uninstall on the same instance (M8 in the design review):

```rust
/// Held while `refresh` is fetching one instance's inventory/updates, over
/// the *same* `held` set `run_operation`'s locks use. Releases on drop, the
/// same idempotent-by-construction shape as the internal `LockGuard`.
pub struct ResourceLockGuard {
    held: Arc<Mutex<HashSet<ResourceLock>>>,
    lock: ResourceLock,
}

impl Drop for ResourceLockGuard {
    fn drop(&mut self) {
        self.held.lock().unwrap().remove(&self.lock);
    }
}

impl OperationManager {
    /// Waits (polling every 50ms, the same cadence `run_operation` already
    /// uses for its own lock-wait loop) until `lock` is free, then holds it
    /// until the returned guard drops. `refresh` uses this to take the same
    /// per-instance lock a submitted install/upgrade/uninstall holds, so the
    /// two can never read/write that instance's filesystem state at once —
    /// while a *different* instance's lock is untouched, so refreshing one
    /// instance never waits on an operation running against another.
    pub async fn acquire_resource_lock(self: &Arc<Self>, lock: ResourceLock) -> ResourceLockGuard {
        loop {
            {
                let mut held = self.held.lock().unwrap();
                if !held.contains(&lock) {
                    held.insert(lock.clone());
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        ResourceLockGuard {
            held: self.held.clone(),
            lock,
        }
    }
}
```

Now insert this into `crates/canager-core/src/session/mod.rs`, between the `use` block and the `#[cfg(test)]` module:

```rust
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

    /// Whether `self` and `other` carry the same *data* — every field
    /// except `generation`, `refreshed_at` and `stale`, which describe the
    /// refresh attempt rather than the fetched data itself.
    ///
    /// Deliberately excludes `refreshed_at`: comparing the *full* struct
    /// (as the design review's M6 suggested) would mean `generation` bumps
    /// on every successful refresh, since `refreshed_at` changes every
    /// time — at which point `generation` stops meaning "the content
    /// changed" and a front end watching it for that reason gets bumped on
    /// every poll for no visible reason. `generation` keeps that meaning by
    /// design; `refresh_seq` below (M5) is the separate counter that
    /// actually solves the concurrent-refresh-coalescing problem M6's
    /// suggestion was trying to fix.
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
/// back from the client (see `Session::submit`).
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
    /// Serialises `refresh()`: whichever caller acquires this first does
    /// the real work; anyone already waiting when it releases just re-reads
    /// `snapshot` (see `refresh`'s doc comment for the exact protocol).
    refresh_gate: tokio::sync::Mutex<()>,
    snapshot: Mutex<Snapshot>,
    /// Bumped every time a refresh actually completes, regardless of
    /// whether its content — and therefore `generation` — changed (M5 in
    /// the design review). `generation` alone cannot tell a waiter "someone
    /// else already finished a refresh while I waited for the gate" apart
    /// from "no one has run since I last checked": two refreshes in a row
    /// can fetch identical data, in which case `generation` does not move
    /// even though a real refresh happened. `refresh_seq` always moves, so
    /// it is what `refresh` actually checks to decide whether to coalesce.
    refresh_seq: AtomicU64,
    /// Plans handed out by `issue_plan` but not yet consumed by `submit`,
    /// keyed by `PlanId`. `submit` removes its entry on use, so each plan
    /// can be submitted at most once; an entry older than 600 seconds is
    /// rejected as expired instead of being proactively swept, since this
    /// only grows by one entry per preview an operator actually looks at.
    issued_plans: Mutex<HashMap<PlanId, IssuedPlan>>,
    next_plan_id: AtomicU64,
    now_fn: Option<fn() -> i64>,
}

impl Session {
    /// Registers the Homebrew adapter with a `RealRunner`. `now_fn` exists
    /// so tests can pin `refreshed_at`; production passes `None`.
    pub fn new(sink: Arc<dyn EventSink>, now_fn: Option<fn() -> i64>) -> Arc<Session> {
        let brew = Arc::new(BrewAdapter::new(Arc::new(crate::runner::RealRunner::new())));
        Session::with_adapters(sink, vec![brew], now_fn)
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

    /// Detect instances, then inventory + check updates for each instance
    /// concurrently (each under that instance's own resource lock — see
    /// below). Bumps `generation` only when the resulting data actually
    /// differs from the previous snapshot. Per-instance failures land in
    /// `errors` and set `stale`; they never abort the whole refresh, and a
    /// failing instance's *previous* artifacts/updates are kept rather than
    /// dropped, so a transient failure never makes something the user
    /// installed appear to vanish. Concurrent calls are serialised: a call
    /// that starts while another is already running waits for it, then
    /// returns the snapshot that other call produced instead of running a
    /// second, redundant refresh — see `refresh_seq` on `Session` for why
    /// that check cannot use `generation`.
    pub async fn refresh(self: &Arc<Self>, env: &HostEnv) -> Snapshot {
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

        if BrewAdapter::refuses_as_root(env) {
            let refused = Snapshot {
                generation: previous.generation,
                detect: DetectOutcome::RefusedAsRoot,
                instances: Vec::new(),
                artifacts: Vec::new(),
                updates: Vec::new(),
                refreshed_at: previous.refreshed_at,
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
                    match adapter.check_updates(&inst).await {
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

    /// Assigns the real generation number (bumping only on a content
    /// change), stores the result as the current snapshot, and marks this
    /// refresh complete via `refresh_seq` regardless of whether `generation`
    /// moved (M5 in the design review — see `refresh_seq`'s field doc).
    fn commit(&self, previous: Snapshot, mut candidate: Snapshot) -> Snapshot {
        if !previous.same_content(&candidate) {
            candidate.generation = previous.generation + 1;
        }
        *self.snapshot.lock().unwrap() = candidate.clone();
        self.refresh_seq.fetch_add(1, Ordering::SeqCst);
        candidate
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap().clone()
    }
}
```

- [ ] **Step 4: Run the refresh/snapshot tests and confirm they pass**

Run: `cargo test -p canager-core --lib session::`
Expected: `test result: ok. 8 passed; 0 failed; ...` — every test written in Step 1 (`refresh`, `snapshot`, and the M5 unchanged-content-coalescing regression test) genuinely compiles and passes; nothing in this module yet references `issue_plan`/`submit`/`cancel`/`operations`, so there is no half-compiling state to describe.

- [ ] **Step 5: Commit the refresh/snapshot half**

```bash
git add crates/canager-core/src/session/mod.rs crates/canager-core/src/lib.rs crates/canager-core/src/ops/mod.rs
git commit -m "$(cat <<'EOF'
feat(core): add Session::{new,with_adapters,refresh,snapshot}

Session owns the registered adapters and a generation-numbered
Snapshot. refresh() serialises concurrent callers, keeps a failing
instance's previous data instead of dropping it, and only bumps the
generation when the fetched data actually changed. issue_plan/submit/
cancel/operations land in the next commit.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 6: Write the failing tests for `Session::{issue_plan, submit, cancel, operations}`**

Append these tests to the `#[cfg(test)] mod tests` block in `crates/canager-core/src/session/mod.rs`, after `test_snapshot_returns_cached_value_without_calling_adapters`:

```rust
    #[tokio::test]
    async fn test_issue_plan_delegates_to_the_owning_adapter() {
        let (adapter, state) = FakeAdapter::new("fake");
        state.lock().unwrap().instances = vec![make_instance("fake", "fake:1")];
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session.refresh(&non_root_env()).await;
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = session.issue_plan(&req).await.expect("issue_plan");
        assert_eq!(issued.id, 1, "PlanId numbering starts at 1");
        assert_eq!(issued.plan.args, vec!["do".to_string(), "jq".to_string()]);
    }

    #[tokio::test]
    async fn test_issue_plan_for_unknown_instance_is_refused() {
        let (adapter, _state) = FakeAdapter::new("fake");
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "does-not-exist".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        match session.issue_plan(&req).await {
            Err(AdapterError::Refused(_)) => {}
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_submit_cancel_and_operations_forward_to_the_operation_manager() {
        let (adapter, state) = FakeAdapter::new("fake");
        {
            let mut s = state.lock().unwrap();
            s.instances = vec![make_instance("fake", "fake:1")];
            s.block_execute = true;
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session.refresh(&non_root_env()).await;
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

    #[tokio::test]
    async fn test_refresh_is_mutually_exclusive_with_an_operation_on_the_same_instance_but_not_others(
    ) {
        // Regression guard for M8 in the design review: refresh() must take
        // the same per-instance resource lock a submitted operation holds,
        // so it can never observe fake:1's filesystem state while an
        // install/upgrade/uninstall on fake:1 is still running — but that
        // must not hold up fake:2's fetch, which uses a different lock.
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
        session.refresh(&non_root_env()).await;
        state.lock().unwrap().inventory_calls.clear();

        // Submit (and thereby lock) an operation against fake:1 only, and
        // hold it there — `block_execute` makes `execute()` wait on
        // cancellation — until this test releases it below.
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
            tokio::spawn(async move { session_for_refresh.refresh(&non_root_env()).await });

        // Give the refresh time to reach fake:2's inventory (no contention)
        // and to *try* fake:1's (which must still be waiting on the lock
        // fake:1's running operation holds).
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

        // Release fake:1's lock; the refresh (and the operation) must now
        // both complete, and the snapshot must reflect both instances.
        session.cancel(op_id);
        let snapshot = tokio::time::timeout(Duration::from_secs(2), refresh_task)
            .await
            .expect("refresh must not hang once the blocking operation is cancelled")
            .expect("refresh task panicked");
        assert!(snapshot.artifacts.iter().any(|a| a.key.name == "jq"));
        assert!(snapshot.artifacts.iter().any(|a| a.key.name == "wget"));
    }
```

- [ ] **Step 7: Run the tests and confirm they fail to compile**

Run: `cargo test -p canager-core --lib session::`
Expected: FAIL to compile — `error[E0599]: no method named `issue_plan`/`submit`/`cancel`/`operations` found for struct `Session`` (Step 4's 8 tests still compile fine on their own; it is only these 4 new ones that reference methods that do not exist yet — and per M4 in the design review, that failure now applies to the whole compile unit, so re-running Step 4's `cargo test` command at this point would report the same failure, not a partial pass).

- [ ] **Step 8: Implement `Session::{issue_plan, submit, cancel, operations}`**

Append these methods to `impl Session` in `crates/canager-core/src/session/mod.rs`, after `snapshot`. This is F1 in the design review: IPC must accept only known operations and server-issued object IDs, never a client-supplied `Plan` — `issue_plan` is the only place a `Plan` is ever computed, and `submit` accepts nothing but the opaque `PlanId` it handed out, looks the stored plan up by that id, removes it (one-time use), and submits exactly what was stored:

```rust
    /// Resolves `req` to its owning adapter, asks it to plan the operation,
    /// then stores the resulting `Plan` under a fresh `PlanId` and returns
    /// both as an `IssuedPlan`. The caller previews `issued.plan`; nothing
    /// in it is ever accepted back — `submit` takes only `issued.id`.
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
            AdapterError::Refused(format!(
                "no adapter registered for {}",
                instance.adapter_id
            ))
        })?;
        let plan = adapter.plan(&instance, req).await?;
        let id = self.next_plan_id.fetch_add(1, Ordering::SeqCst);
        let issued = IssuedPlan {
            id,
            plan,
            issued_at: self.now(),
        };
        self.issued_plans.lock().unwrap().insert(id, issued.clone());
        Ok(issued)
    }

    /// Removes (one-time consumption) the issued plan stored under
    /// `plan_id` and submits exactly that stored `Plan`. Fails with
    /// `SubmitError::Unknown` if `plan_id` was never issued or was already
    /// submitted once, and `SubmitError::Expired` if it was issued more
    /// than 600 seconds ago — the client can never influence what actually
    /// runs, since nothing it sends is used except this opaque id.
    pub fn submit(self: &Arc<Self>, plan_id: PlanId) -> Result<OpId, SubmitError> {
        let issued = {
            let mut plans = self.issued_plans.lock().unwrap();
            plans.remove(&plan_id).ok_or(SubmitError::Unknown)?
        };
        if self.now() - issued.issued_at > 600 {
            return Err(SubmitError::Expired);
        }
        Ok(self.ops.submit(issued.plan))
    }

    pub fn cancel(&self, op_id: OpId) {
        self.ops.cancel(op_id)
    }

    pub fn operations(&self) -> Vec<OpSummary> {
        self.ops.summaries()
    }
```

- [ ] **Step 9: Run the full session test module and confirm everything passes**

Run: `cargo test -p canager-core --lib session::`
Expected: `test result: ok. 12 passed; 0 failed; ...`

- [ ] **Step 10: Run the full workspace definition-of-done check**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: `cargo fmt --all --check` prints nothing and exits 0; clippy ends with `Finished` and no warnings; `cargo test --workspace` reports `test result: ok.` for every suite, including the new `session::` tests, with 0 regressions in `adapters::`, `brew::`, `ops::`, `settings::`, and every `tests/*.rs` integration file.

- [ ] **Step 11: Commit**

```bash
git add crates/canager-core/src/session/mod.rs crates/canager-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(core): add Session::{issue_plan,submit,cancel,operations}

Completes the Session facade: issue_plan() resolves an OpRequest's
instance id to its owning adapter, plans it, and stores the resulting
Plan server-side under a fresh PlanId; submit(plan_id) consumes that
stored plan exactly once and forwards it to OperationManager. IPC (and
any caller) can therefore never hand back a Plan of its own — only the
opaque id this crate issued (spec §6: known operations and
server-issued object IDs only). cancel/operations are thin, tested
passthroughs to OperationManager. canager-core now exposes everything
the Tauri shell needs without depending on tauri.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: `ChannelSink` and `UiEvent`

**Files:**
- Create: `src-tauri/src/events.rs`
- Test: `src-tauri/src/events.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `canager_core::events::{EventSink, OperationEvent}` (existing).
- Produces: `src-tauri/src/events.rs`: `UiEvent { Operation(canager_core::events::OperationEvent), SnapshotChanged { generation: u64 } }` and `ChannelSink::{new, register, broadcast}` implementing `canager_core::events::EventSink`, matching the skeleton exactly. **Verified against the vendored `tauri` 2.11.5 source** (`~/.cargo/registry/src/.../tauri-2.11.5/src/ipc/channel.rs`): `tauri::ipc::Channel<TSend>::new<F: Fn(tauri::ipc::InvokeResponseBody) -> tauri::Result<()> + Send + Sync + 'static>(on_message: F) -> Self` can construct a `Channel` directly with no live webview, and `send(&self, data: TSend) -> tauri::Result<()> where TSend: IpcResponse` is available for any `TSend: Serialize` via a blanket impl. This is what makes `ChannelSink` fully unit-testable below. Task 7's `AppState` holds an `Arc<ChannelSink>` built from `ChannelSink::new()`; Task 8's `subscribe_events` command calls `ChannelSink::register`.
- **Unverified by this task's tests (N2 in the design review):** `test_broadcast_removes_a_channel_from_the_registry_after_its_send_fails` below only proves that `broadcast` drops a channel once its `send` returns an error — it manufactures that error directly (a closure returning `Err`) and cannot say anything about when, or whether, a *real* closed Tauri window actually makes `Channel::send` fail, nor about behavior across a window reload or a duplicate subscription. Confirm the real timing manually against a running app once one exists (Task 7 onward) and record the result in this task's completion report; do not read passing unit tests here as proof of real window-close behavior.

- [ ] **Step 1: Write the failing tests for `ChannelSink`**

Create `src-tauri/src/events.rs` with its `use` block and test module only (no production code yet):

```rust
use canager_core::events::EventSink;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;

#[cfg(test)]
mod tests {
    use super::*;
    use canager_core::events::OperationEvent;
    use canager_core::model::OpStatus;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_broadcast_delivers_to_every_registered_channel() {
        let sink = ChannelSink::new();
        let received_a: Arc<Mutex<Vec<UiEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let received_b: Arc<Mutex<Vec<UiEvent>>> = Arc::new(Mutex::new(Vec::new()));

        let ra = received_a.clone();
        let channel_a: Channel<UiEvent> = Channel::new(move |body| {
            let event: UiEvent = body.deserialize().expect("deserialize UiEvent");
            ra.lock().unwrap().push(event);
            Ok(())
        });
        let rb = received_b.clone();
        let channel_b: Channel<UiEvent> = Channel::new(move |body| {
            let event: UiEvent = body.deserialize().expect("deserialize UiEvent");
            rb.lock().unwrap().push(event);
            Ok(())
        });

        sink.register(channel_a);
        sink.register(channel_b);
        sink.broadcast(UiEvent::SnapshotChanged { generation: 7 });

        assert_eq!(received_a.lock().unwrap().len(), 1);
        assert_eq!(received_b.lock().unwrap().len(), 1);
        // Bind the guard first, then match on it, and terminate the match
        // with a semicolon: matching directly on `&received_a.lock().unwrap()[0]`
        // as this function's tail expression makes the temporary `MutexGuard`
        // outlive the match (its drop is deferred to the end of the
        // enclosing statement, which here is the whole function body),
        // which rustc rejects with E0597 ("borrowed value does not live
        // long enough") since Rust 2021.
        let events = received_a.lock().unwrap();
        match &events[0] {
            UiEvent::SnapshotChanged { generation } => assert_eq!(*generation, 7),
            other => panic!("expected SnapshotChanged, got {other:?}"),
        };
    }

    #[test]
    fn test_broadcast_removes_a_channel_from_the_registry_after_its_send_fails() {
        // This manufactures the send failure directly; it does not — and,
        // from a unit test with no real webview, cannot — prove anything
        // about when (or whether) a real closed window actually makes
        // Channel::send fail. See this task's Interfaces note (N2 in the
        // design review): that must be confirmed by hand later, against a
        // running app.
        let sink = ChannelSink::new();
        let ok_count = Arc::new(AtomicUsize::new(0));

        let failing: Channel<UiEvent> = Channel::new(|_body| -> tauri::Result<()> {
            Err(tauri::Error::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "simulated send failure, e.g. from a closed window",
            )))
        });
        let oc = ok_count.clone();
        let healthy: Channel<UiEvent> = Channel::new(move |_body| {
            oc.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });

        sink.register(failing);
        sink.register(healthy);

        sink.broadcast(UiEvent::SnapshotChanged { generation: 1 });
        assert_eq!(ok_count.load(Ordering::SeqCst), 1);
        assert_eq!(
            sink.channels.lock().unwrap().len(),
            1,
            "the failing channel must be dropped"
        );

        sink.broadcast(UiEvent::SnapshotChanged { generation: 2 });
        assert_eq!(ok_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_emit_wraps_operation_events_as_ui_event_operation() {
        let sink = ChannelSink::new();
        let received: Arc<Mutex<Vec<UiEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let r = received.clone();
        let channel: Channel<UiEvent> = Channel::new(move |body| {
            let event: UiEvent = body.deserialize().expect("deserialize UiEvent");
            r.lock().unwrap().push(event);
            Ok(())
        });
        sink.register(channel);

        EventSink::emit(
            sink.as_ref(),
            OperationEvent::Status {
                op_id: 1,
                status: OpStatus::Running,
            },
        );

        let events = received.lock().unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            UiEvent::Operation(OperationEvent::Status { op_id, status }) => {
                assert_eq!(*op_id, 1);
                assert_eq!(*status, OpStatus::Running);
            }
            other => panic!("expected Operation(Status), got {other:?}"),
        }
    }
}
```

Then add `mod events;` to `src-tauri/src/lib.rs`, right above the existing `#[tauri::command] fn greet...` line (it will be removed in Step 6, but this keeps the module declared while both exist momentarily). This must happen now, not after the implementation exists: the module has to actually be part of the crate for the compile failure in Step 2 below to be the real one (unresolved names), rather than the test filter silently matching zero tests because the file was never compiled at all (M4 in the design review):

```rust
mod events;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
```

- [ ] **Step 2: Run the tests and confirm they fail to compile**

Run: `cargo test -p canager --lib events::`
Expected: FAIL to compile — `error[E0433]: failed to resolve: use of undeclared type `ChannelSink`` and `error[E0412]: cannot find type `UiEvent` in this scope` (the module is now part of the crate via `mod events;`, so this is a real compile failure — the referenced types simply do not exist yet; Step 3 implements them).

- [ ] **Step 3: Implement `UiEvent` and `ChannelSink`**

Insert into `src-tauri/src/events.rs`, between the `use` block and the `#[cfg(test)]` module:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum UiEvent {
    Operation(canager_core::events::OperationEvent),
    SnapshotChanged { generation: u64 },
}

/// Fans every core event out to all registered Channels; a Channel whose
/// send fails (window closed) is dropped from the registry.
pub struct ChannelSink {
    channels: Mutex<Vec<Channel<UiEvent>>>,
}

impl ChannelSink {
    pub fn new() -> Arc<ChannelSink> {
        Arc::new(ChannelSink {
            channels: Mutex::new(Vec::new()),
        })
    }

    pub fn register(&self, channel: Channel<UiEvent>) {
        self.channels.lock().unwrap().push(channel);
    }

    pub fn broadcast(&self, event: UiEvent) {
        let mut channels = self.channels.lock().unwrap();
        channels.retain(|c| c.send(event.clone()).is_ok());
    }
}

impl EventSink for ChannelSink {
    fn emit(&self, event: canager_core::events::OperationEvent) {
        self.broadcast(UiEvent::Operation(event));
    }
}
```

(`mod events;` was already added to `src-tauri/src/lib.rs` in Step 1, so there is nothing left to wire in here.)

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p canager --lib events::`
Expected: `test result: ok. 3 passed; 0 failed; ...`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/events.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(shell): add ChannelSink, a canager_core::EventSink over Tauri Channels

Fans every core OperationEvent out to all subscribed webviews as
UiEvent::Operation, and drops a channel whose send fails (closed
window). Unit-tested directly against tauri::ipc::Channel::new, with
no live webview needed.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: Tauri shell wiring (`AppState`, CSP, window config, remove template `greet`)

**Files:**
- Create: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/lib.rs` (drop the template `greet` command, build `AppState` in `.setup(...)`, register it with `.manage(...)`)
- Modify: `src-tauri/tauri.conf.json` (real CSP)
- Modify: `src/App.tsx` (remove the `greet` `invoke()` call and the template UI it drove — the rest of the front-end rewrite is Task 9's)
- Test: `src-tauri/src/state.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `canager_core::session::Session::new` (Task 5), `canager_core::settings::{self, Settings}` (Task 4), `ChannelSink::new` (Task 6).
- Produces: `src-tauri/src/state.rs`: `AppState { session: Arc<Session>, settings_path: PathBuf, settings: Mutex<Settings>, channel_sink: Arc<ChannelSink> }`, `AppState::new(settings_path, channel_sink) -> AppState`, `AppState::get_settings(&self) -> Settings`, `AppState::set_settings(&self, new: Settings) -> std::io::Result<()>`. `get_settings`/`set_settings` are not fixed by the skeleton (which only names the struct's shape); they exist so Task 8's `get_settings`/`set_settings` commands have real logic to call instead of reaching into the struct's fields directly from `ipc.rs`. `set_settings` holds `settings`'s mutex for its entire save-then-update-memory sequence rather than just the final assignment (M7 in the design review): two overlapping calls could otherwise finish with disk holding one caller's settings and memory holding the other's. The whole method stays synchronous (no `.await` inside), so holding a `std::sync::Mutex` guard across it is safe.

- [ ] **Step 1: Write the failing tests for `AppState`**

Create `src-tauri/src/state.rs` with its `use` block and test module only:

```rust
use crate::events::ChannelSink;
use canager_core::session::Session;
use canager_core::settings::{self, Settings};
use std::path::PathBuf;
use std::sync::Mutex;

#[cfg(test)]
mod tests {
    use super::*;
    use canager_core::model::{ArtifactKey, ArtifactKind};
    use std::sync::Arc;

    fn temp_settings_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "canager-appstate-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn test_new_loads_defaults_when_settings_file_is_missing() {
        let path = temp_settings_path("missing");
        let _ = std::fs::remove_file(&path);
        let state = AppState::new(path, ChannelSink::new());
        assert_eq!(state.get_settings(), Settings::default());
    }

    #[test]
    fn test_set_settings_persists_and_updates_the_in_memory_copy() {
        let path = temp_settings_path("roundtrip");
        let _ = std::fs::remove_file(&path);
        let state = AppState::new(path.clone(), ChannelSink::new());
        let mut new_settings = Settings::default();
        new_settings.show_technical_details = true;
        state
            .set_settings(new_settings.clone())
            .expect("set_settings");
        assert_eq!(state.get_settings(), new_settings);
        assert_eq!(settings::load(&path), new_settings);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_new_builds_a_working_session() {
        let path = temp_settings_path("session");
        let _ = std::fs::remove_file(&path);
        let state = AppState::new(path, ChannelSink::new());
        // A freshly built Session has an empty, generation-0 snapshot until
        // something calls refresh() — proves `session` is a real, usable
        // Session rather than left unconstructed.
        assert_eq!(state.session.snapshot().generation, 0);
    }

    #[test]
    fn test_concurrent_set_settings_calls_leave_disk_and_memory_consistent() {
        // Regression guard for M7 in the design review: set_settings used
        // to save to disk unlocked and only briefly lock memory for the
        // final assignment, so two overlapping calls could finish with
        // disk holding one caller's settings and memory holding the
        // other's. The whole save-then-update sequence is now one critical
        // section, so no matter how many threads race here, whichever
        // write actually lands on disk last must also be the one left in
        // memory.
        let path = temp_settings_path("concurrent");
        let _ = std::fs::remove_file(&path);
        let state = Arc::new(AppState::new(path.clone(), ChannelSink::new()));

        let handles: Vec<_> = (0..8u32)
            .map(|i| {
                let state = state.clone();
                std::thread::spawn(move || {
                    let settings = Settings {
                        show_technical_details: i % 2 == 0,
                        ignored_updates: vec![ArtifactKey {
                            instance_id: "brew:/opt/homebrew".to_string(),
                            kind: ArtifactKind::Formula,
                            name: format!("pkg-{i}"),
                        }],
                        ..Settings::default()
                    };
                    state.set_settings(settings).expect("set_settings");
                })
            })
            .collect();
        for h in handles {
            h.join().expect("writer thread panicked");
        }

        let on_disk = settings::load(&path);
        let in_memory = state.get_settings();
        assert_eq!(
            on_disk, in_memory,
            "whichever write actually landed on disk must also be the one left in memory"
        );
        let _ = std::fs::remove_file(&path);
    }
}
```

Then add `mod state;` to `src-tauri/src/lib.rs`, next to `mod events;`:

```rust
mod events;
mod state;
```

This has to happen now, not after `AppState` exists: `state.rs` must actually be part of the crate for Step 2's compile failure to be the real "the type doesn't exist" error, rather than the test filter silently matching zero tests because the file was never compiled at all (M4 in the design review).

- [ ] **Step 2: Run the tests and confirm they fail to compile**

Run: `cargo test -p canager --lib state::`
Expected: FAIL to compile — `error[E0433]: failed to resolve: use of undeclared type `AppState`` (the module is now part of the crate via `mod state;`, so this is a real compile failure — the struct simply does not exist yet; Step 3 implements it).

- [ ] **Step 3: Implement `AppState`**

Insert into `src-tauri/src/state.rs`, between the `use` block and the `#[cfg(test)]` module:

```rust
pub struct AppState {
    pub session: std::sync::Arc<Session>,
    pub settings_path: PathBuf,
    pub settings: Mutex<Settings>,
    pub channel_sink: std::sync::Arc<ChannelSink>,
}

impl AppState {
    /// Loads settings from `settings_path` (falling back to defaults per
    /// `canager_core::settings::load`'s contract) and builds a `Session`
    /// wired to `channel_sink` as its event sink.
    pub fn new(settings_path: PathBuf, channel_sink: std::sync::Arc<ChannelSink>) -> AppState {
        let loaded = settings::load(&settings_path);
        let session = Session::new(channel_sink.clone(), None);
        AppState {
            session,
            settings_path,
            settings: Mutex::new(loaded),
            channel_sink,
        }
    }

    pub fn get_settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    /// Persists `new_settings` to disk, then updates the in-memory copy —
    /// holding `settings`'s lock across *both*, not just the final
    /// assignment (M7 in the design review). Without this, two overlapping
    /// calls could each save to disk unlocked and then briefly lock memory
    /// only for the assignment, letting them interleave into "disk holds
    /// caller B's settings, memory holds caller A's": e.g. A saves, pauses;
    /// B saves (disk now B) and updates memory (memory now B); A resumes
    /// and updates memory (memory now A) — disk and memory now disagree
    /// even though both calls "succeeded". Holding the lock for the whole
    /// method serialises the two callers instead, so whichever one's write
    /// actually lands on disk last is also the one left in memory. This
    /// stays synchronous throughout (no `.await` inside), so holding a
    /// `std::sync::Mutex` guard across it is safe.
    pub fn set_settings(&self, new_settings: Settings) -> std::io::Result<()> {
        let mut settings = self.settings.lock().unwrap();
        settings::save(&self.settings_path, &new_settings)?;
        *settings = new_settings;
        Ok(())
    }
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p canager --lib state::`
Expected: `test result: ok. 4 passed; 0 failed; ...`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/state.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(shell): add AppState wrapping Session and file-backed Settings

get_settings/set_settings are plain, directly-testable methods so the
IPC commands added next can stay thin adapters over State<AppState>.
set_settings holds one lock across its entire save-then-update-memory
sequence so concurrent callers can never leave disk and the in-memory
copy disagreeing about which write actually won.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 6: Wire `AppState` into the app, remove the template `greet` command, set the real CSP, and drop the front end's `greet` caller**

Replace the whole of `src-tauri/src/lib.rs` with:

```rust
mod events;
mod state;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if fix_path_env::fix().is_err() {
        eprintln!("[canager] failed to fix PATH; falling back to the process's default PATH");
    }
    let host_env = canager_core::runner::HostEnv::discover();
    println!("[canager] discovered PATH dirs: {:?}", host_env.path_dirs);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let settings_path = app.path().app_data_dir()?.join("settings.json");
            let channel_sink = events::ChannelSink::new();
            app.manage(AppState::new(settings_path, channel_sink));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

(`.invoke_handler(...)` is intentionally left out here — there are no `#[tauri::command]`s left after `greet` is removed. Task 8 adds it back with the real nine commands.)

In `src-tauri/tauri.conf.json`, replace `"csp": null` with a real policy:

```json
    "security": {
      "csp": "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: asset: https://asset.localhost; connect-src 'self'"
    }
```

This omits `'unsafe-inline'` for `style-src`. That choice is only justified for the **packaged production build**: Tailwind v4's Vite plugin (`@tailwindcss/vite`, added in Task 9) compiles Tailwind's utility classes into a static stylesheet loaded via a build-time `<link>`, not injected through runtime `<style>` tags, and no front-end component code exists yet at this point in the plan that sets inline `style="..."` attributes. If a later task's Radix UI components (Tasks 9–14 use Popover/Tooltip/DropdownMenu/Select, which position themselves via inline `style` attributes at runtime) turn out to be blocked by this policy once the packaged app actually runs under system WebKit, that task must add `'unsafe-inline'` to `style-src` at that point and say why in its own commit — do not add it here pre-emptively.

**Development mode is a separate case this reasoning does not cover** (N3 in the design review): `tauri dev` serves the front end from Vite's own dev server, which injects CSS through its HMR pipeline — typically `<style>` tags written into the document at runtime, not a build-time `<link>` — and Tauri's `csp` setting applies in dev too. Whether `style-src 'self'` (no `'unsafe-inline'`, no nonce/hash) breaks Vite's dev-time style injection or hot reload is a real open question this task's static edits cannot answer; neither `cargo build` nor `tsc` executes the app or a webview, so neither can confirm CSP is actually compatible with real styling, IPC, or Channel traffic in either mode — they only prove the code compiles and `tauri.conf.json` is syntactically valid JSON tauri-build accepts. See Step 7 for the explicit manual checks this requires.

Replace the whole of `src/App.tsx` (dropping the `greet` invoke call, its form, and the template logos — Task 9 replaces the rest of this file's content with the real app shell):

```tsx
import "./App.css";

function App() {
  return (
    <main className="container">
      <h1 className="text-2xl font-bold">Canager</h1>
    </main>
  );
}

export default App;
```

- [ ] **Step 7: Run and confirm both the Rust and the front-end sides still build, then manually verify the CSP against real styling and IPC**

Run: `cargo build -p canager 2>&1 | tail -20`
Expected: ends with `Finished` and no errors (the `greet` command and its `invoke_handler` registration are gone, `AppState` is built and managed in `.setup`, and `tauri.conf.json`'s new CSP parses — `tauri-build` re-parses `tauri.conf.json` at compile time and would fail the build on invalid JSON or an unrecognised CSP shape). This only proves the CSP string is syntactically acceptable to `tauri-build`; it says nothing about whether real styling, hot reload, or IPC actually work under it.

Run: `pnpm exec tsc -p tsconfig.json`
Expected: no output, exit code 0 (confirms `App.tsx` has no unused imports left — `tsconfig.json` has `noUnusedLocals`/`noUnusedParameters` enabled — and still type-checks). This is a type check only; it does not run the app and proves nothing about CSP compatibility either.

Neither command above executes the app in a real webview, so neither can confirm the CSP change is actually compatible with styling or IPC (N3 in the design review). This task's report must instead record the result of these manual checks:
- **Dev mode** (`pnpm tauri dev` or equivalent): confirm the window renders with real styles, not an unstyled page, and that editing a Tailwind class or `src/index.css` hot-reloads into the running window without a manual restart. This is the check most at risk from the stricter `style-src`, since Vite's dev-time CSS injection is not the same code path as the production build's static stylesheet.
- **Packaged build** (`pnpm tauri build`, run the resulting bundle): confirm the window renders the compiled Tailwind stylesheet correctly. `invoke()` and Channel communication cannot be exercised yet at this point in the plan — there are no `#[tauri::command]`s registered until Task 8 — so re-run this same manual check once Task 8's commands and Task 9's front end exist, and record the result there instead of assuming it here.

If either manual check fails, that is a defect in this task's CSP choice to fix in this task, not a deferred concern.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/tauri.conf.json src/App.tsx
git commit -m "$(cat <<'EOF'
feat(shell): manage AppState, set a real CSP, drop the template greet command

Removes the create-tauri-app leftovers flagged in
docs/superpowers/backlog.md's pre-phase-2 list: the greet command,
its front-end caller, and the null CSP. App.tsx is left as a minimal
placeholder; Task 9 rewrites it into the real app shell.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: Tauri IPC commands

**Files:**
- Create: `src-tauri/src/ipc.rs`
- Modify: `src-tauri/src/lib.rs` (declare `mod ipc;` and register all nine commands via `.invoke_handler(tauri::generate_handler![...])`)
- Modify: `src-tauri/Cargo.toml` (add `[dev-dependencies]` for `async-trait`, `tokio`, `tokio-util` — M2 in the design review)
- Test: `src-tauri/src/ipc.rs` (inline `#[cfg(test)] mod tests`, with a self-contained `FakeAdapter`)

**Interfaces:**
- Consumes: `AppState` (Task 7); `Session::{issue_plan, submit, cancel, operations}`, `Snapshot`, `DetectOutcome`, `IssuedPlan`, `PlanId`, `SubmitError` (Task 5); `OpSummary` (Task 3); `Settings` (Task 4); `ChannelSink::{register, broadcast}`, `UiEvent` (Task 6); `canager_core::model::OpRequest` (existing); `canager_core::runner::HostEnv` (existing).
- Produces: all nine `#[tauri::command]` functions with exactly the signatures fixed by the skeleton (`get_snapshot`, `refresh`, `plan_operation`, `submit_operation`, `cancel_operation`, `list_operations`, `get_settings`, `set_settings`, `subscribe_events`) — `plan_operation` returns `IssuedPlan` and `submit_operation` takes only `plan_id: u64` (F1 in the design review: IPC accepts only known operations and server-issued object IDs, never a client-supplied `Plan`). Each is a **thin, ≤2-line adapter** over a plain, non-`#[tauri::command]` `..._impl` function that takes `&AppState` directly — introduced by this task specifically so the real logic is unit-testable, since `tauri::State<'_, T>` wraps a private field and cannot be constructed outside the `tauri` crate (verified against the vendored source: `pub struct State<'r, T: Send + Sync + 'static>(&'r T);` in `~/.cargo/registry/src/.../tauri-2.11.5/src/state.rs` has no public constructor). These `..._impl` functions (`get_snapshot_impl`, `refresh_impl`, `plan_operation_impl`, `submit_operation_impl`, `cancel_operation_impl`, `list_operations_impl`, `get_settings_impl`, `set_settings_impl`, `subscribe_events_impl`) are not named by the skeleton and are private to this crate (`pub(crate)`); no other task depends on their names. `refresh_impl` also broadcasts `UiEvent::SnapshotChanged` on `state.channel_sink` whenever the refreshed snapshot's `generation` differs from the one before the call (M9 in the design review — this is the only production code path in the whole plan that ever sends `SnapshotChanged`; every other mention of it up to this task is in a test).

- [ ] **Step 1: Add the test-only dependencies this task's tests need**

Task 8's tests are the first in `src-tauri` to use `#[tokio::test]`, `async_trait`, and `tokio_util::sync::CancellationToken`. `canager-core` depends on `tokio`, `tokio-util` and `async-trait`, but a dependency of `canager-core` is not usable directly from the `canager` (src-tauri) crate — each crate must declare what it imports itself (M2 in the design review). Add to `src-tauri/Cargo.toml`:

```toml
[dev-dependencies]
async-trait = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "time", "sync"] }
tokio-util = { version = "0.7", features = ["rt"] }
```

This has to land before Step 2's test file is written: without it, `use async_trait::async_trait;` and `#[tokio::test]` fail to resolve the moment `mod ipc;` makes the file part of the crate — before ever reaching the "`..._impl` not found" errors Step 3 is meant to produce.

- [ ] **Step 2: Write the failing tests for all nine command implementations**

Create `src-tauri/src/ipc.rs` with its `use` block and full test module (the `..._impl` functions and command wrappers referenced by the tests do not exist yet):

```rust
use crate::events::UiEvent;
use crate::state::AppState;
use canager_core::model::OpRequest;
use canager_core::ops::OpSummary;
use canager_core::runner::HostEnv;
use canager_core::session::{IssuedPlan, Snapshot};
use canager_core::settings::Settings;
use tauri::ipc::Channel;
use tauri::State;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ChannelSink;
    use async_trait::async_trait;
    use canager_core::adapters::{Adapter, AdapterError, AdapterMeta, Capabilities};
    use canager_core::events::{EventSink, OpId, OperationEvent};
    use canager_core::model::{
        ArtifactKey, ArtifactKind, CancelPolicy, InstalledArtifact, ManagerInstance, OpKind, Plan,
        Outcome, Reconciled, ResourceLock, Scope, SearchHit, UpdateCandidate,
    };
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    struct FakeAdapter {
        meta: AdapterMeta,
        instance: ManagerInstance,
        /// How many times `execute()` actually ran. Used only by the
        /// plan-rejection tests below to prove a rejected `submit` never
        /// reaches the runner (F1 in the design review); every other test
        /// in this module ignores it.
        execute_calls: Arc<AtomicUsize>,
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
            self.execute_calls.fetch_add(1, Ordering::SeqCst);
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

    fn temp_settings_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "canager-ipc-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn state_with_fake_adapter() -> AppState {
        let (state, _execute_calls) = state_with_fake_adapter_and_now(None);
        state
    }

    /// Like `state_with_fake_adapter`, but also returns a counter of how
    /// many times the fake adapter's `execute()` actually ran, and accepts
    /// an injectable clock — needed only by the plan-rejection tests below,
    /// which must prove a rejected `submit` never reaches the runner and
    /// must simulate a plan aging past its expiry window (F1 in the design
    /// review). `state_with_fake_adapter` above delegates to this with
    /// `None`, so there is exactly one place that builds this fixture.
    ///
    /// `session` and `channel_sink` share the *same* `ChannelSink` (N1 in
    /// the design review): the two used to be built from separate
    /// `ChannelSink::new()` calls, which meant a real operation's events —
    /// emitted into `session`'s sink — could never reach a Channel
    /// registered through `AppState.channel_sink`, and no test caught it
    /// because every test only ever broadcast directly on `channel_sink`
    /// rather than checking that a *real* operation's events arrive.
    fn state_with_fake_adapter_and_now(
        now_fn: Option<fn() -> i64>,
    ) -> (AppState, Arc<AtomicUsize>) {
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
        let adapter: Arc<dyn Adapter> = Arc::new(FakeAdapter {
            meta,
            instance,
            execute_calls: execute_calls.clone(),
        });
        let sink = ChannelSink::new();
        let session =
            canager_core::session::Session::with_adapters(sink.clone(), vec![adapter], now_fn);
        let state = AppState {
            session,
            settings_path: temp_settings_path("appstate"),
            settings: std::sync::Mutex::new(Settings::default()),
            channel_sink: sink,
        };
        (state, execute_calls)
    }

    #[tokio::test]
    async fn test_get_snapshot_impl_returns_the_sessions_current_snapshot() {
        let state = state_with_fake_adapter();
        let snapshot = get_snapshot_impl(&state).expect("get_snapshot_impl");
        assert_eq!(snapshot.generation, 0);
    }

    #[tokio::test]
    async fn test_refresh_impl_detects_the_fake_instance() {
        // `refresh_impl` calls `HostEnv::discover()` internally (there is no
        // way to inject an env from the command layer — the real command
        // never has one to inject either), so this test relies on the test
        // process itself not running as root, same as every other
        // integration test in this workspace that calls real `detect()`
        // logic.
        let state = state_with_fake_adapter();
        let snapshot = refresh_impl(&state).await.expect("refresh_impl");
        assert_eq!(snapshot.instances.len(), 1);
        assert_eq!(snapshot.instances[0].id, "fake:1");
    }

    #[tokio::test]
    async fn test_plan_operation_impl_delegates_to_session_issue_plan() {
        let state = state_with_fake_adapter();
        refresh_impl(&state).await.expect("refresh_impl");
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = plan_operation_impl(&state, req)
            .await
            .expect("plan_operation_impl");
        assert_eq!(issued.plan.args, vec!["do".to_string(), "jq".to_string()]);
    }

    #[tokio::test]
    async fn test_plan_operation_impl_maps_session_error_to_string() {
        let state = state_with_fake_adapter();
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "does-not-exist".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let err = plan_operation_impl(&state, req)
            .await
            .expect_err("expected an error for an unknown instance");
        assert!(
            err.contains("does-not-exist"),
            "error string should name the unknown instance, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_submit_and_list_and_cancel_operations_impl_round_trip() {
        let state = state_with_fake_adapter();
        refresh_impl(&state).await.expect("refresh_impl");

        // N1 in the design review: state_with_fake_adapter now wires
        // `session` and `channel_sink` to the *same* ChannelSink, so a real
        // subscriber registered here — through the same `subscribe_events_impl`
        // path the real `subscribe_events` command uses — proves that
        // wiring is actually connected end to end, not merely that
        // ChannelSink::broadcast works when called directly on a sink no
        // Session ever emits into.
        let received: Arc<std::sync::Mutex<Vec<UiEvent>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let r = received.clone();
        let channel: Channel<UiEvent> = Channel::new(move |body| {
            let event: UiEvent = body.deserialize().expect("deserialize UiEvent");
            r.lock().unwrap().push(event);
            Ok(())
        });
        subscribe_events_impl(&state, channel).expect("subscribe_events_impl");

        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = plan_operation_impl(&state, req)
            .await
            .expect("plan_operation_impl");
        let op_id = submit_operation_impl(&state, issued.id).expect("submit_operation_impl");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let summaries = list_operations_impl(&state).expect("list_operations_impl");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, op_id);

        cancel_operation_impl(&state, op_id).expect("cancel_operation_impl on a finished op");

        let events = received.lock().unwrap();
        assert!(
            events.iter().any(|e| matches!(
                e,
                UiEvent::Operation(OperationEvent::Status { op_id: id, .. }) if *id == op_id
            )),
            "the real operation's Status events must reach a subscriber through AppState.channel_sink"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                UiEvent::Operation(OperationEvent::Finished { op_id: id, .. }) if *id == op_id
            )),
            "the real operation's Finished event must reach a subscriber through AppState.channel_sink"
        );
    }

    #[tokio::test]
    async fn test_submit_operation_impl_rejects_an_unissued_plan_id() {
        // F1 in the design review: submit must accept only a server-issued
        // PlanId, never anything the caller invents — including a
        // tampered or forged id nothing ever issued. The runner must never
        // be reached.
        let (state, execute_calls) = state_with_fake_adapter_and_now(None);
        refresh_impl(&state).await.expect("refresh_impl");
        let err = submit_operation_impl(&state, 999_999)
            .expect_err("an unissued plan id must be rejected");
        assert!(
            err.contains("no such plan"),
            "expected the Unknown-plan error, got: {err}"
        );
        assert_eq!(
            execute_calls.load(Ordering::SeqCst),
            0,
            "a rejected submit must never reach the runner"
        );
    }

    #[tokio::test]
    async fn test_submit_operation_impl_rejects_the_same_plan_id_submitted_twice() {
        // F1: each issued plan is single-use. Resubmitting the same id —
        // e.g. a replayed IPC call — must be rejected the second time, not
        // silently run the operation again.
        let (state, execute_calls) = state_with_fake_adapter_and_now(None);
        refresh_impl(&state).await.expect("refresh_impl");
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = plan_operation_impl(&state, req)
            .await
            .expect("plan_operation_impl");
        submit_operation_impl(&state, issued.id).expect("the first submit must succeed");
        let err = submit_operation_impl(&state, issued.id)
            .expect_err("resubmitting the same plan id must be rejected");
        assert!(
            err.contains("no such plan"),
            "expected the Unknown-plan error, got: {err}"
        );

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            execute_calls.load(Ordering::SeqCst),
            1,
            "exactly one execute() call, from the first legitimate submit — not two"
        );
    }

    #[tokio::test]
    async fn test_submit_operation_impl_rejects_an_expired_plan() {
        // F1: a plan previewed too long ago must be re-previewed, not run
        // blind. Simulates 601 seconds passing between issue_plan and
        // submit via an injectable clock — `Session::{new,with_adapters}`'s
        // `now_fn` seam exists specifically so tests like this one do not
        // need to actually wait 10 minutes.
        static EXPIRED_PLAN_TEST_NOW: AtomicI64 = AtomicI64::new(1_700_000_000);
        fn expired_plan_test_now() -> i64 {
            EXPIRED_PLAN_TEST_NOW.load(Ordering::SeqCst)
        }

        let (state, execute_calls) =
            state_with_fake_adapter_and_now(Some(expired_plan_test_now));
        refresh_impl(&state).await.expect("refresh_impl");
        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = plan_operation_impl(&state, req)
            .await
            .expect("plan_operation_impl");

        EXPIRED_PLAN_TEST_NOW.fetch_add(601, Ordering::SeqCst);

        let err = submit_operation_impl(&state, issued.id)
            .expect_err("a plan older than 600 seconds must be rejected");
        assert!(
            err.contains("older than 10 minutes"),
            "expected the Expired-plan error, got: {err}"
        );
        assert_eq!(
            execute_calls.load(Ordering::SeqCst),
            0,
            "an expired submit must never reach the runner"
        );
    }

    #[test]
    fn test_get_and_set_settings_impl_round_trip() {
        let path = temp_settings_path("settings-roundtrip");
        let _ = std::fs::remove_file(&path);
        let state = AppState::new(path, ChannelSink::new());
        let mut settings = get_settings_impl(&state).expect("get_settings_impl");
        assert_eq!(settings, Settings::default());
        settings.show_technical_details = true;
        set_settings_impl(&state, settings.clone()).expect("set_settings_impl");
        assert_eq!(
            get_settings_impl(&state).expect("get_settings_impl again"),
            settings
        );
    }

    #[test]
    fn test_subscribe_events_impl_registers_a_channel_that_receives_broadcasts() {
        let path = temp_settings_path("subscribe");
        let _ = std::fs::remove_file(&path);
        let state = AppState::new(path, ChannelSink::new());
        let received: Arc<std::sync::Mutex<Vec<UiEvent>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let r = received.clone();
        let channel: Channel<UiEvent> = Channel::new(move |body| {
            let event: UiEvent = body.deserialize().expect("deserialize UiEvent");
            r.lock().unwrap().push(event);
            Ok(())
        });
        subscribe_events_impl(&state, channel).expect("subscribe_events_impl");
        state
            .channel_sink
            .broadcast(UiEvent::SnapshotChanged { generation: 42 });
        assert_eq!(received.lock().unwrap().len(), 1);
    }
}
```

Then add `mod ipc;` to `src-tauri/src/lib.rs`, alongside the existing module declarations:

```rust
mod events;
mod ipc;
mod state;
```

This has to happen now, not after the `..._impl` functions exist: `ipc.rs` must actually be part of the crate for Step 2's compile failure below to be the real "these functions don't exist" error, rather than the test filter silently matching zero tests because the file was never compiled at all (M4 in the design review).

- [ ] **Step 3: Run the tests and confirm they fail to compile**

Run: `cargo test -p canager --lib ipc::`
Expected: FAIL to compile — `error[E0425]: cannot find function `get_snapshot_impl` in this scope` and similarly for every other `..._impl` function referenced by the test module (the module is now part of the crate via `mod ipc;`, and `AppState` itself resolves fine via the existing `use crate::state::AppState;` — it is only the `..._impl` functions and command wrappers that do not exist yet).

- [ ] **Step 4: Implement all nine `..._impl` functions and their thin `#[tauri::command]` wrappers**

Insert into `src-tauri/src/ipc.rs`, between the `use` block and the `#[cfg(test)]` module:

```rust
pub(crate) fn get_snapshot_impl(state: &AppState) -> Result<Snapshot, String> {
    Ok(state.session.snapshot())
}

#[tauri::command]
pub async fn get_snapshot(state: State<'_, AppState>) -> Result<Snapshot, String> {
    get_snapshot_impl(&state)
}

/// Also broadcasts `UiEvent::SnapshotChanged` on `state.channel_sink`
/// whenever the refreshed snapshot's `generation` differs from the one
/// before this call (M9 in the design review). `canager-core` must never
/// depend on `tauri`, so `Session::refresh` itself cannot send this — the
/// shell is the only layer that can, and this is the only place in the
/// whole plan that does so outside a test.
pub(crate) async fn refresh_impl(state: &AppState) -> Result<Snapshot, String> {
    let generation_before = state.session.snapshot().generation;
    let snapshot = state.session.refresh(&HostEnv::discover()).await;
    if snapshot.generation != generation_before {
        state.channel_sink.broadcast(UiEvent::SnapshotChanged {
            generation: snapshot.generation,
        });
    }
    Ok(snapshot)
}

#[tauri::command]
pub async fn refresh(state: State<'_, AppState>) -> Result<Snapshot, String> {
    refresh_impl(&state).await
}

/// Resolves and plans `request` through `Session::issue_plan`, returning
/// the server-issued `IssuedPlan` for the caller to preview. Nothing in
/// the returned `Plan` is ever accepted back from the client — F1 in the
/// design review: IPC accepts only known operations and server-issued
/// object IDs, never a client-supplied `Plan`. `submit_operation_impl`
/// below is the only way to actually run it, and takes only the id.
pub(crate) async fn plan_operation_impl(
    state: &AppState,
    request: OpRequest,
) -> Result<IssuedPlan, String> {
    state
        .session
        .issue_plan(&request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn plan_operation(
    state: State<'_, AppState>,
    request: OpRequest,
) -> Result<IssuedPlan, String> {
    plan_operation_impl(&state, request).await
}

/// Consumes the plan stored under `plan_id` (one-time use) and submits
/// exactly that stored `Plan`. Rejects an unknown, already-submitted, or
/// expired `plan_id` (`Session::submit`'s `SubmitError`) without ever
/// constructing or accepting a `Plan` from the caller.
pub(crate) fn submit_operation_impl(state: &AppState, plan_id: u64) -> Result<u64, String> {
    state.session.submit(plan_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn submit_operation(state: State<'_, AppState>, plan_id: u64) -> Result<u64, String> {
    submit_operation_impl(&state, plan_id)
}

pub(crate) fn cancel_operation_impl(state: &AppState, op_id: u64) -> Result<(), String> {
    state.session.cancel(op_id);
    Ok(())
}

#[tauri::command]
pub async fn cancel_operation(state: State<'_, AppState>, op_id: u64) -> Result<(), String> {
    cancel_operation_impl(&state, op_id)
}

pub(crate) fn list_operations_impl(state: &AppState) -> Result<Vec<OpSummary>, String> {
    Ok(state.session.operations())
}

#[tauri::command]
pub async fn list_operations(state: State<'_, AppState>) -> Result<Vec<OpSummary>, String> {
    list_operations_impl(&state)
}

pub(crate) fn get_settings_impl(state: &AppState) -> Result<Settings, String> {
    Ok(state.get_settings())
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    get_settings_impl(&state)
}

pub(crate) fn set_settings_impl(state: &AppState, settings: Settings) -> Result<(), String> {
    state.set_settings(settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    set_settings_impl(&state, settings)
}

pub(crate) fn subscribe_events_impl(
    state: &AppState,
    channel: Channel<UiEvent>,
) -> Result<(), String> {
    state.channel_sink.register(channel);
    Ok(())
}

#[tauri::command]
pub async fn subscribe_events(
    state: State<'_, AppState>,
    channel: Channel<UiEvent>,
) -> Result<(), String> {
    subscribe_events_impl(&state, channel)
}
```

- [ ] **Step 5: Run the tests and confirm they pass**

Run: `cargo test -p canager --lib ipc::`
Expected: `test result: ok. 10 passed; 0 failed; ...`

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/ipc.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(shell): add all nine IPC commands as thin AppState adapters

Each #[tauri::command] is a one- or two-line wrapper over a plain
..._impl function taking &AppState, so the real logic is unit-tested
directly (tauri::State cannot be constructed outside the tauri crate).
Not yet registered with invoke_handler! — the next commit wires that
up once the front end has something to call them with.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 7: Register all nine commands with `invoke_handler!`**

In `src-tauri/src/lib.rs`, add the `.invoke_handler(...)` call to the builder chain, right after `.setup(...)`:

```rust
        .setup(|app| {
            let settings_path = app.path().app_data_dir()?.join("settings.json");
            let channel_sink = events::ChannelSink::new();
            app.manage(AppState::new(settings_path, channel_sink));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc::get_snapshot,
            ipc::refresh,
            ipc::plan_operation,
            ipc::submit_operation,
            ipc::cancel_operation,
            ipc::list_operations,
            ipc::get_settings,
            ipc::set_settings,
            ipc::subscribe_events,
        ])
        .run(tauri::generate_context!())
```

- [ ] **Step 8: Run a full build to confirm the macro-generated dispatch compiles, including the `Channel<UiEvent>` command argument**

Run: `cargo build --workspace 2>&1 | tail -20`
Expected: ends with `Finished` and no errors (proves `tauri::generate_handler!` accepts all nine commands together, and that `Channel<UiEvent>` resolves correctly as a command argument type via its `CommandArg` impl).

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(shell): register all nine IPC commands with invoke_handler!

The front end (Task 9 onward) can now call every command named in
docs/superpowers/plans/2026-09-19-phase-2-ui-shell.md's Core
Interfaces section.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 10: Run the full workspace definition-of-done check**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: `cargo fmt --all --check` prints nothing and exits 0; clippy ends with `Finished` and no warnings; `cargo test --workspace` reports `test result: ok.` for every suite in the workspace with zero failures — in `canager-core`, that includes `adapters::` (35 tests: the pre-existing 29 across `adapters::tests`/`brew::tests`/`brew::plan_execute_tests` plus this plan's 6 new ones), `settings::` (7 tests, new), `session::` (12 tests, new — refresh/snapshot/issue_plan/submit plus the M5 unchanged-content-coalescing and M8 same-instance-lock regression tests), `ops::` unit tests, and every `tests/*.rs` integration file including the new `ops_summaries_test.rs` (5 tests, including N4's multiple-waiters regression test); in `canager` (`src-tauri`), that includes `events::` (3 tests, new), `state::` (4 tests, new — including the M7 concurrent-save regression test) and `ipc::` (10 tests, new — including the three F1 plan-rejection tests) — with zero regressions anywhere in the workspace relative to the pre-Task-1 baseline.

---

### Task 9: Front-end foundation

**Files:**
- Modify: `package.json`
- Modify: `pnpm-lock.yaml`
- Modify: `vite.config.ts`
- Create: `src/test/setup.ts`
- Create: `src/i18n/index.ts`
- Create: `src/i18n/en.json`
- Create: `src/components/Sidebar.tsx`
- Test: `src/components/Sidebar.test.tsx`
- Create: `src/components/ui/Dialog.tsx`
- Test: `src/components/ui/Dialog.test.tsx`
- Create: `src/components/ui/Switch.tsx`
- Test: `src/components/ui/Switch.test.tsx`
- Modify: `src/App.tsx`
- Test: `src/App.test.tsx` (new file)
- Modify: `src/index.css`
- Modify: `src/main.tsx`
- Delete: `src/App.css`
- Delete: `src/assets/react.svg`

**Interfaces:**
- Consumes: none from earlier tasks — this is the front-end foundation. No IPC calls yet (Tasks 1–8 exist only in `canager-core` / `src-tauri`, not called from React until Task 10).
- Produces: `renderWithProviders(ui: ReactElement)` test helper (`src/test/setup.ts`), used by every later test in Tasks 10–18; a global `vi.mock` of `@tauri-apps/api/core`'s `invoke` and `Channel`; `Sidebar` component and its `SidebarPage` type (`"installed" | "updates" | "settings"`) from `src/components/Sidebar.tsx` (Task 11 later replaces this local type with the shared `Page` type from `src/store/ui.ts`); the default `i18n` instance from `src/i18n/index.ts` with `en.json` resources loaded under keys `app.*` and `nav.*`; `Dialog({ open, onOpenChange, title, children, footer? })` and `Switch({ checked, onCheckedChange, id?, "aria-label"? })` (`src/components/ui/Dialog.tsx` and `src/components/ui/Switch.tsx` — the two Radix wrappers every later dialog/toggle in this plan builds on: Task 12's `UpdatesPage`, Task 14's `UninstallDialog`, and Task 15's `SettingsPage`).

- [ ] **Step 1: Add front-end dependencies and install them**

Replace `package.json` with:

```json
{
  "name": "canager",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "packageManager": "pnpm@12.4.2",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "tauri": "tauri",
    "test": "vitest run"
  },
  "dependencies": {
    "react": "^19.1.0",
    "react-dom": "^19.1.0",
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-opener": "^2",
    "@tanstack/react-query": "^5.103.1",
    "@tanstack/react-virtual": "^3.14.13",
    "zustand": "^5.0.15",
    "i18next": "^26.4.2",
    "react-i18next": "^17.0.14",
    "i18next-browser-languagedetector": "^8.2.1",
    "@radix-ui/react-dialog": "^1.1.23",
    "@radix-ui/react-switch": "^1.3.7",
    "@radix-ui/react-scroll-area": "^1.2.18"
  },
  "devDependencies": {
    "@types/react": "^19.1.8",
    "@types/react-dom": "^19.1.6",
    "@vitejs/plugin-react": "^6.0.2",
    "typescript": "~6.0.3",
    "vite": "^8.0.16",
    "@tauri-apps/cli": "^2",
    "tailwindcss": "^4.3.3",
    "@tailwindcss/vite": "^4.3.3",
    "vitest": "^5.0.1",
    "jsdom": "^30.1.0",
    "@testing-library/react": "^16.3.3",
    "@testing-library/jest-dom": "^7.0.1",
    "@testing-library/user-event": "^14.6.7"
  }
}
```

Versions were read from the registry on 2026-09-19 (`npm view <pkg> version`): `@tanstack/react-query` 5.103.1, `@tanstack/react-virtual` 3.14.13, `zustand` 5.0.15, `i18next` 26.4.2, `react-i18next` 17.0.14, `i18next-browser-languagedetector` 8.2.1, `@radix-ui/react-dialog` 1.1.23, `@radix-ui/react-switch` 1.3.7, `@radix-ui/react-scroll-area` 1.2.18, `vitest` 5.0.1 (peer `vite: ^6 || ^7 || ^8`, matching the repo's `vite ^8.0.16`), `@testing-library/react` 16.3.3, `@testing-library/jest-dom` 7.0.1 (ships a `./vitest` subpath export), `jsdom` 30.1.0, `@testing-library/user-event` 14.6.7. `@radix-ui/react-tabs` and `@radix-ui/react-tooltip` are deliberately **not** installed: no task in this plan builds a `Tabs` or `Tooltip` wrapper or imports either package (the language picker in Task 15 uses a hand-rolled `role="radiogroup"` instead), so they would be dead weight.

Run: `pnpm install`
Expected: exits 0; `pnpm-lock.yaml` is rewritten to include the new packages; no `ERR_PNPM` output.

- [ ] **Step 2: Create the i18next bootstrap**

Create `src/i18n/en.json`:

```json
{
  "app": {
    "title": "Canager",
    "operationBarRegion": "Operation status"
  },
  "nav": {
    "label": "Sections",
    "installed": "Installed",
    "updates": "Updates",
    "settings": "Settings"
  }
}
```

Create `src/i18n/index.ts`:

```ts
import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./en.json";

void i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
  },
  lng: "en",
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

export default i18n;
```

- [ ] **Step 3: Create the vitest harness**

Create `src/test/setup.ts`:

```ts
import "@testing-library/jest-dom/vitest";
import type { ReactElement, ReactNode } from "react";
import React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";
import { I18nextProvider } from "react-i18next";
import i18n from "../i18n";

afterEach(() => {
  cleanup();
});

// jsdom has no ResizeObserver. @tanstack/react-virtual (Task 11) and Radix
// ScrollArea (Task 13) both use it to measure their container; a no-op stub
// is enough because neither needs a *real* resize callback to run in tests —
// react-virtual takes its first measurement synchronously on mount.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

if (!("ResizeObserver" in globalThis)) {
  (globalThis as unknown as { ResizeObserver: typeof ResizeObserverStub }).ResizeObserver =
    ResizeObserverStub;
}

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T = unknown> {
    onmessage: (response: T) => void = () => {};
  }
  return {
    invoke: vi.fn(),
    Channel,
  };
});

export function renderWithProviders(ui: ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  function Wrapper({ children }: { children: ReactNode }) {
    return React.createElement(
      QueryClientProvider,
      { client: queryClient },
      React.createElement(I18nextProvider, { i18n }, children),
    );
  }

  return {
    queryClient,
    ...render(ui, { wrapper: Wrapper }),
  };
}
```

Modify `vite.config.ts` to wire vitest into the same config (full file):

```ts
/// <reference types="vitest/config" />
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
  test: {
    environment: "jsdom",
    setupFiles: ["src/test/setup.ts"],
    css: false,
  },
}));
```

The triple-slash reference to `vitest/config` merges vitest's `test` field into `defineConfig`'s type from plain `vite`, which is the officially documented way to add a `test` block without importing `defineConfig` from `vitest/config` (`vitest` 5.0.1 ships a `./config` type-only export for exactly this).

- [ ] **Step 4: Write a failing test for `Sidebar`**

Create `src/components/Sidebar.test.tsx`:

```tsx
import { describe, expect, it, vi } from "vitest";
import { renderWithProviders } from "../test/setup";
import { Sidebar } from "./Sidebar";

describe("Sidebar", () => {
  it("renders a button for each page and marks the active one", () => {
    const onSelectPage = vi.fn();
    const { getByRole } = renderWithProviders(
      <Sidebar page="installed" onSelectPage={onSelectPage} />,
    );

    const installedButton = getByRole("button", { name: "Installed" });
    const updatesButton = getByRole("button", { name: "Updates" });
    const settingsButton = getByRole("button", { name: "Settings" });

    expect(installedButton).toHaveAttribute("aria-current", "page");
    expect(updatesButton).not.toHaveAttribute("aria-current");
    expect(settingsButton).not.toHaveAttribute("aria-current");
  });

  it("calls onSelectPage with the clicked page", () => {
    const onSelectPage = vi.fn();
    const { getByRole } = renderWithProviders(
      <Sidebar page="installed" onSelectPage={onSelectPage} />,
    );

    getByRole("button", { name: "Updates" }).click();

    expect(onSelectPage).toHaveBeenCalledWith("updates");
  });
});
```

- [ ] **Step 5: Run the test, verify it fails**

Run: `pnpm exec vitest run src/components/Sidebar.test.tsx`
Expected: FAIL — Vite import analysis error: `Failed to resolve import "./Sidebar" from "src/components/Sidebar.test.tsx". Does the file exist?`

- [ ] **Step 6: Implement `Sidebar`**

Create `src/components/Sidebar.tsx`:

```tsx
import { useTranslation } from "react-i18next";

export type SidebarPage = "installed" | "updates" | "settings";

interface SidebarProps {
  page: SidebarPage;
  onSelectPage: (page: SidebarPage) => void;
}

const PAGES: SidebarPage[] = ["installed", "updates", "settings"];

export function Sidebar({ page, onSelectPage }: SidebarProps) {
  const { t } = useTranslation();

  return (
    <nav
      aria-label={t("nav.label")}
      className="flex w-56 shrink-0 flex-col gap-1 border-r border-[var(--color-border)] bg-[var(--color-sidebar-bg)] p-2"
    >
      {PAGES.map((p) => (
        <button
          key={p}
          type="button"
          aria-current={page === p ? "page" : undefined}
          onClick={() => onSelectPage(p)}
          className={
            page === p
              ? "rounded-md bg-[var(--color-accent)] px-3 py-2 text-left text-sm font-medium text-[var(--color-accent-foreground)]"
              : "rounded-md px-3 py-2 text-left text-sm font-medium text-[var(--color-foreground)] hover:bg-[var(--color-hover)]"
          }
        >
          {t(`nav.${p}`)}
        </button>
      ))}
    </nav>
  );
}
```

- [ ] **Step 7: Run the test, verify it passes**

Run: `pnpm exec vitest run src/components/Sidebar.test.tsx`
Expected: PASS (2 tests)

- [ ] **Step 8: Write a failing test for the app shell**

Create `src/App.test.tsx`:

```tsx
import { describe, expect, it } from "vitest";
import { renderWithProviders } from "./test/setup";
import App from "./App";

describe("App", () => {
  it("shows the Installed page heading by default", () => {
    const { getByRole } = renderWithProviders(<App />);
    expect(getByRole("heading", { name: "Installed" })).toBeInTheDocument();
  });

  it("switches the content area when a sidebar link is clicked", () => {
    const { getByRole } = renderWithProviders(<App />);

    getByRole("button", { name: "Updates" }).click();

    expect(getByRole("heading", { name: "Updates" })).toBeInTheDocument();
  });
});
```

- [ ] **Step 9: Run the test, verify it fails**

Run: `pnpm exec vitest run src/App.test.tsx`
Expected: FAIL — the current `src/App.tsx` still renders the create-tauri-app template (a "Canager" `h1`, Vite/Tauri/React logos and a greet form), so no heading named "Installed" exists yet.

- [ ] **Step 10: Rewrite the shell, delete template leftovers**

Run: `git rm src/App.css src/assets/react.svg`
Expected: both files removed from the working tree and staged for deletion.

Replace `src/App.tsx`:

```tsx
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Sidebar, type SidebarPage } from "./components/Sidebar";

function App() {
  const { t } = useTranslation();
  const [page, setPage] = useState<SidebarPage>("installed");

  return (
    <div className="flex h-screen flex-col bg-[var(--color-background)] text-[var(--color-foreground)]">
      <div className="flex flex-1 overflow-hidden">
        <Sidebar page={page} onSelectPage={setPage} />
        <main className="flex-1 overflow-y-auto p-6">
          <h1 className="text-lg font-semibold">{t(`nav.${page}`)}</h1>
        </main>
      </div>
      <footer
        aria-label={t("app.operationBarRegion")}
        className="h-12 shrink-0 border-t border-[var(--color-border)]"
      />
    </div>
  );
}

export default App;
```

Replace `src/index.css`:

```css
@import "tailwindcss";

:root {
  --color-background: #ffffff;
  --color-foreground: #1d1d1f;
  --color-sidebar-bg: #f5f5f7;
  --color-border: #d2d2d7;
  --color-accent: #0071e3;
  --color-accent-foreground: #ffffff;
  --color-hover: #e8e8ed;
  --color-muted: #6e6e73;
  --color-muted-foreground: #6b7280;
  --color-danger: #d70015;
}

@media (prefers-color-scheme: dark) {
  :root {
    --color-background: #1e1e1e;
    --color-foreground: #f5f5f7;
    --color-sidebar-bg: #252526;
    --color-border: #3a3a3c;
    --color-accent: #0a84ff;
    --color-accent-foreground: #ffffff;
    --color-hover: #3a3a3c;
    --color-muted: #98989d;
    --color-muted-foreground: #9ca3af;
    --color-danger: #ff453a;
  }
}

body {
  margin: 0;
  font-family:
    -apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", Arial, sans-serif;
}
```

Replace `src/main.tsx`:

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import "./i18n";
import "./index.css";

const queryClient = new QueryClient();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </React.StrictMode>,
);
```

- [ ] **Step 11: Run the full suite and the production build, verify both pass**

Run: `pnpm test`
Expected: PASS — `src/App.test.tsx` and `src/components/Sidebar.test.tsx` both green, 4 tests total, 0 failures.

Run: `pnpm build`
Expected: `tsc` reports no type errors; `vite build` completes and (re)writes `dist/`.

- [ ] **Step 12: Write the failing tests for the shared `Dialog` and `Switch` wrappers**

Every later dialog (Task 12's update confirmation, Task 14's uninstall confirmation) and every later toggle (Task 15's Settings switches) builds on these two thin Radix wrappers, so they belong in the foundation task rather than being created — or, worse, re-created — by whichever page happens to need one first.

Create `src/components/ui/Dialog.test.tsx`:

```tsx
import { describe, expect, it, vi } from "vitest";
import { renderWithProviders } from "../../test/setup";
import { Dialog } from "./Dialog";

describe("Dialog", () => {
  it("renders the title, children and footer when open", () => {
    const { getByRole, getByText } = renderWithProviders(
      <Dialog
        open
        onOpenChange={vi.fn()}
        title="Confirm"
        footer={<button type="button">OK</button>}
      >
        <p>Body content</p>
      </Dialog>,
    );

    expect(getByRole("dialog")).toBeInTheDocument();
    expect(getByText("Confirm")).toBeInTheDocument();
    expect(getByText("Body content")).toBeInTheDocument();
    expect(getByRole("button", { name: "OK" })).toBeInTheDocument();
  });

  it("does not render when closed", () => {
    const { queryByRole } = renderWithProviders(
      <Dialog open={false} onOpenChange={vi.fn()} title="Confirm">
        <p>Body content</p>
      </Dialog>,
    );

    expect(queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("renders without a footer when none is passed", () => {
    const { getByRole, queryByRole } = renderWithProviders(
      <Dialog open onOpenChange={vi.fn()} title="Confirm">
        <p>Body content</p>
      </Dialog>,
    );

    expect(getByRole("dialog")).toBeInTheDocument();
    expect(queryByRole("button")).not.toBeInTheDocument();
  });
});
```

Create `src/components/ui/Switch.test.tsx`:

```tsx
import { describe, expect, it, vi } from "vitest";
import { renderWithProviders } from "../../test/setup";
import { Switch } from "./Switch";

describe("Switch", () => {
  it("reflects the checked prop and exposes an aria-label", () => {
    const { getByRole } = renderWithProviders(
      <Switch checked onCheckedChange={vi.fn()} aria-label="Show technical details" />,
    );

    expect(getByRole("switch", { name: "Show technical details" })).toBeChecked();
  });

  it("calls onCheckedChange with the new value when clicked", () => {
    const onCheckedChange = vi.fn();
    const { getByRole } = renderWithProviders(
      <Switch checked={false} onCheckedChange={onCheckedChange} aria-label="Greedy casks" />,
    );

    getByRole("switch", { name: "Greedy casks" }).click();
    expect(onCheckedChange).toHaveBeenCalledWith(true);
  });
});
```

- [ ] **Step 13: Run both tests, verify they fail**

Run: `pnpm exec vitest run src/components/ui/Dialog.test.tsx src/components/ui/Switch.test.tsx`
Expected: FAIL — both error with "Failed to resolve import" for `./Dialog` and `./Switch` respectively (neither file exists yet).

- [ ] **Step 14: Implement `Dialog` and `Switch`**

Create `src/components/ui/Dialog.tsx`:

```tsx
import * as RadixDialog from "@radix-ui/react-dialog";
import type { ReactNode } from "react";

export interface DialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  children: ReactNode;
  footer?: ReactNode;
}

export function Dialog({ open, onOpenChange, title, children, footer }: DialogProps) {
  return (
    <RadixDialog.Root open={open} onOpenChange={onOpenChange}>
      <RadixDialog.Portal>
        <RadixDialog.Overlay className="fixed inset-0 bg-black/40" />
        <RadixDialog.Content className="fixed left-1/2 top-1/2 w-full max-w-md -translate-x-1/2 -translate-y-1/2 rounded-lg bg-[var(--color-background)] p-6 shadow-lg">
          <RadixDialog.Title className="text-base font-semibold text-[var(--color-foreground)]">
            {title}
          </RadixDialog.Title>
          <div className="mt-4">{children}</div>
          {footer ? <div className="mt-6 flex justify-end gap-2">{footer}</div> : null}
        </RadixDialog.Content>
      </RadixDialog.Portal>
    </RadixDialog.Root>
  );
}
```

Create `src/components/ui/Switch.tsx`:

```tsx
import * as RadixSwitch from "@radix-ui/react-switch";

export interface SwitchProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  id?: string;
  "aria-label"?: string;
}

export function Switch({ checked, onCheckedChange, id, "aria-label": ariaLabel }: SwitchProps) {
  return (
    <RadixSwitch.Root
      id={id}
      aria-label={ariaLabel}
      checked={checked}
      onCheckedChange={onCheckedChange}
      className="relative h-6 w-10 shrink-0 rounded-full bg-[var(--color-hover)] outline-none data-[state=checked]:bg-[var(--color-accent)]"
    >
      <RadixSwitch.Thumb className="block h-5 w-5 translate-x-0.5 rounded-full bg-[var(--color-background)] transition-transform duration-150 data-[state=checked]:translate-x-[18px]" />
    </RadixSwitch.Root>
  );
}
```

- [ ] **Step 15: Run both tests, verify they pass**

Run: `pnpm exec vitest run src/components/ui/Dialog.test.tsx src/components/ui/Switch.test.tsx`
Expected: PASS (3 + 2 = 5 tests)

- [ ] **Step 16: Commit**

```bash
git add package.json pnpm-lock.yaml vite.config.ts src/test/setup.ts src/i18n/index.ts src/i18n/en.json src/components/Sidebar.tsx src/components/Sidebar.test.tsx src/components/ui/Dialog.tsx src/components/ui/Dialog.test.tsx src/components/ui/Switch.tsx src/components/ui/Switch.test.tsx src/App.tsx src/App.test.tsx src/index.css src/main.tsx
git commit -m "$(cat <<'EOF'
feat(ui): scaffold front-end shell, i18n, vitest harness, and shared Dialog/Switch wrappers

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 10: TS types + API client + event bridge

**Files:**
- Create: `src/lib/types.ts`
- Create: `src/lib/api.ts`
- Create: `src/lib/events.ts`
- Create: `src/lib/queries.ts`
- Create: `src/store/ui.ts`
- Test: `src/lib/types.test.ts`
- Test: `src/lib/api.test.ts`
- Test: `src/store/ui.test.ts`
- Test: `src/lib/queries.test.ts`
- Test: `src/lib/events.test.ts`

**Interfaces:**
- Consumes: the nine `#[tauri::command]`s from Task 8 (`get_snapshot`, `refresh`, `plan_operation`, `submit_operation`, `cancel_operation`, `list_operations`, `get_settings`, `set_settings`, `subscribe_events`) and the `UiEvent` wire shape from Task 6's `src-tauri/src/events.rs`, both exactly as given in the skeleton's Core Interfaces; `renderWithProviders` and the global `invoke`/`Channel` mock from Task 9's `src/test/setup.ts`.
- Produces: every name in the skeleton's `types.ts` / `api.ts` / `queries.ts` / `store/ui.ts` blocks, reproduced verbatim below; plus `useOperationEvents(): void` (`src/lib/events.ts`, not in the skeleton — a zero-argument hook with no return value that a page mounts once to start forwarding `UiEvent`s from the Channel into the Zustand store and into TanStack Query cache invalidation). Later tasks import `useOperationEvents` from `../lib/events`.

- [ ] **Step 1: Write failing tests for the data layer (`types.ts`, `api.ts`, `store/ui.ts`)**

Create `src/lib/types.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { Snapshot, Outcome, OperationEvent, UiEvent, Plan, OpSummary, Settings } from "./types";

describe("types", () => {
  it("parses a realistic Snapshot JSON payload (shape copied from canager-core's brew fixtures)", () => {
    const raw = `{
      "generation": 3,
      "detect": "Found",
      "instances": [
        {
          "id": "brew:/opt/homebrew",
          "adapter_id": "brew",
          "exe_path": "/opt/homebrew/bin/brew",
          "prefix": "/opt/homebrew",
          "scope": "User",
          "version": "7.0.3",
          "healthy": true
        }
      ],
      "artifacts": [
        {
          "key": { "instance_id": "brew:/opt/homebrew", "kind": "Formula", "name": "jq" },
          "display_name": "jq",
          "version": "1.8.2",
          "reason": "Requested",
          "description": "Lightweight and flexible command-line JSON processor",
          "homepage": "https://jqlang.github.io/jq/",
          "size_bytes": null,
          "installed_at": 1783762037,
          "path": null,
          "auto_updates": false
        },
        {
          "key": { "instance_id": "brew:/opt/homebrew", "kind": "Cask", "name": "onyx" },
          "display_name": "OnyX",
          "version": "5.0.2",
          "reason": "Requested",
          "description": "Verify system files structure, run miscellaneous maintenance and more",
          "homepage": "https://www.titanium-software.fr/en/onyx.html",
          "size_bytes": null,
          "installed_at": null,
          "path": null,
          "auto_updates": false
        }
      ],
      "updates": [
        {
          "key": { "instance_id": "brew:/opt/homebrew", "kind": "Cask", "name": "onyx" },
          "current": "5.0.2",
          "target": "5.1.0",
          "channel": "Native",
          "checkable": true,
          "warnings": []
        }
      ],
      "refreshed_at": 1789700000,
      "stale": false,
      "errors": []
    }`;

    const parsed = JSON.parse(raw) as Snapshot;

    expect(parsed.generation).toBe(3);
    expect(parsed.detect).toBe("Found");
    expect(parsed.instances[0].scope).toBe("User");
    expect(parsed.artifacts[0].key.kind).toBe("Formula");
    expect(parsed.artifacts[1].key.kind).toBe("Cask");
    expect(parsed.artifacts[1].installed_at).toBeNull();
    expect(parsed.updates[0].channel).toBe("Native");
    expect(parsed.stale).toBe(false);
    expect(parsed.errors).toEqual([]);
  });

  it("parses tagged Outcome variants", () => {
    const succeeded = JSON.parse('"Succeeded"') as Outcome;
    const needsAttention = JSON.parse(
      '{"NeedsAttention": "command succeeded but the package is not installed"}',
    ) as Outcome;
    const failed = JSON.parse('{"Failed": {"exit_code": 1, "summary": "boom"}}') as Outcome;

    expect(succeeded).toBe("Succeeded");
    expect(needsAttention).toEqual({
      NeedsAttention: "command succeeded but the package is not installed",
    });
    expect(failed).toEqual({ Failed: { exit_code: 1, summary: "boom" } });
  });

  it("parses OperationEvent and UiEvent wire shapes", () => {
    const log = JSON.parse(
      '{"Log": {"op_id": 1, "stream": "Stdout", "line": "Installing jq"}}',
    ) as OperationEvent;
    const uiEvent = JSON.parse(
      '{"Operation": {"Status": {"op_id": 1, "status": "Running"}}}',
    ) as UiEvent;
    const snapshotChanged = JSON.parse('{"SnapshotChanged": {"generation": 7}}') as UiEvent;

    expect(log).toEqual({ Log: { op_id: 1, stream: "Stdout", line: "Installing jq" } });
    expect("Operation" in uiEvent && uiEvent.Operation).toEqual({
      Status: { op_id: 1, status: "Running" },
    });
    expect("SnapshotChanged" in snapshotChanged && snapshotChanged.SnapshotChanged).toEqual({
      generation: 7,
    });
  });

  it("parses a Plan, an OpSummary and Settings", () => {
    const plan = JSON.parse(
      '{"request": {"kind": "Install", "instance_id": "brew:/opt/homebrew", "artifact_kind": "Formula", "name": "jq"}, "program": "/opt/homebrew/bin/brew", "args": ["install", "--formula", "jq"], "env": [], "needs_password": false, "locks": ["brew:/opt/homebrew"], "cancel_policy": "KillThenReconcile", "warnings": [], "affected": [], "timeout_secs": 1800}',
    ) as Plan;
    const opSummary = JSON.parse(
      '{"id": 1, "kind": "Install", "instance_id": "brew:/opt/homebrew", "artifact_kind": "Formula", "name": "jq", "status": "Running", "outcome": null, "argv_preview": ["/opt/homebrew/bin/brew", "install", "--formula", "jq"]}',
    ) as OpSummary;
    const settings = JSON.parse(
      '{"language": "ZhCn", "show_technical_details": true, "ignored_updates": []}',
    ) as Settings;

    expect(plan.cancel_policy).toBe("KillThenReconcile");
    expect(plan.locks).toEqual(["brew:/opt/homebrew"]);
    expect(opSummary.status).toBe("Running");
    expect(opSummary.outcome).toBeNull();
    expect(settings.language).toBe("ZhCn");
  });
});
```

Create `src/lib/api.test.ts`:

```ts
import { describe, expect, it, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  getSnapshot,
  refresh,
  planOperation,
  submitOperation,
  cancelOperation,
  listOperations,
  getSettings,
  setSettings,
  subscribeEvents,
} from "./api";
import type { IssuedPlan, OpRequest, Settings, UiEvent } from "./types";

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockInvoke.mockReset();
});

describe("api", () => {
  it("getSnapshot invokes get_snapshot with no args", async () => {
    mockInvoke.mockResolvedValueOnce({} as never);
    await getSnapshot();
    expect(mockInvoke).toHaveBeenCalledWith("get_snapshot");
  });

  it("refresh invokes refresh", async () => {
    mockInvoke.mockResolvedValueOnce({} as never);
    await refresh();
    expect(mockInvoke).toHaveBeenCalledWith("refresh");
  });

  it("planOperation invokes plan_operation with the request and returns the IssuedPlan", async () => {
    const request: OpRequest = {
      kind: "Uninstall",
      instance_id: "brew:/opt/homebrew",
      artifact_kind: "Formula",
      name: "jq",
    };
    const issued: IssuedPlan = {
      id: 1,
      plan: {
        request,
        program: "/opt/homebrew/bin/brew",
        args: ["uninstall", "--formula", "jq"],
        env: [],
        needs_password: false,
        locks: ["brew:/opt/homebrew"],
        cancel_policy: "KillThenReconcile",
        warnings: [],
        affected: [],
        timeout_secs: 1800,
      },
      issued_at: 1758000000,
    };
    mockInvoke.mockResolvedValueOnce(issued as never);
    const result = await planOperation(request);
    expect(mockInvoke).toHaveBeenCalledWith("plan_operation", { request });
    expect(result).toEqual(issued);
  });

  it("submitOperation invokes submit_operation with only the plan id", async () => {
    mockInvoke.mockResolvedValueOnce(7 as never);
    await submitOperation(1);
    expect(mockInvoke).toHaveBeenCalledWith("submit_operation", { planId: 1 });
  });

  it("cancelOperation invokes cancel_operation with opId", async () => {
    mockInvoke.mockResolvedValueOnce(undefined as never);
    await cancelOperation(7);
    expect(mockInvoke).toHaveBeenCalledWith("cancel_operation", { opId: 7 });
  });

  it("listOperations invokes list_operations", async () => {
    mockInvoke.mockResolvedValueOnce([] as never);
    await listOperations();
    expect(mockInvoke).toHaveBeenCalledWith("list_operations");
  });

  it("getSettings invokes get_settings", async () => {
    mockInvoke.mockResolvedValueOnce({} as never);
    await getSettings();
    expect(mockInvoke).toHaveBeenCalledWith("get_settings");
  });

  it("setSettings invokes set_settings with the settings", async () => {
    const settings: Settings = {
      language: "System",
      show_technical_details: false,
      ignored_updates: [],
    };
    mockInvoke.mockResolvedValueOnce(undefined as never);
    await setSettings(settings);
    expect(mockInvoke).toHaveBeenCalledWith("set_settings", { settings });
  });

  it("subscribeEvents registers a Channel and forwards messages to the callback", async () => {
    mockInvoke.mockResolvedValueOnce(undefined as never);
    const received: UiEvent[] = [];
    await subscribeEvents((event) => {
      received.push(event);
    });

    expect(mockInvoke).toHaveBeenCalledWith(
      "subscribe_events",
      expect.objectContaining({ channel: expect.anything() }),
    );
    const channelArg = mockInvoke.mock.calls[0][1] as {
      channel: { onmessage: (e: UiEvent) => void };
    };
    channelArg.channel.onmessage({ SnapshotChanged: { generation: 3 } });

    expect(received).toEqual([{ SnapshotChanged: { generation: 3 } }]);
  });
});
```

Create `src/store/ui.test.ts`:

```ts
import { describe, expect, it, beforeEach } from "vitest";
import { useUiStore, artifactKeyId } from "./ui";
import type { ArtifactKey } from "../lib/types";

const key: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" };

beforeEach(() => {
  useUiStore.setState({
    page: "installed",
    query: "",
    showDependencies: false,
    drawerOpen: false,
    focusedOpId: null,
    logs: [],
    selectedUpdates: [],
  });
});

describe("useUiStore", () => {
  it("artifactKeyId joins the key's fields with |", () => {
    expect(artifactKeyId(key)).toBe("brew:/opt/homebrew|Formula|jq");
  });

  it("setPage changes the active page", () => {
    useUiStore.getState().setPage("updates");
    expect(useUiStore.getState().page).toBe("updates");
  });

  it("setQuery changes the filter text", () => {
    useUiStore.getState().setQuery("jq");
    expect(useUiStore.getState().query).toBe("jq");
  });

  it("toggleDependencies flips the flag", () => {
    expect(useUiStore.getState().showDependencies).toBe(false);
    useUiStore.getState().toggleDependencies();
    expect(useUiStore.getState().showDependencies).toBe(true);
    useUiStore.getState().toggleDependencies();
    expect(useUiStore.getState().showDependencies).toBe(false);
  });

  it("setDrawerOpen and setFocusedOpId update independently", () => {
    useUiStore.getState().setDrawerOpen(true);
    useUiStore.getState().setFocusedOpId(5);
    expect(useUiStore.getState().drawerOpen).toBe(true);
    expect(useUiStore.getState().focusedOpId).toBe(5);
  });

  it("appendLog appends in order and clearLogs removes only that op's lines", () => {
    useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "a" });
    useUiStore.getState().appendLog({ opId: 2, stream: "Stdout", line: "b" });
    useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "c" });

    expect(useUiStore.getState().logs.map((l) => l.line)).toEqual(["a", "b", "c"]);

    useUiStore.getState().clearLogs(1);
    expect(useUiStore.getState().logs.map((l) => l.line)).toEqual(["b"]);
  });

  it("appendLog keeps only the newest 2000 lines", () => {
    for (let i = 0; i < 2001; i += 1) {
      useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: `line-${i}` });
    }
    const logs = useUiStore.getState().logs;
    expect(logs).toHaveLength(2000);
    expect(logs[0].line).toBe("line-1");
    expect(logs[1999].line).toBe("line-2000");
  });

  it("toggleUpdate adds then removes the key's id from selectedUpdates", () => {
    useUiStore.getState().toggleUpdate(key);
    expect(useUiStore.getState().selectedUpdates).toEqual([artifactKeyId(key)]);
    useUiStore.getState().toggleUpdate(key);
    expect(useUiStore.getState().selectedUpdates).toEqual([]);
  });

  it("clearSelectedUpdates empties the selection", () => {
    useUiStore.getState().toggleUpdate(key);
    useUiStore.getState().clearSelectedUpdates();
    expect(useUiStore.getState().selectedUpdates).toEqual([]);
  });
});
```

- [ ] **Step 2: Run the tests, verify they fail**

Run: `pnpm exec vitest run src/lib/types.test.ts src/lib/api.test.ts src/store/ui.test.ts`
Expected: FAIL — all three files error with "Failed to resolve import" for `./types`, `./api` and `./ui` respectively.

- [ ] **Step 3: Implement `types.ts`, `api.ts` and `store/ui.ts`**

Create `src/lib/types.ts`:

```ts
export type ArtifactKind = "Formula" | "Cask" | "Package" | "Tool" | "Model" | "Binary";
export type InstallReason = "Requested" | "Dependency" | "Unknown";
export type OpKind = "Install" | "Uninstall" | "Upgrade";
export type OpStatus =
  | "Queued"
  | "Running"
  | "CancelRequested"
  | "Cancelling"
  | "Verifying"
  | "Done";
export type DetectOutcome = "Found" | "Missing" | "RefusedAsRoot";
export type Outcome =
  | "Succeeded"
  | "NoChange"
  | "PartialSuccess"
  | "Unconfirmed"
  | { NeedsAttention: string }
  | { Failed: { exit_code: number | null; summary: string } };
export interface ArtifactKey {
  instance_id: string;
  kind: ArtifactKind;
  name: string;
}
export interface InstalledArtifact {
  key: ArtifactKey;
  display_name: string;
  version: string;
  reason: InstallReason;
  description: string | null;
  homepage: string | null;
  size_bytes: number | null;
  installed_at: number | null;
  path: string | null;
  auto_updates: boolean;
}
export interface UpdateCandidate {
  key: ArtifactKey;
  current: string;
  target: string;
  channel: "Native" | "Registry" | "Digest";
  checkable: boolean;
  warnings: string[];
}
export interface ManagerInstance {
  id: string;
  adapter_id: string;
  exe_path: string;
  prefix: string;
  scope: "User" | "System";
  version: string | null;
  healthy: boolean;
}
export interface Plan {
  request: OpRequest;
  program: string;
  args: string[];
  env: [string, string][];
  needs_password: boolean;
  locks: string[];
  cancel_policy: "SafeKill" | "KillThenReconcile" | "NoCancel";
  warnings: string[];
  affected: string[];
  timeout_secs: number;
}
export interface OpRequest {
  kind: OpKind;
  instance_id: string;
  artifact_kind: ArtifactKind;
  name: string;
}
export interface IssuedPlan {
  id: number;
  plan: Plan;
  issued_at: number;
}
export interface OpSummary {
  id: number;
  kind: OpKind;
  instance_id: string;
  artifact_kind: ArtifactKind;
  name: string;
  status: OpStatus;
  outcome: Outcome | null;
  argv_preview: string[];
}
export interface SourceError {
  instance_id: string;
  message: string;
}
export interface Snapshot {
  generation: number;
  detect: DetectOutcome;
  instances: ManagerInstance[];
  artifacts: InstalledArtifact[];
  updates: UpdateCandidate[];
  refreshed_at: number | null;
  stale: boolean;
  errors: SourceError[];
}
export type Language = "System" | "En" | "ZhCn";
export interface Settings {
  language: Language;
  show_technical_details: boolean;
  ignored_updates: ArtifactKey[];
}
export type OperationEvent =
  | { Status: { op_id: number; status: OpStatus } }
  | { Log: { op_id: number; stream: "Stdout" | "Stderr"; line: string } }
  | { Finished: { op_id: number; outcome: Outcome } };
export type UiEvent = { Operation: OperationEvent } | { SnapshotChanged: { generation: number } };
```

Create `src/lib/api.ts`:

```ts
import { invoke, Channel } from "@tauri-apps/api/core";
import type { IssuedPlan, OpRequest, Settings, Snapshot, OpSummary, UiEvent } from "./types";

export function getSnapshot(): Promise<Snapshot> {
  return invoke("get_snapshot");
}

export function refresh(): Promise<Snapshot> {
  return invoke("refresh");
}

export function planOperation(request: OpRequest): Promise<IssuedPlan> {
  return invoke("plan_operation", { request });
}

export function submitOperation(planId: number): Promise<number> {
  return invoke("submit_operation", { planId });
}

export function cancelOperation(opId: number): Promise<void> {
  return invoke("cancel_operation", { opId });
}

export function listOperations(): Promise<OpSummary[]> {
  return invoke("list_operations");
}

export function getSettings(): Promise<Settings> {
  return invoke("get_settings");
}

export function setSettings(settings: Settings): Promise<void> {
  return invoke("set_settings", { settings });
}

/**
 * Registers a fresh Channel with the backend and forwards every UiEvent it
 * receives to `onEvent`. There is no `unsubscribe_events` command — the
 * backend only drops a Channel from its broadcast registry once a send to it
 * fails (the window closed). The returned function is a client-side detach:
 * it stops this callback from firing, it does not tell the backend anything.
 */
export function subscribeEvents(onEvent: (e: UiEvent) => void): Promise<() => void> {
  const channel = new Channel<UiEvent>();
  channel.onmessage = onEvent;
  return invoke("subscribe_events", { channel }).then(() => {
    return () => {
      channel.onmessage = () => {};
    };
  });
}
```

Create `src/store/ui.ts`:

```ts
import { create } from "zustand";
import type { ArtifactKey } from "../lib/types";

export type Page = "installed" | "updates" | "settings";

export interface LogLine {
  opId: number;
  stream: "Stdout" | "Stderr";
  line: string;
  seq: number;
}

export interface UiState {
  page: Page;
  setPage(p: Page): void;
  query: string;
  setQuery(q: string): void;
  showDependencies: boolean;
  toggleDependencies(): void;
  drawerOpen: boolean;
  setDrawerOpen(open: boolean): void;
  focusedOpId: number | null;
  setFocusedOpId(id: number | null): void;
  logs: LogLine[];
  appendLog(l: Omit<LogLine, "seq">): void;
  clearLogs(opId: number): void;
  selectedUpdates: string[];
  toggleUpdate(key: ArtifactKey): void;
  clearSelectedUpdates(): void;
}

const MAX_LOG_LINES = 2000;

export function artifactKeyId(key: ArtifactKey): string {
  return `${key.instance_id}|${key.kind}|${key.name}`;
}

// Ever-increasing across the page's lifetime (React keys need stable
// ordering even after the ring buffer below has evicted older lines); tests
// never assert its absolute value, only that appended lines keep the order
// they were appended in.
let logSeq = 0;

export const useUiStore = create<UiState>((set) => ({
  page: "installed",
  setPage: (p) => set({ page: p }),
  query: "",
  setQuery: (q) => set({ query: q }),
  showDependencies: false,
  toggleDependencies: () => set((s) => ({ showDependencies: !s.showDependencies })),
  drawerOpen: false,
  setDrawerOpen: (open) => set({ drawerOpen: open }),
  focusedOpId: null,
  setFocusedOpId: (id) => set({ focusedOpId: id }),
  logs: [],
  appendLog: (l) =>
    set((s) => {
      const next = [...s.logs, { ...l, seq: logSeq++ }];
      return {
        logs: next.length > MAX_LOG_LINES ? next.slice(next.length - MAX_LOG_LINES) : next,
      };
    }),
  clearLogs: (opId) => set((s) => ({ logs: s.logs.filter((l) => l.opId !== opId) })),
  selectedUpdates: [],
  toggleUpdate: (key) =>
    set((s) => {
      const id = artifactKeyId(key);
      return {
        selectedUpdates: s.selectedUpdates.includes(id)
          ? s.selectedUpdates.filter((x) => x !== id)
          : [...s.selectedUpdates, id],
      };
    }),
  clearSelectedUpdates: () => set({ selectedUpdates: [] }),
}));
```

- [ ] **Step 4: Run the tests, verify they pass**

Run: `pnpm exec vitest run src/lib/types.test.ts src/lib/api.test.ts src/store/ui.test.ts`
Expected: PASS (4 + 9 + 9 = 22 tests)

- [ ] **Step 5: Write failing tests for `queries.ts` and `events.ts`**

Create `src/lib/queries.test.ts`:

```ts
import { describe, expect, it, vi, beforeEach } from "vitest";
import React from "react";
import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { useSnapshot, useRefresh, usePlanOperation, useSubmitOperation } from "./queries";
import type { IssuedPlan, Snapshot } from "./types";

const mockInvoke = vi.mocked(invoke);

function wrapper(queryClient: QueryClient) {
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return React.createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

function newClient() {
  return new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
}

const snapshot: Snapshot = {
  generation: 1,
  detect: "Found",
  instances: [],
  artifacts: [],
  updates: [],
  refreshed_at: null,
  stale: false,
  errors: [],
};

beforeEach(() => {
  mockInvoke.mockReset();
});

describe("queries", () => {
  it("useSnapshot fetches through getSnapshot", async () => {
    mockInvoke.mockResolvedValueOnce(snapshot as never);
    const queryClient = newClient();
    const { result } = renderHook(() => useSnapshot(), { wrapper: wrapper(queryClient) });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual(snapshot);
    expect(mockInvoke).toHaveBeenCalledWith("get_snapshot");
  });

  it("useRefresh writes its result into the snapshot cache", async () => {
    mockInvoke.mockResolvedValueOnce({ ...snapshot, generation: 2 } as never);
    const queryClient = newClient();
    const { result } = renderHook(() => useRefresh(), { wrapper: wrapper(queryClient) });

    result.current.mutate();
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect((queryClient.getQueryData(["snapshot"]) as Snapshot).generation).toBe(2);
  });

  it("usePlanOperation calls planOperation and returns its IssuedPlan", async () => {
    const issued: IssuedPlan = {
      id: 1,
      plan: {
        request: { kind: "Uninstall", instance_id: "brew:/opt/homebrew", artifact_kind: "Formula", name: "jq" },
        program: "/opt/homebrew/bin/brew",
        args: ["uninstall", "--formula", "jq"],
        env: [],
        needs_password: false,
        locks: ["brew:/opt/homebrew"],
        cancel_policy: "KillThenReconcile",
        warnings: [],
        affected: [],
        timeout_secs: 1800,
      },
      issued_at: 1758000000,
    };
    mockInvoke.mockResolvedValueOnce(issued as never);
    const queryClient = newClient();
    const { result } = renderHook(() => usePlanOperation(), { wrapper: wrapper(queryClient) });

    result.current.mutate(issued.plan.request);
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual(issued);
  });

  it("useSubmitOperation invalidates the operations query on success", async () => {
    mockInvoke.mockResolvedValueOnce(9 as never);
    const queryClient = newClient();
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");
    const { result } = renderHook(() => useSubmitOperation(), { wrapper: wrapper(queryClient) });

    result.current.mutate(1);

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["operations"] });
  });
});
```

Create `src/lib/events.test.ts`:

```ts
import { describe, expect, it, vi, beforeEach } from "vitest";
import React from "react";
import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke, Channel } from "@tauri-apps/api/core";
import { useOperationEvents } from "./events";
import { useUiStore } from "../store/ui";
import { queryKeys } from "./queries";

const mockInvoke = vi.mocked(invoke);

function wrapper(queryClient: QueryClient) {
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return React.createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

beforeEach(() => {
  mockInvoke.mockReset();
  useUiStore.setState({ logs: [] });
});

describe("useOperationEvents", () => {
  it("appends streamed Log events to the ui store", async () => {
    let capturedChannel: InstanceType<typeof Channel> | null = null;
    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "subscribe_events") {
        capturedChannel = (args as { channel: InstanceType<typeof Channel> }).channel;
      }
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();

    renderHook(() => useOperationEvents(), { wrapper: wrapper(queryClient) });

    await waitFor(() => expect(capturedChannel).not.toBeNull());
    capturedChannel!.onmessage({
      Operation: { Log: { op_id: 1, stream: "Stdout", line: "Installing jq" } },
    });

    expect(useUiStore.getState().logs).toMatchObject([
      { opId: 1, stream: "Stdout", line: "Installing jq" },
    ]);
  });

  it("invalidates the snapshot query on SnapshotChanged", async () => {
    let capturedChannel: InstanceType<typeof Channel> | null = null;
    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "subscribe_events") {
        capturedChannel = (args as { channel: InstanceType<typeof Channel> }).channel;
      }
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");

    renderHook(() => useOperationEvents(), { wrapper: wrapper(queryClient) });

    await waitFor(() => expect(capturedChannel).not.toBeNull());
    capturedChannel!.onmessage({ SnapshotChanged: { generation: 4 } });

    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.snapshot });
  });
});
```

- [ ] **Step 6: Run the tests, verify they fail**

Run: `pnpm exec vitest run src/lib/queries.test.ts src/lib/events.test.ts`
Expected: FAIL — both error with "Failed to resolve import" for `./queries` and `./events`.

- [ ] **Step 7: Implement `queries.ts` and `events.ts`**

Create `src/lib/queries.ts`:

```ts
import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
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

export function useSnapshot(): UseQueryResult<Snapshot> {
  return useQuery({ queryKey: queryKeys.snapshot, queryFn: getSnapshot });
}

export function useSettings(): UseQueryResult<Settings> {
  return useQuery({ queryKey: queryKeys.settings, queryFn: getSettings });
}

export function useOperations(): UseQueryResult<OpSummary[]> {
  return useQuery({ queryKey: queryKeys.operations, queryFn: listOperations });
}

export function useRefresh(): UseMutationResult<Snapshot, Error, void> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: refresh,
    onSuccess: (snapshot) => {
      queryClient.setQueryData(queryKeys.snapshot, snapshot);
    },
  });
}

export function useSaveSettings(): UseMutationResult<void, Error, Settings> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: setSettings,
    onSuccess: (_data, settings) => {
      queryClient.setQueryData(queryKeys.settings, settings);
    },
  });
}

/** Plans, shows nothing itself; callers render the IssuedPlan's Plan then submit its id. */
export function usePlanOperation(): UseMutationResult<IssuedPlan, Error, OpRequest> {
  return useMutation({ mutationFn: planOperation });
}

export function useSubmitOperation(): UseMutationResult<number, Error, number> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: submitOperation,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.operations });
    },
  });
}

export function useCancelOperation(): UseMutationResult<void, Error, number> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: cancelOperation,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.operations });
    },
  });
}
```

Create `src/lib/events.ts`:

```ts
import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { subscribeEvents } from "./api";
import { queryKeys } from "./queries";
import { useUiStore } from "../store/ui";
import type { UiEvent } from "./types";

/**
 * Mounted once (by `App`, in Task 13) to bridge the backend's Channel into
 * React state: `Operation.Log` events are appended to the Zustand log ring
 * buffer, `Operation.Status`/`Operation.Finished` invalidate the operations
 * query, and `SnapshotChanged` invalidates the snapshot query. Not part of
 * the skeleton's Core Interfaces — introduced here because `events.ts` needs
 * a concrete hook shape and none was specified.
 */
export function useOperationEvents(): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    let detach: (() => void) | undefined;
    let cancelled = false;

    function handle(event: UiEvent) {
      if ("Operation" in event) {
        const opEvent = event.Operation;
        if ("Log" in opEvent) {
          useUiStore.getState().appendLog({
            opId: opEvent.Log.op_id,
            stream: opEvent.Log.stream,
            line: opEvent.Log.line,
          });
        } else {
          queryClient.invalidateQueries({ queryKey: queryKeys.operations });
        }
      } else {
        queryClient.invalidateQueries({ queryKey: queryKeys.snapshot });
      }
    }

    subscribeEvents(handle).then((unsubscribe) => {
      if (cancelled) {
        unsubscribe();
      } else {
        detach = unsubscribe;
      }
    });

    return () => {
      cancelled = true;
      detach?.();
    };
  }, [queryClient]);
}
```

- [ ] **Step 8: Run the tests, verify they pass**

Run: `pnpm exec vitest run src/lib/queries.test.ts src/lib/events.test.ts`
Expected: PASS (4 + 2 = 6 tests)

- [ ] **Step 9: Run the full suite**

Run: `pnpm test`
Expected: PASS — every test file from Tasks 9 and 10 green, 0 failures.

- [ ] **Step 10: Commit**

```bash
git add src/lib/types.ts src/lib/api.ts src/lib/events.ts src/lib/queries.ts src/store/ui.ts src/lib/types.test.ts src/lib/api.test.ts src/store/ui.test.ts src/lib/queries.test.ts src/lib/events.test.ts
git commit -m "$(cat <<'EOF'
feat(ui): add typed IPC client, TanStack Query hooks and ui store

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 11: Installed page

**Files:**
- Create: `src/components/ArtifactRow.tsx`
- Create: `src/pages/InstalledPage.tsx`
- Test: `src/components/ArtifactRow.test.tsx`
- Test: `src/pages/InstalledPage.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/components/Sidebar.tsx`
- Modify: `src/i18n/en.json`

**Interfaces:**
- Consumes: `useSnapshot`, `useSettings`, `usePlanOperation` (`src/lib/queries.ts`, Task 10); `useUiStore`, `artifactKeyId`, `type Page` (`src/store/ui.ts`, Task 10); `InstalledArtifact`, `OpRequest` (`src/lib/types.ts`, Task 10); `Sidebar` (`src/components/Sidebar.tsx`, Task 9); `renderWithProviders` (Task 9).
- Produces: `ArtifactRow` and its prop types `ArtifactRowProps`, `ArtifactRowSelectable`, `BadgeVariant` (`src/components/ArtifactRow.tsx`, not in the skeleton — designed so Task 12's Updates page reuses it via the `selectable` and `secondaryContent` slots); `InstalledPage` (`src/pages/InstalledPage.tsx`). `App.tsx` is modified to route the real `Page` type and `useUiStore` (replacing Task 9's local `useState` placeholder) instead of the `SidebarPage`/local-state scaffold Task 9 built. `Sidebar.tsx` is modified in the same step to import that same `Page` type from `../store/ui` instead of declaring its own, now-redundant `SidebarPage` union — later tasks (12, 13) build on this same `App.tsx`.

- [ ] **Step 1: Write failing tests for `ArtifactRow`**

Create `src/components/ArtifactRow.test.tsx`:

```tsx
import { describe, expect, it, vi } from "vitest";
import { renderWithProviders } from "../test/setup";
import { ArtifactRow } from "./ArtifactRow";

describe("ArtifactRow", () => {
  it("renders name, description and badge, and fires the primary action", () => {
    const onPrimaryAction = vi.fn();
    const { getByText, getByRole } = renderWithProviders(
      <ArtifactRow
        name="jq"
        description="Lightweight and flexible command-line JSON processor"
        badgeText="Up to date"
        badgeVariant="neutral"
        primaryActionLabel="Uninstall"
        onPrimaryAction={onPrimaryAction}
      />,
    );

    expect(getByText("jq")).toBeInTheDocument();
    expect(
      getByText("Lightweight and flexible command-line JSON processor"),
    ).toBeInTheDocument();
    expect(getByText("Up to date")).toBeInTheDocument();

    getByRole("button", { name: "Uninstall" }).click();
    expect(onPrimaryAction).toHaveBeenCalledTimes(1);
  });

  it("disables the primary action when primaryActionDisabled is set", () => {
    const { getByRole } = renderWithProviders(
      <ArtifactRow
        name="jq"
        description="desc"
        badgeText="Up to date"
        badgeVariant="neutral"
        primaryActionLabel="Uninstall"
        onPrimaryAction={vi.fn()}
        primaryActionDisabled
      />,
    );

    expect(getByRole("button", { name: "Uninstall" })).toBeDisabled();
  });

  it("renders a checkbox and calls onToggle when selectable", () => {
    const onToggle = vi.fn();
    const { getByRole } = renderWithProviders(
      <ArtifactRow
        name="glib"
        description="desc"
        badgeText="Update"
        badgeVariant="info"
        primaryActionLabel="Update"
        onPrimaryAction={vi.fn()}
        selectable={{ checked: false, onToggle, ariaLabel: "Select glib for update" }}
      />,
    );

    getByRole("checkbox", { name: "Select glib for update" }).click();
    expect(onToggle).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: Run the test, verify it fails**

Run: `pnpm exec vitest run src/components/ArtifactRow.test.tsx`
Expected: FAIL — `Failed to resolve import "./ArtifactRow"`.

- [ ] **Step 3: Implement `ArtifactRow`**

Create `src/components/ArtifactRow.tsx`:

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
  primaryActionLabel: string;
  onPrimaryAction: () => void;
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
      <button
        type="button"
        onClick={onPrimaryAction}
        disabled={primaryActionDisabled}
        className="shrink-0 rounded-md bg-[var(--color-accent)] px-3 py-1 text-sm font-medium text-[var(--color-accent-foreground)] disabled:opacity-50"
      >
        {primaryActionLabel}
      </button>
    </div>
  );
}
```

- [ ] **Step 4: Run the test, verify it passes**

Run: `pnpm exec vitest run src/components/ArtifactRow.test.tsx`
Expected: PASS (3 tests)

- [ ] **Step 5: Add i18n keys, write a failing test for `InstalledPage`**

Replace `src/i18n/en.json`:

```json
{
  "app": {
    "title": "Canager",
    "operationBarRegion": "Operation status"
  },
  "nav": {
    "label": "Sections",
    "installed": "Installed",
    "updates": "Updates",
    "settings": "Settings"
  },
  "common": {
    "loading": "Loading…"
  },
  "adapters": {
    "brew": "Homebrew"
  },
  "installed": {
    "filterLabel": "Filter installed items",
    "filterPlaceholder": "Search installed items",
    "uninstall": "Uninstall",
    "upToDate": "Up to date",
    "updateAvailable": "Update available",
    "noDescription": "No description available",
    "showDependencies_one": "{{count}} component installed by other software",
    "showDependencies_other": "{{count}} components installed by other software"
  }
}
```

Create `src/pages/InstalledPage.test.tsx`:

```tsx
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { fireEvent } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { InstalledPage } from "./InstalledPage";
import type { Settings, Snapshot } from "../lib/types";

const mockInvoke = vi.mocked(invoke);

const snapshot: Snapshot = {
  generation: 1,
  detect: "Found",
  instances: [
    {
      id: "brew:/opt/homebrew",
      adapter_id: "brew",
      exe_path: "/opt/homebrew/bin/brew",
      prefix: "/opt/homebrew",
      scope: "User",
      version: "7.0.3",
      healthy: true,
    },
  ],
  artifacts: [
    {
      key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" },
      display_name: "jq",
      version: "1.8.2",
      reason: "Requested",
      description: "Lightweight and flexible command-line JSON processor",
      homepage: "https://jqlang.github.io/jq/",
      size_bytes: null,
      installed_at: 1783762037,
      path: null,
      auto_updates: false,
    },
    {
      key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "glib" },
      display_name: "glib",
      version: "2.88.3",
      reason: "Dependency",
      description: "Core application library for C",
      homepage: "https://docs.gtk.org/glib/",
      size_bytes: null,
      installed_at: 1788244409,
      path: null,
      auto_updates: false,
    },
  ],
  updates: [
    {
      key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "glib" },
      current: "2.88.3",
      target: "2.90.0",
      channel: "Native",
      checkable: true,
      warnings: [],
    },
  ],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
};

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
};

beforeEach(() => {
  mockInvoke.mockReset();
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    width: 800,
    height: 600,
    top: 0,
    left: 0,
    bottom: 600,
    right: 800,
    x: 0,
    y: 0,
    toJSON: () => {},
  } as DOMRect);
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(snapshot);
    if (cmd === "get_settings") return Promise.resolve(settings);
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("InstalledPage", () => {
  it("shows the requested artifact and collapses the dependency behind a toggle", async () => {
    const { findByText, queryByText, getByRole } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    expect(queryByText("glib")).not.toBeInTheDocument();
    expect(
      getByRole("button", { name: "1 component installed by other software" }),
    ).toBeInTheDocument();
  });

  it("reveals the dependency once the toggle is clicked", async () => {
    const { findByText, getByRole } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    getByRole("button", { name: "1 component installed by other software" }).click();

    await findByText("glib");
  });

  it("filters rows by the query box", async () => {
    const { findByText, queryByText, getByLabelText } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    fireEvent.change(getByLabelText("Filter installed items"), {
      target: { value: "nonexistent" },
    });

    expect(queryByText("jq")).not.toBeInTheDocument();
  });

  it("plans an uninstall when the row's primary button is clicked", async () => {
    const { findByText, getByRole } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    getByRole("button", { name: "Uninstall" }).click();

    expect(mockInvoke).toHaveBeenCalledWith("plan_operation", {
      request: {
        kind: "Uninstall",
        instance_id: "brew:/opt/homebrew",
        artifact_kind: "Formula",
        name: "jq",
      },
    });
  });
});
```

- [ ] **Step 6: Run the test, verify it fails**

Run: `pnpm exec vitest run src/pages/InstalledPage.test.tsx`
Expected: FAIL — `Failed to resolve import "./InstalledPage"`.

- [ ] **Step 7: Implement `InstalledPage`**

Create `src/pages/InstalledPage.tsx`:

```tsx
import { useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useSnapshot, useSettings, usePlanOperation } from "../lib/queries";
import { useUiStore, artifactKeyId } from "../store/ui";
import { ArtifactRow } from "../components/ArtifactRow";
import type { InstalledArtifact } from "../lib/types";

const ADAPTER_LABEL_KEYS: Record<string, string> = {
  brew: "adapters.brew",
};

type ListItem =
  | { type: "group"; instanceId: string; label: string }
  | { type: "artifact"; artifact: InstalledArtifact }
  | { type: "toggle"; instanceId: string; hiddenCount: number };

export function InstalledPage() {
  const { t } = useTranslation();
  const { data: snapshot, isLoading } = useSnapshot();
  const { data: settings } = useSettings();
  const planMutation = usePlanOperation();
  const query = useUiStore((s) => s.query);
  const setQuery = useUiStore((s) => s.setQuery);
  const showDependencies = useUiStore((s) => s.showDependencies);
  const toggleDependencies = useUiStore((s) => s.toggleDependencies);
  const parentRef = useRef<HTMLDivElement>(null);

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
      const artifacts = byInstance.get(instance.id);
      if (!artifacts || artifacts.length === 0) continue;
      const labelKey = ADAPTER_LABEL_KEYS[instance.adapter_id];
      result.push({
        type: "group",
        instanceId: instance.id,
        label: labelKey ? t(labelKey) : instance.adapter_id,
      });
      const primary = artifacts.filter((a) => a.reason === "Requested");
      const dependencies = artifacts.filter((a) => a.reason !== "Requested");
      for (const artifact of primary) {
        result.push({ type: "artifact", artifact });
      }
      if (dependencies.length > 0) {
        if (showDependencies) {
          for (const artifact of dependencies) {
            result.push({ type: "artifact", artifact });
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
                  <p className="px-4 py-2 text-xs font-semibold uppercase text-[var(--color-muted)]">
                    {item.label}
                  </p>
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
                        ? `${item.artifact.display_name} · ${item.artifact.version}`
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
                    primaryActionLabel={t("installed.uninstall")}
                    onPrimaryAction={() =>
                      planMutation.mutate({
                        kind: "Uninstall",
                        instance_id: item.artifact.key.instance_id,
                        artifact_kind: item.artifact.key.kind,
                        name: item.artifact.key.name,
                      })
                    }
                  />
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 8: Run the test, verify it passes**

Run: `pnpm exec vitest run src/pages/InstalledPage.test.tsx`
Expected: PASS (4 tests)

- [ ] **Step 9: Wire `InstalledPage` into the app shell**

Replace `src/App.tsx`:

```tsx
import { useTranslation } from "react-i18next";
import { Sidebar } from "./components/Sidebar";
import { InstalledPage } from "./pages/InstalledPage";
import { useUiStore } from "./store/ui";

function App() {
  const { t } = useTranslation();
  const page = useUiStore((s) => s.page);
  const setPage = useUiStore((s) => s.setPage);

  return (
    <div className="flex h-screen flex-col bg-[var(--color-background)] text-[var(--color-foreground)]">
      <div className="flex flex-1 overflow-hidden">
        <Sidebar page={page} onSelectPage={setPage} />
        <main className="flex-1 overflow-y-auto">
          {page === "installed" ? (
            <InstalledPage />
          ) : (
            <h1 className="p-6 text-lg font-semibold">{t(`nav.${page}`)}</h1>
          )}
        </main>
      </div>
      <footer
        aria-label={t("app.operationBarRegion")}
        className="h-12 shrink-0 border-t border-[var(--color-border)]"
      />
    </div>
  );
}

export default App;
```

Replace `src/App.test.tsx` (the default page now renders `InstalledPage`'s own content instead of a generic heading, and the store is global so the invoke mock must satisfy it):

```tsx
import { describe, expect, it, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "./test/setup";
import App from "./App";

const mockInvoke = vi.mocked(invoke);

const emptySnapshot = {
  generation: 0,
  detect: "Found",
  instances: [],
  artifacts: [],
  updates: [],
  refreshed_at: null,
  stale: false,
  errors: [],
};

const defaultSettings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
};

beforeEach(() => {
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(emptySnapshot);
    if (cmd === "get_settings") return Promise.resolve(defaultSettings);
    return Promise.resolve(undefined);
  });
});

describe("App", () => {
  it("shows the Installed page's filter box by default", async () => {
    const { findByLabelText } = renderWithProviders(<App />);
    await findByLabelText("Filter installed items");
  });

  it("switches the content area when a sidebar link is clicked", async () => {
    const { getByRole, findByLabelText } = renderWithProviders(<App />);
    await findByLabelText("Filter installed items");

    getByRole("button", { name: "Updates" }).click();

    expect(getByRole("heading", { name: "Updates" })).toBeInTheDocument();
  });
});
```

- [ ] **Step 10: Update `Sidebar` to use the shared `Page` type instead of its own `SidebarPage`**

`Sidebar.tsx` (Task 9) still declares its own `export type SidebarPage = "installed" | "updates" | "settings"` — a union that is structurally identical to, but a separate declaration from, `Page` in `src/store/ui.ts` (Task 10). Now that `App.tsx` (this task, Step 9 above) routes on the real `Page` type from the store, `Sidebar` should import that same type rather than keep a redundant, hand-duplicated copy of it. Replace the top of `src/components/Sidebar.tsx`:

```tsx
import { useTranslation } from "react-i18next";

export type SidebarPage = "installed" | "updates" | "settings";

interface SidebarProps {
  page: SidebarPage;
  onSelectPage: (page: SidebarPage) => void;
}

const PAGES: SidebarPage[] = ["installed", "updates", "settings"];
```

with:

```tsx
import { useTranslation } from "react-i18next";
import type { Page } from "../store/ui";

interface SidebarProps {
  page: Page;
  onSelectPage: (page: Page) => void;
}

const PAGES: Page[] = ["installed", "updates", "settings"];
```

The rest of `Sidebar.tsx` (the `Sidebar` function body) is unchanged — only the type it uses for `page`/`onSelectPage` changes name and home. `Sidebar.test.tsx` (Task 9) passes the literal strings `"installed"`/`"updates"` either way, so it needs no changes and must still pass.

- [ ] **Step 11: Run the full suite**

Run: `pnpm test`
Expected: PASS — every test file from Tasks 9–11 green, 0 failures, including `Sidebar.test.tsx` unchanged.

- [ ] **Step 12: Commit**

```bash
git add src/components/ArtifactRow.tsx src/components/ArtifactRow.test.tsx src/pages/InstalledPage.tsx src/pages/InstalledPage.test.tsx src/App.tsx src/App.test.tsx src/components/Sidebar.tsx src/i18n/en.json
git commit -m "$(cat <<'EOF'
feat(ui): add the Installed page with grouping, filtering and uninstall planning

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 12: Updates page

**Files:**
- Create: `src/components/CommandPreview.tsx`
- Create: `src/pages/UpdatesPage.tsx`
- Test: `src/components/CommandPreview.test.tsx`
- Test: `src/pages/UpdatesPage.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx`
- Modify: `src/i18n/en.json`

**Interfaces:**
- Consumes: `ArtifactRow` and its `selectable`/`secondaryContent` props (`src/components/ArtifactRow.tsx`, Task 11); `useSnapshot`, `useSettings`, `useSaveSettings`, `usePlanOperation`, `useSubmitOperation` (`src/lib/queries.ts`, Task 10); `useUiStore` (`selectedUpdates`, `toggleUpdate`, `clearSelectedUpdates`), `artifactKeyId` (`src/store/ui.ts`, Task 10); `OpRequest`, `IssuedPlan`, `UpdateCandidate` (`src/lib/types.ts`, Task 10); `Dialog({ open, onOpenChange, title, children, footer? })` (`src/components/ui/Dialog.tsx`, Task 9).
- Produces: `CommandPreview` (`src/components/CommandPreview.tsx`, not in the skeleton — renders a `Plan`'s `program`/`args` as the exact command that will run, satisfying the Global Constraint that every destructive action previews its command before running; Task 14's uninstall dialog reuses this same component rather than creating its own); `UpdatesPage` (`src/pages/UpdatesPage.tsx`). Both single-row Update and multi-select "Update selected" route through one shared confirmation dialog inside `UpdatesPage` before calling `useSubmitOperation`, so an update never runs without the operator having seen its command first.

- [ ] **Step 1: Add i18n keys**

Replace `src/i18n/en.json`:

```json
{
  "app": {
    "title": "Canager",
    "operationBarRegion": "Operation status"
  },
  "nav": {
    "label": "Sections",
    "installed": "Installed",
    "updates": "Updates",
    "settings": "Settings"
  },
  "common": {
    "loading": "Loading…",
    "cancel": "Cancel"
  },
  "adapters": {
    "brew": "Homebrew"
  },
  "installed": {
    "filterLabel": "Filter installed items",
    "filterPlaceholder": "Search installed items",
    "uninstall": "Uninstall",
    "upToDate": "Up to date",
    "updateAvailable": "Update available",
    "noDescription": "No description available",
    "showDependencies_one": "{{count}} component installed by other software",
    "showDependencies_other": "{{count}} components installed by other software"
  },
  "updates": {
    "upToDate": "Everything is up to date",
    "count_one": "{{count}} update available",
    "count_other": "{{count}} updates available",
    "updateSelected": "Update selected",
    "update": "Update",
    "ignore": "Ignore",
    "available": "Update",
    "warnings_one": "{{count}} warning",
    "warnings_other": "{{count}} warnings",
    "versionChange": "{{current}} → {{target}}",
    "selectRow": "Select {{name}} for update",
    "confirmTitle": "Confirm update",
    "confirmUpdate": "Confirm"
  },
  "commandPreview": {
    "label": "This will run:"
  }
}
```

`Dialog` itself already exists (`src/components/ui/Dialog.tsx`, Task 9) — this task only imports it, in `UpdatesPage` below.

- [ ] **Step 2: Write a failing test for `CommandPreview`**

Create `src/components/CommandPreview.test.tsx`:

```tsx
import { describe, expect, it } from "vitest";
import { renderWithProviders } from "../test/setup";
import { CommandPreview } from "./CommandPreview";

describe("CommandPreview", () => {
  it("renders the joined program and arguments under a label", () => {
    const { getByText } = renderWithProviders(
      <CommandPreview program="/opt/homebrew/bin/brew" args={["upgrade", "--cask", "onyx"]} />,
    );

    expect(getByText("This will run:")).toBeInTheDocument();
    expect(getByText("/opt/homebrew/bin/brew upgrade --cask onyx")).toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Run the test, verify it fails**

Run: `pnpm exec vitest run src/components/CommandPreview.test.tsx`
Expected: FAIL — `Failed to resolve import "./CommandPreview"`.

- [ ] **Step 4: Implement `CommandPreview`**

Create `src/components/CommandPreview.tsx`:

```tsx
import { useTranslation } from "react-i18next";

export interface CommandPreviewProps {
  program: string;
  args: string[];
}

export function CommandPreview({ program, args }: CommandPreviewProps) {
  const { t } = useTranslation();
  return (
    <div>
      <p className="text-xs font-medium uppercase text-[var(--color-muted)]">
        {t("commandPreview.label")}
      </p>
      <code className="mt-1 block overflow-x-auto rounded-md bg-[var(--color-hover)] px-3 py-2 text-xs text-[var(--color-foreground)]">
        {[program, ...args].join(" ")}
      </code>
    </div>
  );
}
```

- [ ] **Step 5: Run the test, verify it passes**

Run: `pnpm exec vitest run src/components/CommandPreview.test.tsx`
Expected: PASS (1 test)

- [ ] **Step 6: Write a failing test for `UpdatesPage`**

Create `src/pages/UpdatesPage.test.tsx`:

```tsx
import { describe, expect, it, vi, beforeEach } from "vitest";
import { waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { UpdatesPage } from "./UpdatesPage";
import type { Settings, Snapshot } from "../lib/types";

const mockInvoke = vi.mocked(invoke);

const snapshot: Snapshot = {
  generation: 2,
  detect: "Found",
  instances: [],
  artifacts: [],
  updates: [
    {
      key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "glib" },
      current: "2.88.3",
      target: "2.90.0",
      channel: "Native",
      checkable: true,
      warnings: [],
    },
    {
      key: { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "onyx" },
      current: "5.0.2",
      target: "5.1.0",
      channel: "Native",
      checkable: true,
      warnings: [],
    },
  ],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
};

let settings: Settings;

beforeEach(() => {
  settings = {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
  };
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === "get_snapshot") return Promise.resolve(snapshot);
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "set_settings") {
      settings = (args as { settings: Settings }).settings;
      return Promise.resolve(undefined);
    }
    if (cmd === "plan_operation") {
      const request = (
        args as { request: { instance_id: string; artifact_kind: string; name: string } }
      ).request;
      return Promise.resolve({
        id: 1,
        plan: {
          request,
          program: "/opt/homebrew/bin/brew",
          args: ["upgrade", request.artifact_kind === "Cask" ? "--cask" : "--formula", request.name],
          env: [],
          needs_password: false,
          locks: ["brew:/opt/homebrew"],
          cancel_policy: "KillThenReconcile",
          warnings: [],
          affected: [],
          timeout_secs: 1800,
        },
        issued_at: 1758000000,
      });
    }
    if (cmd === "submit_operation") return Promise.resolve(1);
    return Promise.resolve(undefined);
  });
});

describe("UpdatesPage", () => {
  it("lists each update with its version change", async () => {
    const { findByText } = renderWithProviders(<UpdatesPage />);

    await findByText("2.88.3 → 2.90.0");
    await findByText("5.0.2 → 5.1.0");
  });

  it("plans and, after confirming, submits a single update", async () => {
    const { findAllByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    const updateButtons = await findAllByRole("button", { name: "Update" });
    updateButtons[0].click();

    await findByRole("dialog");
    (await findByRole("button", { name: "Confirm" })).click();

    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("submit_operation", expect.anything()),
    );
    expect(mockInvoke).toHaveBeenCalledWith("plan_operation", {
      request: {
        kind: "Upgrade",
        instance_id: "brew:/opt/homebrew",
        artifact_kind: "Formula",
        name: "glib",
      },
    });
  });

  it("submits one operation per selected item from Update selected", async () => {
    const { findAllByRole, getByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    const checkboxes = await findAllByRole("checkbox");
    checkboxes[0].click();
    checkboxes[1].click();

    getByRole("button", { name: "Update selected" }).click();
    await findByRole("dialog");
    (await findByRole("button", { name: "Confirm" })).click();

    await waitFor(() => {
      const submitCalls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "submit_operation");
      expect(submitCalls).toHaveLength(2);
    });
  });

  it("removes an item from the list when Ignore is clicked", async () => {
    const { findByText, queryByText, findAllByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("2.88.3 → 2.90.0");
    const ignoreButtons = await findAllByRole("button", { name: "Ignore" });
    ignoreButtons[0].click();

    await waitFor(() => expect(queryByText("2.88.3 → 2.90.0")).not.toBeInTheDocument());
  });

  it("shows the up-to-date state once every update is ignored", async () => {
    settings.ignored_updates = [
      { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "glib" },
      { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "onyx" },
    ];
    const { findByText } = renderWithProviders(<UpdatesPage />);

    await findByText("Everything is up to date");
  });
});
```

- [ ] **Step 7: Run the test, verify it fails**

Run: `pnpm exec vitest run src/pages/UpdatesPage.test.tsx`
Expected: FAIL — `Failed to resolve import "./UpdatesPage"`.

- [ ] **Step 8: Implement `UpdatesPage`**

Create `src/pages/UpdatesPage.tsx`:

```tsx
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  useSnapshot,
  useSettings,
  useSaveSettings,
  usePlanOperation,
  useSubmitOperation,
} from "../lib/queries";
import { useUiStore, artifactKeyId } from "../store/ui";
import { ArtifactRow } from "../components/ArtifactRow";
import { CommandPreview } from "../components/CommandPreview";
import { Dialog } from "../components/ui/Dialog";
import type { IssuedPlan, OpRequest, UpdateCandidate } from "../lib/types";

function toRequest(candidate: UpdateCandidate): OpRequest {
  return {
    kind: "Upgrade",
    instance_id: candidate.key.instance_id,
    artifact_kind: candidate.key.kind,
    name: candidate.key.name,
  };
}

export function UpdatesPage() {
  const { t } = useTranslation();
  const { data: snapshot, isLoading } = useSnapshot();
  const { data: settings } = useSettings();
  const saveSettings = useSaveSettings();
  const planMutation = usePlanOperation();
  const submitMutation = useSubmitOperation();
  const selectedUpdates = useUiStore((s) => s.selectedUpdates);
  const toggleUpdate = useUiStore((s) => s.toggleUpdate);
  const clearSelectedUpdates = useUiStore((s) => s.clearSelectedUpdates);

  const [pendingPlans, setPendingPlans] = useState<IssuedPlan[] | null>(null);

  const visibleUpdates = useMemo(() => {
    if (!snapshot || !settings) return [];
    const ignored = new Set(settings.ignored_updates.map((k) => artifactKeyId(k)));
    return snapshot.updates.filter((u) => !ignored.has(artifactKeyId(u.key)));
  }, [snapshot, settings]);

  async function openConfirm(candidates: UpdateCandidate[]) {
    const plans = await Promise.all(candidates.map((c) => planMutation.mutateAsync(toRequest(c))));
    setPendingPlans(plans);
  }

  async function confirmAndSubmit() {
    if (!pendingPlans) return;
    for (const issued of pendingPlans) {
      await submitMutation.mutateAsync(issued.id);
    }
    clearSelectedUpdates();
    setPendingPlans(null);
  }

  function ignore(candidate: UpdateCandidate) {
    if (!settings) return;
    saveSettings.mutate({
      ...settings,
      ignored_updates: [...settings.ignored_updates, candidate.key],
    });
  }

  if (isLoading) {
    return <p className="p-4 text-sm text-[var(--color-muted)]">{t("common.loading")}</p>;
  }
  if (!snapshot || !settings) {
    return null;
  }

  if (visibleUpdates.length === 0) {
    return <p className="p-4 text-sm text-[var(--color-muted)]">{t("updates.upToDate")}</p>;
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b border-[var(--color-border)] p-4">
        <p className="text-sm text-[var(--color-muted)]">
          {t("updates.count", { count: visibleUpdates.length })}
        </p>
        <button
          type="button"
          disabled={selectedUpdates.length === 0}
          onClick={() =>
            openConfirm(visibleUpdates.filter((u) => selectedUpdates.includes(artifactKeyId(u.key))))
          }
          className="rounded-md bg-[var(--color-accent)] px-3 py-1 text-sm font-medium text-[var(--color-accent-foreground)] disabled:opacity-50"
        >
          {t("updates.updateSelected")}
        </button>
      </div>
      <div className="flex-1 overflow-y-auto">
        {visibleUpdates.map((candidate) => (
          <ArtifactRow
            key={artifactKeyId(candidate.key)}
            name={candidate.key.name}
            description={t("updates.versionChange", {
              current: candidate.current,
              target: candidate.target,
            })}
            badgeText={
              candidate.warnings.length > 0
                ? t("updates.warnings", { count: candidate.warnings.length })
                : t("updates.available")
            }
            badgeVariant={candidate.warnings.length > 0 ? "warning" : "info"}
            primaryActionLabel={t("updates.update")}
            onPrimaryAction={() => openConfirm([candidate])}
            selectable={{
              checked: selectedUpdates.includes(artifactKeyId(candidate.key)),
              onToggle: () => toggleUpdate(candidate.key),
              ariaLabel: t("updates.selectRow", { name: candidate.key.name }),
            }}
            secondaryContent={
              <button
                type="button"
                onClick={() => ignore(candidate)}
                className="shrink-0 text-xs text-[var(--color-muted)] underline"
              >
                {t("updates.ignore")}
              </button>
            }
          />
        ))}
      </div>
      <Dialog
        open={pendingPlans !== null}
        onOpenChange={(open) => {
          if (!open) setPendingPlans(null);
        }}
        title={t("updates.confirmTitle")}
        footer={
          <>
            <button
              type="button"
              onClick={() => setPendingPlans(null)}
              className="rounded-md px-3 py-1 text-sm"
            >
              {t("common.cancel")}
            </button>
            <button
              type="button"
              onClick={confirmAndSubmit}
              className="rounded-md bg-[var(--color-accent)] px-3 py-1 text-sm font-medium text-[var(--color-accent-foreground)]"
            >
              {t("updates.confirmUpdate")}
            </button>
          </>
        }
      >
        <div className="flex flex-col gap-3">
          {(pendingPlans ?? []).map((issued, index) => (
            <CommandPreview key={index} program={issued.plan.program} args={issued.plan.args} />
          ))}
        </div>
      </Dialog>
    </div>
  );
}
```

- [ ] **Step 9: Run the test, verify it passes**

Run: `pnpm exec vitest run src/pages/UpdatesPage.test.tsx`
Expected: PASS (5 tests)

- [ ] **Step 10: Wire `UpdatesPage` into the app shell**

Modify `src/App.tsx` (only the highlighted lines change; full file shown):

```tsx
import { useTranslation } from "react-i18next";
import { Sidebar } from "./components/Sidebar";
import { InstalledPage } from "./pages/InstalledPage";
import { UpdatesPage } from "./pages/UpdatesPage";
import { useUiStore } from "./store/ui";

function App() {
  const { t } = useTranslation();
  const page = useUiStore((s) => s.page);
  const setPage = useUiStore((s) => s.setPage);

  return (
    <div className="flex h-screen flex-col bg-[var(--color-background)] text-[var(--color-foreground)]">
      <div className="flex flex-1 overflow-hidden">
        <Sidebar page={page} onSelectPage={setPage} />
        <main className="flex-1 overflow-y-auto">
          {page === "installed" ? (
            <InstalledPage />
          ) : page === "updates" ? (
            <UpdatesPage />
          ) : (
            <h1 className="p-6 text-lg font-semibold">{t(`nav.${page}`)}</h1>
          )}
        </main>
      </div>
      <footer
        aria-label={t("app.operationBarRegion")}
        className="h-12 shrink-0 border-t border-[var(--color-border)]"
      />
    </div>
  );
}

export default App;
```

Run: `pnpm test`
Expected: FAIL — one assertion in `src/App.test.tsx` ("switches the content area when a sidebar link is clicked") now reads `Unable to find an accessible element with the role "heading" and name "Updates"`. That is correct: clicking Updates used to fall through to the generic `<h1>{t(`nav.${page}`)}</h1>`, and this step just replaced that fallback with the real `UpdatesPage`, which has no heading. Step 11 retargets the assertion.

- [ ] **Step 11: Retarget the App shell test at what the real Updates page renders**

The shell test must assert on content the Updates page actually shows, not on the placeholder heading this task removed. With `updates: []` in the test's mocked snapshot, `UpdatesPage` renders its up-to-date message. Replace only that one test in `src/App.test.tsx`, leaving the rest of the file untouched:

```tsx
  it("switches the content area when a sidebar link is clicked", async () => {
    const { getByRole, findByLabelText, findByText } = renderWithProviders(<App />);
    await findByLabelText("Filter installed items");

    getByRole("button", { name: "Updates" }).click();

    expect(await findByText("Everything is up to date")).toBeInTheDocument();
  });
```

Run: `pnpm test`
Expected: PASS — every test file from Tasks 9–12 green, 0 failures.

- [ ] **Step 12: Commit**

```bash
git add src/components/CommandPreview.tsx src/components/CommandPreview.test.tsx src/pages/UpdatesPage.tsx src/pages/UpdatesPage.test.tsx src/App.tsx src/App.test.tsx src/i18n/en.json
git commit -m "$(cat <<'EOF'
feat(ui): add the Updates page with command-previewed single and batch updates

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 13: Operation bar + log drawer

**Files:**
- Create: `src/components/ui/ScrollArea.tsx`
- Create: `src/components/OperationBar.tsx`
- Create: `src/components/LogDrawer.tsx`
- Test: `src/components/OperationBar.test.tsx`
- Test: `src/components/LogDrawer.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/i18n/en.json`

**Interfaces:**
- Consumes: `useOperations`, `useCancelOperation` (`src/lib/queries.ts`, Task 10); `useOperationEvents` (`src/lib/events.ts`, Task 10); `useUiStore` (`drawerOpen`, `setDrawerOpen`, `focusedOpId`, `setFocusedOpId`, `logs`) (`src/store/ui.ts`, Task 10); `OpStatus`, `Outcome` (`src/lib/types.ts`, Task 10).
- Produces: `ScrollArea` (`src/components/ui/ScrollArea.tsx`, a thin Radix ScrollArea wrapper — not in the skeleton); `OperationBar`, `LogDrawer`. `App.tsx` is modified to mount both and to call `useOperationEvents()` once at the top level, which is what starts the Channel subscription for the whole app.

- [ ] **Step 1: Add i18n keys and the ScrollArea wrapper**

Replace `src/i18n/en.json`:

```json
{
  "app": {
    "title": "Canager",
    "operationBarRegion": "Operation status"
  },
  "nav": {
    "label": "Sections",
    "installed": "Installed",
    "updates": "Updates",
    "settings": "Settings"
  },
  "common": {
    "loading": "Loading…",
    "cancel": "Cancel",
    "close": "Close"
  },
  "adapters": {
    "brew": "Homebrew"
  },
  "installed": {
    "filterLabel": "Filter installed items",
    "filterPlaceholder": "Search installed items",
    "uninstall": "Uninstall",
    "upToDate": "Up to date",
    "updateAvailable": "Update available",
    "noDescription": "No description available",
    "showDependencies_one": "{{count}} component installed by other software",
    "showDependencies_other": "{{count}} components installed by other software"
  },
  "updates": {
    "upToDate": "Everything is up to date",
    "count_one": "{{count}} update available",
    "count_other": "{{count}} updates available",
    "updateSelected": "Update selected",
    "update": "Update",
    "ignore": "Ignore",
    "available": "Update",
    "warnings_one": "{{count}} warning",
    "warnings_other": "{{count}} warnings",
    "versionChange": "{{current}} → {{target}}",
    "selectRow": "Select {{name}} for update",
    "confirmTitle": "Confirm update",
    "confirmUpdate": "Confirm"
  },
  "commandPreview": {
    "label": "This will run"
  },
  "operations": {
    "idle": "No operation running",
    "cancel": "Cancel",
    "logDrawerTitle": "Operation log",
    "kind": {
      "Install": "Installing",
      "Uninstall": "Uninstalling",
      "Upgrade": "Updating"
    },
    "status": {
      "Queued": "queued",
      "Running": "running",
      "CancelRequested": "cancelling",
      "Cancelling": "cancelling",
      "Verifying": "verifying",
      "Done": "done"
    },
    "outcome": {
      "Succeeded": "Succeeded",
      "NoChange": "No change needed",
      "PartialSuccess": "Partially succeeded",
      "Unconfirmed": "Result unconfirmed",
      "NeedsAttention": "Needs attention: {{message}}",
      "Failed": "Failed: {{summary}}"
    }
  }
}
```

Create `src/components/ui/ScrollArea.tsx`:

```tsx
import * as RadixScrollArea from "@radix-ui/react-scroll-area";
import { forwardRef, type ReactNode, type UIEvent } from "react";

export interface ScrollAreaProps {
  children: ReactNode;
  className?: string;
  onViewportScroll?: (event: UIEvent<HTMLDivElement>) => void;
}

export const ScrollArea = forwardRef<HTMLDivElement, ScrollAreaProps>(
  ({ children, className, onViewportScroll }, ref) => (
    <RadixScrollArea.Root className={className} type="auto">
      <RadixScrollArea.Viewport ref={ref} className="h-full w-full" onScroll={onViewportScroll}>
        {children}
      </RadixScrollArea.Viewport>
      <RadixScrollArea.Scrollbar orientation="vertical" className="w-2 bg-transparent">
        <RadixScrollArea.Thumb className="rounded-full bg-[var(--color-border)]" />
      </RadixScrollArea.Scrollbar>
    </RadixScrollArea.Root>
  ),
);
ScrollArea.displayName = "ScrollArea";
```

- [ ] **Step 2: Write a failing test for `OperationBar`**

Create `src/components/OperationBar.test.tsx`:

```tsx
import { describe, expect, it, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { OperationBar } from "./OperationBar";
import { useUiStore } from "../store/ui";

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockInvoke.mockReset();
  useUiStore.setState({ drawerOpen: false, focusedOpId: null });
});

describe("OperationBar", () => {
  it("shows an idle message when nothing is running", async () => {
    mockInvoke.mockResolvedValue([]);
    const { findByText } = renderWithProviders(<OperationBar />);

    await findByText("No operation running");
  });

  it("shows the running operation and cancels it on click", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "list_operations") {
        return Promise.resolve([
          {
            id: 5,
            kind: "Upgrade",
            instance_id: "brew:/opt/homebrew",
            artifact_kind: "Cask",
            name: "onyx",
            status: "Running",
            outcome: null,
            argv_preview: ["/opt/homebrew/bin/brew", "upgrade", "--cask", "onyx"],
          },
        ]);
      }
      return Promise.resolve(undefined);
    });

    const { findByRole } = renderWithProviders(<OperationBar />);
    const cancelButton = await findByRole("button", { name: "Cancel" });
    cancelButton.click();

    expect(mockInvoke).toHaveBeenCalledWith("cancel_operation", { opId: 5 });
  });
});
```

- [ ] **Step 3: Run the test, verify it fails**

Run: `pnpm exec vitest run src/components/OperationBar.test.tsx`
Expected: FAIL — `Failed to resolve import "./OperationBar"`.

- [ ] **Step 4: Implement `OperationBar`**

Create `src/components/OperationBar.tsx`:

```tsx
import { useTranslation } from "react-i18next";
import { useOperations, useCancelOperation } from "../lib/queries";
import { useUiStore } from "../store/ui";
import type { OpStatus } from "../lib/types";

const ACTIVE_STATUSES: OpStatus[] = ["Queued", "Running", "CancelRequested", "Cancelling", "Verifying"];

export function OperationBar() {
  const { t } = useTranslation();
  const { data: operations } = useOperations();
  const cancelMutation = useCancelOperation();
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  const setFocusedOpId = useUiStore((s) => s.setFocusedOpId);

  const current = (operations ?? []).find((op) => ACTIVE_STATUSES.includes(op.status));

  if (!current) {
    return (
      <div className="flex h-full items-center px-4 text-sm text-[var(--color-muted)]">
        {t("operations.idle")}
      </div>
    );
  }

  return (
    <div className="flex h-full items-center justify-between gap-4 px-4">
      <button
        type="button"
        onClick={() => {
          setFocusedOpId(current.id);
          setDrawerOpen(true);
        }}
        className="min-w-0 flex-1 truncate text-left text-sm text-[var(--color-foreground)]"
      >
        {t(`operations.kind.${current.kind}`)} {current.name} — {t(`operations.status.${current.status}`)}
      </button>
      <button
        type="button"
        onClick={() => cancelMutation.mutate(current.id)}
        disabled={current.status === "CancelRequested" || current.status === "Cancelling"}
        className="shrink-0 rounded-md border border-[var(--color-border)] px-3 py-1 text-sm disabled:opacity-50"
      >
        {t("operations.cancel")}
      </button>
    </div>
  );
}
```

- [ ] **Step 5: Run the test, verify it passes**

Run: `pnpm exec vitest run src/components/OperationBar.test.tsx`
Expected: PASS (2 tests)

- [ ] **Step 6: Write a failing test for `LogDrawer`, including a synthetic event sequence**

Create `src/components/LogDrawer.test.tsx`:

```tsx
import { describe, expect, it, beforeEach, vi } from "vitest";
import { act } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { LogDrawer } from "./LogDrawer";
import { useUiStore } from "../store/ui";

const mockInvoke = vi.mocked(invoke);

const runningOp = {
  id: 1,
  kind: "Install",
  instance_id: "brew:/opt/homebrew",
  artifact_kind: "Formula",
  name: "jq",
  status: "Running",
  outcome: null,
  argv_preview: ["/opt/homebrew/bin/brew", "install", "--formula", "jq"],
};

beforeEach(() => {
  mockInvoke.mockReset();
  mockInvoke.mockResolvedValue([runningOp]);
  useUiStore.setState({ logs: [], drawerOpen: true, focusedOpId: 1 });
});

describe("LogDrawer", () => {
  it("renders a synthetic sequence of streamed log lines in order", async () => {
    const { findByText } = renderWithProviders(<LogDrawer />);

    act(() => {
      useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "Fetching jq" });
      useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "Installing jq" });
      useUiStore
        .getState()
        .appendLog({ opId: 1, stream: "Stderr", line: "warning: cask deprecated" });
    });

    await findByText("Fetching jq");
    await findByText("Installing jq");
    await findByText("warning: cask deprecated");
  });

  it("only shows log lines for the focused operation", async () => {
    const { findByText, queryByText } = renderWithProviders(<LogDrawer />);

    act(() => {
      useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "for op 1" });
      useUiStore.getState().appendLog({ opId: 2, stream: "Stdout", line: "for op 2" });
    });

    await findByText("for op 1");
    expect(queryByText("for op 2")).not.toBeInTheDocument();
  });

  it("shows the terminal outcome once the operation finishes", async () => {
    mockInvoke.mockResolvedValue([{ ...runningOp, status: "Done", outcome: "Succeeded" }]);

    const { findByText } = renderWithProviders(<LogDrawer />);

    await findByText("Succeeded");
  });

  it("does not render when the drawer is closed", () => {
    useUiStore.setState({ drawerOpen: false });
    const { queryByRole } = renderWithProviders(<LogDrawer />);

    expect(queryByRole("dialog")).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 7: Run the test, verify it fails**

Run: `pnpm exec vitest run src/components/LogDrawer.test.tsx`
Expected: FAIL — `Failed to resolve import "./LogDrawer"`.

- [ ] **Step 8: Implement `LogDrawer`**

Create `src/components/LogDrawer.tsx`:

```tsx
import { useEffect, useRef, useState, type UIEvent } from "react";
import { useTranslation } from "react-i18next";
import { useUiStore } from "../store/ui";
import { useOperations } from "../lib/queries";
import { ScrollArea } from "./ui/ScrollArea";
import type { Outcome } from "../lib/types";

const NEAR_BOTTOM_PX = 32;

function outcomeKey(outcome: Outcome): string {
  if (typeof outcome === "string") return outcome;
  if ("NeedsAttention" in outcome) return "NeedsAttention";
  return "Failed";
}

function outcomeArgs(outcome: Outcome): Record<string, unknown> {
  if (typeof outcome === "string") return {};
  if ("NeedsAttention" in outcome) return { message: outcome.NeedsAttention };
  return { summary: outcome.Failed.summary };
}

export function LogDrawer() {
  const { t } = useTranslation();
  const drawerOpen = useUiStore((s) => s.drawerOpen);
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  const focusedOpId = useUiStore((s) => s.focusedOpId);
  const logs = useUiStore((s) => s.logs);
  const { data: operations } = useOperations();
  const viewportRef = useRef<HTMLDivElement>(null);
  const [stickToBottom, setStickToBottom] = useState(true);

  const visibleLogs = logs.filter((l) => l.opId === focusedOpId);
  const operation = (operations ?? []).find((op) => op.id === focusedOpId);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (viewport && stickToBottom) {
      viewport.scrollTop = viewport.scrollHeight;
    }
  }, [visibleLogs.length, stickToBottom]);

  if (!drawerOpen) {
    return null;
  }

  function handleScroll(event: UIEvent<HTMLDivElement>) {
    const el = event.currentTarget;
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    setStickToBottom(distanceFromBottom <= NEAR_BOTTOM_PX);
  }

  return (
    <div
      role="dialog"
      aria-label={t("operations.logDrawerTitle")}
      className="fixed inset-x-0 bottom-12 top-1/2 border-t border-[var(--color-border)] bg-[var(--color-background)]"
    >
      <div className="flex items-center justify-between border-b border-[var(--color-border)] px-4 py-2">
        <p className="text-sm font-medium text-[var(--color-foreground)]">
          {t("operations.logDrawerTitle")}
        </p>
        <button
          type="button"
          onClick={() => setDrawerOpen(false)}
          className="text-sm text-[var(--color-muted)]"
        >
          {t("common.close")}
        </button>
      </div>
      <ScrollArea className="h-[calc(100%-96px)]" ref={viewportRef} onViewportScroll={handleScroll}>
        <div className="px-4 py-2 font-mono text-xs">
          {visibleLogs.map((line) => (
            <p
              key={line.seq}
              className={line.stream === "Stderr" ? "text-[var(--color-danger)]" : undefined}
            >
              {line.line}
            </p>
          ))}
        </div>
      </ScrollArea>
      {operation?.outcome ? (
        <div className="border-t border-[var(--color-border)] px-4 py-2 text-sm">
          {t(`operations.outcome.${outcomeKey(operation.outcome)}`, outcomeArgs(operation.outcome))}
        </div>
      ) : null}
    </div>
  );
}
```

- [ ] **Step 9: Run the test, verify it passes**

Run: `pnpm exec vitest run src/components/LogDrawer.test.tsx`
Expected: PASS (4 tests)

- [ ] **Step 10: Mount `OperationBar` and `LogDrawer`, start the event bridge**

Replace `src/App.tsx`:

```tsx
import { useTranslation } from "react-i18next";
import { Sidebar } from "./components/Sidebar";
import { InstalledPage } from "./pages/InstalledPage";
import { UpdatesPage } from "./pages/UpdatesPage";
import { OperationBar } from "./components/OperationBar";
import { LogDrawer } from "./components/LogDrawer";
import { useOperationEvents } from "./lib/events";
import { useUiStore } from "./store/ui";

function App() {
  const { t } = useTranslation();
  const page = useUiStore((s) => s.page);
  const setPage = useUiStore((s) => s.setPage);
  useOperationEvents();

  return (
    <div className="flex h-screen flex-col bg-[var(--color-background)] text-[var(--color-foreground)]">
      <div className="flex flex-1 overflow-hidden">
        <Sidebar page={page} onSelectPage={setPage} />
        <main className="flex-1 overflow-y-auto">
          {page === "installed" ? (
            <InstalledPage />
          ) : page === "updates" ? (
            <UpdatesPage />
          ) : (
            <h1 className="p-6 text-lg font-semibold">{t(`nav.${page}`)}</h1>
          )}
        </main>
      </div>
      <footer
        aria-label={t("app.operationBarRegion")}
        className="h-12 shrink-0 border-t border-[var(--color-border)]"
      >
        <OperationBar />
      </footer>
      <LogDrawer />
    </div>
  );
}

export default App;
```

- [ ] **Step 11: Run the full suite**

Run: `pnpm test`
Expected: PASS — every test file from Tasks 9–13 green, 0 failures.

- [ ] **Step 12: Commit**

```bash
git add src/components/ui/ScrollArea.tsx src/components/OperationBar.tsx src/components/OperationBar.test.tsx src/components/LogDrawer.tsx src/components/LogDrawer.test.tsx src/App.tsx src/i18n/en.json
git commit -m "$(cat <<'EOF'
feat(ui): add the operation bar and log drawer, wire the event bridge into App

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 14: Uninstall dialog

**Files:**
- Create: `src/components/UninstallDialog.tsx`
- Create: `src/components/UninstallDialog.test.tsx`
- Modify: `src/pages/InstalledPage.tsx`
- Modify: `src/pages/InstalledPage.test.tsx`
- Modify: `src/i18n/en.json`

**Interfaces:**
- Consumes: `usePlanOperation(): UseMutationResult<IssuedPlan, Error, OpRequest>` and `useSubmitOperation(): UseMutationResult<number, Error, number>` from `src/lib/queries.ts` (Task 10); `OpRequest`, `IssuedPlan` from `src/lib/types.ts` (Task 10); `useUiStore` (`setFocusedOpId`, `setDrawerOpen`) from `src/store/ui.ts` (Task 10); `Dialog({ open, onOpenChange, title, children, footer? })` from `src/components/ui/Dialog.tsx` (Task 9); `CommandPreview({ program, args })` from `src/components/CommandPreview.tsx` (Task 12); `InstalledPage`'s existing row rendering and `ArtifactRow` usage (Task 11).
- Produces: `export interface UninstallDialogProps { open: boolean; onOpenChange: (open: boolean) => void; request: OpRequest; displayName: string; onSubmitted?: (opId: number) => void }` and `export function UninstallDialog(props: UninstallDialogProps): JSX.Element`. The caller (`InstalledPage`) owns the `open` boolean and passes the artifact's `OpRequest` (`kind: "Uninstall"`) plus its human-readable name; `onSubmitted` is how the caller learns the new op id to focus in the log drawer. `InstalledPage` now owns an `uninstallTarget` piece of state and renders `UninstallDialog` conditionally instead of calling `planMutation.mutate` directly from the row's primary action — this is what makes the Global Constraint "every destructive action shows the exact command that will run before it runs" actually true for uninstall, not just true of `UninstallDialog` in isolation.

- [ ] **Step 1: Write the failing tests for `UninstallDialog`**

```tsx
// src/components/UninstallDialog.test.tsx
import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { UninstallDialog } from "./UninstallDialog";
import type { IssuedPlan, OpRequest, Plan } from "../lib/types";

const request: OpRequest = {
  kind: "Uninstall",
  instance_id: "brew:/opt/homebrew",
  artifact_kind: "Formula",
  name: "jq",
};

function issuedPlanFor(overrides: Partial<Plan> = {}): IssuedPlan {
  return {
    id: 1,
    plan: {
      request,
      program: "/opt/homebrew/bin/brew",
      args: ["uninstall", "--formula", "jq"],
      env: [],
      needs_password: false,
      locks: ["brew:/opt/homebrew"],
      cancel_policy: "KillThenReconcile",
      warnings: [],
      affected: [],
      timeout_secs: 1800,
      ...overrides,
    },
    issued_at: 1758000000,
  };
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

describe("UninstallDialog", () => {
  it("shows a checking message and a disabled confirm button while the plan is loading", () => {
    vi.mocked(invoke).mockImplementation(() => new Promise(() => {}));

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    expect(screen.getByText("Checking what this would affect…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeDisabled();
  });

  it("disables confirm and explains what would break when something depends on it", async () => {
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ affected: ["jq-cli-wrapper"] }));

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    await waitFor(() => expect(screen.getByText("jq-cli-wrapper")).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeDisabled();
    expect(
      screen.getByText("/opt/homebrew/bin/brew uninstall --formula jq"),
    ).toBeInTheDocument();
  });

  it("submits the plan id and reports the new op id when nothing would break", async () => {
    const issued = issuedPlanFor();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "plan_operation") return issued;
      if (cmd === "submit_operation") return 7;
      throw new Error(`unexpected command ${cmd}`);
    });
    const onSubmitted = vi.fn();
    const onOpenChange = vi.fn();

    renderWithProviders(
      <UninstallDialog
        open
        onOpenChange={onOpenChange}
        request={request}
        displayName="jq"
        onSubmitted={onSubmitted}
      />,
    );

    const confirmButton = await screen.findByRole("button", { name: "Uninstall" });
    await waitFor(() => expect(confirmButton).not.toBeDisabled());
    fireEvent.click(confirmButton);

    await waitFor(() => expect(onSubmitted).toHaveBeenCalledWith(7));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `pnpm exec vitest run src/components/UninstallDialog.test.tsx`
Expected: FAIL — `Cannot find module './UninstallDialog'` (the file does not exist yet).

- [ ] **Step 3: Implement `UninstallDialog` and its i18n keys**

`Dialog` (Task 9) takes `{ open, onOpenChange, title, children, footer? }` — there is no `DialogContent`/`DialogHeader`/`DialogTitle`/`DialogDescription`/`DialogFooter` family of sub-components, so the title goes in the `title` prop and the buttons go in the `footer` prop; everything else is plain `children`.

```tsx
// src/components/UninstallDialog.tsx
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { usePlanOperation, useSubmitOperation } from "../lib/queries";
import type { OpRequest } from "../lib/types";
import { CommandPreview } from "./CommandPreview";
import { Dialog } from "./ui/Dialog";

export interface UninstallDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  request: OpRequest;
  displayName: string;
  onSubmitted?: (opId: number) => void;
}

export function UninstallDialog({
  open,
  onOpenChange,
  request,
  displayName,
  onSubmitted,
}: UninstallDialogProps) {
  const { t } = useTranslation();
  const planMutation = usePlanOperation();
  const submitMutation = useSubmitOperation();

  useEffect(() => {
    if (open) {
      planMutation.mutate(request);
    } else {
      planMutation.reset();
      submitMutation.reset();
    }
    // planMutation/submitMutation are stable across renders; only re-run
    // when the dialog opens/closes or targets a different artifact.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, request.instance_id, request.artifact_kind, request.name]);

  const issued = planMutation.data;
  const plan = issued?.plan;
  const hasAffected = (plan?.affected.length ?? 0) > 0;

  function handleConfirm() {
    if (!issued) return;
    submitMutation.mutate(issued.id, {
      onSuccess: (opId) => {
        onSubmitted?.(opId);
        onOpenChange(false);
      },
    });
  }

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("uninstall.title", { name: displayName })}
      footer={
        <>
          <button type="button" onClick={() => onOpenChange(false)}>
            {t("uninstall.cancel")}
          </button>
          <button
            type="button"
            onClick={handleConfirm}
            disabled={!plan || hasAffected || submitMutation.isPending}
            title={hasAffected ? t("uninstall.confirmDisabledHint") : undefined}
          >
            {t("uninstall.confirm")}
          </button>
        </>
      }
    >
      <p>{t("uninstall.description")}</p>

      {planMutation.isPending && <p>{t("uninstall.checking")}</p>}

      {planMutation.isError && <p role="alert">{t("uninstall.planError")}</p>}

      {plan && (
        <div className="flex flex-col gap-3">
          {plan.warnings.length > 0 && (
            <div>
              <p className="font-medium">{t("uninstall.warningsTitle")}</p>
              <ul className="list-disc pl-5">
                {plan.warnings.map((warning) => (
                  <li key={warning}>{warning}</li>
                ))}
              </ul>
            </div>
          )}

          {hasAffected && (
            <div>
              <p className="font-medium">{t("uninstall.affectedTitle")}</p>
              <ul className="list-disc pl-5">
                {plan.affected.map((name) => (
                  <li key={name}>{name}</li>
                ))}
              </ul>
            </div>
          )}

          <CommandPreview program={plan.program} args={plan.args} />
        </div>
      )}
    </Dialog>
  );
}
```

Add this top-level key to `src/i18n/en.json` (add a comma after the last existing top-level key and paste this in before the file's final closing brace — `commandPreview` already exists, from Task 12, so it is **not** repeated here):

```json
"uninstall": {
  "title": "Uninstall {{name}}?",
  "description": "Review what this will do before you continue.",
  "checking": "Checking what this would affect…",
  "planError": "Couldn't check what this would affect. Try again in a moment.",
  "warningsTitle": "Before you continue:",
  "affectedTitle": "These will stop working if you remove it:",
  "confirm": "Uninstall",
  "cancel": "Cancel",
  "confirmDisabledHint": "Resolve the items above before uninstalling."
}
```

- [ ] **Step 4: Run it and confirm it passes**

Run: `pnpm exec vitest run src/components/UninstallDialog.test.tsx`
Expected: PASS — 3 tests passed.

- [ ] **Step 5: Write the failing tests for wiring `UninstallDialog` into `InstalledPage`**

`InstalledPage` (Task 11) currently wires the row's "Uninstall" button straight to `planMutation.mutate({...})`, with no dialog and no command preview — a direct violation of the Global Constraint that every destructive action previews its command first. Replace the existing "plans an uninstall when the row's primary button is clicked" test in `src/pages/InstalledPage.test.tsx` with the following three, and add the `within` import:

```tsx
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { fireEvent, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { InstalledPage } from "./InstalledPage";
import type { OpRequest, Settings, Snapshot } from "../lib/types";
```

(only the `@testing-library/react` and new `../lib/types` imports change — `within` and `OpRequest` are new; everything else in the file's `snapshot`/`settings` fixtures and `beforeEach` stays exactly as Task 11 left it.)

Replace this test:

```tsx
  it("plans an uninstall when the row's primary button is clicked", async () => {
    const { findByText, getByRole } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    getByRole("button", { name: "Uninstall" }).click();

    expect(mockInvoke).toHaveBeenCalledWith("plan_operation", {
      request: {
        kind: "Uninstall",
        instance_id: "brew:/opt/homebrew",
        artifact_kind: "Formula",
        name: "jq",
      },
    });
  });
```

with:

```tsx
  it("opens the uninstall dialog and plans it when the row's primary button is clicked", async () => {
    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "get_snapshot") return Promise.resolve(snapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      if (cmd === "plan_operation") {
        return Promise.resolve({
          id: 1,
          plan: {
            request: (args as { request: OpRequest }).request,
            program: "/opt/homebrew/bin/brew",
            args: ["uninstall", "--formula", "jq"],
            env: [],
            needs_password: false,
            locks: ["brew:/opt/homebrew"],
            cancel_policy: "KillThenReconcile",
            warnings: [],
            affected: [],
            timeout_secs: 1800,
          },
          issued_at: 1758000000,
        });
      }
      return Promise.resolve(undefined);
    });

    const { findByText, getByRole, findByRole } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    getByRole("button", { name: "Uninstall" }).click();

    const dialog = await findByRole("dialog");
    expect(mockInvoke).toHaveBeenCalledWith("plan_operation", {
      request: {
        kind: "Uninstall",
        instance_id: "brew:/opt/homebrew",
        artifact_kind: "Formula",
        name: "jq",
      },
    });
    await within(dialog).findByText("/opt/homebrew/bin/brew uninstall --formula jq");
  });

  it("disables the dialog's confirm button when the plan reports dependents", async () => {
    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "get_snapshot") return Promise.resolve(snapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      if (cmd === "plan_operation") {
        return Promise.resolve({
          id: 1,
          plan: {
            request: (args as { request: OpRequest }).request,
            program: "/opt/homebrew/bin/brew",
            args: ["uninstall", "--formula", "jq"],
            env: [],
            needs_password: false,
            locks: ["brew:/opt/homebrew"],
            cancel_policy: "KillThenReconcile",
            warnings: [],
            affected: ["jq-cli-wrapper"],
            timeout_secs: 1800,
          },
          issued_at: 1758000000,
        });
      }
      return Promise.resolve(undefined);
    });

    const { findByText, getByRole, findByRole } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    getByRole("button", { name: "Uninstall" }).click();

    const dialog = await findByRole("dialog");
    await within(dialog).findByText("jq-cli-wrapper");
    expect(within(dialog).getByRole("button", { name: "Uninstall" })).toBeDisabled();
  });
```

(the row's own "Uninstall" button and the dialog's confirm button share the same accessible name, so once the dialog is open, later assertions scope their query with `within(dialog)` rather than `getByRole` on the whole document.)

- [ ] **Step 6: Run the suite and confirm the new tests fail**

Run: `pnpm exec vitest run src/pages/InstalledPage.test.tsx`
Expected: FAIL — both new tests time out waiting for `findByRole("dialog")`, because `InstalledPage` still calls `planMutation.mutate` directly and renders no dialog at all.

- [ ] **Step 7: Wire `UninstallDialog` into `InstalledPage`**

Replace `src/pages/InstalledPage.tsx`'s imports and the top of the component (through the hook declarations) — full file shown, only the uninstall wiring changes from Task 11's version:

```tsx
import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useSnapshot, useSettings } from "../lib/queries";
import { useUiStore, artifactKeyId } from "../store/ui";
import { ArtifactRow } from "../components/ArtifactRow";
import { UninstallDialog } from "../components/UninstallDialog";
import type { InstalledArtifact, OpRequest } from "../lib/types";

const ADAPTER_LABEL_KEYS: Record<string, string> = {
  brew: "adapters.brew",
};

type ListItem =
  | { type: "group"; instanceId: string; label: string }
  | { type: "artifact"; artifact: InstalledArtifact }
  | { type: "toggle"; instanceId: string; hiddenCount: number };

export function InstalledPage() {
  const { t } = useTranslation();
  const { data: snapshot, isLoading } = useSnapshot();
  const { data: settings } = useSettings();
  const query = useUiStore((s) => s.query);
  const setQuery = useUiStore((s) => s.setQuery);
  const showDependencies = useUiStore((s) => s.showDependencies);
  const toggleDependencies = useUiStore((s) => s.toggleDependencies);
  const setFocusedOpId = useUiStore((s) => s.setFocusedOpId);
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  const parentRef = useRef<HTMLDivElement>(null);

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
      const artifacts = byInstance.get(instance.id);
      if (!artifacts || artifacts.length === 0) continue;
      const labelKey = ADAPTER_LABEL_KEYS[instance.adapter_id];
      result.push({
        type: "group",
        instanceId: instance.id,
        label: labelKey ? t(labelKey) : instance.adapter_id,
      });
      const primary = artifacts.filter((a) => a.reason === "Requested");
      const dependencies = artifacts.filter((a) => a.reason !== "Requested");
      for (const artifact of primary) {
        result.push({ type: "artifact", artifact });
      }
      if (dependencies.length > 0) {
        if (showDependencies) {
          for (const artifact of dependencies) {
            result.push({ type: "artifact", artifact });
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
                  <p className="px-4 py-2 text-xs font-semibold uppercase text-[var(--color-muted)]">
                    {item.label}
                  </p>
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
                        ? `${item.artifact.display_name} · ${item.artifact.version}`
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
                    primaryActionLabel={t("installed.uninstall")}
                    onPrimaryAction={() =>
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

(`usePlanOperation` is no longer imported or called here — `UninstallDialog` now owns planning internally, as it already did in Step 3's implementation.)

- [ ] **Step 8: Run the suite and confirm it passes**

Run: `pnpm exec vitest run src/pages/InstalledPage.test.tsx src/components/UninstallDialog.test.tsx`
Expected: PASS — 6 tests passed in `InstalledPage.test.tsx` (the 3 unchanged from Task 11 plus the 2 replacing the old direct-`plan_operation` test, plus the untouched filter test), 3 in `UninstallDialog.test.tsx`.

- [ ] **Step 9: Run the full front-end suite**

Run: `pnpm test`
Expected: PASS — every suite from Tasks 9–14 green, 0 failures.

- [ ] **Step 10: Commit**

```bash
git add src/components/UninstallDialog.tsx src/components/UninstallDialog.test.tsx src/pages/InstalledPage.tsx src/pages/InstalledPage.test.tsx src/i18n/en.json
git commit -m "$(cat <<'EOF'
feat(ui): add uninstall dialog and wire it into the Installed page

Uninstall no longer calls plan_operation and discards the result: the
row's action now opens UninstallDialog, which plans first, shows the
exact command and what would break, and disables confirm when
anything depends on the artifact being removed.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 15: Settings page

**Files:**
- Create: `src/pages/SettingsPage.tsx`
- Create: `src/pages/SettingsPage.test.tsx`
- Modify: `src/i18n/en.json`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `useSettings(): UseQueryResult<Settings>` and `useSaveSettings(): UseMutationResult<void, Error, Settings>` from `src/lib/queries.ts`; `Settings`, `Language` from `src/lib/types.ts`; `artifactKeyId(key: ArtifactKey): string` from `src/store/ui.ts`; `Switch` from `src/components/ui/Switch.tsx` (Task 9's controlled Radix wrapper: `checked: boolean`, `onCheckedChange: (checked: boolean) => void`, `id?: string`, `"aria-label"?: string`).
- Produces: `export function SettingsPage(): JSX.Element` — the component the app shell routes to for the "settings" page (per `src/store/ui.ts`'s `Page` type). `App.tsx` is modified to import it and replace the placeholder `<h1>` that Task 13 left in the "settings" branch of its page switch.

- [ ] **Step 1: Write the failing test for reading settings**

```tsx
// src/pages/SettingsPage.test.tsx
import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { SettingsPage } from "./SettingsPage";
import type { Settings } from "../lib/types";

function baseSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
    ...overrides,
  };
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

describe("SettingsPage", () => {
  it("renders the settings loaded from the backend", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") {
        return baseSettings({
          show_technical_details: true,
          ignored_updates: [
            { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" },
          ],
        });
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    expect(
      await screen.findByRole("switch", { name: "Show technical details" }),
    ).toBeChecked();
    expect(screen.getByRole("radio", { name: "System" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(screen.getByText("jq")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `pnpm exec vitest run src/pages/SettingsPage.test.tsx`
Expected: FAIL — `Cannot find module './SettingsPage'` (the file does not exist yet).

- [ ] **Step 3: Implement a minimal `SettingsPage` (read + save, no optimistic UI yet)**

```tsx
// src/pages/SettingsPage.tsx
import { useTranslation } from "react-i18next";
import { useSettings, useSaveSettings } from "../lib/queries";
import type { Language } from "../lib/types";
import { artifactKeyId } from "../store/ui";
import { Switch } from "../components/ui/Switch";

const LANGUAGES: Language[] = ["System", "En", "ZhCn"];

function languageLabelKey(lang: Language): string {
  if (lang === "System") return "settings.language.system";
  if (lang === "En") return "settings.language.english";
  return "settings.language.chinese";
}

export function SettingsPage() {
  const { t } = useTranslation();
  const settingsQuery = useSettings();
  const saveMutation = useSaveSettings();
  const current = settingsQuery.data;

  if (settingsQuery.isLoading || !current) {
    return <p>{t("settings.loading")}</p>;
  }

  return (
    <div className="flex flex-col gap-6 p-6">
      <h1 className="text-lg font-semibold">{t("settings.title")}</h1>

      <div className="flex items-center justify-between gap-4">
        <label htmlFor="settings-show-technical" className="flex flex-col">
          <span>{t("settings.showTechnicalDetails.label")}</span>
          <span className="text-sm text-[var(--color-muted-foreground)]">
            {t("settings.showTechnicalDetails.description")}
          </span>
        </label>
        <Switch
          id="settings-show-technical"
          checked={current.show_technical_details}
          onCheckedChange={(checked) =>
            saveMutation.mutate({ ...current, show_technical_details: checked })
          }
        />
      </div>

      <div>
        <p className="mb-2">{t("settings.language.label")}</p>
        <div role="radiogroup" aria-label={t("settings.language.label")} className="flex gap-2">
          {LANGUAGES.map((lang) => (
            <button
              key={lang}
              type="button"
              role="radio"
              aria-checked={current.language === lang}
              onClick={() => saveMutation.mutate({ ...current, language: lang })}
            >
              {t(languageLabelKey(lang))}
            </button>
          ))}
        </div>
      </div>

      <div>
        <p className="mb-2 font-medium">{t("settings.ignoredUpdates.title")}</p>
        {current.ignored_updates.length === 0 ? (
          <p>{t("settings.ignoredUpdates.empty")}</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {current.ignored_updates.map((key) => (
              <li key={artifactKeyId(key)} className="flex items-center justify-between gap-4">
                <span>{key.name}</span>
                <button
                  type="button"
                  aria-label={t("settings.ignoredUpdates.unignoreAriaLabel", {
                    name: key.name,
                  })}
                  onClick={() =>
                    saveMutation.mutate({
                      ...current,
                      ignored_updates: current.ignored_updates.filter(
                        (k) => artifactKeyId(k) !== artifactKeyId(key),
                      ),
                    })
                  }
                >
                  {t("settings.ignoredUpdates.unignore")}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
```

Add these top-level keys to `src/i18n/en.json`:

```json
"settings": {
  "title": "Settings",
  "loading": "Loading settings…",
  "showTechnicalDetails": {
    "label": "Show technical details",
    "description": "Reveal version numbers, file paths, and the exact commands Canager runs."
  },
  "language": {
    "label": "Language",
    "system": "System",
    "english": "English",
    "chinese": "简体中文"
  },
  "ignoredUpdates": {
    "title": "Ignored updates",
    "empty": "You haven't ignored any updates.",
    "unignore": "Stop ignoring",
    "unignoreAriaLabel": "Stop ignoring {{name}}"
  },
  "saveError": "Couldn't save that change. It's been reverted."
}
```

- [ ] **Step 4: Run it and confirm it passes**

Run: `pnpm exec vitest run src/pages/SettingsPage.test.tsx`
Expected: PASS — 1 test passed.

- [ ] **Step 5: Write the failing tests for optimistic save with rollback, and un-ignoring**

Replace `src/pages/SettingsPage.test.tsx` with the following (it supersedes Step 1's file — same first test, plus two new ones):

```tsx
// src/pages/SettingsPage.test.tsx
import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { SettingsPage } from "./SettingsPage";
import type { Settings } from "../lib/types";

function baseSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
    ...overrides,
  };
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

describe("SettingsPage", () => {
  it("renders the settings loaded from the backend", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") {
        return baseSettings({
          show_technical_details: true,
          ignored_updates: [
            { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" },
          ],
        });
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    expect(
      await screen.findByRole("switch", { name: "Show technical details" }),
    ).toBeChecked();
    expect(screen.getByRole("radio", { name: "System" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(screen.getByText("jq")).toBeInTheDocument();
  });

  it("optimistically applies a toggle and rolls it back when the save fails", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings();
      if (cmd === "set_settings") throw new Error("disk full");
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const toggle = await screen.findByRole("switch", { name: "Show technical details" });
    expect(toggle).not.toBeChecked();

    fireEvent.click(toggle);
    expect(toggle).toBeChecked();

    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
    await waitFor(() => expect(toggle).not.toBeChecked());
  });

  it("removes an item from the ignored list and saves the shorter list", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_settings") {
        return baseSettings({
          ignored_updates: [
            { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" },
          ],
        });
      }
      if (cmd === "set_settings") {
        expect((args?.settings as Settings).ignored_updates).toEqual([]);
        return undefined;
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const unignoreButton = await screen.findByRole("button", { name: "Stop ignoring jq" });
    fireEvent.click(unignoreButton);

    await waitFor(() =>
      expect(screen.getByText("You haven't ignored any updates.")).toBeInTheDocument(),
    );
  });
});
```

- [ ] **Step 6: Run it and confirm the new tests fail**

Run: `pnpm exec vitest run src/pages/SettingsPage.test.tsx`
Expected: FAIL — the rollback test fails because the switch never reverts and no `role="alert"` is rendered (Step 3's version calls `saveMutation.mutate` directly with no local optimistic state or error surface); the first test still passes.

- [ ] **Step 7: Implement optimistic save with rollback**

Replace `src/pages/SettingsPage.tsx` with the following (it supersedes Step 3's file):

```tsx
// src/pages/SettingsPage.tsx
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettings, useSaveSettings } from "../lib/queries";
import type { Settings, Language } from "../lib/types";
import { artifactKeyId } from "../store/ui";
import { Switch } from "../components/ui/Switch";

const LANGUAGES: Language[] = ["System", "En", "ZhCn"];

function languageLabelKey(lang: Language): string {
  if (lang === "System") return "settings.language.system";
  if (lang === "En") return "settings.language.english";
  return "settings.language.chinese";
}

export function SettingsPage() {
  const { t } = useTranslation();
  const settingsQuery = useSettings();
  const saveMutation = useSaveSettings();
  const [draft, setDraft] = useState<Settings | null>(null);

  useEffect(() => {
    if (settingsQuery.data && draft === null) {
      setDraft(settingsQuery.data);
    }
  }, [settingsQuery.data, draft]);

  const current = draft ?? settingsQuery.data;

  if (settingsQuery.isLoading || !current) {
    return <p>{t("settings.loading")}</p>;
  }

  function persist(next: Settings) {
    const previous = current as Settings;
    setDraft(next);
    saveMutation.mutate(next, {
      onError: () => setDraft(previous),
    });
  }

  function unignore(key: Settings["ignored_updates"][number]) {
    persist({
      ...current,
      ignored_updates: current.ignored_updates.filter(
        (k) => artifactKeyId(k) !== artifactKeyId(key),
      ),
    });
  }

  return (
    <div className="flex flex-col gap-6 p-6">
      <h1 className="text-lg font-semibold">{t("settings.title")}</h1>

      {saveMutation.isError && <p role="alert">{t("settings.saveError")}</p>}

      <div className="flex items-center justify-between gap-4">
        <label htmlFor="settings-show-technical" className="flex flex-col">
          <span>{t("settings.showTechnicalDetails.label")}</span>
          <span className="text-sm text-[var(--color-muted-foreground)]">
            {t("settings.showTechnicalDetails.description")}
          </span>
        </label>
        <Switch
          id="settings-show-technical"
          checked={current.show_technical_details}
          onCheckedChange={(checked) =>
            persist({ ...current, show_technical_details: checked })
          }
        />
      </div>

      <div>
        <p className="mb-2">{t("settings.language.label")}</p>
        <div role="radiogroup" aria-label={t("settings.language.label")} className="flex gap-2">
          {LANGUAGES.map((lang) => (
            <button
              key={lang}
              type="button"
              role="radio"
              aria-checked={current.language === lang}
              onClick={() => persist({ ...current, language: lang })}
            >
              {t(languageLabelKey(lang))}
            </button>
          ))}
        </div>
      </div>

      <div>
        <p className="mb-2 font-medium">{t("settings.ignoredUpdates.title")}</p>
        {current.ignored_updates.length === 0 ? (
          <p>{t("settings.ignoredUpdates.empty")}</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {current.ignored_updates.map((key) => (
              <li key={artifactKeyId(key)} className="flex items-center justify-between gap-4">
                <span>{key.name}</span>
                <button
                  type="button"
                  aria-label={t("settings.ignoredUpdates.unignoreAriaLabel", {
                    name: key.name,
                  })}
                  onClick={() => unignore(key)}
                >
                  {t("settings.ignoredUpdates.unignore")}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 8: Run it and confirm it passes**

Run: `pnpm exec vitest run src/pages/SettingsPage.test.tsx`
Expected: PASS — 3 tests passed.

- [ ] **Step 9: Wire `SettingsPage` into the app shell**

`src/App.tsx` currently falls through to a placeholder heading for any page that isn't `"installed"` or `"updates"` (the only two branches Task 13 wired up). Add the import:

```tsx
import { SettingsPage } from "./pages/SettingsPage";
```

to its import list, and replace the placeholder branch:

```tsx
          {page === "installed" ? (
            <InstalledPage />
          ) : page === "updates" ? (
            <UpdatesPage />
          ) : (
            <h1 className="p-6 text-lg font-semibold">{t(`nav.${page}`)}</h1>
          )}
```

with:

```tsx
          {page === "installed" ? (
            <InstalledPage />
          ) : page === "updates" ? (
            <UpdatesPage />
          ) : (
            <SettingsPage />
          )}
```

`t` stays imported and used elsewhere in this file (the operation bar region's `aria-label`), so this leaves no unused import behind.

- [ ] **Step 10: Run the full suite**

Run: `pnpm test`
Expected: PASS — every suite from Tasks 9–15 green, 0 failures (no regression in `App.test.tsx`, which only ever navigates to "installed" and "updates").

- [ ] **Step 11: Commit**

```bash
git add src/pages/SettingsPage.tsx src/pages/SettingsPage.test.tsx src/i18n/en.json src/App.tsx
git commit -m "feat(ui): add settings page with optimistic save and ignored-updates list

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 16: i18n completeness

**Files:**
- Create: `src/i18n/zh-CN.json`
- Create: `src/i18n/completeness.test.ts`
- Create: `src/i18n/no-literal-strings.test.ts`
- Create: `src/i18n/useLanguageSync.ts`
- Create: `src/i18n/useLanguageSync.test.tsx`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `useSettings(): UseQueryResult<Settings>` from `src/lib/queries.ts`; the i18next instance exported as the default export of `src/i18n/index.ts` (created in Task 9; assumed to run `i18next-browser-languagedetector` at startup and to expose `i18n.language` / `i18n.changeLanguage(lng)`).
- Produces: `export function useLanguageSync(): void` from `src/i18n/useLanguageSync.ts` — call it once, near the app root, to keep i18next's active language following `Settings.language`. `src/i18n/zh-CN.json` becomes the second resource bundle every later task's `en.json` edits must be mirrored into.

- [ ] **Step 1: Write the failing key-parity test**

```ts
// src/i18n/completeness.test.ts
import { describe, expect, it } from "vitest";
import en from "./en.json";
import zhCN from "./zh-CN.json";

const PLURAL_SUFFIXES = ["_zero", "_one", "_two", "_few", "_many", "_other"];

function stripPluralSuffix(key: string): string {
  const suffix = PLURAL_SUFFIXES.find((s) => key.endsWith(s));
  return suffix ? key.slice(0, -suffix.length) : key;
}

function flattenKeys(value: unknown, prefix = ""): string[] {
  if (typeof value !== "object" || value === null) {
    return [prefix];
  }
  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    flattenKeys(child, prefix ? `${prefix}.${key}` : key),
  );
}

function normalizedKeySet(resource: unknown): Set<string> {
  return new Set(flattenKeys(resource).map(stripPluralSuffix));
}

describe("i18n key parity", () => {
  it("has the same set of keys in en.json and zh-CN.json", () => {
    const enKeys = normalizedKeySet(en);
    const zhKeys = normalizedKeySet(zhCN);

    const missingInZh = [...enKeys].filter((k) => !zhKeys.has(k)).sort();
    const missingInEn = [...zhKeys].filter((k) => !enKeys.has(k)).sort();

    expect(missingInZh, `zh-CN.json is missing: ${missingInZh.join(", ")}`).toEqual([]);
    expect(missingInEn, `en.json is missing: ${missingInEn.join(", ")}`).toEqual([]);
  });
});
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `pnpm exec vitest run src/i18n/completeness.test.ts`
Expected: FAIL — `Cannot find module './zh-CN.json'` (the file does not exist yet).

- [ ] **Step 3: Create `src/i18n/zh-CN.json`**

This file needs a Simplified Chinese entry for every key in `src/i18n/en.json` — every namespace added since Task 9 (`app`, `nav`, `common`, `adapters`, `installed`, `updates`, `commandPreview`, `operations`) as well as the two Tasks just before this one (`uninstall`, Task 14; `settings`, Task 15). The wording below aims at a non-programmer, not a literal jargon-for-jargon translation (e.g. this is why `nav.label` is not translated character-by-character, and why `settings.language.english`/`settings.language.chinese` keep the language names in their own language rather than translating "English" into Chinese):

```json
{
  "app": {
    "title": "Canager",
    "operationBarRegion": "操作状态"
  },
  "nav": {
    "label": "导航",
    "installed": "已安装",
    "updates": "更新",
    "settings": "设置"
  },
  "common": {
    "loading": "加载中…",
    "cancel": "取消",
    "close": "关闭"
  },
  "adapters": {
    "brew": "Homebrew"
  },
  "installed": {
    "filterLabel": "筛选已安装项目",
    "filterPlaceholder": "搜索已安装项目",
    "uninstall": "卸载",
    "upToDate": "已是最新",
    "updateAvailable": "有可用更新",
    "noDescription": "暂无描述",
    "showDependencies_other": "{{count}} 个由其他软件附带安装的组件"
  },
  "updates": {
    "upToDate": "所有内容都已是最新",
    "count_other": "{{count}} 个可用更新",
    "updateSelected": "更新所选项",
    "update": "更新",
    "ignore": "忽略",
    "available": "更新",
    "warnings_other": "{{count}} 条警告",
    "versionChange": "{{current}} → {{target}}",
    "selectRow": "选择要更新的 {{name}}",
    "confirmTitle": "确认更新",
    "confirmUpdate": "确认"
  },
  "commandPreview": {
    "label": "将执行:"
  },
  "operations": {
    "idle": "当前没有正在进行的操作",
    "cancel": "取消",
    "logDrawerTitle": "操作日志",
    "kind": {
      "Install": "正在安装",
      "Uninstall": "正在卸载",
      "Upgrade": "正在更新"
    },
    "status": {
      "Queued": "排队中",
      "Running": "进行中",
      "CancelRequested": "正在取消",
      "Cancelling": "正在取消",
      "Verifying": "正在核实",
      "Done": "已完成"
    },
    "outcome": {
      "Succeeded": "已成功",
      "NoChange": "无需更改",
      "PartialSuccess": "部分成功",
      "Unconfirmed": "结果未确认",
      "NeedsAttention": "需要留意:{{message}}",
      "Failed": "失败:{{summary}}"
    }
  },
  "uninstall": {
    "title": "卸载 {{name}}?",
    "description": "继续之前,先看看会发生什么。",
    "checking": "正在检查会有什么影响…",
    "planError": "没能检查清楚会有什么影响,请稍后重试。",
    "warningsTitle": "继续之前请注意:",
    "affectedTitle": "删除后这些会受影响:",
    "confirm": "卸载",
    "cancel": "取消",
    "confirmDisabledHint": "请先处理上面列出的问题,才能卸载。"
  },
  "settings": {
    "title": "设置",
    "loading": "正在加载设置…",
    "showTechnicalDetails": {
      "label": "显示技术细节",
      "description": "显示版本号、文件路径,以及 Canager 实际执行的命令。"
    },
    "language": {
      "label": "语言",
      "system": "跟随系统",
      "english": "English",
      "chinese": "简体中文"
    },
    "ignoredUpdates": {
      "title": "已忽略的更新",
      "empty": "你还没有忽略任何更新。",
      "unignore": "取消忽略",
      "unignoreAriaLabel": "取消忽略 {{name}}"
    },
    "saveError": "保存失败,已恢复原来的设置。"
  }
}
```

Chinese has no plural forms, so every English key that only exists as `_one`/`_other` (`installed.showDependencies_one` / `_other`, `updates.count_one` / `_other`, `updates.warnings_one` / `_other`) is mirrored here with a single `_other` entry — i18next falls back to the `_other` form for any count in a locale that declares no plural rule for the key, and Step 1's completeness test already normalizes away plural suffixes before comparing key sets, so a lone `_other` on the Chinese side counts as parity with both `_one` and `_other` on the English side.

- [ ] **Step 4: Run it and confirm it passes**

Run: `pnpm exec vitest run src/i18n/completeness.test.ts`
Expected: PASS — `en.json` and `zh-CN.json` now carry the exact same normalized key set.

- [ ] **Step 5: Write the literal-string guard test**

```ts
// src/i18n/no-literal-strings.test.ts
import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SCAN_DIRS = ["../components", "../pages"].map((p) => path.resolve(__dirname, p));

function collectTsxFiles(dir: string): string[] {
  const entries = readdirSync(dir);
  return entries.flatMap((entry) => {
    const full = path.join(dir, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) return collectTsxFiles(full);
    if (full.endsWith(".tsx") && !full.endsWith(".test.tsx")) return [full];
    return [];
  });
}

// Matches a JSX text child sitting directly between two tags, e.g.
// `<p>Nothing installed yet</p>`. A child that is itself an expression
// (`<p>{t("x")}</p>`) never matches, because the `>` is immediately
// followed by `{`, not a letter.
const SUSPICIOUS_JSX_TEXT = />[ \t]*[A-Za-z][A-Za-z0-9 ,.'!?:;()-]{3,}[ \t]*</g;

const files = SCAN_DIRS.flatMap((dir) => collectTsxFiles(dir));

describe("no literal user-visible strings in JSX", () => {
  it.each(files)("has no literal JSX text in %s", (file) => {
    const source = readFileSync(file, "utf-8");
    const matches = source.match(SUSPICIOUS_JSX_TEXT) ?? [];
    const real = matches.filter((m) => m.slice(1, -1).trim().length > 0);
    expect(real, `${file} has literal text: ${real.join(" | ")}`).toEqual([]);
  });
});
```

- [ ] **Step 6: Run it**

Run: `pnpm exec vitest run src/i18n/no-literal-strings.test.ts`
Expected: PASS if every `src/components/**/*.tsx` and `src/pages/**/*.tsx` file already routes its copy through `t()` (true for Tasks 14 and 15's own files). Otherwise FAIL, naming the exact file and the literal text found — replace it with a `t()` call and a matching key in both `en.json` and `zh-CN.json`, then re-run.

- [ ] **Step 7: Write the failing test for the Settings-language override**

```tsx
// src/i18n/useLanguageSync.test.tsx
import { describe, expect, it, vi, beforeEach } from "vitest";
import { waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { useLanguageSync } from "./useLanguageSync";
import i18n from "./index";
import type { Settings } from "../lib/types";

function baseSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
    ...overrides,
  };
}

function Probe() {
  useLanguageSync();
  return null;
}

beforeEach(async () => {
  vi.mocked(invoke).mockReset();
  await i18n.changeLanguage("en");
});

describe("useLanguageSync", () => {
  it("switches i18next to Simplified Chinese when Settings overrides the language", async () => {
    vi.mocked(invoke).mockResolvedValue(baseSettings({ language: "ZhCn" }));

    renderWithProviders(<Probe />);

    await waitFor(() => expect(i18n.language).toBe("zh-CN"));
  });

  it("leaves i18next's detected language alone when Settings says 'System'", async () => {
    vi.mocked(invoke).mockResolvedValue(baseSettings({ language: "System" }));

    renderWithProviders(<Probe />);

    await waitFor(() => expect(vi.mocked(invoke)).toHaveBeenCalled());
    expect(i18n.language).toBe("en");
  });
});
```

- [ ] **Step 8: Run it and confirm it fails**

Run: `pnpm exec vitest run src/i18n/useLanguageSync.test.tsx`
Expected: FAIL — `Cannot find module './useLanguageSync'` (the file does not exist yet).

- [ ] **Step 9: Implement `useLanguageSync` and wire it into the app shell**

```ts
// src/i18n/useLanguageSync.ts
import { useEffect } from "react";
import { useSettings } from "../lib/queries";
import i18n from "./index";

/**
 * Keeps i18next's active language in sync with the user's Settings
 * override. When `settings.language` is `"System"`, i18next keeps
 * whatever language `i18next-browser-languagedetector` picked at startup
 * (see `src/i18n/index.ts`); otherwise this forces the exact language the
 * user chose (spec §9: the language follows the system unless overridden
 * in Settings).
 */
export function useLanguageSync(): void {
  const { data: settings } = useSettings();

  useEffect(() => {
    if (!settings) return;
    if (settings.language === "System") return;
    const target = settings.language === "ZhCn" ? "zh-CN" : "en";
    if (i18n.language !== target) {
      void i18n.changeLanguage(target);
    }
  }, [settings]);
}
```

`src/i18n/index.ts` (Task 9) is assumed to end with `export default i18n;` after its `i18next.use(...).init(...)` call — that default export is what the line above imports.

Open `src/App.tsx` and add:

```tsx
import { useLanguageSync } from "./i18n/useLanguageSync";
```

to its import list, and add:

```tsx
useLanguageSync();
```

as the first line inside the `App` function body, before whatever it already does. (The rest of `App.tsx` is Task 9's file and isn't fixed by this plan — this is the only change this task makes to it.)

- [ ] **Step 10: Run the full front-end suite**

Run: `pnpm test`
Expected: PASS — every suite green, including the 2 new tests in `useLanguageSync.test.tsx`, with no regressions in any file from Tasks 9–15.

- [ ] **Step 11: Commit**

```bash
git add src/i18n/zh-CN.json src/i18n/completeness.test.ts src/i18n/no-literal-strings.test.ts src/i18n/useLanguageSync.ts src/i18n/useLanguageSync.test.tsx src/App.tsx
git commit -m "feat(i18n): add zh-CN translations, completeness guards, and Settings language override

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 17: Empty and error states

**Files:**
- Create: `src/components/EmptyState.tsx`
- Create: `src/components/EmptyState.test.tsx`
- Create: `src/components/SnapshotStatus.tsx`
- Create: `src/components/SnapshotStatus.test.tsx`
- Modify: `src/i18n/en.json`
- Modify: `src/i18n/zh-CN.json`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `useSnapshot(): UseQueryResult<Snapshot>` and `useRefresh(): UseMutationResult<Snapshot, Error, void>` from `src/lib/queries.ts`; `Snapshot`, `DetectOutcome` from `src/lib/types.ts`.
- Produces:
  - `export interface EmptyStateAction { label: string; onClick: () => void }` and `export function EmptyState(props: { title: string; description: string; action?: EmptyStateAction; variant?: "empty" | "banner"; icon?: ReactNode }): JSX.Element`. `variant: "banner"` renders `role="status"` and is meant to sit above still-visible content (the stale-refresh case); the default `"empty"` variant fills the space where content would otherwise be.
  - `export function SnapshotStatus(props: { children: ReactNode }): JSX.Element` — reads the current `Snapshot` and renders the matching `EmptyState` in place of (or, for the stale case, above) `children`.

- [ ] **Step 1: Write the failing test for `EmptyState`**

```tsx
// src/components/EmptyState.test.tsx
import { describe, expect, it, vi } from "vitest";
import { screen, fireEvent } from "@testing-library/react";
import { renderWithProviders } from "../test/setup";
import { EmptyState } from "./EmptyState";

describe("EmptyState", () => {
  it("renders the title, the description and an optional action", () => {
    const onClick = vi.fn();
    renderWithProviders(
      <EmptyState
        title="Nothing installed yet"
        description="Once you install something with Homebrew, it will show up here."
        action={{ label: "Refresh", onClick }}
      />,
    );

    expect(screen.getByText("Nothing installed yet")).toBeInTheDocument();
    expect(
      screen.getByText("Once you install something with Homebrew, it will show up here."),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("renders as a status banner when variant is 'banner'", () => {
    renderWithProviders(
      <EmptyState
        title="Some data might be out of date"
        description="The last refresh couldn't finish for 1 source, so what you see below may be stale."
        variant="banner"
      />,
    );

    expect(screen.getByRole("status")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `pnpm exec vitest run src/components/EmptyState.test.tsx`
Expected: FAIL — `Cannot find module './EmptyState'` (the file does not exist yet).

- [ ] **Step 3: Implement `EmptyState`**

```tsx
// src/components/EmptyState.tsx
import type { ReactNode } from "react";

export interface EmptyStateAction {
  label: string;
  onClick: () => void;
}

export interface EmptyStateProps {
  title: string;
  description: string;
  action?: EmptyStateAction;
  variant?: "empty" | "banner";
  icon?: ReactNode;
}

export function EmptyState({
  title,
  description,
  action,
  variant = "empty",
  icon,
}: EmptyStateProps) {
  const isBanner = variant === "banner";

  return (
    <div
      role={isBanner ? "status" : undefined}
      className={
        isBanner
          ? "flex items-center gap-4 border-b border-[var(--color-border)] bg-[var(--color-muted)] px-6 py-3"
          : "flex flex-1 flex-col items-center justify-center gap-3 p-12 text-center"
      }
    >
      {icon}
      <div className={isBanner ? "flex-1" : undefined}>
        <p className={isBanner ? "font-medium" : "text-lg font-semibold"}>{title}</p>
        <p className="text-sm text-[var(--color-muted-foreground)]">{description}</p>
      </div>
      {action && (
        <button type="button" onClick={action.onClick}>
          {action.label}
        </button>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Run it and confirm it passes**

Run: `pnpm exec vitest run src/components/EmptyState.test.tsx`
Expected: PASS — 2 tests passed.

- [ ] **Step 5: Write the failing tests for `SnapshotStatus`**

```tsx
// src/components/SnapshotStatus.test.tsx
import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, fireEvent, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { SnapshotStatus } from "./SnapshotStatus";
import type { Snapshot } from "../lib/types";

function baseSnapshot(overrides: Partial<Snapshot> = {}): Snapshot {
  return {
    generation: 1,
    detect: "Found",
    instances: [],
    artifacts: [],
    updates: [],
    refreshed_at: 1700000000,
    stale: false,
    errors: [],
    ...overrides,
  };
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

describe("SnapshotStatus", () => {
  it("shows the no-Homebrew empty state and hides children when detect is Missing", async () => {
    vi.mocked(invoke).mockResolvedValue(baseSnapshot({ detect: "Missing" }));

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Homebrew isn't installed yet")).toBeInTheDocument();
    expect(screen.queryByText("installed list")).not.toBeInTheDocument();
  });

  it("shows the root-refusal empty state when detect is RefusedAsRoot", async () => {
    vi.mocked(invoke).mockResolvedValue(baseSnapshot({ detect: "RefusedAsRoot" }));

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(
      await screen.findByText("Canager can't run as an administrator"),
    ).toBeInTheDocument();
  });

  it("shows a stale banner above the existing data when the last refresh failed", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") {
        return baseSnapshot({
          stale: true,
          errors: [{ instance_id: "brew:/opt/homebrew", message: "timed out" }],
        });
      }
      if (cmd === "refresh") return baseSnapshot();
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Some data might be out of date")).toBeInTheDocument();
    expect(screen.getByText("installed list")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() =>
      expect(vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === "refresh")).toBe(true),
    );
  });

  it("shows the nothing-installed empty state when there are no artifacts", async () => {
    vi.mocked(invoke).mockResolvedValue(baseSnapshot({ artifacts: [] }));

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Nothing installed yet")).toBeInTheDocument();
  });

  it("renders children unchanged once something is installed", async () => {
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({
        artifacts: [
          {
            key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" },
            display_name: "jq",
            version: "1.7",
            reason: "Requested",
            description: null,
            homepage: null,
            size_bytes: null,
            installed_at: null,
            path: null,
            auto_updates: false,
          },
        ],
      }),
    );

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("installed list")).toBeInTheDocument();
  });
});
```

- [ ] **Step 6: Run it and confirm it fails**

Run: `pnpm exec vitest run src/components/SnapshotStatus.test.tsx`
Expected: FAIL — `Cannot find module './SnapshotStatus'` (the file does not exist yet).

- [ ] **Step 7: Implement `SnapshotStatus` and its copy**

```tsx
// src/components/SnapshotStatus.tsx
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useRefresh, useSnapshot } from "../lib/queries";
import { EmptyState } from "./EmptyState";

export interface SnapshotStatusProps {
  children: ReactNode;
}

export function SnapshotStatus({ children }: SnapshotStatusProps) {
  const { t } = useTranslation();
  const snapshotQuery = useSnapshot();
  const refreshMutation = useRefresh();
  const snapshot = snapshotQuery.data;

  if (!snapshot) {
    return <>{children}</>;
  }

  if (snapshot.detect === "Missing") {
    return (
      <EmptyState
        title={t("emptyStates.noHomebrew.title")}
        description={t("emptyStates.noHomebrew.description")}
      />
    );
  }

  if (snapshot.detect === "RefusedAsRoot") {
    return (
      <EmptyState
        title={t("emptyStates.refusedAsRoot.title")}
        description={t("emptyStates.refusedAsRoot.description")}
      />
    );
  }

  if (snapshot.stale && snapshot.errors.length > 0) {
    return (
      <>
        <EmptyState
          variant="banner"
          title={t("emptyStates.refreshFailed.title")}
          description={t("emptyStates.refreshFailed.description", {
            count: snapshot.errors.length,
          })}
          action={{
            label: t("emptyStates.refreshFailed.retry"),
            onClick: () => refreshMutation.mutate(),
          }}
        />
        {children}
      </>
    );
  }

  if (snapshot.artifacts.length === 0) {
    return (
      <EmptyState
        title={t("emptyStates.nothingInstalled.title")}
        description={t("emptyStates.nothingInstalled.description")}
      />
    );
  }

  return <>{children}</>;
}
```

Add this top-level key to `src/i18n/en.json`:

```json
"emptyStates": {
  "noHomebrew": {
    "title": "Homebrew isn't installed yet",
    "description": "Canager manages tools installed through Homebrew, npm, and more. Install Homebrew first, then come back here."
  },
  "refusedAsRoot": {
    "title": "Canager can't run as an administrator",
    "description": "Homebrew refuses to run under the root user for safety. Quit Canager, then open it again from your normal user account."
  },
  "refreshFailed": {
    "title": "Some data might be out of date",
    "description_one": "The last refresh couldn't finish for {{count}} source, so what you see below may be stale.",
    "description_other": "The last refresh couldn't finish for {{count}} sources, so what you see below may be stale.",
    "retry": "Try again"
  },
  "nothingInstalled": {
    "title": "Nothing installed yet",
    "description": "Once you install something with Homebrew, it will show up here."
  }
}
```

And this matching top-level key to `src/i18n/zh-CN.json` (this keeps Task 16's key-parity test green — English needs both `_one` and `_other` forms, Chinese only ever needs `_other`, and the parity test already normalizes plural suffixes before comparing):

```json
"emptyStates": {
  "noHomebrew": {
    "title": "还没有安装 Homebrew",
    "description": "Canager 管理通过 Homebrew、npm 等方式安装的工具。请先安装 Homebrew,然后再回到这里。"
  },
  "refusedAsRoot": {
    "title": "Canager 不能以管理员身份运行",
    "description": "出于安全考虑,Homebrew 拒绝以 root 用户运行。请退出 Canager,改用你平时的用户账户重新打开。"
  },
  "refreshFailed": {
    "title": "部分数据可能不是最新的",
    "description_other": "上次刷新有 {{count}} 个来源没能完成,下面显示的内容可能不是最新的。",
    "retry": "重试"
  },
  "nothingInstalled": {
    "title": "还没有安装任何东西",
    "description": "用 Homebrew 装点什么之后,就会显示在这里。"
  }
}
```

- [ ] **Step 8: Run it and confirm everything passes**

Run: `pnpm exec vitest run src/components/SnapshotStatus.test.tsx src/i18n/completeness.test.ts`
Expected: PASS — 5 tests passed in `SnapshotStatus.test.tsx`, 1 in `completeness.test.ts` (confirming the new `emptyStates` keys didn't break parity).

- [ ] **Step 9: Wire `SnapshotStatus` into the app shell**

Open `src/App.tsx` and add:

```tsx
import { SnapshotStatus } from "./components/SnapshotStatus";
```

to its import list. Then wrap only the routed-page area — whatever expression already renders the active one of `InstalledPage` / `UpdatesPage` / `SettingsPage` — in `<SnapshotStatus>…</SnapshotStatus>`, leaving the sidebar, the operation bar and the log drawer outside the wrapper so they stay visible in every state:

```tsx
<SnapshotStatus>
  {/* whatever App.tsx already renders for the active page goes here, unchanged */}
</SnapshotStatus>
```

- [ ] **Step 10: Run the full front-end suite and the production type-check**

Run: `pnpm test && pnpm build`
Expected: PASS — every suite green (no regressions from the `App.tsx` edit), and `tsc && vite build` completes with no type errors.

- [ ] **Step 11: Commit**

```bash
git add src/components/EmptyState.tsx src/components/EmptyState.test.tsx src/components/SnapshotStatus.tsx src/components/SnapshotStatus.test.tsx src/i18n/en.json src/i18n/zh-CN.json src/App.tsx
git commit -m "feat(ui): add the four empty/error surfaces for a new or offline user

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 18: Front-end tests in CI

**Files:**
- Modify: `package.json`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: the `pnpm test` script (this task pins it to run vitest non-interactively) and every `*.test.ts`/`*.test.tsx` file added by Tasks 9–17.
- Produces: nothing new for later tasks to import — this task only changes what CI runs.

- [ ] **Step 1: Confirm the CI workflow doesn't run front-end tests yet**

Run: `grep -n "pnpm test" .github/workflows/ci.yml`
Expected: no output, exit code 1 — `ci.yml` currently only runs `cargo fmt`/`clippy`/`test`, the live brew smoke test, `pnpm install`, `pnpm build` and the unsigned `tauri build`.

- [ ] **Step 2: Pin the `test` script to a non-interactive vitest run**

```bash
node -e '
const fs = require("fs");
const pkg = JSON.parse(fs.readFileSync("package.json", "utf-8"));
pkg.scripts = pkg.scripts || {};
pkg.scripts.test = "vitest run";
fs.writeFileSync("package.json", JSON.stringify(pkg, null, 2) + "\n");
'
```

This sets (or overwrites) exactly `"test": "vitest run"` in `package.json`'s `scripts` object, leaving every other key untouched — `vitest run` never enters watch mode, so `pnpm test` is safe to run unattended in CI regardless of how vitest's own CI auto-detection behaves.

- [ ] **Step 3: Run the tests locally to confirm the script works**

Run: `pnpm test`
Expected: PASS — every suite from Tasks 9–17 green, process exits 0 (it does not hang in watch mode).

- [ ] **Step 4: Add the `pnpm test` step to the workflow**

Replace `.github/workflows/ci.yml` with:

```yaml
name: CI

on:
  pull_request:
  push:
    # NOTE (backlog: 工作流与发布): this "feat/**" push trigger is a
    # temporary carry-over from the phase 0-1 branch-verification period —
    # it makes every PR's macOS job (including the real `brew install`
    # smoke test) run twice. Before merging to main it should either be
    # removed or paired with a concurrency group + timeout-minutes. Left
    # as-is here: reviewing it is out of scope for this task.
    branches: [main, "feat/**"]

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

      - name: live homebrew smoke (install/inventory/uninstall hello)
        env:
          CANAGER_LIVE: "1"
        run: cargo test -p canager-core --test brew_live -- --ignored --nocapture

      - name: pnpm install
        run: pnpm install --frozen-lockfile

      - name: pnpm test
        run: pnpm test

      - name: pnpm build
        run: pnpm build

      - name: tauri build (unsigned, no bundle)
        run: pnpm tauri build --target universal-apple-darwin --no-bundle
```

- [ ] **Step 5: Confirm the step landed**

Run: `grep -n "pnpm test" .github/workflows/ci.yml`
Expected: one match — `        run: pnpm test` — on the line just added between the `pnpm install` and `pnpm build` steps.

- [ ] **Step 6: Confirm the whole workflow still passes, command by command**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && pnpm install --frozen-lockfile && pnpm test && pnpm build && pnpm tauri build --target universal-apple-darwin --no-bundle`
Expected: PASS end to end, exit code 0 — this is the exact command sequence `ci.yml` now runs (minus the `CANAGER_LIVE` smoke test, which needs a real Homebrew and is optional here), so a clean local pass is strong evidence the workflow will pass in CI too.

- [ ] **Step 7: Commit**

```bash
git add package.json .github/workflows/ci.yml
git commit -m "ci: run pnpm test non-interactively as part of the build-and-test job

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```
