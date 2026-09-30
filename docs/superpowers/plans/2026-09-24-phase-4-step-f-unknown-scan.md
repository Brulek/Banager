# Phase 4 Step F: Unknown-Source Scan Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Banager a fourth page, *Unknown*, that lists the command-line programs on this Mac that none of the registered sources installed — a tool's own installer dropped a binary into `~/.local/bin`, a GUI app put a helper into `/usr/local/bin`, a link whose target has since been deleted — with what each one is, where it points, its size and date, and who put it there. The list is honest about what it did not see (which directories it read, how many programs known sources accounted for, whether it stopped early and at which limit) and it never runs, moves or deletes anything.

**Architecture:** A read-only pure function in a new `banager-core` module, `scan/`, over a clone of the current `Snapshot`'s instances and artifacts; a one-method `Session::scan_unknown`; one IPC command on Tauri's blocking pool; one TanStack query that only runs when asked; one React page. It is deliberately **not** an `Adapter` (phase 4 spec §8.1): it has no plan, execute, reconcile, instance or recorded fixture, and — decisively — attribution needs every *other* source's instances, which `inventory(&self, inst)` never sees. It does not enter the `Snapshot` and is not part of `refresh`: a ten-second directory walk is not something every refresh should pay, and snapshot data is about the managed sources.

**Tech Stack:** Rust (banager-core, tauri 2.11.x; `std::fs` + `std::os::unix::fs::MetadataExt` only — no new crate); React 19, TanStack Query v5, Zustand, i18next, vitest.

## Global Constraints

Binding project rules, copied from the phase 4 spec (`docs/superpowers/2026-09-24-phase-4-standalone-spec.md`):

- 产品规则一条不让（spec §1、§6）：每一步说人话；后台工作绝不问密码；执行前先看到确切命令；结果诚实——版本没动是 `NeedsAttention(UnchangedAfterUpgrade)`，中途停止是 `Unconfirmed`，没有证据绝不说成功；fixture 只收真机录制；Banager 不跑 shell、不把下载管进 `sh`；界面绝不提供 Rust 会拒绝的操作；所有文案 en + zh-CN。
- 每一步只带**该步有生产者**的变体与字段——「先定义、后面某步再用」正是本项目最常见的缺陷（§十）。线格式的每个字段点名读取方（§8.2）。
- For this step specifically (§8.4, appendix B): `scan_unknown` 是 `read_dir`/`symlink_metadata`/`canonicalize` 上的纯函数，没有 `CommandRunner`、没有 `OperationManager`；不执行（`Executable` 判断只看 mode 位，不跑 `file(1)`）、不写、不读列出目录之外的任何东西；不取资源锁；不并入 `Snapshot`、不并入 `refresh`。
- What this step is **not** (§8.1, §十 row F): rule 4 (`backup_globs`) and the `globs` parameter arrive with step D; `Glob` is not defined here. No per-row action ("Reveal in Finder", "Move to Trash") — §十一.
- No hard-coded user-visible strings — every one goes through `t()`, with matching keys in `en.json` and `zh-CN.json`. `src/i18n/completeness.test.ts` also requires every key to be looked up by a *literal* in non-test source (so lookups go through `Record`s of literal keys, never assembled strings) and registers interpolated heads in `INTERPOLATED_SUBTREES`; `src/i18n/no-literal-strings.test.ts` forbids English literals in JSX. zh-CN prose uses full-width `，：（）` between CJK characters.
- Colours use `bg-[var(--color-x)]` arbitrary values only; never bare semantic classes, never hex.
- Components never call `invoke`; only `src/lib/api.ts` does. No business logic in TypeScript.
- Fixtures come from real machines only. This step has **no fixture directory**: `scan` is not an adapter, and `tests/fixtures_layout_test.rs` requires the fixture directory set to equal the registered adapter ids exactly. The synthetic directory trees below are built in temp directories by the tests themselves; nothing under `adapters/fixtures/` changes (Task 3b *reads* the pipx fixture already recorded there and adds nothing to it). Inline literals in tests are not fixtures.
- Test names and test data carry no author-machine details: no author-machine paths, users or private app names. Public tool names the spec itself uses (`claude`, `agy`, `ruff`, `hexyl`, `python3.12`, rustup's thirteen proxies) are fine; everything else is invented (the research file with the real ones is deliberately not in the repo, §十三 #30).
- Definition of done for every task — the five gates from `README.md` "Tests — all five must pass before anything is committed":
  ```bash
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  pnpm test
  pnpm typecheck
  ```
  (`pnpm typecheck` is the TypeScript gate: it runs `tsc -p tsconfig.json` over production code with no Node types and `tsc -p tsconfig.test.json` over the vitest files.) Run `cargo fmt --all` before the `--check` gate: the Rust below is written *for* rustfmt, not *by* it, and rustfmt is the authority on line breaks. `pnpm exec prettier --write` is not a gate; leave the TypeScript as pasted.
- Commit messages: imperative subject in sentence case, a body that says why, then a blank line and the `Co-Authored-By:` attribution line **the executing session is instructed to use**. Every commit block below ends with the placeholder `Co-Authored-By: <the executing session's attribution line>`; substitute it, never paste it (this plan was written under one model's attribution and reviewed under another's, and neither binds whoever commits). `git add` names paths; never `-A`.

## Baseline

Branch `feat/phase-4-standalone`, HEAD `26bc640` = `8ba6f52` plus the spec commit. Every line number below was checked against that HEAD. Step F depends on nothing in B–E (§十): the attribution rules read `exe_path`, `InstalledArtifact.path` and an owned-roots table over the adapters that exist today. Step A may or may not have merged first; the only file both touch is `docs/what-we-run.md` (Task 10 appends a self-contained section either way).

## Rulings this plan makes

The spec leaves these to the step; each is decided here so no task has to.

1. **Standalone rows in `owned_roots` are added in step B, not here.** §8.3 lists `standalone-claude/agy/grok → <prefix>` among the owned roots. No adapter registers those ids until B's `standalone::all` runs, so at F a row for them can never match an instance: it would be a definition with no producer — exactly the pattern §十 forbids ("先定义、后面某步再用"). Task 3 leaves a comment in the table naming B as the step that adds those three rows (and `standalone-rustup` → nothing, everything of rustup's being rule 1's). Consequence for the interim, recorded in Task 10's delivery note: until B, a native `~/.local/bin/claude` (a symlink into `~/.local/share/claude/versions/…`) is listed on this page.
2. **`~` abbreviation happens in Rust, on `ScannedDir.path` and `UnknownEntry.path`.** §8.5 wants the row to show a `~`-abbreviated path and the front end has no `HOME` to strip. The spec's own precedent decides it: `Warning`'s `{{path}}` payloads have `$HOME` replaced by `~` on the Rust side "那是数据不是句子" (§6.5). `resolved` stays canonical and absolute — it is the technical detail. Attribution compares absolute paths; only the output is abbreviated. No field is added to the wire types.
3. **`scan_dirs` is the testable inner function.** `scan_unknown(env, …)` builds the directory list from `HostEnv` (§8.3's seven fixed directories ∪ `PATH` entries under `home`) and calls `scan_dirs(dirs, env, …)`. The synthetic-tree tests call `scan_dirs` with temp directories, so a *synthetic-tree* test never reads the `/usr/local/bin` of the machine running it. `scan_dirs` does the existence check and canonical de-duplication, so both are covered through the public path. The two tests that go through `scan_unknown` — the session test (Task 4) and the IPC test (Task 5) — do read `/usr/local/bin`, read-only and bounded by the budget, and nothing else of the machine's: both pass a `HostEnv` with no `PATH` entries and a home under the temp directory, so the other six fixed directories do not exist.
4. **`$CARGO_HOME/bin` joins the fixed list when `HostEnv.cargo_home` is set.** The spec's list is literal (`~/.cargo/bin`), but `HostEnv` carries `cargo_home` for the same reason it carries `path_dirs` (`runner/path_env.rs:9-14`), and with it set the proxies live under the override and `~/.cargo/bin` is usually absent. One `push`, de-duplicated with the rest.
5. **The Unknown page is routed outside `SnapshotStatus`.** `SnapshotStatus` replaces its children with "Nothing for Banager to manage yet" when `detect === "Missing"` — a Mac with no source at all is exactly where this page is most useful (everything on it is unknown), and the scan is not snapshot data. It is routed like `SettingsPage` (`App.tsx:26-32`).
6. **`formatBytes` is 1000-based** (`1 KB = 1000 B`), the convention Finder uses on macOS, so the number on the row matches the one the user sees in Finder's Get Info.
7. **A broken link has no size and no date.** Both describe the target, and a broken link has none; the link's own `mtime` says when an installer made the link, which is not what the row is about. So `size_bytes` and `modified_at` are `None` for `BrokenSymlink` — a real producer for both `None`s.
8. **"Scan again" is disabled while a scan is in flight.** §8.4's "连点两次跑两次，页面显示最新" describes what the Rust side tolerates — no lock, so two concurrent scans are safe — not a promise the button makes. While one runs the button reads "Scanning…" and is disabled: a second concurrent scan could only show what the first is about to show. (`useUnknownScan`'s `refetch` would in fact run both — TanStack cancels the first observer-side and the Rust one runs to completion — so dropping `disabled` would also satisfy the spec; this plan keeps it because a button that visibly does nothing on a second press is confusing to exactly the user this app is for.)
9. **The page re-scans when the snapshot's `generation` moves while it is open.** A scan judges against the snapshot as committed (§8.1). Opened before the startup refresh commits — or while any refresh is running — it would list every managed launcher as unknown, and nothing would correct that until "Scan again". So `UnknownPage` reads `useSnapshot().data?.generation` and runs the scan once per defined generation: on open, as soon as the snapshot query has answered (an in-memory read), and again whenever `generation` changes — which is how both `refreshIntoCache`'s cache write and a `SnapshotChanged` invalidation reach it. Still not part of `refresh` (Q11): Rust's refresh never runs the scan, nothing runs while the page is closed, and `SnapshotChanged` still invalidates only the snapshot query (`src/lib/events.ts:177`). A scan already in flight when the generation moves is restarted, not joined, so the page never settles on a judgement against a snapshot that has since moved; the price is that in development React's `StrictMode` (`src/main.tsx:11`) double-runs the mount effect and restarts the first scan once — a read that is thrown away, accepted (Task 8's comment says so).
10. **npm has an owned root: `<prefix>/lib/node_modules`.** §8.3 puts npm in the "empty" group on the premise that its `prefix` is `exe_path.parent()` (`npm.rs:194-197`). That line range is the `NotResponding` fallback only. A responding npm's prefix is the global prefix root `npm prefix -g` reports (`npm.rs:157-158`, `:240`; its doc comment at `:35-40`), and npm writes every global package under `<prefix>/lib/node_modules` (`:50`), with `<prefix>/bin/<tool>` a link into it. For a home prefix such as `~/.npm-global` that bin directory is on `PATH` and so scanned; without the row, every global npm CLI on such a Mac (the setup npm's own docs recommend over `sudo`) would be listed here while also listed under npm on the Installed page. `<prefix>/bin` itself is *not* a root, for the same reason Homebrew's prefix is not: on `/usr/local` it is where third-party installers drop things. A deviation from the spec's literal table, on the table's own principle — it lists what each adapter *owns*.
11. **pipx fills `InstalledArtifact.path` in this step (Task 3b).** pipx's inventory leaves `path: None` (`pipx.rs:90`), pipx has no owned root, and its shims in `~/.local/bin` are links into `<PIPX_HOME>/venvs/<tool>/bin/…` — so every pipx tool would be a row here and a row under pipx on the Installed page at once, and §8.3's "其余今天没有能归属的东西" is true only of a Mac with no pipx tools. `pipx list --json` already reports each exposed app's absolute path (`main_package.app_paths[].__Path__`, present in the recorded fixture `adapters/fixtures/pipx/1.17.3/list.json:11-16`); the venv directory is two levels up, and rule 2 then claims the shim exactly as it claims uv's. Producer and reader land in the same step, which is the rule §十 exists to enforce. What still has no producer: uv's own `uvx` (a second binary in `~/.local/bin`, not a link to `uv`, so rule 1 cannot see it) stays listed until the second batch's uv recipe (§十一); the delivery note says so.

## What already exists (do not rebuild)

`ManagerInstance { id, adapter_id, exe_path, prefix, … }` and `InstalledArtifact { key, path: Option<PathBuf>, … }` (`model.rs:122-146`, `:191-211`); `HostEnv { path_dirs, home, euid, cargo_home, ollama_host }` (`runner/path_env.rs:5-21`); `Session` with a private `snapshot: Mutex<Snapshot>` (`session/mod.rs:215`) read by `snapshot()` (`:369-371`) and by the child modules `refresh.rs`/`plans.rs`; `crate::testing::manager_instance` (`testing.rs:44-56`) and `session/test_support.rs`'s `fake_adapter_meta`/`fake_plan`/`fake_reconciled`; the `open_ollama_app` command's `spawn_blocking` shape (`src-tauri/src/ipc.rs:551-561`); `ArtifactRow` (name, `ReactNode` description with `wrapDescription`, badge), `SourceNotice` (a presentational banner), `renderWithProviders` (`src/test/setup.ts`, which also returns its `queryClient`), `useSettings`, `useSnapshot` (`src/lib/queries.ts:25-42`), the `nav.*` sidebar, and the two i18n guard tests.

`uv` already fills `InstalledArtifact.path` with each tool's venv directory (`adapters/uv.rs:65`); that is rule 2's first real input. pipx fills it in Task 3b (ruling 11); `cargo` fills it from step E; the standalone adapters from step B.

## File Structure

```
crates/banager-core/src/scan/mod.rs          NEW  wire types, ScanBudget, the walk, attribution rules 0–3, owned_roots table
crates/banager-core/src/lib.rs               MOD  `pub mod scan;` + one clause in the crate doc
crates/banager-core/src/session/scan.rs      NEW  Session::scan_unknown (clone snapshot, call scan::scan_unknown)
crates/banager-core/src/session/mod.rs       MOD  `mod scan;`
crates/banager-core/tests/unknown_scan_test.rs NEW synthetic directory trees: kinds, skips, budget, dedupe, rules 0–3
crates/banager-core/src/adapters/pipx.rs     MOD  InstalledArtifact.path from `app_paths` (rule 2's second producer, Task 3b)
src-tauri/src/ipc.rs                         MOD  scan_unknown_impl(session, env) + #[tauri::command] scan_unknown (spawn_blocking) + test
src-tauri/src/lib.rs                         MOD  register ipc::scan_unknown
src/lib/types.ts                             MOD  EntryKind, ScanStop, ScannedDir, UnknownEntry, UnknownScan
src/lib/types.test.ts                        MOD  shape test for the five wire types
src/lib/api.ts, api.test.ts                  MOD  scanUnknown()
src/lib/queryKeys.ts                         MOD  queryKeys.unknown
src/lib/queries.ts, queries.test.ts          MOD  useUnknownScan() (enabled: false)
src/lib/format.ts, format.test.ts            MOD  formatBytes
src/pages/UnknownPage.tsx                    NEW  the page: header + Scan again, stopped banner, rows, footer; one scan per snapshot generation while open
src/pages/UnknownPage.test.tsx               NEW
src/store/ui.ts, ui.test.ts                  MOD  Page union gains "unknown"
src/components/Sidebar.tsx, Sidebar.test.tsx MOD  PAGES gains "unknown"
src/App.tsx, App.test.tsx                    MOD  route
src/i18n/completeness.test.ts                MOD  INTERPOLATED_SUBTREES.nav gains "unknown"
src/i18n/en.json, zh-CN.json                 MOD  nav.unknown + unknown.*
docs/what-we-run.md                          MOD  "Unknown-source scan" section
README.md                                    MOD  the page in "What it manages"; the on-demand-refresh limitation; test counts
docs/superpowers/backlog.md                  MOD  one sentence under 「整个应用没有刷新按钮」: the page-scoped exception
```

## Core Interfaces (authoritative — every task uses these names verbatim)

```rust
// crates/banager-core/src/scan/mod.rs
pub struct ScanBudget { pub max_entries: usize, pub max_duration: std::time::Duration }   // Default: 2000, 10 s
pub enum ScanStop { FileLimit { max_entries: u32 }, TimeLimit { max_secs: u32 } }
pub struct ScannedDir { pub path: PathBuf, pub entries: u32 }
pub enum EntryKind { File, Symlink, BrokenSymlink }
pub struct UnknownEntry {
    pub path: PathBuf, pub kind: EntryKind, pub resolved: Option<PathBuf>, pub link_target: Option<String>,
    pub size_bytes: Option<u64>, pub modified_at: Option<i64>, pub owned_by_me: bool, pub app_bundle: Option<String>,
}
pub struct UnknownScan { pub scanned: Vec<ScannedDir>, pub entries: Vec<UnknownEntry>, pub attributed: u32, pub stopped: Option<ScanStop> }

pub fn scan_unknown(env: &HostEnv, instances: &[ManagerInstance], artifacts: &[InstalledArtifact], budget: ScanBudget) -> UnknownScan;
pub fn scan_dirs(dirs: &[PathBuf], env: &HostEnv, instances: &[ManagerInstance], artifacts: &[InstalledArtifact], budget: ScanBudget) -> UnknownScan;
pub fn owned_roots(inst: &ManagerInstance) -> Vec<PathBuf>;

// crates/banager-core/src/session/scan.rs
impl Session { pub fn scan_unknown(&self, env: &HostEnv) -> UnknownScan; }

// src-tauri/src/ipc.rs
pub(crate) fn scan_unknown_impl(session: &Session, env: &HostEnv) -> UnknownScan;
#[tauri::command] pub async fn scan_unknown(state: State<'_, AppState>) -> Result<UnknownScan, String>;
```

```ts
// src/lib/types.ts
export type EntryKind = "File" | "Symlink" | "BrokenSymlink";
export type ScanStop = { FileLimit: { max_entries: number } } | { TimeLimit: { max_secs: number } };
export interface ScannedDir { path: string; entries: number }
export interface UnknownEntry { path: string; kind: EntryKind; resolved: string | null; link_target: string | null;
  size_bytes: number | null; modified_at: number | null; owned_by_me: boolean; app_bundle: string | null }
export interface UnknownScan { scanned: ScannedDir[]; entries: UnknownEntry[]; attributed: number; stopped: ScanStop | null }
// src/lib/api.ts        export function scanUnknown(): Promise<UnknownScan>
// src/lib/queryKeys.ts  unknown: ["unknown"] as const
// src/lib/queries.ts    export function useUnknownScan(): UseQueryResult<UnknownScan>
// src/lib/format.ts     export function formatBytes(bytes: number): string
// src/store/ui.ts       export type Page = "installed" | "updates" | "unknown" | "settings"
```

## Task List

| # | Task | Deliverable |
|---|---|---|
| 1 | Wire types and `ScanBudget` in `scan/mod.rs`, with shape tests | the five wire types the page reads, byte-for-byte what the TS mirror expects |
| 2 | The walk: `scan_dirs`/`scan_unknown`, kinds, skips, budget, dedupe, `~`, `.app`, rules 0–2 | `tests/unknown_scan_test.rs` on synthetic trees |
| 3 | Rule 3: the `owned_roots` table (brew, ollama, npm), longest root wins, the pip counter-example | `owned_roots` and its tests; the standalone-rows decision recorded |
| 3b | pipx fills `InstalledArtifact.path` from `app_paths` | rule 2's second producer: a pipx shim is claimed the way uv's is |
| 4 | `Session::scan_unknown` | the core's one entry point, over a cloned snapshot, no lock, no commit |
| 5 | IPC `scan_unknown` on the blocking pool, registered | the shell can be asked |
| 6 | TS mirror, `scanUnknown`, `queryKeys.unknown`, `useUnknownScan` | the front end can ask, and the shapes are pinned |
| 7 | `formatBytes` | the size on the row |
| 8 | `UnknownPage` + `unknown.*` copy in both locales | the page, fully worded, one scan per snapshot generation, not yet reachable |
| 9 | Navigation: `Page`, `Sidebar`, `App` route, `INTERPOLATED_SUBTREES.nav`, `nav.unknown` | the sidebar entry |
| 10 | Docs: `what-we-run.md` section, README, test counts; delivery note | the trust file says what the scan reads |

---

### Task 1: Wire types and `ScanBudget`

**Files:**
- Create: `crates/banager-core/src/scan/mod.rs`
- Modify: `crates/banager-core/src/lib.rs:3-10` (crate doc), `:46-53` (module list)

**Interfaces:**
- Consumes: nothing new.
- Produces (verbatim, from Core Interfaces): `ScanBudget` (+ `Default`), `ScanStop`, `ScannedDir`, `EntryKind`, `UnknownEntry`, `UnknownScan`. Readers: Task 2's `scan_dirs` builds every one of them; Task 5's command returns `UnknownScan`; Task 8's page reads every field (§8.2 names the reader per field; those names are in the doc comments below).

- [ ] **Step 1: Write the failing shape tests**

Create `crates/banager-core/src/scan/mod.rs` with only the module doc, the imports, and the test module (the types come in Step 3):

```rust
//! The unknown-source scan: the command-line programs on this Mac that
//! none of the registered sources installed (design spec §4.2; phase 4
//! spec §八).
//!
//! Not an `Adapter`, on purpose. An adapter has a `plan`, an `execute`, a
//! `reconcile`, an instance and a fixture directory of recorded output,
//! and this has none of them; registering it as one would trip the
//! fixture-set equality test, put it in the operation manager's registry
//! and make the plan gate answer "is this actionable?" for something
//! that can never be. The decisive reason is simpler: deciding who owns a
//! program needs *every other* source's instances and artifacts, and an
//! adapter's `inventory(&self, inst)` sees only its own. So this is a
//! pure function over a clone of the snapshot (`Session::scan_unknown`),
//! run on demand from the Unknown page -- never from a refresh, never
//! into the `Snapshot`, never under a lock.
//!
//! Read-only in the strictest sense: `read_dir`, `symlink_metadata`,
//! `metadata`, `read_link` and `canonicalize`, one level deep, over a
//! fixed list of bin directories. No `CommandRunner`, so nothing it finds
//! is ever run; no write of any kind.
//!
//! Under `scan/`, not `adapters/unknown.rs` as the design spec's §3 drew
//! it: someone reading `adapters/` should not find a module there with no
//! `impl Adapter` (phase 4 spec §8.1, Q12; its appendix C records the
//! deviation).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_budget_default_is_the_spec_numbers() {
        let budget = ScanBudget::default();
        assert_eq!(budget.max_entries, 2000);
        assert_eq!(budget.max_duration, Duration::from_secs(10));
    }

    #[test]
    fn test_scan_wire_shapes_match_the_hand_written_ts_mirror() {
        // `src/lib/types.ts` spells `EntryKind` as bare strings and
        // `ScanStop` as externally tagged single-key objects carrying the
        // limit the scan really enforced -- so `unknown.stopped.*` can
        // print that number rather than a copy typed into the locale
        // files (`Fault::HomebrewStillUpdating { minutes }` is the
        // precedent, model.rs).
        assert_eq!(serde_json::to_string(&EntryKind::File).unwrap(), r#""File""#);
        assert_eq!(
            serde_json::to_string(&EntryKind::Symlink).unwrap(),
            r#""Symlink""#
        );
        assert_eq!(
            serde_json::to_string(&EntryKind::BrokenSymlink).unwrap(),
            r#""BrokenSymlink""#
        );
        assert_eq!(
            serde_json::to_string(&ScanStop::FileLimit { max_entries: 2000 }).unwrap(),
            r#"{"FileLimit":{"max_entries":2000}}"#
        );
        assert_eq!(
            serde_json::to_string(&ScanStop::TimeLimit { max_secs: 10 }).unwrap(),
            r#"{"TimeLimit":{"max_secs":10}}"#
        );

        let scan = UnknownScan {
            scanned: vec![ScannedDir {
                path: PathBuf::from("~/.local/bin"),
                entries: 5,
            }],
            entries: vec![UnknownEntry {
                path: PathBuf::from("~/.local/bin/old-script"),
                kind: EntryKind::BrokenSymlink,
                resolved: None,
                link_target: Some(
                    "/Applications/Removed.app/Contents/Resources/index.js".to_string(),
                ),
                size_bytes: None,
                modified_at: None,
                owned_by_me: true,
                app_bundle: Some("Removed".to_string()),
            }],
            attributed: 4,
            stopped: None,
        };
        let json = serde_json::to_string(&scan).expect("serialize");
        assert!(
            json.contains(r#""stopped":null"#),
            "a complete scan carries an explicit null, not a missing key: {json}"
        );
        assert!(json.contains(r#""kind":"BrokenSymlink""#), "{json}");
        assert!(json.contains(r#""resolved":null"#), "{json}");
        assert!(json.contains(r#""owned_by_me":true"#), "{json}");
        assert_eq!(
            serde_json::from_str::<UnknownScan>(&json).expect("deserialize"),
            scan
        );

        let stopped = UnknownScan {
            stopped: Some(ScanStop::TimeLimit { max_secs: 10 }),
            ..scan
        };
        let json = serde_json::to_string(&stopped).expect("serialize");
        assert!(json.contains(r#""stopped":{"TimeLimit":{"max_secs":10}}"#), "{json}");
        assert_eq!(
            serde_json::from_str::<UnknownScan>(&json).expect("deserialize"),
            stopped
        );
    }
}
```

Modify `crates/banager-core/src/lib.rs` — add the module between `pub mod runner;` and `pub mod session;` (`:51-52`):

```rust
pub mod runner;
pub mod scan;
pub mod session;
```

and extend the crate doc's first paragraph (`:3-10`) so the module list there stays true — replace the sentence ending `and reports what actually happened.` with:

```rust
//! engine that turns a user's request into a plan, executes it under locks
//! and cancellation, and reports what actually happened. [`scan`] is the
//! one read-only path beside them: which programs in the usual bin
//! directories none of those sources put there.
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core scan::`
Expected: FAIL to compile — `error[E0422]: cannot find struct, variant or union type \`UnknownScan\` in this scope` (and the same for `ScannedDir`, `UnknownEntry`, at the struct literals) and `error[E0433]: failed to resolve: use of undeclared type \`ScanBudget\`` (and the same for `EntryKind`, `ScanStop`, at the path uses `ScanBudget::default()`, `EntryKind::File`, `ScanStop::FileLimit { … }`).

- [ ] **Step 3: Add the types**

Insert into `crates/banager-core/src/scan/mod.rs`, between the `use` lines and `#[cfg(test)]`:

```rust
/// How much of the file system one scan may look at before it stops and
/// says so. Runtime values rather than constants, so the two numbers the
/// user reads in the "this list may be incomplete" banner come from the
/// same place the scan enforced them (`ScanStop` carries them out;
/// `Fault::HomebrewStillUpdating { minutes }` in model.rs is the
/// precedent for putting the number in the payload).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanBudget {
    /// Directory entries examined, counting the ones a known source
    /// claimed and the ones skipped as subdirectories or non-executables.
    pub max_entries: usize,
    /// Wall-clock time from the start of the walk -- the clock starts
    /// after the known sources are indexed, so canonicalising their paths
    /// is not charged to it -- checked before every `read_dir` and before
    /// every entry.
    pub max_duration: Duration,
}

impl Default for ScanBudget {
    /// The design spec's §4.2 numbers. Sized for a `~/bin` of a few
    /// thousand files; the seven directories on the research machine held
    /// 26 entries between them and took 64 ms.
    fn default() -> ScanBudget {
        ScanBudget {
            max_entries: 2000,
            max_duration: Duration::from_secs(10),
        }
    }
}

/// Why a scan stopped before it had looked at everything. Read by the
/// Unknown page's banner (`unknown.stopped.FileLimit` /
/// `unknown.stopped.TimeLimit`), which prints the number carried here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScanStop {
    /// `ScanBudget::max_entries` entries had been examined and there was
    /// another.
    FileLimit { max_entries: u32 },
    /// `ScanBudget::max_duration` had elapsed before the next `read_dir`
    /// or the next entry.
    TimeLimit { max_secs: u32 },
}

/// One directory the scan actually read, and how many of its entries it
/// examined (claimed, listed or skipped alike). The page's "Looked in:"
/// footer, so an empty list reads as "looked in seven places", not
/// "didn't look".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScannedDir {
    /// With the home directory abbreviated to `~`, on this side, for the
    /// reason `Warning`'s `{{path}}` payloads are: the front end has no
    /// `HOME` to strip, and this is data, not a sentence.
    pub path: PathBuf,
    pub entries: u32,
}

/// What one listed entry is. Read by the page's kind badge
/// (`unknown.kind.*`, through a `Record<EntryKind, string>` so a variant
/// added here without copy fails `tsc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    File,
    Symlink,
    /// A symlink whose target `canonicalize` could not reach. `link_target`
    /// still carries what it says; the research machine had one pointing
    /// into an app that had since been deleted.
    BrokenSymlink,
}

/// One program no registered source accounts for. Every field is read by
/// the Unknown page (`src/pages/UnknownPage.tsx`); the comment on each
/// says where.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnknownEntry {
    /// The entry as found, `~`-abbreviated like `ScannedDir::path`. The
    /// row's name is its last component; the row's first line is the path.
    pub path: PathBuf,
    /// The badge.
    pub kind: EntryKind,
    /// `canonicalize` of the entry: every link hop followed, absolute, never
    /// abbreviated. `None` for a broken link, and for the rare regular
    /// file whose parent cannot be resolved. Shown under technical details
    /// as "Links to …" for a `Symlink`.
    pub resolved: Option<PathBuf>,
    /// `readlink`'s text, verbatim, for links only -- relative or absolute
    /// as the installer wrote it. The `{{target}}` of the broken-link
    /// sentence.
    pub link_target: Option<String>,
    /// The target's size. `None` for a broken link: there is no target to
    /// measure. Formatted by `formatBytes` into the size · date subtitle.
    pub size_bytes: Option<u64>,
    /// The target's modification time, unix seconds. `None` for a broken
    /// link, whose own `mtime` would only say when the link was made.
    /// Formatted with `Intl.DateTimeFormat` (an absolute date; this
    /// repository deliberately has no relative-time formatter).
    pub modified_at: Option<i64>,
    /// Whether the entry itself belongs to the user Banager runs as
    /// (`st_uid == euid`, of the entry, not its target: the question is
    /// who put it here). `false` renders "Put here by an installer with
    /// administrator rights".
    pub owned_by_me: bool,
    /// The `.app` bundle any component of the path runs inside, without
    /// the `.app`; tried on `resolved`, then on a link's own text (the
    /// only path a broken link has), then on the entry's path. Renders
    /// "Part of {{app}}".
    pub app_bundle: Option<String>,
}

/// The result of one scan. Not the `Snapshot`'s: produced on demand by
/// `Session::scan_unknown`, returned by the `scan_unknown` IPC command,
/// held only by the Unknown page's query.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnknownScan {
    /// Every directory actually read, in scan order. Directories that do
    /// not exist are not here (29 of the research machine's 36 candidates
    /// did not).
    pub scanned: Vec<ScannedDir>,
    /// The programs nobody claimed: the rows.
    pub entries: Vec<UnknownEntry>,
    /// How many examined programs a registered source accounted for and
    /// are therefore not listed. The page's "N more programs came from
    /// sources Banager knows" sentence.
    pub attributed: u32,
    /// `Some` when the scan hit its budget; the page's banner. What was
    /// scanned before that point is still in the fields above.
    pub stopped: Option<ScanStop>,
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core scan::`
Expected: `test scan::tests::test_scan_budget_default_is_the_spec_numbers ... ok`, `test scan::tests::test_scan_wire_shapes_match_the_hand_written_ts_mirror ... ok`.

- [ ] **Step 5: Run the gates**

Run all five from Global Constraints. Expected: all clean. (`ScanBudget` has no non-test reader yet; it is `pub`, so `dead_code` does not fire — Task 2 reads it.)

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/scan/mod.rs crates/banager-core/src/lib.rs
git commit -m "Add the unknown-source scan's wire types and budget

The scan that lists programs none of the registered sources installed
is a read-only pure function, not an Adapter: attribution needs every
other source's instances, which inventory(&self, inst) never sees. This
is its module, with the five wire types the Unknown page reads and the
budget it stops at. ScanStop carries the limit it hit so the banner's
number is the one the scan enforced, never a second copy in the locale
files.

Co-Authored-By: <the executing session's attribution line>"
```

---

### Task 2: The walk — `scan_dirs`, kinds, skips, budget, dedupe, `~`, `.app`, and rules 0–2

**Files:**
- Modify: `crates/banager-core/src/scan/mod.rs` (add the functions between the types and `#[cfg(test)]`; add unit tests to `mod tests`)
- Create: `crates/banager-core/tests/unknown_scan_test.rs`

**Interfaces:**
- Consumes: `HostEnv` (`runner/path_env.rs:5-21`), `ManagerInstance.exe_path` / `.id`, `InstalledArtifact.path` / `.key.instance_id` (`model.rs:126`, `:201`, `:186`), `crate::testing::manager_instance` (`testing.rs:44`).
- Produces (verbatim):
  ```rust
  pub fn scan_unknown(env: &HostEnv, instances: &[ManagerInstance], artifacts: &[InstalledArtifact], budget: ScanBudget) -> UnknownScan;
  pub fn scan_dirs(dirs: &[PathBuf], env: &HostEnv, instances: &[ManagerInstance], artifacts: &[InstalledArtifact], budget: ScanBudget) -> UnknownScan;
  ```
  plus private `candidate_dirs`, `display_path`, `app_bundle`, `examine`, `Known { exe_raw, exe_canonical, artifact_roots }` with `Known::index` and `Known::claimant`. Rules implemented here: 0 (raw `exe_path` equality), 1 (canonical `exe_path` equality), 2 (resolved path starts with a canonical `artifact.path`). Rule 3 is Task 3.

- [ ] **Step 1: Write the failing synthetic-tree tests**

Create `crates/banager-core/tests/unknown_scan_test.rs`:

```rust
//! The unknown-source scan over directory trees each test builds itself
//! in a temp directory. No recorded fixture: `scan` is not an adapter
//! and has no fixture directory (`fixtures_layout_test` requires the
//! fixture set to equal the registered adapter ids), and every shape a
//! scan has to handle -- a broken link, a two-hop link, a subdirectory, a
//! file with no execute bit, 2001 files -- is something a test can make
//! in a millisecond and the research machine could not (its seven
//! directories held 26 entries; §8.4 says the budget is covered
//! synthetically, not by recording).
//!
//! Every name here is invented. The research file with the real ones is
//! deliberately not in the repository (phase 4 spec §十三 #30).

use banager_core::model::{
    ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, ManagerInstance,
};
use banager_core::runner::HostEnv;
use banager_core::scan::{scan_dirs, EntryKind, ScanBudget, ScanStop, ScannedDir};
use banager_core::testing::manager_instance;
use std::fs;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A fresh home directory for one test, removed when the test ends.
/// Canonical (`fs::canonicalize`) so the paths a test writes compare
/// equal to the ones the scan canonicalises: on macOS `temp_dir()` is
/// `/var/folders/…`, and `/var` is a symlink to `/private/var`.
struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Home {
        let raw = std::env::temp_dir().join(format!(
            "banager-scan-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&raw).expect("create temp home");
        Home(fs::canonicalize(&raw).expect("canonical temp home"))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    /// A directory under this home, created.
    fn dir(&self, rel: &str) -> PathBuf {
        let dir = self.0.join(rel);
        fs::create_dir_all(&dir).expect("create dir");
        dir
    }

    /// `HostEnv` for this home: the scan's `euid` is the owner of the home
    /// itself, which is the user running the test.
    fn env(&self, path_dirs: Vec<PathBuf>) -> HostEnv {
        HostEnv {
            path_dirs,
            home: self.0.clone(),
            euid: fs::metadata(&self.0).expect("stat home").uid(),
            cargo_home: None,
            ollama_host: None,
        }
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// An executable regular file holding `bytes`.
fn exe(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, bytes).expect("write file");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

/// A regular file with no execute bit at all.
fn plain(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, b"text").expect("write file");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");
    path
}

/// A symlink `dir/name` whose text is exactly `target`.
fn link(dir: &Path, name: &str, target: &Path) -> PathBuf {
    let path = dir.join(name);
    symlink(target, &path).expect("symlink");
    path
}

fn artifact(instance_id: &str, name: &str, path: &Path) -> InstalledArtifact {
    InstalledArtifact {
        key: ArtifactKey {
            instance_id: instance_id.to_string(),
            kind: ArtifactKind::Tool,
            name: name.to_string(),
        },
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
    }
}

fn tilde(rel: &str) -> PathBuf {
    PathBuf::from("~").join(rel)
}

#[test]
fn test_lists_an_executable_nobody_claims_with_kind_size_date_and_home_abbreviated() {
    let home = Home::new("plain-exe");
    let bin = home.dir(".local/bin");
    let tool = exe(&bin, "standalone-tool", b"#!/bin/sh\necho hi\n");

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[], &[], ScanBudget::default());

    assert_eq!(
        scan.scanned,
        vec![ScannedDir {
            path: tilde(".local/bin"),
            entries: 1
        }]
    );
    assert_eq!(scan.attributed, 0);
    assert_eq!(scan.stopped, None);
    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    let entry = &scan.entries[0];
    assert_eq!(entry.path, tilde(".local/bin/standalone-tool"));
    assert_eq!(entry.kind, EntryKind::File);
    assert_eq!(entry.resolved.as_deref(), Some(tool.as_path()));
    assert_eq!(entry.link_target, None);
    assert_eq!(entry.size_bytes, Some(18));
    assert!(entry.modified_at.is_some_and(|t| t > 0), "{entry:?}");
    assert!(entry.owned_by_me);
    assert_eq!(entry.app_bundle, None);
}

#[test]
fn test_a_broken_symlink_is_listed_with_its_link_text_no_size_and_the_app_it_named() {
    let home = Home::new("broken");
    let bin = home.dir(".local/bin");
    let target = Path::new("/Applications/Removed.app/Contents/Resources/scripts/index.js");
    link(&bin, "old-script", target);

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[], &[], ScanBudget::default());

    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    let entry = &scan.entries[0];
    assert_eq!(entry.path, tilde(".local/bin/old-script"));
    assert_eq!(entry.kind, EntryKind::BrokenSymlink);
    assert_eq!(entry.resolved, None);
    assert_eq!(entry.link_target.as_deref(), Some(target.to_str().unwrap()));
    // Both describe the target, and a broken link has none.
    assert_eq!(entry.size_bytes, None);
    assert_eq!(entry.modified_at, None);
    assert_eq!(entry.app_bundle.as_deref(), Some("Removed"));
}

#[test]
fn test_a_two_hop_symlink_resolves_to_its_final_target() {
    // `python3.12 -> …/cpython-3.12-…/bin/python3.12`, where the
    // versionless directory is itself a link to the patch-versioned one:
    // one `readlink` is not enough, `canonicalize` is.
    let home = Home::new("two-hop");
    let bin = home.dir(".local/bin");
    let real_dir = home.dir(".local/share/runtime/versions/3.12.14/bin");
    let real = exe(&real_dir, "python3", b"binary");
    let versions = home.path().join(".local/share/runtime/versions");
    link(&versions, "3.12", Path::new("3.12.14"));
    let hop = versions.join("3.12/bin/python3");
    link(&bin, "python3", &hop);

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[], &[], ScanBudget::default());

    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    let entry = &scan.entries[0];
    assert_eq!(entry.kind, EntryKind::Symlink);
    assert_eq!(entry.resolved.as_deref(), Some(real.as_path()));
    assert_eq!(entry.link_target.as_deref(), Some(hop.to_str().unwrap()));
    assert_eq!(entry.size_bytes, Some(6));
}

#[test]
fn test_skips_subdirectories_files_without_an_execute_bit_and_links_to_directories() {
    let home = Home::new("skips");
    let bin = home.dir(".local/bin");
    home.dir(".local/bin/store");
    plain(&bin, "notes.txt");
    link(&bin, "data", Path::new("store"));

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[], &[], ScanBudget::default());

    assert!(scan.entries.is_empty(), "nothing here is a program: {:?}", scan.entries);
    // All three were examined -- they count against the budget and in the
    // footer -- they are just not listed.
    assert_eq!(scan.scanned[0].entries, 3);
    assert_eq!(scan.attributed, 0);
}

#[test]
fn test_a_directory_that_does_not_exist_is_skipped_and_not_reported() {
    let home = Home::new("missing-dir");
    let bin = home.dir(".local/bin");
    exe(&bin, "tool", b"x");
    let missing = home.path().join("bin");

    let scan = scan_dirs(
        &[missing, bin],
        &home.env(vec![]),
        &[],
        &[],
        ScanBudget::default(),
    );

    assert_eq!(
        scan.scanned,
        vec![ScannedDir {
            path: tilde(".local/bin"),
            entries: 1
        }]
    );
}

#[test]
fn test_two_paths_to_the_same_directory_are_read_once() {
    // A PATH that names `~/.local/bin` through a link as well as directly.
    let home = Home::new("dedupe");
    let bin = home.dir(".local/bin");
    exe(&bin, "tool", b"x");
    let alias = link(home.path(), "linkbin", Path::new(".local/bin"));

    let scan = scan_dirs(
        &[bin, alias],
        &home.env(vec![]),
        &[],
        &[],
        ScanBudget::default(),
    );

    assert_eq!(scan.scanned.len(), 1, "{:?}", scan.scanned);
    assert_eq!(scan.scanned[0].path, tilde(".local/bin"));
    assert_eq!(scan.entries.len(), 1);
}

#[test]
fn test_stops_at_the_file_limit_and_says_which_limit() {
    let home = Home::new("file-limit");
    let bin = home.dir("bin");
    for i in 0..2000 {
        exe(&bin, &format!("t{i:04}"), b"x");
    }

    let full = scan_dirs(
        std::slice::from_ref(&bin),
        &home.env(vec![]),
        &[],
        &[],
        ScanBudget::default(),
    );
    assert_eq!(full.stopped, None, "exactly the budget is not over it");
    assert_eq!(full.scanned[0].entries, 2000);
    assert_eq!(full.entries.len(), 2000);

    exe(&bin, "t2000", b"x");
    let over = scan_dirs(&[bin], &home.env(vec![]), &[], &[], ScanBudget::default());
    assert_eq!(over.stopped, Some(ScanStop::FileLimit { max_entries: 2000 }));
    // What was examined before the stop is still reported.
    assert_eq!(
        over.scanned,
        vec![ScannedDir {
            path: tilde("bin"),
            entries: 2000
        }]
    );
    assert_eq!(over.entries.len(), 2000);
}

#[test]
fn test_stops_at_a_zero_time_budget_before_reading_anything() {
    let home = Home::new("time-limit");
    let bin = home.dir(".local/bin");
    exe(&bin, "tool", b"x");
    let budget = ScanBudget {
        max_entries: 2000,
        max_duration: Duration::ZERO,
    };

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[], &[], budget);

    assert_eq!(scan.stopped, Some(ScanStop::TimeLimit { max_secs: 0 }));
    assert!(scan.scanned.is_empty(), "{:?}", scan.scanned);
    assert!(scan.entries.is_empty(), "{:?}", scan.entries);
    assert_eq!(scan.attributed, 0);
}

#[test]
fn test_a_path_component_ending_in_dot_app_names_the_bundle() {
    let home = Home::new("app-bundle");
    let bin = home.dir(".local/bin");
    let helpers = home.dir("Applications/Helper.app/Contents/Helpers");
    let real = exe(&helpers, "helper-cli", b"x");
    link(&bin, "helper-cli", &real);

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[], &[], ScanBudget::default());

    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    assert_eq!(scan.entries[0].kind, EntryKind::Symlink);
    assert_eq!(scan.entries[0].app_bundle.as_deref(), Some("Helper"));
}

#[test]
fn test_rule_0_claims_an_instances_launcher_by_its_raw_path_even_when_dangling() {
    // The half-uninstalled state step B calls `LauncherOnly`: the program
    // directory is gone, the launcher link is still there. `canonicalize`
    // fails on it, so rules 1-3 cannot see it; without rule 0 it would be
    // a "broken link" row here and a source on the Installed page at once.
    let home = Home::new("rule-0-dangling");
    let bin = home.dir(".local/bin");
    let launcher = link(
        &bin,
        "claude",
        &home.path().join(".local/share/claude/versions/2.1.281"),
    );
    let instance = ManagerInstance {
        exe_path: launcher,
        prefix: home.path().join(".local/share/claude"),
        ..manager_instance("standalone-claude", "standalone-claude")
    };

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[instance], &[], ScanBudget::default());

    assert!(
        scan.entries.is_empty(),
        "the dangling launcher is the source's, not unknown: {:?}",
        scan.entries
    );
    assert_eq!(scan.attributed, 1);
}

#[test]
fn test_rule_0_claims_an_instances_launcher_and_nothing_else_in_its_directory() {
    // The research machine's `~/.local/bin/python3.12`: a link into uv's
    // Python that the pip adapter detects as an interpreter, giving a pip
    // instance whose `exe_path` is that link and whose `prefix` is
    // `~/.local/bin` itself (pip.rs:125-128 takes `exe_path.parent()`).
    // Rule 0 claims the interpreter -- it really is a listed source's
    // executable. Nothing claims `agy` beside it: the prefix is not an
    // owned root (Task 3 makes that explicit; this test must keep
    // passing once rule 3 exists).
    let home = Home::new("rule-0-raw");
    let bin = home.dir(".local/bin");
    let python_dir = home.dir(".local/share/uv/python/cpython-3.12.14/bin");
    let python = exe(&python_dir, "python3.12", b"x");
    let interpreter = link(&bin, "python3.12", &python);
    exe(&bin, "agy", b"x");
    let pip = ManagerInstance {
        exe_path: interpreter.clone(),
        prefix: bin.clone(),
        ..manager_instance("pip", &format!("pip:{}", interpreter.display()))
    };

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[pip], &[], ScanBudget::default());

    assert_eq!(scan.attributed, 1);
    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    assert_eq!(scan.entries[0].path, tilde(".local/bin/agy"));
}

#[test]
fn test_rule_1_claims_everything_that_resolves_to_an_instances_launcher() {
    // `~/.cargo/bin`: rustup itself, thirteen proxies that are relative
    // symlinks to it, and one crate installed with `cargo install`. The
    // cargo instance's own executable is one of the proxies, so
    // everything that resolves to `rustup` is cargo's. `hexyl` is not --
    // until step E fills `InstalledArtifact.path` for cargo binaries it
    // is listed here, honestly (spec §8.3; Task 10's delivery note).
    let home = Home::new("rule-1");
    let bin = home.dir(".cargo/bin");
    exe(&bin, "rustup", b"x");
    let proxies = [
        "cargo",
        "cargo-clippy",
        "cargo-fmt",
        "cargo-miri",
        "clippy-driver",
        "rls",
        "rust-analyzer",
        "rust-gdb",
        "rust-gdbgui",
        "rust-lldb",
        "rustc",
        "rustdoc",
        "rustfmt",
    ];
    for proxy in proxies {
        link(&bin, proxy, Path::new("rustup"));
    }
    exe(&bin, "hexyl", b"x");
    let cargo = ManagerInstance {
        exe_path: bin.join("cargo"),
        prefix: home.path().join(".cargo"),
        ..manager_instance("cargo", &format!("cargo:{}", home.path().join(".cargo").display()))
    };

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[cargo], &[], ScanBudget::default());

    assert_eq!(scan.attributed, 14, "{:?}", scan.entries);
    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    assert_eq!(scan.entries[0].path, tilde(".cargo/bin/hexyl"));
}

#[test]
fn test_rule_2_claims_a_shim_that_resolves_under_an_artifacts_path() {
    // uv fills `InstalledArtifact.path` with the tool's venv directory
    // (uv.rs:65); its shim in `~/.local/bin` resolves to a file *under*
    // that directory, never to it -- so the rule is "starts with", not
    // "equals" (§十三 #35).
    let home = Home::new("rule-2");
    let bin = home.dir(".local/bin");
    let venv = home.path().join(".local/share/uv/tools/ruff");
    let venv_bin = home.dir(".local/share/uv/tools/ruff/bin");
    let real = exe(&venv_bin, "ruff", b"x");
    link(&bin, "ruff", &real);
    let uv = ManagerInstance {
        exe_path: home.path().join("elsewhere/uv"),
        prefix: bin.clone(),
        ..manager_instance("uv", "uv")
    };
    let ruff = artifact("uv", "ruff", &venv);

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[uv], &[ruff], ScanBudget::default());

    assert!(scan.entries.is_empty(), "{:?}", scan.entries);
    assert_eq!(scan.attributed, 1);
}
```

Add to `mod tests` in `crates/banager-core/src/scan/mod.rs` (after `test_scan_wire_shapes_match_the_hand_written_ts_mirror`):

```rust
    fn env(home: &str, path_dirs: &[&str], cargo_home: Option<&str>) -> HostEnv {
        HostEnv {
            path_dirs: path_dirs.iter().map(PathBuf::from).collect(),
            home: PathBuf::from(home),
            euid: 501,
            cargo_home: cargo_home.map(PathBuf::from),
            ollama_host: None,
        }
    }

    #[test]
    fn test_candidate_dirs_are_the_seven_fixed_ones_plus_path_entries_under_home() {
        let dirs = candidate_dirs(&env(
            "/Users/someone",
            &[
                "/Users/someone/.opencode/bin",
                "/opt/homebrew/bin",
                "/usr/bin",
                "/Users/someone/.local/bin",
            ],
            None,
        ));
        let expected: Vec<PathBuf> = [
            "/Users/someone/.local/bin",
            "/Users/someone/bin",
            "/usr/local/bin",
            "/Users/someone/.cargo/bin",
            "/Users/someone/go/bin",
            "/Users/someone/.bun/bin",
            "/Users/someone/.deno/bin",
            "/Users/someone/.opencode/bin",
            // Raw: the duplicate is `scan_dirs`'s to drop, by canonical path.
            "/Users/someone/.local/bin",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        assert_eq!(dirs, expected);
    }

    #[test]
    fn test_candidate_dirs_add_cargo_home_bin_when_the_host_sets_it() {
        let dirs = candidate_dirs(&env("/Users/someone", &[], Some("/Volumes/Data/cargo")));
        assert!(dirs.contains(&PathBuf::from("/Volumes/Data/cargo/bin")), "{dirs:?}");
        assert!(dirs.contains(&PathBuf::from("/Users/someone/.cargo/bin")), "{dirs:?}");
    }

    #[test]
    fn test_display_path_abbreviates_home_and_only_home() {
        let home = Path::new("/Users/someone");
        assert_eq!(
            display_path(Path::new("/Users/someone/.local/bin/agy"), home),
            PathBuf::from("~/.local/bin/agy")
        );
        assert_eq!(display_path(home, home), PathBuf::from("~"));
        assert_eq!(
            display_path(Path::new("/usr/local/bin/helper"), home),
            PathBuf::from("/usr/local/bin/helper")
        );
        // A sibling that merely starts with the same characters is not under home.
        assert_eq!(
            display_path(Path::new("/Users/someone-else/bin/x"), home),
            PathBuf::from("/Users/someone-else/bin/x")
        );
    }

    #[test]
    fn test_app_bundle_takes_the_first_candidate_with_a_dot_app_component() {
        let none = app_bundle([Path::new("/Users/someone/.local/bin/agy")]);
        assert_eq!(none, None);
        let resolved = app_bundle([
            Path::new("/Applications/Helper.app/Contents/Helpers/helper-cli"),
            Path::new("/usr/local/bin/helper-cli"),
        ]);
        assert_eq!(resolved.as_deref(), Some("Helper"));
        // A broken link's own text, relative as the installer wrote it.
        let relative = app_bundle([Path::new("../../Removed.app/Contents/MacOS/x")]);
        assert_eq!(relative.as_deref(), Some("Removed"));
    }
```

and extend the test module's imports (`use super::*;` already brings the private functions in; add `HostEnv` and `Path`):

```rust
    use super::*;
    use crate::runner::HostEnv;
    use std::path::Path;
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --test unknown_scan_test` and `cargo test -p banager-core scan::`
Expected: FAIL to compile — `error[E0432]: unresolved import \`banager_core::scan::scan_dirs\`` in the integration test; `error[E0425]: cannot find function \`candidate_dirs\`` (and `display_path`, `app_bundle`) in the unit tests.

- [ ] **Step 3: Implement the walk and rules 0–2**

Extend the imports at the top of `crates/banager-core/src/scan/mod.rs`:

```rust
use crate::model::{InstalledArtifact, InstanceId, ManagerInstance};
use crate::runner::HostEnv;
use serde::{Deserialize, Serialize};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};
```

Insert after the `UnknownScan` struct, before `#[cfg(test)]`:

```rust
/// The directories one scan looks at: the design spec's §4.2 seven,
/// `$CARGO_HOME/bin` when the host sets `CARGO_HOME` -- resolved the way
/// `CargoAdapter` resolves it (cargo.rs:133-136) but *added alongside*
/// `~/.cargo/bin` where cargo substitutes it; with it set the proxies
/// live under the override and `~/.cargo/bin` is usually absent, and an
/// absent directory costs nothing -- and every `PATH` entry under the
/// home directory. Raw, in this order,
/// duplicates included: `scan_dirs` drops the ones that do not exist and
/// reads each distinct directory once, by canonical path, so a `PATH`
/// that names `~/.local/bin` twice, or through a link, costs one read.
///
/// Only `PATH` entries under `home` are taken. The rest --
/// `/opt/homebrew/bin`, `/usr/bin` -- are Homebrew's and macOS's, and
/// not what this page is for. Which `PATH` that is depends on how Banager
/// was launched (`fix_path_env` restores a login shell's for a Finder
/// launch; a terminal launch inherits that terminal's, temporary agent
/// directories and all); the research machine's `PATH` held 23 entries
/// that did not exist a session later, which is why missing directories
/// are silently skipped rather than reported.
fn candidate_dirs(env: &HostEnv) -> Vec<PathBuf> {
    let home = &env.home;
    let mut dirs = vec![
        home.join(".local/bin"),
        home.join("bin"),
        PathBuf::from("/usr/local/bin"),
        home.join(".cargo/bin"),
        home.join("go/bin"),
        home.join(".bun/bin"),
        home.join(".deno/bin"),
    ];
    if let Some(cargo_home) = &env.cargo_home {
        dirs.push(cargo_home.join("bin"));
    }
    dirs.extend(
        env.path_dirs
            .iter()
            .filter(|dir| dir.starts_with(home))
            .cloned(),
    );
    dirs
}

/// `path` with the home directory replaced by `~`, for the two wire
/// fields the page shows as they are (`ScannedDir::path`,
/// `UnknownEntry::path`). `resolved` is never passed through this: it is
/// the technical detail, and stays canonical and absolute. Attribution
/// compares absolute paths; only the output is abbreviated.
fn display_path(path: &Path, home: &Path) -> PathBuf {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => PathBuf::from("~"),
        Ok(rest) => Path::new("~").join(rest),
        Err(_) => path.to_path_buf(),
    }
}

/// The name of the `.app` bundle a path runs inside, if any component of
/// any candidate ends in `.app`: `/Applications/Helper.app/Contents/x`
/// gives `Helper`. Candidates are tried in order -- the real path first,
/// then a broken link's own text (the only path a broken link has), then
/// the entry's own path.
fn app_bundle<'a>(candidates: impl IntoIterator<Item = &'a Path>) -> Option<String> {
    for path in candidates {
        for component in path.components() {
            if let Component::Normal(part) = component {
                let part = part.to_string_lossy();
                if let Some(name) = part.strip_suffix(".app") {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

/// What the registered sources have said is theirs, indexed once per scan
/// so the rules are lookups rather than a `canonicalize` per entry per
/// instance. Built from a clone of the snapshot (`Session::scan_unknown`):
/// a refresh committing meanwhile does not move it.
///
/// The rules, in order; the first that matches wins (spec §8.3):
///
/// 0. The entry *is* an instance's `exe_path`, byte for byte, no
///    `canonicalize`. This is what catches a launcher that is a dangling
///    symlink (step B's `InstanceNote::LauncherOnly`, the state a
///    stopped uninstall leaves): `canonicalize` fails on it, so rules
///    1-3 cannot see it, and it would otherwise be listed as a broken
///    link here while also being a source on the Installed page.
/// 1. The entry resolves to the same file an instance's `exe_path`
///    resolves to. rustup's thirteen proxies in `~/.cargo/bin` are
///    relative links to `rustup`, and so is the cargo instance's own
///    `cargo`; grok's `agent` and `grok` links resolve to one download.
/// 2. The entry resolves to a path *under* an artifact's
///    `InstalledArtifact.path` (equal, when that path is a file). uv is
///    the first real input: its `path` is the tool's venv directory
///    (uv.rs:65) and the shim resolves to `<venv>/bin/<tool>`, so equality
///    would never match (§十三 #35). cargo fills `path` from step E, the
///    standalone adapters from step B; for those, rules 1 and 2 compare
///    the same file and rule 2 decides nothing new.
/// 3. The entry resolves to a path under a directory the instance's
///    adapter *owns* -- `owned_roots`, the longest matching root.
struct Known {
    exe_raw: Vec<(PathBuf, InstanceId)>,
    exe_canonical: Vec<(PathBuf, InstanceId)>,
    artifact_roots: Vec<(PathBuf, InstanceId)>,
}

impl Known {
    fn index(instances: &[ManagerInstance], artifacts: &[InstalledArtifact]) -> Known {
        let mut exe_raw = Vec::with_capacity(instances.len());
        let mut exe_canonical = Vec::with_capacity(instances.len());
        for inst in instances {
            exe_raw.push((inst.exe_path.clone(), inst.id.clone()));
            if let Ok(canonical) = std::fs::canonicalize(&inst.exe_path) {
                exe_canonical.push((canonical, inst.id.clone()));
            }
        }
        let artifact_roots = artifacts
            .iter()
            .filter_map(|artifact| {
                let path = artifact.path.as_ref()?;
                let canonical = std::fs::canonicalize(path).ok()?;
                Some((canonical, artifact.key.instance_id.clone()))
            })
            .collect();
        Known {
            exe_raw,
            exe_canonical,
            artifact_roots,
        }
    }

    /// The source that put `raw` (real path `resolved`; `None` for a broken
    /// link) there, by the first rule that matches -- or `None`: unknown.
    fn claimant(&self, raw: &Path, resolved: Option<&Path>) -> Option<&InstanceId> {
        if let Some((_, id)) = self.exe_raw.iter().find(|(exe, _)| exe == raw) {
            return Some(id);
        }
        let resolved = resolved?;
        if let Some((_, id)) = self.exe_canonical.iter().find(|(exe, _)| exe == resolved) {
            return Some(id);
        }
        if let Some((_, id)) = self
            .artifact_roots
            .iter()
            .find(|(root, _)| resolved.starts_with(root))
        {
            return Some(id);
        }
        None
    }
}

/// One directory entry as the page will describe it, or `None` for the
/// ones the scan does not list at all: a subdirectory (depth 1, never
/// recursed -- `~/Library/pnpm` on the research machine held `bin/` and
/// `store/`), a link to a directory, a file with no execute bit in any
/// position (checked on the target; a theoretical boundary, the
/// research machine had none), anything that is neither a file nor a
/// link, and an entry whose `lstat` failed -- one failed `stat` costs that
/// entry and nothing else. Every file-system read of the scan is here or
/// in `scan_dirs`'s `read_dir`.
fn examine(raw: &Path, home: &Path, euid: u32) -> Option<UnknownEntry> {
    let lstat = std::fs::symlink_metadata(raw).ok()?;
    let file_type = lstat.file_type();
    let (kind, resolved, link_target) = if file_type.is_symlink() {
        let link_target = std::fs::read_link(raw)
            .ok()
            .map(|target| target.to_string_lossy().into_owned());
        match std::fs::canonicalize(raw) {
            Ok(resolved) => (EntryKind::Symlink, Some(resolved), link_target),
            Err(_) => (EntryKind::BrokenSymlink, None, link_target),
        }
    } else if file_type.is_file() {
        (EntryKind::File, std::fs::canonicalize(raw).ok(), None)
    } else {
        return None;
    };
    // Size, date and the executable check are the target's: a link's own
    // say only when the installer made the link.
    let target = match kind {
        EntryKind::BrokenSymlink => None,
        EntryKind::File | EntryKind::Symlink => Some(std::fs::metadata(raw).ok()?),
    };
    if let Some(target) = &target {
        if target.is_dir() || (target.mode() & 0o111) == 0 {
            return None;
        }
    }
    let (size_bytes, modified_at) = match &target {
        Some(target) => (Some(target.len()), Some(target.mtime())),
        None => (None, None),
    };
    let mut bundle_candidates: Vec<&Path> = Vec::new();
    if let Some(resolved) = &resolved {
        bundle_candidates.push(resolved);
    }
    if let Some(target) = &link_target {
        bundle_candidates.push(Path::new(target));
    }
    bundle_candidates.push(raw);
    let app_bundle = app_bundle(bundle_candidates);
    Some(UnknownEntry {
        path: display_path(raw, home),
        kind,
        resolved,
        link_target,
        size_bytes,
        modified_at,
        owned_by_me: lstat.uid() == euid,
        app_bundle,
    })
}

/// The scan over an explicit directory list. `scan_unknown` is what
/// production calls; this is what the synthetic-tree tests call, so a
/// test never reads the `/usr/local/bin` of the machine running it.
///
/// Directories that do not exist are skipped without a trace; each
/// distinct directory (by canonical path) is read once; entries are taken
/// in name order so a stop at the budget is reproducible. The budget is
/// checked before every `read_dir` and before every entry
/// (`ScanBudget`); when it trips, what was examined so far is returned as
/// it is, with `stopped` saying which limit -- a directory whose first
/// entry tripped it is not reported as read.
pub fn scan_dirs(
    dirs: &[PathBuf],
    env: &HostEnv,
    instances: &[ManagerInstance],
    artifacts: &[InstalledArtifact],
    budget: ScanBudget,
) -> UnknownScan {
    let file_stop = ScanStop::FileLimit {
        max_entries: u32::try_from(budget.max_entries).unwrap_or(u32::MAX),
    };
    let time_stop = ScanStop::TimeLimit {
        max_secs: u32::try_from(budget.max_duration.as_secs()).unwrap_or(u32::MAX),
    };
    let known = Known::index(instances, artifacts);
    // The clock starts here, after indexing: `ScanBudget::max_duration`
    // bounds the walk, not the `canonicalize` per known path above.
    let started = Instant::now();
    let mut scanned = Vec::new();
    let mut entries = Vec::new();
    let mut attributed = 0u32;
    let mut stopped = None;
    let mut examined = 0usize;
    let mut seen: Vec<PathBuf> = Vec::new();
    'dirs: for dir in dirs {
        let Ok(canonical) = std::fs::canonicalize(dir) else {
            continue;
        };
        if seen.contains(&canonical) {
            continue;
        }
        seen.push(canonical);
        if started.elapsed() >= budget.max_duration {
            stopped = Some(time_stop.clone());
            break;
        }
        // Unreadable (permissions) is not "read": it is not reported either.
        let Ok(read) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut names: Vec<_> = read
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .collect();
        names.sort();
        let mut count = 0u32;
        for name in names {
            let over_budget = if examined >= budget.max_entries {
                Some(file_stop.clone())
            } else if started.elapsed() >= budget.max_duration {
                Some(time_stop.clone())
            } else {
                None
            };
            if let Some(stop) = over_budget {
                stopped = Some(stop);
                if count > 0 {
                    scanned.push(ScannedDir {
                        path: display_path(dir, &env.home),
                        entries: count,
                    });
                }
                break 'dirs;
            }
            examined += 1;
            count += 1;
            let raw = dir.join(name);
            let Some(entry) = examine(&raw, &env.home, env.euid) else {
                continue;
            };
            match known.claimant(&raw, entry.resolved.as_deref()) {
                Some(_) => attributed += 1,
                None => entries.push(entry),
            }
        }
        scanned.push(ScannedDir {
            path: display_path(dir, &env.home),
            entries: count,
        });
    }
    UnknownScan {
        scanned,
        entries,
        attributed,
        stopped,
    }
}

/// The unknown-source scan: `scan_dirs` over `candidate_dirs(env)`. Pure
/// over its arguments and the file system; synchronous, and blocking for
/// up to `budget.max_duration` -- the Tauri shell runs it on the blocking
/// pool (`ipc::scan_unknown`). `instances` and `artifacts` are the
/// snapshot's, cloned by `Session::scan_unknown`.
pub fn scan_unknown(
    env: &HostEnv,
    instances: &[ManagerInstance],
    artifacts: &[InstalledArtifact],
    budget: ScanBudget,
) -> UnknownScan {
    scan_dirs(&candidate_dirs(env), env, instances, artifacts, budget)
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --test unknown_scan_test` and `cargo test -p banager-core scan::`
Expected: all 13 integration tests and 6 unit tests `ok`. If `test_stops_at_the_file_limit_and_says_which_limit` reports `entries: 2001`, the budget check ran after `examined += 1` instead of before — the order in the loop above is the contract.

- [ ] **Step 5: Run the gates**

Run all five from Global Constraints. Expected: all clean. Two things clippy is known to want here and the code above already does: `filter_map(Result::ok)` rather than a closure, and `std::slice::from_ref(&bin)` rather than `&[bin.clone()]` in the test.

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/scan/mod.rs crates/banager-core/tests/unknown_scan_test.rs
git commit -m "Scan the usual bin directories for programs no source accounts for

One level deep over the design spec's seven directories, CARGO_HOME's
bin when it is set, and every PATH entry under the home folder; missing
directories are skipped silently (29 of the 36 on the research machine
were), each distinct directory is read once, and the walk stops at 2000
entries or 10 seconds and says which. An entry is claimed when it is an
instance's launcher by raw path (so a dangling launcher is not listed
twice), resolves to the same file a launcher does (rustup's proxies), or
resolves under a path an artifact reported (uv's tool venvs).

Co-Authored-By: <the executing session's attribution line>"
```

---

### Task 3: Rule 3 — the `owned_roots` table, longest root wins

**Files:**
- Modify: `crates/banager-core/src/scan/mod.rs` (`Known` gains `owned`; new `pub fn owned_roots`; unit tests)
- Modify: `crates/banager-core/tests/unknown_scan_test.rs` (four tests appended)

**Interfaces:**
- Consumes: `ManagerInstance.adapter_id` / `.prefix` (`model.rs:125`, `:127`); the prefix each adapter really sets: brew `prefix_for` = two levels above the executable (`adapters/brew/mod.rs:289-295`), ollama `env.home.join(".ollama")` (`adapters/ollama/mod.rs:289`), uv/pipx/pip = `exe_path.parent()` (`uv.rs:138-141`, `pipx.rs:213-216`, `pip.rs:125-128`), npm = the global prefix root `npm prefix -g` reports (`npm.rs:157-158`, `:240`) and `exe_path.parent()` only in its `NotResponding` arm (`:194-197`), cargo = `$CARGO_HOME` (`cargo.rs:133-136`).
- Produces (verbatim): `pub fn owned_roots(inst: &ManagerInstance) -> Vec<PathBuf>`. Reader: `Known::index`, in the same change.

**The standalone rows are not added here.** §8.3 lists `standalone-claude/agy/grok → <prefix>` (their tool roots) and `standalone-rustup → nothing`. No adapter produces an instance with those ids until step B registers `standalone::all`, so at F those rows could never match anything: a definition with no producer, which §十 forbids in so many words. Step B adds the three rows (and a unit test that `standalone-claude`'s prefix is an owned root) in the same change that first produces such an instance; the comment in the table below says so. Until then the native `~/.local/bin/claude` is listed on this page — Task 10's delivery note says that too.

- [ ] **Step 1: Write the failing tests**

Append to `crates/banager-core/tests/unknown_scan_test.rs`:

```rust
#[test]
fn test_rule_3_claims_a_link_into_homebrews_cellar_but_not_into_the_rest_of_its_prefix() {
    // An Intel Mac: `/usr/local/bin` is both Homebrew's bin and where
    // third-party installers drop things. A link into `Cellar` is
    // Homebrew's; a program under the prefix's own `bin` is not thereby
    // Homebrew's -- `brew info --installed` would never list it.
    let home = Home::new("rule-3-brew");
    let bin = home.dir(".local/bin");
    let prefix = home.dir("opt/homebrew");
    let keg = home.dir("opt/homebrew/Cellar/jq/1.8.1/bin");
    let jq = exe(&keg, "jq", b"x");
    link(&bin, "jq", &jq);
    let prefix_bin = home.dir("opt/homebrew/bin");
    let dropped = exe(&prefix_bin, "dropped-in", b"x");
    link(&bin, "dropped-in", &dropped);
    let brew = ManagerInstance {
        exe_path: prefix.join("bin/brew"),
        prefix: prefix.clone(),
        ..manager_instance("brew", &format!("brew:{}", prefix.display()))
    };

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[brew], &[], ScanBudget::default());

    assert_eq!(scan.attributed, 1, "{:?}", scan.entries);
    assert_eq!(scan.entries.len(), 1, "{:?}", scan.entries);
    assert_eq!(scan.entries[0].path, tilde(".local/bin/dropped-in"));
}

#[test]
fn test_rule_3_claims_what_resolves_into_a_root_an_instance_owns_outright() {
    // Ollama owns all of `~/.ollama`.
    let home = Home::new("rule-3-ollama");
    let bin = home.dir(".local/bin");
    let ollama_bin = home.dir(".ollama/bin");
    let real = exe(&ollama_bin, "model-tool", b"x");
    link(&bin, "model-tool", &real);
    let ollama = ManagerInstance {
        exe_path: home.path().join("elsewhere/ollama"),
        prefix: home.path().join(".ollama"),
        ..manager_instance("ollama", "ollama:http://127.0.0.1:11434")
    };

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[ollama], &[], ScanBudget::default());

    assert!(scan.entries.is_empty(), "{:?}", scan.entries);
    assert_eq!(scan.attributed, 1);
}

#[test]
fn test_rule_3_claims_an_npm_global_cli_under_a_home_prefix() {
    // `npm config set prefix ~/.npm-global`, the setup npm's own docs
    // recommend over `sudo`: every global package unpacks under
    // `~/.npm-global/lib/node_modules`, and `~/.npm-global/bin/<tool>` is
    // a relative link into it. That bin directory is on `PATH`, so it is
    // scanned; without npm's root every global CLI on such a Mac would be
    // a row here and a row under npm on the Installed page at once
    // (ruling 10). The npm executable lives elsewhere so rules 0 and 1
    // cannot be why the link is claimed.
    let home = Home::new("rule-3-npm");
    let prefix = home.path().join(".npm-global");
    let package_bin = home.dir(".npm-global/lib/node_modules/some-tool/bin");
    exe(&package_bin, "cli.js", b"#!/usr/bin/env node\n");
    let bin = home.dir(".npm-global/bin");
    link(&bin, "some-tool", Path::new("../lib/node_modules/some-tool/bin/cli.js"));
    let npm = ManagerInstance {
        exe_path: home.path().join("elsewhere/npm"),
        prefix: prefix.clone(),
        ..manager_instance("npm", &format!("npm:{}", prefix.display()))
    };

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[npm], &[], ScanBudget::default());

    assert!(scan.entries.is_empty(), "{:?}", scan.entries);
    assert_eq!(scan.attributed, 1);
}

#[test]
fn test_rule_3_never_treats_a_parent_derived_prefix_as_owned() {
    // The counter-example spec §8.3 is built around: a pip instance whose
    // prefix is `~/.local/bin` itself (pip.rs:125-128 takes
    // `exe_path.parent()`). Its executable lives elsewhere here so rule 0
    // cannot be why anything is or is not claimed; the prefix alone must
    // claim nothing, or this page would lose every program in the one
    // directory it exists to look at.
    let home = Home::new("rule-3-parent-prefix");
    let bin = home.dir(".local/bin");
    exe(&bin, "agy", b"x");
    exe(&bin, "standalone-tool", b"x");
    let pip = ManagerInstance {
        exe_path: home.path().join("elsewhere/python3"),
        prefix: bin.clone(),
        ..manager_instance("pip", "pip:elsewhere")
    };

    let scan = scan_dirs(&[bin], &home.env(vec![]), &[pip], &[], ScanBudget::default());

    assert_eq!(scan.attributed, 0);
    assert_eq!(scan.entries.len(), 2, "{:?}", scan.entries);
}
```

Append to `mod tests` in `crates/banager-core/src/scan/mod.rs`:

```rust
    #[test]
    fn test_owned_roots_table() {
        let brew = ManagerInstance {
            prefix: PathBuf::from("/opt/homebrew"),
            ..crate::testing::manager_instance("brew", "brew:/opt/homebrew")
        };
        assert_eq!(
            owned_roots(&brew),
            vec![
                PathBuf::from("/opt/homebrew/Cellar"),
                PathBuf::from("/opt/homebrew/Caskroom"),
                PathBuf::from("/opt/homebrew/opt"),
            ]
        );
        let ollama = ManagerInstance {
            prefix: PathBuf::from("/Users/someone/.ollama"),
            ..crate::testing::manager_instance("ollama", "ollama:http://127.0.0.1:11434")
        };
        assert_eq!(owned_roots(&ollama), vec![PathBuf::from("/Users/someone/.ollama")]);
        // npm: where global packages unpack and every bin link points
        // (npm.rs:50). Not `<prefix>/bin`, which on `/usr/local` is where
        // third-party installers drop things.
        let npm = ManagerInstance {
            prefix: PathBuf::from("/usr/local"),
            ..crate::testing::manager_instance("npm", "npm:/usr/local")
        };
        assert_eq!(owned_roots(&npm), vec![PathBuf::from("/usr/local/lib/node_modules")]);
        // A `parent()`-derived prefix, or `$CARGO_HOME`, is never a root.
        for (adapter, id, prefix) in [
            ("cargo", "cargo:/Users/someone/.cargo", "/Users/someone/.cargo"),
            ("uv", "uv", "/Users/someone/.local/bin"),
            ("pipx", "pipx", "/Users/someone/.local/bin"),
            ("pip", "pip:/usr/bin/python3", "/usr/bin"),
        ] {
            let inst = ManagerInstance {
                prefix: PathBuf::from(prefix),
                ..crate::testing::manager_instance(adapter, id)
            };
            assert_eq!(owned_roots(&inst), Vec::<PathBuf>::new(), "{adapter}");
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let raw = std::env::temp_dir().join(format!(
            "banager-scan-unit-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&raw).expect("create temp dir");
        std::fs::canonicalize(&raw).expect("canonical temp dir")
    }

    #[test]
    fn test_the_longest_owned_root_wins() {
        // Two Ollama instances (two hosts) whose roots nest: the one whose
        // root is the longer prefix of the entry is the closer owner.
        let tmp = temp_dir("longest-root");
        let outer = tmp.join("outer");
        let inner = outer.join("inner");
        let inner_bin = inner.join("bin");
        std::fs::create_dir_all(&inner_bin).expect("create dirs");
        let entry = inner_bin.join("x");
        std::fs::write(&entry, b"x").expect("write");
        let outer_inst = ManagerInstance {
            prefix: outer.clone(),
            ..crate::testing::manager_instance("ollama", "ollama:http://outer:11434")
        };
        let inner_inst = ManagerInstance {
            prefix: inner.clone(),
            ..crate::testing::manager_instance("ollama", "ollama:http://inner:11434")
        };
        let known = Known::index(&[outer_inst, inner_inst], &[]);
        assert_eq!(
            known.claimant(&entry, Some(&entry)).map(String::as_str),
            Some("ollama:http://inner:11434")
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
```

The unit-test module's imports need `ManagerInstance` (`use super::*;` already re-exports it through the module's own `use crate::model::{…}`; nothing to add).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --test unknown_scan_test rule_3` and `cargo test -p banager-core scan::`
Expected: of the four new integration tests, three FAIL because rule 3 does not exist yet — `test_rule_3_claims_a_link_into_homebrews_cellar_but_not_into_the_rest_of_its_prefix` on `assert_eq!(scan.attributed, 1)` (left `0`: `jq` is listed), `test_rule_3_claims_what_resolves_into_a_root_an_instance_owns_outright` and `test_rule_3_claims_an_npm_global_cli_under_a_home_prefix` on `assert!(scan.entries.is_empty())` (`model-tool` and `some-tool` are listed) — and `test_rule_3_never_treats_a_parent_derived_prefix_as_owned` already passes (there is nothing to claim with, which is the point of that test). The unit-test target FAILS to compile as a whole: `error[E0425]: cannot find function \`owned_roots\` in this scope` from `test_owned_roots_table`, so no unit test runs until Step 3 adds the function and the `owned` field together.

- [ ] **Step 3: Add the table and rule 3**

Insert into `crates/banager-core/src/scan/mod.rs`, directly above `Known`'s doc comment (the `/// What the registered sources have said is theirs` block from Task 2 — not between that block and `struct Known`, or the block would attach to `owned_roots` and `Known` would lose its documentation):

```rust
/// The directories a source *owns*: whatever resolves to a path under one
/// of them was put there by that source. Keyed by adapter id here,
/// pending the `Adapter::owned_roots()` trait method the phase 4 spec's
/// §十一 records as this table's eventual home.
///
/// Deliberately not `ManagerInstance.prefix`. For uv, pipx and pip the
/// prefix is `exe_path.parent()` (uv.rs:138-141, pipx.rs:213-216,
/// pip.rs:125-128): a pip instance detected through
/// `~/.local/bin/python3.12` has prefix `~/.local/bin`, and treating that
/// as owned would claim every unrelated program in the one directory
/// this scan exists to look at. Homebrew's prefix is all of
/// `/opt/homebrew` or `/usr/local`, which on an Intel Mac would swallow
/// whatever a third-party installer dropped into `/usr/local/bin` --
/// programs `brew info --installed` never lists. npm's prefix is the
/// global prefix root `npm prefix -g` reports (npm.rs:157-158; the
/// `exe_path.parent()` at npm.rs:194-197 is its `NotResponding` arm
/// only), and what npm owns under it is `lib/node_modules`, not `bin`.
///
/// The standalone adapters (phase 4 step B) add their tool roots --
/// `standalone-claude` → `~/.local/share/claude`, `standalone-agy` →
/// `~/.gemini/antigravity-cli`, `standalone-grok` → `~/.grok`, each the
/// instance's `prefix`; `standalone-rustup` nothing, since everything of
/// rustup's resolves to its launcher and rule 1 has it -- in the same
/// change that first produces an instance with one of those ids. A row
/// here with no adapter that can produce its instance would be a
/// definition without a producer (spec §十).
pub fn owned_roots(inst: &ManagerInstance) -> Vec<PathBuf> {
    match inst.adapter_id.as_str() {
        "brew" => vec![
            inst.prefix.join("Cellar"),
            inst.prefix.join("Caskroom"),
            inst.prefix.join("opt"),
        ],
        // `~/.ollama`: nothing in a bin directory resolves into it today;
        // listed so the table says what Ollama owns, not by omission.
        "ollama" => vec![inst.prefix.clone()],
        // npm unpacks every global package under `<prefix>/lib/node_modules`
        // (npm.rs:50, `real_prefix_is_writable`), and `<prefix>/bin/<tool>`
        // is a link into it -- with a home prefix such as `~/.npm-global`
        // that bin directory is on `PATH` and scanned. Not `<prefix>/bin`
        // itself: on `/usr/local` it is where third-party installers drop
        // things, exactly as for Homebrew above. A `NotResponding` npm has
        // `prefix = exe_path.parent()`; the root derived from that does
        // not exist and is simply absent from the index.
        "npm" => vec![inst.prefix.join("lib").join("node_modules")],
        // cargo: `$CARGO_HOME` holds `bin/`, the very directory being
        // scanned; rule 1 places the proxies and, from step E, rule 2
        // places `cargo install`ed binaries. uv and (from Task 3b) pipx:
        // rule 2, through the tool venv their artifacts carry. pip: a
        // `parent()`-derived prefix, never a root.
        _ => Vec::new(),
    }
}
```

Change `struct Known` and its `impl` to:

```rust
struct Known {
    exe_raw: Vec<(PathBuf, InstanceId)>,
    exe_canonical: Vec<(PathBuf, InstanceId)>,
    artifact_roots: Vec<(PathBuf, InstanceId)>,
    /// Rule 3: every `owned_roots` of every instance, canonical, with the
    /// instance that owns it. A root that does not exist (Homebrew with
    /// no casks has no `Caskroom`) is simply absent.
    owned: Vec<(PathBuf, InstanceId)>,
}

impl Known {
    fn index(instances: &[ManagerInstance], artifacts: &[InstalledArtifact]) -> Known {
        let mut exe_raw = Vec::with_capacity(instances.len());
        let mut exe_canonical = Vec::with_capacity(instances.len());
        for inst in instances {
            exe_raw.push((inst.exe_path.clone(), inst.id.clone()));
            if let Ok(canonical) = std::fs::canonicalize(&inst.exe_path) {
                exe_canonical.push((canonical, inst.id.clone()));
            }
        }
        let artifact_roots = artifacts
            .iter()
            .filter_map(|artifact| {
                let path = artifact.path.as_ref()?;
                let canonical = std::fs::canonicalize(path).ok()?;
                Some((canonical, artifact.key.instance_id.clone()))
            })
            .collect();
        let owned = instances
            .iter()
            .flat_map(|inst| {
                owned_roots(inst).into_iter().filter_map(move |root| {
                    let canonical = std::fs::canonicalize(root).ok()?;
                    Some((canonical, inst.id.clone()))
                })
            })
            .collect();
        Known {
            exe_raw,
            exe_canonical,
            artifact_roots,
            owned,
        }
    }

    /// The source that put `raw` (real path `resolved`; `None` for a broken
    /// link) there, by the first rule that matches -- or `None`: unknown.
    fn claimant(&self, raw: &Path, resolved: Option<&Path>) -> Option<&InstanceId> {
        if let Some((_, id)) = self.exe_raw.iter().find(|(exe, _)| exe == raw) {
            return Some(id);
        }
        let resolved = resolved?;
        if let Some((_, id)) = self.exe_canonical.iter().find(|(exe, _)| exe == resolved) {
            return Some(id);
        }
        if let Some((_, id)) = self
            .artifact_roots
            .iter()
            .find(|(root, _)| resolved.starts_with(root))
        {
            return Some(id);
        }
        // The longest matching root: the closest owner when roots nest.
        self.owned
            .iter()
            .filter(|(root, _)| resolved.starts_with(root))
            .max_by_key(|(root, _)| root.as_os_str().len())
            .map(|(_, id)| id)
    }
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --test unknown_scan_test` and `cargo test -p banager-core scan::`
Expected: all 17 integration tests and 8 unit tests `ok` — including `test_rule_0_claims_an_instances_launcher_and_nothing_else_in_its_directory` from Task 2, which is now the spec's counter-example proper (a pip instance with prefix `~/.local/bin` claims its interpreter by rule 0 and nothing else).

- [ ] **Step 5: Run the gates**

Run all five from Global Constraints. Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/scan/mod.rs crates/banager-core/tests/unknown_scan_test.rs
git commit -m "Claim programs that resolve into a directory a source owns

Homebrew owns Cellar, Caskroom and opt under its prefix; Ollama owns
~/.ollama; npm owns lib/node_modules under its global prefix, where
every global package unpacks and every bin link points. Not the
instance's prefix: for uv, pipx and pip that is exe_path.parent(), so a
pip detected through ~/.local/bin/python3.12 would have claimed
everything in ~/.local/bin, and Homebrew's is the whole of /usr/local,
which on an Intel Mac would claim what third-party installers drop into
/usr/local/bin. The standalone adapters' tool roots join this table in
the step that first produces their instances.

Co-Authored-By: <the executing session's attribution line>"
```

---

### Task 3b: pipx fills `InstalledArtifact.path` — rule 2's second producer

**Files:**
- Modify: `crates/banager-core/src/adapters/pipx.rs:17` (import), `:60-64` (`PipxMainPackage`), `:71-95` (`parse_list`), `:510-522` (the fixture test) and `mod tests` (one inline case appended after it)

**Interfaces:**
- Consumes: `main_package.app_paths[].__Path__` in `pipx list --json` (recorded: `adapters/fixtures/pipx/1.17.3/list.json:11-16`, the app at `<venv>/bin/<app>`); `InstalledArtifact.path` (`model.rs:201`).
- Produces: `path: Some(<PIPX_HOME>/venvs/<tool>)` on every pipx artifact that exposes at least one app; `None` when it exposes none. Reader: rule 2 (`Known::index`'s `artifact_roots`, Task 2), exactly as it reads uv's venv directory — and only there: no page shows `path` (ruling 11).

Why this is F's and not a step of its own: the field already exists and rule 2 is its reader; what is missing is the second producer, and without it every pipx tool is a row under pipx on the Installed page and a row on the Unknown page at once — the two pages contradicting each other is the one thing this page must never do. pipx's shims are symlinks (`~/.local/bin/<app>` → `<venv>/bin/<app>`), so "resolves under the venv directory" is the same test that claims uv's. Nothing under `adapters/fixtures/` changes; the fixture already carries `app_paths`.

- [ ] **Step 1: Write the failing test**

Extend `test_parse_list_from_the_recorded_fixture` in `crates/banager-core/src/adapters/pipx.rs` (`:511-522`) — append after `assert_eq!(artifacts[0].reason, InstallReason::Requested);`:

```rust
        // The tool's venv directory, two levels above its exposed app:
        // the path the unknown-source scan's rule 2 (scan/mod.rs) compares
        // a `~/.local/bin` shim's target against, the way it does uv's.
        // `ends_with` rather than the fixture's absolute path, so the test
        // does not repeat the recording machine's home directory.
        let venv = artifacts[0].path.as_deref().expect("pipx fills path from app_paths");
        assert!(venv.ends_with("pipx/venvs/cowsay"), "{venv:?}");
```

and append a new test directly after that function (before the next `#[test]`, `:524`):

```rust
    #[test]
    fn test_parse_list_leaves_path_none_for_a_venv_that_exposes_no_app() {
        // A venv pipx could expose nothing from (`include_apps` false, or
        // a package with no console script): `app_paths` is empty and
        // there is no directory to hand rule 2. Inline: the recorded
        // fixture has no such venv, and this is a literal, not a fixture.
        let json = r#"{
            "pipx_spec_version": "0.1",
            "venvs": {
                "lib-only": {
                    "metadata": {
                        "main_package": {
                            "app_paths": [],
                            "package": "lib-only",
                            "package_version": "0.1"
                        }
                    }
                }
            }
        }"#;
        let artifacts = parse_list(json, "pipx").expect("parse inline pipx list");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].path, None);
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core pipx::tests::test_parse_list`
Expected: `test_parse_list_from_the_recorded_fixture` FAILS — `panicked at … pipx fills path from app_paths` (the artifact's `path` is `None`, pipx.rs:90). `test_parse_list_leaves_path_none_for_a_venv_that_exposes_no_app` already passes: it pins the `None` arm so Step 3 cannot turn an empty `app_paths` into a panic or a `Some("/")`.

- [ ] **Step 3: Parse `app_paths` and fill `path`**

Modify the import at `crates/banager-core/src/adapters/pipx.rs:17`:

```rust
use std::path::{Path, PathBuf};
```

Replace `PipxMainPackage` (`:60-64`) with:

```rust
/// One entry of `main_package.app_paths`: pipx serialises a `Path` as
/// `{"__Path__": "…", "__type__": "Path"}`, and an exposed app lives at
/// `<venv>/bin/<app>`.
#[derive(Debug, Deserialize)]
struct PipxAppPath {
    #[serde(rename = "__Path__")]
    path: String,
}

#[derive(Debug, Deserialize)]
struct PipxMainPackage {
    package: String,
    package_version: String,
    /// Every executable pipx exposed for this package, absolute. Empty for
    /// a venv with no app; `default` for a `pipx list --json` too old to
    /// write the key at all, which then simply gives rule 2 nothing.
    #[serde(default)]
    app_paths: Vec<PipxAppPath>,
}
```

In `parse_list` (`:71-95`), replace the `.map(|(tool_name, venv)| InstalledArtifact { … })` closure with a block that computes `path` first, so the borrow of `venv` ends before its fields move:

```rust
        .map(|(tool_name, venv)| {
            // The venv directory, two levels above any exposed app
            // (`<venv>/bin/<app>`): what a `~/.local/bin` shim resolves
            // under, so the unknown-source scan's rule 2 (scan/mod.rs)
            // can claim the shim the way it claims a uv tool's -- uv.rs:65
            // fills the same thing from `uv tool list --show-paths`. No
            // app, no path.
            let path = venv
                .metadata
                .main_package
                .app_paths
                .first()
                .and_then(|app| Path::new(&app.path).parent()?.parent())
                .map(Path::to_path_buf);
            InstalledArtifact {
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
                path,
                auto_updates: false,
                // pipx pins, but `pipx uninstall` removes a pinned tool: pipx
                // 1.17.3's `commands/uninstall.py` never reads `pinned`.
                uninstall_blocked: None,
            }
        })
```

Everything else in the function — the `PipxListRoot` parse, the `sort_by`, the `Ok(out)` — stays as it is.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core pipx::`
Expected: both `test_parse_list_*` cases `ok`, and every other pipx test still `ok` — none of them asserts a whole `InstalledArtifact` literal (`path: None` appears in pipx.rs only in production code, `:90`), so filling the field breaks no existing expectation.

- [ ] **Step 5: Run the gates**

Run all five from Global Constraints. Expected: all clean. `Snapshot::same_content` compares artifacts structurally, so a pipx snapshot's content changes once — on the first refresh after this lands — and is stable after; no test in `session/` builds a pipx artifact.

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/adapters/pipx.rs
git commit -m "Report each pipx tool's venv directory as its installed path

pipx list --json names every app it exposed, at <venv>/bin/<app>; the
venv directory two levels up is the path a ~/.local/bin shim resolves
under, so the unknown-source scan's rule 2 claims the shim the way it
claims a uv tool's. Without it every pipx tool was a row under pipx on
the Installed page and a row on the Unknown page at once.

Co-Authored-By: <the executing session's attribution line>"
```

---

### Task 4: `Session::scan_unknown`

**Files:**
- Create: `crates/banager-core/src/session/scan.rs`
- Modify: `crates/banager-core/src/session/mod.rs:9-10` (module list)

**Interfaces:**
- Consumes: `Session.snapshot: Mutex<Snapshot>` (`session/mod.rs:215`; a child module reads the private field, as `refresh.rs` and `plans.rs` do), `scan::scan_unknown`, `ScanBudget::default()`, `HostEnv`.
- Produces (verbatim): `impl Session { pub fn scan_unknown(&self, env: &HostEnv) -> UnknownScan }`. Reader: Task 5's `scan_unknown_impl`.

- [ ] **Step 1: Write the failing test**

Create `crates/banager-core/src/session/scan.rs`:

```rust
//! `Session::scan_unknown`: the unknown-source scan over this session's
//! last committed snapshot. Its own file, like `refresh.rs` and
//! `plans.rs`, so the facade in `mod.rs` stays a facade.

use super::Session;
use crate::runner::HostEnv;
use crate::scan::{self, ScanBudget, UnknownScan};

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
```

Modify `crates/banager-core/src/session/mod.rs:9-10`:

```rust
mod plans;
mod refresh;
mod scan;
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core session::scan::`
Expected: FAIL to compile — `error[E0599]: no method named \`scan_unknown\` found for struct \`Arc<Session>\``.

- [ ] **Step 3: Implement**

Insert into `crates/banager-core/src/session/scan.rs`, between the `use` lines and `#[cfg(test)]`:

```rust
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
    /// bump `same_content` on every scan or be ignored by it).
    pub fn scan_unknown(&self, env: &HostEnv) -> UnknownScan {
        let (instances, artifacts) = {
            let snapshot = self.snapshot.lock().unwrap();
            (snapshot.instances.clone(), snapshot.artifacts.clone())
        };
        scan::scan_unknown(env, &instances, &artifacts, ScanBudget::default())
    }
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core session::scan::`
Expected: `test session::scan::tests::test_scan_unknown_judges_against_the_committed_snapshot_and_never_commits_one ... ok`. (The scan also reads the real `/usr/local/bin` of the machine — a read; the assertions use `any`/`find` and never assume the list is only the temp home's.)

- [ ] **Step 5: Run the gates**

Run all five from Global Constraints. Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/session/scan.rs crates/banager-core/src/session/mod.rs
git commit -m "Let a Session run the unknown-source scan over its snapshot

A clone of the committed instances and artifacts, taken under the
snapshot mutex and released before any directory is read; no resource
lock, since nothing here opens a package manager's own files; and no
commit, since the result is the page's, not state. The scan never asks
an adapter anything, so before the first refresh everything is unknown.

Co-Authored-By: <the executing session's attribution line>"
```

---

### Task 5: IPC `scan_unknown` on the blocking pool

**Files:**
- Modify: `src-tauri/src/ipc.rs:1-11` (imports), after `open_ollama_app` (`:551-561`; `#[cfg(test)]` is `:563`), and its `mod tests` (after `state_with_fake_adapter`, `:700-703`)
- Modify: `src-tauri/src/lib.rs:37-48`

**Interfaces:**
- Consumes: `Session::scan_unknown` (Task 4), `HostEnv::discover()` (`runner/path_env.rs:64-84`), `AppState.session: Arc<Session>` (`state.rs:8`), the `spawn_blocking` shape of `open_ollama_app` (`ipc.rs:551-561`).
- Produces (verbatim):
  ```rust
  pub(crate) fn scan_unknown_impl(session: &Session, env: &HostEnv) -> UnknownScan;
  #[tauri::command] pub async fn scan_unknown(state: State<'_, AppState>) -> Result<UnknownScan, String>;
  ```
  Reader: `src/lib/api.ts`'s `scanUnknown` (Task 6) through `generate_handler!`. `env` is a parameter, not read inside, so the test can keep the scan off the developer's own home (ruling 3); the command passes `HostEnv::discover()`.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `src-tauri/src/ipc.rs`, directly after `state_with_fake_adapter` (`:700-703`):

```rust
    #[tokio::test]
    async fn test_scan_unknown_impl_reads_the_session_and_never_refreshes_it() {
        let state = state_with_fake_adapter();
        refresh_impl(&state).await.expect("refresh");
        let generation = state.session.snapshot().generation;
        // No `PATH` entries and a home that does not exist: of the scan's
        // candidate directories only `/usr/local/bin` can be read on the
        // machine running this, and reading is all that happens to it.
        // The command itself passes `HostEnv::discover()`.
        let env = HostEnv {
            path_dirs: Vec::new(),
            home: std::env::temp_dir().join(format!("banager-ipc-scan-{}", std::process::id())),
            euid: 0,
            cargo_home: None,
            ollama_host: None,
        };

        let scan = scan_unknown_impl(&state.session, &env);

        // The fake instance's executable is `/bin/true`, which no scanned
        // directory holds, so attribution is not the subject here --
        // `session/scan.rs` proves that on a synthetic home. This proves
        // the shell-level contract: a scan is a read of the session, never
        // a refresh, and everything it lists or claims it also examined.
        assert_eq!(state.session.snapshot().generation, generation);
        let examined: u64 = scan.scanned.iter().map(|dir| u64::from(dir.entries)).sum();
        assert!(
            u64::from(scan.attributed) + scan.entries.len() as u64 <= examined,
            "{scan:?}"
        );
        assert!(
            scan.scanned.iter().all(|dir| !dir.path.starts_with("~")),
            "nothing under the non-existent home was read: {:?}",
            scan.scanned
        );
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager scan_unknown`
Expected: FAIL to compile — `error[E0425]: cannot find function \`scan_unknown_impl\` in this scope`.

- [ ] **Step 3: Implement the command and register it**

Modify the imports at `src-tauri/src/ipc.rs:6-7`:

```rust
use banager_core::runner::HostEnv;
use banager_core::scan::UnknownScan;
use banager_core::session::{IssuedPlan, Session, Snapshot};
```

Insert after `open_ollama_app` (after its closing brace at `:561`, before `#[cfg(test)]` at `:563`):

```rust
/// The unknown-source scan over the session's current snapshot
/// (`Session::scan_unknown`), for the `HostEnv` the caller read. The
/// command reads it fresh, as `refresh_impl` does, so the directory list
/// follows the `PATH` this process was launched with; the test passes one
/// that keeps the scan off the developer's own home.
pub(crate) fn scan_unknown_impl(session: &Session, env: &HostEnv) -> UnknownScan {
    session.scan_unknown(env)
}

#[tauri::command]
pub async fn scan_unknown(state: State<'_, AppState>) -> Result<UnknownScan, String> {
    // On the blocking pool, as `open_ollama_app` is: the scan is
    // synchronous file-system work bounded by `ScanBudget::default()` --
    // up to ten seconds by design -- and running it inline would hold one
    // of the async runtime's worker threads, the ones every other command
    // and the refresh run on, for that long. `State` cannot move into the
    // task; the `Arc<Session>` inside it can.
    let session = state.session.clone();
    let env = HostEnv::discover();
    tauri::async_runtime::spawn_blocking(move || scan_unknown_impl(&session, &env))
        .await
        // Only a panic inside the scan reaches this arm. The page shows
        // the text verbatim under `unknown.scanFailed`, the way a failed
        // load shows the backend's own words under
        // `emptyStates.loadFailed`; it is the runtime's sentence, not one
        // of Banager's to translate.
        .map_err(|e| e.to_string())
}
```

Modify `src-tauri/src/lib.rs:37-48`:

```rust
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
            ipc::open_ollama_app,
            ipc::scan_unknown,
        ])
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager scan_unknown`
Expected: `test ipc::tests::test_scan_unknown_impl_reads_the_session_and_never_refreshes_it ... ok`.

- [ ] **Step 5: Run the gates**

Run all five from Global Constraints. Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/ipc.rs src-tauri/src/lib.rs
git commit -m "Expose the unknown-source scan as an IPC command on the blocking pool

Synchronous directory reads bounded at ten seconds must not hold an
async worker thread, so the command moves the Arc<Session> onto
spawn_blocking the way open_ollama_app does. The command reads the
session and never refreshes it; the only error it can return is a
panic inside the scan, passed through as the runtime's own text.

Co-Authored-By: <the executing session's attribution line>"
```

---

### Task 6: TS mirror, `scanUnknown`, `queryKeys.unknown`, `useUnknownScan`

**Files:**
- Modify: `src/lib/types.ts:218-219` (between `Snapshot` and `Language`)
- Modify: `src/lib/types.test.ts:2-15` (imports), `:300-301` (append a case)
- Modify: `src/lib/api.ts:2` (import), after `:75`
- Modify: `src/lib/api.test.ts:3-14` (imports), `:124-125` (append a case)
- Modify: `src/lib/queryKeys.ts:1-5`
- Modify: `src/lib/queries.ts:8-17` (api imports), `:21` (type import), after `:50` (`useOperations`)
- Modify: `src/lib/queries.test.ts:6-14` (imports), `:234-235` (append a case)

**Interfaces:**
- Consumes: the Rust wire shapes pinned in Task 1's `test_scan_wire_shapes_match_the_hand_written_ts_mirror`; `call<T>` in `api.ts:13-22`; `queryKeys`.
- Produces (verbatim, from Core Interfaces): `EntryKind`, `ScanStop`, `ScannedDir`, `UnknownEntry`, `UnknownScan`; `scanUnknown(): Promise<UnknownScan>`; `queryKeys.unknown`; `useUnknownScan(): UseQueryResult<UnknownScan>`. Reader: `UnknownPage` (Task 8).

- [ ] **Step 1: Write the failing tests**

Modify `src/lib/types.test.ts:2-15` — add the three names to the type import:

```ts
import type {
  Snapshot,
  Outcome,
  OperationEvent,
  UiEvent,
  Plan,
  OpSummary,
  ReadOnlyReason,
  InstanceStatus,
  Settings,
  UninstallBlocked,
  UpdateBlocked,
  Warning,
  EntryKind,
  ScanStop,
  UnknownScan,
} from "./types";
```

and append inside the `describe("types", …)` block, before its closing `});` (`:301`):

```ts
  it("spells the unknown-source scan's shapes as Rust sends them", () => {
    // Mirrors `crates/banager-core/src/scan/mod.rs`, whose
    // `test_scan_wire_shapes_match_the_hand_written_ts_mirror` asserts
    // these exact spellings from the Rust side: `EntryKind` bare strings,
    // `ScanStop` externally tagged with the limit the scan enforced, and
    // an explicit `null` for a complete scan.
    const kinds: EntryKind[] = ["File", "Symlink", "BrokenSymlink"];
    expect(JSON.stringify(kinds)).toBe('["File","Symlink","BrokenSymlink"]');
    const fileLimit: ScanStop = { FileLimit: { max_entries: 2000 } };
    const timeLimit: ScanStop = { TimeLimit: { max_secs: 10 } };
    expect(JSON.stringify(fileLimit)).toBe('{"FileLimit":{"max_entries":2000}}');
    expect(JSON.stringify(timeLimit)).toBe('{"TimeLimit":{"max_secs":10}}');

    const scan: UnknownScan = {
      scanned: [{ path: "~/.local/bin", entries: 5 }],
      entries: [
        {
          path: "~/.local/bin/old-script",
          kind: "BrokenSymlink",
          resolved: null,
          link_target: "/Applications/Removed.app/Contents/Resources/index.js",
          size_bytes: null,
          modified_at: null,
          owned_by_me: true,
          app_bundle: "Removed",
        },
      ],
      attributed: 4,
      stopped: null,
    };
    expect(JSON.stringify(scan)).toBe(
      '{"scanned":[{"path":"~/.local/bin","entries":5}],"entries":[{"path":"~/.local/bin/old-script","kind":"BrokenSymlink","resolved":null,"link_target":"/Applications/Removed.app/Contents/Resources/index.js","size_bytes":null,"modified_at":null,"owned_by_me":true,"app_bundle":"Removed"}],"attributed":4,"stopped":null}',
    );
    expect(roundTrip(scan)).toEqual(scan);
    const stopped: UnknownScan = { ...scan, stopped: timeLimit };
    expect(roundTrip(stopped).stopped).toEqual({ TimeLimit: { max_secs: 10 } });
  });
```

Modify `src/lib/api.test.ts:3-14` — import `scanUnknown` and the type:

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
  subscribeEvents,
  scanUnknown,
} from "./api";
import type { IssuedPlan, OpRequest, Settings, UiEvent, UnknownScan } from "./types";
```

and append inside `describe("api", …)`, before its closing `});` (`:125`):

```ts
  it("scanUnknown invokes scan_unknown with no args and returns the scan", async () => {
    const scan: UnknownScan = { scanned: [], entries: [], attributed: 0, stopped: null };
    mockInvoke.mockResolvedValueOnce(scan as never);
    const result = await scanUnknown();
    expect(mockInvoke).toHaveBeenCalledWith("scan_unknown");
    expect(result).toEqual(scan);
  });
```

Modify `src/lib/queries.test.ts:6-14` — import the hook and the type:

```ts
import {
  useSnapshot,
  useRefresh,
  usePlanOperation,
  useSubmitOperation,
  useOpenOllamaApp,
  useUnknownScan,
} from "./queries";
import { refreshIntoCache } from "./events";
import type { IssuedPlan, ManagerInstance, Snapshot, UnknownScan } from "./types";
```

and append inside `describe("queries", …)`, before its closing `});` (`:235`):

```ts
  it("useUnknownScan runs nothing until asked, then fetches through scanUnknown", async () => {
    // `enabled: false`: the scan is a directory walk of up to ten seconds,
    // run when the Unknown page opens and when "Scan again" is pressed --
    // never because a component happened to mount, and never as part of
    // a refresh.
    const scan: UnknownScan = {
      scanned: [{ path: "~/.local/bin", entries: 1 }],
      entries: [],
      attributed: 1,
      stopped: null,
    };
    mockInvoke.mockResolvedValue(scan as never);
    const queryClient = newClient();
    const { result } = renderHook(() => useUnknownScan(), { wrapper: wrapper(queryClient) });

    expect(mockInvoke).not.toHaveBeenCalled();
    expect(result.current.data).toBeUndefined();

    await result.current.refetch();

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mockInvoke).toHaveBeenCalledWith("scan_unknown");
    expect(result.current.data).toEqual(scan);
    expect(queryClient.getQueryData(["unknown"])).toEqual(scan);
  });
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm typecheck`
Expected: FAIL — `src/lib/types.test.ts` `error TS2305: Module '"./types"' has no exported member 'EntryKind'` (and `ScanStop`, `UnknownScan`); `src/lib/api.test.ts` `has no exported member 'scanUnknown'`; `src/lib/queries.test.ts` `has no exported member 'useUnknownScan'`. (`pnpm test` fails the same three files at import time.)

- [ ] **Step 3: Implement**

Insert into `src/lib/types.ts` after the `Snapshot` interface (after `:218`, before `export type Language`):

```ts
/**
 * Rust `EntryKind` (crates/banager-core/src/scan/mod.rs): what one entry
 * of a scanned bin directory is. Bare-string unit variants. Read through
 * `KIND_KEYS` in src/pages/UnknownPage.tsx, a `Record` over this union, so
 * a variant added here without a badge fails `tsc`.
 */
export type EntryKind = "File" | "Symlink" | "BrokenSymlink";
/**
 * Rust `ScanStop`: why a scan stopped before it had looked at everything.
 * Both variants carry data -- the limit the scan really enforced, so the
 * banner prints that number and never a second copy typed into the
 * locale files -- hence externally tagged single-key objects, like
 * `Fault`'s data variants. The page branches on `"FileLimit" in stopped`
 * with a `never` default (`stoppedText` in src/pages/UnknownPage.tsx).
 */
export type ScanStop = { FileLimit: { max_entries: number } } | { TimeLimit: { max_secs: number } };
/**
 * One directory a scan read and how many entries it examined there.
 * `path` has the home folder abbreviated to `~` on the Rust side: data,
 * not a sentence, and the front end has no `HOME` to strip.
 */
export interface ScannedDir {
  path: string;
  entries: number;
}
/** One program no registered source accounts for. Rust `UnknownEntry`. */
export interface UnknownEntry {
  /** `~`-abbreviated like `ScannedDir.path`; the row's name is its last component. */
  path: string;
  kind: EntryKind;
  /** Canonical and absolute, every link hop followed; `null` for a broken link. The technical detail. */
  resolved: string | null;
  /** `readlink`'s text as the installer wrote it, links only. */
  link_target: string | null;
  /** The target's; `null` for a broken link, which has none. */
  size_bytes: number | null;
  /** Unix seconds, the target's; `null` for a broken link. */
  modified_at: number | null;
  /** `st_uid == euid` of the entry itself: who put it here. */
  owned_by_me: boolean;
  /** The `.app` any component of the path runs inside, without `.app`. */
  app_bundle: string | null;
}
/**
 * Rust `UnknownScan`: the result of one `scan_unknown`. Not part of the
 * `Snapshot` and not written by `refresh`; held only by `useUnknownScan`.
 */
export interface UnknownScan {
  scanned: ScannedDir[];
  entries: UnknownEntry[];
  /** Examined programs a known source accounted for, and so not listed. */
  attributed: number;
  stopped: ScanStop | null;
}
```

Modify `src/lib/api.ts:2`:

```ts
import type {
  IssuedPlan,
  OpRequest,
  PlanId,
  Settings,
  Snapshot,
  OpSummary,
  UiEvent,
  UnknownScan,
} from "./types";
```

and append after `openOllamaApp` (`:73-75`):

```ts
/**
 * The unknown-source scan over the current snapshot: a directory walk of
 * the usual bin folders, up to ten seconds, on the Rust side. Nothing is
 * cached here; `useUnknownScan` decides when it runs.
 */
export function scanUnknown(): Promise<UnknownScan> {
  return call<UnknownScan>("scan_unknown");
}
```

Modify `src/lib/queryKeys.ts`:

```ts
export const queryKeys = {
  snapshot: ["snapshot"] as const,
  operations: ["operations"] as const,
  settings: ["settings"] as const,
  unknown: ["unknown"] as const,
};
```

Modify `src/lib/queries.ts:8-17` and `:21`:

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
  scanUnknown,
} from "./api";
import { isNewerSnapshot, refreshIntoCache } from "./events";
import { queryKeys } from "./queryKeys";
import { isAvailable } from "./sources";
import type {
  IssuedPlan,
  OpRequest,
  OpSummary,
  PlanId,
  Settings,
  Snapshot,
  UnknownScan,
} from "./types";
```

and insert after `useOperations` (`:48-50`):

```ts
/**
 * The unknown-source scan. `enabled: false`: nothing runs until asked. The
 * Unknown page asks through `refetch` -- once per snapshot generation
 * while it is open, and on "Scan again" -- and it is the only reader. This
 * is not the snapshot: `refresh` never writes it, and `SnapshotChanged`
 * invalidates only the snapshot query (src/lib/events.ts), because it is
 * not about the managed sources. Each scan judges against whatever
 * snapshot is committed when it runs (spec §8.1, Q11), which is why the
 * page re-asks when that snapshot's `generation` moves: a scan made before
 * the startup refresh committed would otherwise stand until pressed.
 */
export function useUnknownScan(): UseQueryResult<UnknownScan> {
  return useQuery({ queryKey: queryKeys.unknown, queryFn: scanUnknown, enabled: false });
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm typecheck && pnpm test -- src/lib`
Expected: typecheck clean; `types.test.ts`, `api.test.ts`, `queries.test.ts` all green including the three new cases.

- [ ] **Step 5: Run the gates**

Run all five from Global Constraints. Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add src/lib/types.ts src/lib/types.test.ts src/lib/api.ts src/lib/api.test.ts src/lib/queryKeys.ts src/lib/queries.ts src/lib/queries.test.ts
git commit -m "Mirror the unknown-source scan's types and ask for it only on demand

The five wire types, pinned byte-for-byte against the Rust shape test;
scanUnknown() through the one invoke choke point; and a query that is
enabled: false, so the ten-second directory walk runs when the page asks
and never because something mounted or a refresh ran.

Co-Authored-By: <the executing session's attribution line>"
```

---

### Task 7: `formatBytes`

**Files:**
- Modify: `src/lib/format.ts` (append after `faultArgs`, `:105`)
- Modify: `src/lib/format.test.ts:2` (import), append a `describe` after the `displayToken` one (`:39`)

**Interfaces:**
- Produces (verbatim): `export function formatBytes(bytes: number): string`. Reader: `UnknownPage`'s size · date subtitle (Task 8).

- [ ] **Step 1: Write the failing test**

Modify `src/lib/format.test.ts:2`:

```ts
import { displayToken, formatBytes, outcomeArgs, outcomeKey } from "./format";
```

and insert after the `describe("displayToken", …)` block (after `:39`):

```ts
describe("formatBytes", () => {
  it("uses 1000-based units, the ones Finder shows", () => {
    // The number on the row should match Get Info in Finder, which
    // counts a kilobyte as 1000 bytes on macOS.
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(999)).toBe("999 B");
    expect(formatBytes(1000)).toBe("1 KB");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(12_000_000)).toBe("12 MB");
    expect(formatBytes(144_300_000)).toBe("144.3 MB");
    expect(formatBytes(4_400_000_000)).toBe("4.4 GB");
  });

  it("does not print a thousand of the smaller unit", () => {
    // 999,970 bytes is 999.97 KB, which one decimal rounds to 1000.0 KB;
    // that is 1 MB.
    expect(formatBytes(999_970)).toBe("1 MB");
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm typecheck`
Expected: FAIL — `src/lib/format.test.ts: error TS2305: Module '"./format"' has no exported member 'formatBytes'`.

- [ ] **Step 3: Implement**

Append to `src/lib/format.ts`:

```ts
/**
 * A byte count as the user reads it in Finder: 1000-based units, at most
 * one decimal, no trailing ".0". Units are symbols, not words, so they
 * are the same in both locales and this needs no `t()`.
 */
export function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (unit < units.length - 1 && value >= 1000) {
    value /= 1000;
    unit += 1;
  }
  // 999.97 KB rounds to "1000.0 KB" at one decimal; that is 1 MB.
  if (unit < units.length - 1 && Number(value.toFixed(1)) >= 1000) {
    value /= 1000;
    unit += 1;
  }
  const text = unit === 0 ? String(value) : value.toFixed(1).replace(/\.0$/, "");
  return `${text} ${units[unit]}`;
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm test -- src/lib/format`
Expected: both new cases green.

- [ ] **Step 5: Run the gates**

Run all five from Global Constraints. Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add src/lib/format.ts src/lib/format.test.ts
git commit -m "Format a byte count the way Finder shows it

1000-based units with one decimal, for the size on an Unknown row, so
the number matches Get Info on the same file.

Co-Authored-By: <the executing session's attribution line>"
```

---

### Task 8: `UnknownPage` and its copy

**Files:**
- Create: `src/pages/UnknownPage.tsx`, `src/pages/UnknownPage.test.tsx`
- Modify: `src/i18n/en.json:238-239`, `src/i18n/zh-CN.json:231-232` (insert an `unknown` block between `settingsSaveFailed` and `emptyStates`)

**Interfaces:**
- Consumes: `useUnknownScan`, `useSettings` (`queries.ts:44-46`), `formatBytes`, `ArtifactRow` (`components/ArtifactRow.tsx:53-107`: `name`, `description: ReactNode`, `badgeText`, `badgeVariant`, `wrapDescription`), `SourceNotice` (`components/SourceNotice.tsx:38`: `variant`, `title`), `EntryKind`, `ScanStop`, `UnknownEntry`.
- Produces: `export function UnknownPage(): JSX.Element` (reader: `App.tsx`, Task 9) and every `unknown.*` key, each looked up by a literal in this file (so `completeness.test.ts` passes before Task 9 routes the page). Not yet reachable from the sidebar — that is Task 9.

Copy is §9.2's, verbatim. zh-CN has only `_other` plural forms (Chinese has one), as `updates.count_other` does today.

- [ ] **Step 1: Write the failing test**

Create `src/pages/UnknownPage.test.tsx`:

```tsx
import { describe, expect, it, vi, beforeEach } from "vitest";
import { act, fireEvent, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { UnknownPage } from "./UnknownPage";
import i18n from "../i18n";
import { formatBytes } from "../lib/format";
import { queryKeys } from "../lib/queryKeys";
import type { Settings, Snapshot, UnknownScan } from "../lib/types";

const mockInvoke = vi.mocked(invoke);

// Every name here is invented; the research machine's real ones are
// deliberately not in the repository.
const baseScan: UnknownScan = {
  scanned: [
    { path: "~/.local/bin", entries: 5 },
    { path: "/usr/local/bin", entries: 1 },
  ],
  entries: [
    {
      path: "~/.opencode/bin/standalone-tool",
      kind: "File",
      resolved: "/Users/someone/.opencode/bin/standalone-tool",
      link_target: null,
      size_bytes: 144_300_000,
      modified_at: 1_758_000_000,
      owned_by_me: true,
      app_bundle: null,
    },
    {
      path: "~/.local/bin/old-script",
      kind: "BrokenSymlink",
      resolved: null,
      link_target: "/Applications/Removed.app/Contents/Resources/scripts/index.js",
      size_bytes: null,
      modified_at: null,
      owned_by_me: true,
      app_bundle: "Removed",
    },
    {
      path: "/usr/local/bin/helper-cli",
      kind: "Symlink",
      resolved: "/Applications/Helper.app/Contents/Helpers/helper-cli",
      link_target: "/Applications/Helper.app/Contents/Helpers/helper-cli",
      size_bytes: 2_100_000,
      modified_at: 1_700_000_000,
      owned_by_me: false,
      app_bundle: "Helper",
    },
  ],
  attributed: 4,
  stopped: null,
};

// A refreshed snapshot with nothing in it: the page scans once per
// snapshot `generation` (ruling 9), so `get_snapshot` has to answer for
// the first scan to run at all; what it holds is irrelevant here, the
// scan's judgement is Rust's.
const snapshot: Snapshot = {
  generation: 1,
  detect: "Found",
  instances: [],
  artifacts: [],
  updates: [],
  refreshed_at: 1_789_700_000,
  stale: false,
  errors: [],
};

let settings: Settings;
let scan: UnknownScan;
let scanFailure: string | null;

beforeEach(() => {
  settings = {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
    include_self_updating: false,
  };
  scan = baseScan;
  scanFailure = null;
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "scan_unknown") {
      return scanFailure === null ? Promise.resolve(scan) : Promise.reject(scanFailure);
    }
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "get_snapshot") return Promise.resolve(snapshot);
    return Promise.resolve(undefined);
  });
});

function scanCalls(): number {
  return mockInvoke.mock.calls.filter(([cmd]) => cmd === "scan_unknown").length;
}

function dateOf(seconds: number): string {
  return new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium" }).format(
    new Date(seconds * 1000),
  );
}

describe("UnknownPage", () => {
  it("scans once when it opens and lists each program under its kind badge", async () => {
    const { findByText, getByText } = renderWithProviders(<UnknownPage />);

    expect(await findByText("standalone-tool")).toBeInTheDocument();
    expect(getByText("old-script")).toBeInTheDocument();
    expect(getByText("helper-cli")).toBeInTheDocument();
    expect(getByText("Program")).toBeInTheDocument();
    expect(getByText("Broken link")).toBeInTheDocument();
    expect(getByText("Program (link)")).toBeInTheDocument();
    // The path is the row's first line, home abbreviated as Rust sent it.
    expect(getByText("~/.opencode/bin/standalone-tool")).toBeInTheDocument();
    expect(scanCalls()).toBe(1);
  });

  it("explains a broken link and names the app a program runs inside", async () => {
    const { findByText, getByText } = renderWithProviders(<UnknownPage />);

    expect(
      await findByText(
        "Points at /Applications/Removed.app/Contents/Resources/scripts/index.js, which no longer exists",
      ),
    ).toBeInTheDocument();
    expect(getByText("Part of Removed")).toBeInTheDocument();
    expect(getByText("Part of Helper")).toBeInTheDocument();
  });

  it("shows size and date, and says when an installer with administrator rights put it there", async () => {
    const { findByText, getByText, queryByText } = renderWithProviders(<UnknownPage />);

    expect(
      await findByText(`${formatBytes(144_300_000)} · ${dateOf(1_758_000_000)}`),
    ).toBeInTheDocument();
    expect(getByText(`${formatBytes(2_100_000)} · ${dateOf(1_700_000_000)}`)).toBeInTheDocument();
    // Exactly one row is not the user's own.
    expect(getByText("Put here by an installer with administrator rights")).toBeInTheDocument();
    // A broken link has no size and no date, and is not blank either: its
    // sentence is the broken-link one, tested above.
    expect(queryByText(/^ · /)).not.toBeInTheDocument();
  });

  it("shows where a link resolves only with technical details on", async () => {
    const hidden = renderWithProviders(<UnknownPage />);
    await hidden.findByText("helper-cli");
    expect(
      hidden.queryByText("Links to /Applications/Helper.app/Contents/Helpers/helper-cli"),
    ).not.toBeInTheDocument();
    hidden.unmount();

    settings = { ...settings, show_technical_details: true };
    const shown = renderWithProviders(<UnknownPage />);
    expect(
      await shown.findByText("Links to /Applications/Helper.app/Contents/Helpers/helper-cli"),
    ).toBeInTheDocument();
    // A plain file resolves to itself; there is nothing to add.
    expect(
      shown.queryByText("Links to /Users/someone/.opencode/bin/standalone-tool"),
    ).not.toBeInTheDocument();
  });

  it("says how many programs known sources accounted for, and where it looked", async () => {
    const { findByText, getByText } = renderWithProviders(<UnknownPage />);

    expect(
      await findByText(
        "4 more programs came from sources Banager knows and are listed under them.",
      ),
    ).toBeInTheDocument();
    expect(getByText("Looked in:")).toBeInTheDocument();
    expect(getByText("~/.local/bin (5 items)")).toBeInTheDocument();
    expect(getByText("/usr/local/bin (1 item)")).toBeInTheDocument();
  });

  it("warns with the scan's own numbers when it stopped early", async () => {
    scan = { ...baseScan, stopped: { FileLimit: { max_entries: 2000 } } };
    const byFiles = renderWithProviders(<UnknownPage />);
    expect(
      await byFiles.findByText(
        "Banager stopped after looking at 2000 items, so this list may be incomplete.",
      ),
    ).toBeInTheDocument();
    byFiles.unmount();

    scan = { ...baseScan, stopped: { TimeLimit: { max_secs: 10 } } };
    const byTime = renderWithProviders(<UnknownPage />);
    expect(
      await byTime.findByText("Banager stopped after 10 seconds, so this list may be incomplete."),
    ).toBeInTheDocument();
  });

  it("says so when nothing is unexplained, and still says where it looked", async () => {
    scan = { ...baseScan, entries: [], attributed: 7 };
    const { findByText, getByText } = renderWithProviders(<UnknownPage />);

    expect(
      await findByText(
        "Nothing unexplained: every command-line program Banager found came from a source it knows.",
      ),
    ).toBeInTheDocument();
    expect(getByText("~/.local/bin (5 items)")).toBeInTheDocument();
  });

  it("scans again when the button is pressed", async () => {
    const { findByText, getByRole } = renderWithProviders(<UnknownPage />);
    await findByText("standalone-tool");
    expect(scanCalls()).toBe(1);

    fireEvent.click(getByRole("button", { name: "Scan again" }));

    await waitFor(() => expect(scanCalls()).toBe(2));
  });

  it("scans again when the sources' snapshot moves underneath it", async () => {
    // Opened before the startup refresh commits, the page judged against
    // an empty snapshot and listed every managed launcher. The refresh
    // landing -- a higher `generation` in the snapshot cache, which is
    // how both `refreshIntoCache` and a `SnapshotChanged` invalidation
    // arrive -- re-runs the scan, so the list corrects itself instead of
    // waiting for a press (ruling 9).
    const { findByText, queryClient } = renderWithProviders(<UnknownPage />);
    await findByText("standalone-tool");
    expect(scanCalls()).toBe(1);

    act(() => {
      queryClient.setQueryData<Snapshot>(queryKeys.snapshot, { ...snapshot, generation: 2 });
    });

    await waitFor(() => expect(scanCalls()).toBe(2));
  });

  it("shows the backend's reason when the scan fails", async () => {
    scanFailure = "boom";
    const { findByRole } = renderWithProviders(<UnknownPage />);

    const alert = await findByRole("alert");
    expect(alert).toHaveTextContent("Couldn't scan: boom");
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm typecheck`
Expected: FAIL — `src/pages/UnknownPage.test.tsx: error TS2307: Cannot find module './UnknownPage'`.

- [ ] **Step 3: Add the copy**

Insert into `src/i18n/en.json` between the `settingsSaveFailed` block and `"emptyStates"` (after `:238`):

```json
  "unknown": {
    "title": "Programs Banager can't place",
    "intro": "These command-line programs are on your Mac, but none of the sources Banager knows installed them. Banager only lists them — it never runs or deletes anything here.",
    "lookedIn": "Looked in:",
    "dirCount_one": "{{path}} ({{count}} item)",
    "dirCount_other": "{{path}} ({{count}} items)",
    "attributed_one": "{{count}} more program came from a source Banager knows and is listed under it.",
    "attributed_other": "{{count}} more programs came from sources Banager knows and are listed under them.",
    "stopped": {
      "FileLimit": "Banager stopped after looking at {{count}} items, so this list may be incomplete.",
      "TimeLimit": "Banager stopped after {{seconds}} seconds, so this list may be incomplete."
    },
    "kind": {
      "File": "Program",
      "Symlink": "Program (link)",
      "BrokenSymlink": "Broken link"
    },
    "brokenLink": "Points at {{target}}, which no longer exists",
    "linksTo": "Links to {{path}}",
    "partOfApp": "Part of {{app}}",
    "adminOwned": "Put here by an installer with administrator rights",
    "sizeAndDate": "{{size}} · {{date}}",
    "scanAgain": "Scan again",
    "scanning": "Scanning…",
    "scanFailed": "Couldn't scan: {{message}}",
    "empty": "Nothing unexplained: every command-line program Banager found came from a source it knows."
  },
```

Insert into `src/i18n/zh-CN.json` at the same place (after `:231`):

```json
  "unknown": {
    "title": "Banager 说不清来源的程序",
    "intro": "这些命令行程序在你的 Mac 上，但 Banager 认识的来源都没有装过它们。Banager 只是列出来——这里什么都不会运行，也不会删除。",
    "lookedIn": "查看了：",
    "dirCount_other": "{{path}}（{{count}} 项）",
    "attributed_other": "另有 {{count}} 个程序来自 Banager 认识的来源，已列在对应来源下。",
    "stopped": {
      "FileLimit": "Banager 查看 {{count}} 项后停下了，这个列表可能不完整。",
      "TimeLimit": "Banager 查看 {{seconds}} 秒后停下了，这个列表可能不完整。"
    },
    "kind": {
      "File": "程序",
      "Symlink": "程序（链接）",
      "BrokenSymlink": "失效的链接"
    },
    "brokenLink": "指向 {{target}}，但那里已经没有了",
    "linksTo": "指向 {{path}}",
    "partOfApp": "{{app}} 的一部分",
    "adminOwned": "由一个用了管理员权限的安装器放在这里",
    "sizeAndDate": "{{size}} · {{date}}",
    "scanAgain": "重新扫描",
    "scanning": "正在扫描…",
    "scanFailed": "没能扫描：{{message}}",
    "empty": "没有说不清的：Banager 找到的命令行程序都来自它认识的来源。"
  },
```

- [ ] **Step 4: Write the page**

Create `src/pages/UnknownPage.tsx`:

```tsx
import { useEffect } from "react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { ArtifactRow } from "../components/ArtifactRow";
import { SourceNotice } from "../components/SourceNotice";
import { formatBytes } from "../lib/format";
import { useSettings, useSnapshot, useUnknownScan } from "../lib/queries";
import type { EntryKind, ScanStop, UnknownEntry } from "../lib/types";

/**
 * The badge for each kind of entry. A `Record` over `EntryKind`, so a
 * variant added to the mirror without a badge here fails `tsc` -- this
 * project's signature defect is a variant that is defined, mirrored and
 * never rendered.
 */
const KIND_KEYS: Record<EntryKind, string> = {
  File: "unknown.kind.File",
  Symlink: "unknown.kind.Symlink",
  BrokenSymlink: "unknown.kind.BrokenSymlink",
};

/** Whatever `useTranslation()`'s `t` needs to look a key up; same convention as `Translate` in src/lib/sources.ts. */
type Translate = (key: string, options?: Record<string, string | number>) => string;

/**
 * The banner for a scan that stopped early, carrying the number it
 * stopped at -- the one Rust enforced, never a copy in the locale files.
 * `in` branches with a `never` default, as `faultKey` in src/lib/format.ts.
 */
function stoppedText(t: Translate, stopped: ScanStop): string {
  if ("FileLimit" in stopped) {
    return t("unknown.stopped.FileLimit", { count: stopped.FileLimit.max_entries });
  }
  if ("TimeLimit" in stopped) {
    return t("unknown.stopped.TimeLimit", { seconds: stopped.TimeLimit.max_secs });
  }
  const unhandled: never = stopped;
  return unhandled;
}

function fileName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

/** An absolute date in the user's language. This repository deliberately has no relative-time formatter. */
function formatDate(seconds: number, language: string): string {
  return new Intl.DateTimeFormat(language, { dateStyle: "medium" }).format(
    new Date(seconds * 1000),
  );
}

/**
 * Everything the row says under its name, one line each: the path as
 * found, what a broken link pointed at, the app it runs inside, size and
 * date, who put it there, and -- with technical details on -- where a
 * link resolves. A plain file resolves to itself, so that last line is
 * for links only.
 */
function describeEntry(
  entry: UnknownEntry,
  t: Translate,
  language: string,
  technical: boolean,
): ReactNode {
  const lines: string[] = [entry.path];
  if (entry.kind === "BrokenSymlink") {
    lines.push(t("unknown.brokenLink", { target: entry.link_target ?? "" }));
  }
  if (entry.app_bundle !== null) {
    lines.push(t("unknown.partOfApp", { app: entry.app_bundle }));
  }
  const size = entry.size_bytes === null ? null : formatBytes(entry.size_bytes);
  const date = entry.modified_at === null ? null : formatDate(entry.modified_at, language);
  if (size !== null && date !== null) {
    lines.push(t("unknown.sizeAndDate", { size, date }));
  } else if (size !== null) {
    lines.push(size);
  } else if (date !== null) {
    lines.push(date);
  }
  if (!entry.owned_by_me) {
    lines.push(t("unknown.adminOwned"));
  }
  if (technical && entry.kind === "Symlink" && entry.resolved !== null) {
    lines.push(t("unknown.linksTo", { path: entry.resolved }));
  }
  // Keyed by position: the list is rebuilt from the entry on every render
  // and two lines can read the same (a size with no date is one bare
  // string), so the text itself is not a safe key.
  return lines.map((line, index) => (
    <span key={index} className="block">
      {line}
    </span>
  ));
}

export function UnknownPage() {
  const { t, i18n } = useTranslation();
  const { data: settings } = useSettings();
  const { data: snapshot } = useSnapshot();
  const scan = useUnknownScan();
  const { refetch } = scan;
  const generation = snapshot?.generation;

  // One scan per snapshot generation while the page is open. The query is
  // `enabled: false` (src/lib/queries.ts), so nothing runs until asked:
  // the first defined `generation` -- the snapshot query's answer, an
  // in-memory read -- asks once, and every later change to it asks again,
  // which is how a refresh landing after the page opened (the startup
  // refresh, most often) corrects a list judged against an empty
  // snapshot (ruling 9). The button asks regardless. `refetch` is stable
  // across renders. In development, StrictMode (src/main.tsx) runs this
  // effect twice on mount and the second `refetch` restarts the first
  // scan: a read that is thrown away, accepted over `cancelRefetch:
  // false`, which would make a generation change join a scan still
  // judging against the old snapshot.
  useEffect(() => {
    if (generation === undefined) return;
    void refetch();
  }, [refetch, generation]);

  const result = scan.data;

  return (
    <div className="flex h-full flex-col overflow-y-auto">
      <div className="flex items-start justify-between gap-4 p-4">
        <div className="min-w-0">
          <h1 className="text-lg font-semibold">{t("unknown.title")}</h1>
          <p className="mt-1 text-sm text-[var(--color-muted)]">{t("unknown.intro")}</p>
        </div>
        {/* The app's first standing refresh control, scoped to this page:
            it re-runs only this scan, never the sources' refresh. */}
        <button
          type="button"
          onClick={() => void refetch()}
          disabled={scan.isFetching}
          className="shrink-0 rounded-md bg-[var(--color-accent)] px-3 py-1 text-sm font-medium text-[var(--color-accent-foreground)] disabled:opacity-50"
        >
          {scan.isFetching ? t("unknown.scanning") : t("unknown.scanAgain")}
        </button>
      </div>
      {scan.isError ? (
        <p role="alert" className="px-4 pb-2 text-sm text-[var(--color-danger)]">
          {t("unknown.scanFailed", { message: scan.error.message })}
        </p>
      ) : null}
      {result ? (
        <>
          {result.stopped !== null ? (
            <div className="px-4">
              <SourceNotice variant="warning" title={stoppedText(t, result.stopped)} />
            </div>
          ) : null}
          {result.entries.length === 0 ? (
            <p className="p-12 text-center text-sm text-[var(--color-muted)]">
              {t("unknown.empty")}
            </p>
          ) : (
            result.entries.map((entry) => (
              <ArtifactRow
                key={entry.path}
                name={fileName(entry.path)}
                description={describeEntry(
                  entry,
                  t,
                  i18n.language,
                  settings?.show_technical_details ?? false,
                )}
                wrapDescription
                badgeText={t(KIND_KEYS[entry.kind])}
                badgeVariant="neutral"
              />
            ))
          )}
          <div className="p-4 text-xs text-[var(--color-muted)]">
            {result.attributed > 0 ? (
              <p>{t("unknown.attributed", { count: result.attributed })}</p>
            ) : null}
            <p className="mt-2">{t("unknown.lookedIn")}</p>
            <ul>
              {result.scanned.map((dir) => (
                <li key={dir.path}>
                  {t("unknown.dirCount", { path: dir.path, count: dir.entries })}
                </li>
              ))}
            </ul>
          </div>
        </>
      ) : null}
    </div>
  );
}
```

- [ ] **Step 5: Run to verify it passes**

Run: `pnpm test -- src/pages/UnknownPage src/i18n`
Expected: all ten `UnknownPage` cases green; `completeness.test.ts` green (every `unknown.*` key is looked up by a literal above: the `KIND_KEYS` record, the two `unknown.stopped.*` branches, and each `t("unknown.…")` call; plural keys are matched by their stem); `no-literal-strings.test.ts` green (no JSX text literal — every string is a `t()` or data).

- [ ] **Step 6: Run the gates**

Run all five from Global Constraints. Expected: all clean. If `tsc` reports `scan.error` as possibly `null`, the `scan.isError ?` guard is not narrowing — keep the discriminated `UseQueryResult` narrowing by reading `scan.error.message` only inside that ternary, as written.

- [ ] **Step 7: Commit**

```bash
git add src/pages/UnknownPage.tsx src/pages/UnknownPage.test.tsx src/i18n/en.json src/i18n/zh-CN.json
git commit -m "Add the Unknown page: programs no known source installed

Each row names the program, shows its path with the home folder
abbreviated, a kind badge, what a broken link pointed at, the app it
runs inside, size and date, and whether an installer with administrator
rights put it there; technical details add where a link resolves. The
footer says how many programs known sources accounted for and which
directories were read, so an empty list reads as looked-in-seven-places
rather than didn't-look. A stopped scan says which limit, with the
number it enforced. The page scans once per snapshot generation while
it is open, so a refresh landing after it opened corrects a list judged
against the empty startup snapshot; Scan again re-runs only this scan.
It is not reachable from the sidebar yet.

Co-Authored-By: <the executing session's attribution line>"
```

---

### Task 9: Navigation — `Page`, `Sidebar`, the `App` route, `INTERPOLATED_SUBTREES.nav`, `nav.unknown`

**Files:**
- Modify: `src/store/ui.ts:4`; `src/store/ui.test.ts:26-29`
- Modify: `src/components/Sidebar.tsx:9`; `src/components/Sidebar.test.tsx:5-31`
- Modify: `src/App.tsx:4-6` (imports), `:26-32` (route); `src/App.test.tsx:57-64` (`mockBackend`), append a case before `:95`
- Modify: `src/i18n/completeness.test.ts:188-189`
- Modify: `src/i18n/en.json:5-10`, `src/i18n/zh-CN.json:5-10`

**Interfaces:**
- Consumes: `UnknownPage` (Task 8), `Page`, `PAGES`.
- Produces: `export type Page = "installed" | "updates" | "unknown" | "settings"`; the sidebar entry `nav.unknown` (looked up through `t(\`nav.${p}\`)` at `Sidebar.tsx:31`, which is why the key must be registered in `INTERPOLATED_SUBTREES.nav` — §十三 #47); the route.

The page goes between Updates and Settings: it is about the machine, like the two before it, and Settings stays last. It is routed **outside** `SnapshotStatus` (ruling 5): `SnapshotStatus` would replace it with "Nothing for Banager to manage yet" on a Mac with no source at all, which is exactly where everything is unknown and this page is most useful; and the scan is not snapshot data, so the stale banner has nothing to say about it.

- [ ] **Step 1: Write the failing tests**

Modify `src/store/ui.test.ts:26-29`:

```ts
  it("setPage changes the active page", () => {
    useUiStore.getState().setPage("updates");
    expect(useUiStore.getState().page).toBe("updates");
    useUiStore.getState().setPage("unknown");
    expect(useUiStore.getState().page).toBe("unknown");
  });
```

Replace `src/components/Sidebar.test.tsx:5-31` (the whole `describe`):

```tsx
describe("Sidebar", () => {
  it("renders a button for each page, in order, and marks the active one", () => {
    const onSelectPage = vi.fn();
    const { getByRole, getAllByRole } = renderWithProviders(
      <Sidebar page="installed" onSelectPage={onSelectPage} />,
    );

    const installedButton = getByRole("button", { name: "Installed" });
    const updatesButton = getByRole("button", { name: "Updates" });
    const unknownButton = getByRole("button", { name: "Unknown" });
    const settingsButton = getByRole("button", { name: "Settings" });

    expect(installedButton).toHaveAttribute("aria-current", "page");
    expect(updatesButton).not.toHaveAttribute("aria-current");
    expect(unknownButton).not.toHaveAttribute("aria-current");
    expect(settingsButton).not.toHaveAttribute("aria-current");
    // Unknown sits with the two pages about the machine; Settings stays last.
    expect(getAllByRole("button").map((b) => b.textContent)).toEqual([
      "Installed",
      "Updates",
      "Unknown",
      "Settings",
    ]);
  });

  it("calls onSelectPage with the clicked page", () => {
    const onSelectPage = vi.fn();
    const { getByRole } = renderWithProviders(
      <Sidebar page="installed" onSelectPage={onSelectPage} />,
    );

    getByRole("button", { name: "Updates" }).click();
    expect(onSelectPage).toHaveBeenCalledWith("updates");

    getByRole("button", { name: "Unknown" }).click();
    expect(onSelectPage).toHaveBeenCalledWith("unknown");
  });
});
```

Modify `src/App.test.tsx:6` and `:57-64` so the backend answers the scan too:

```ts
import type { Settings, Snapshot, UnknownScan } from "./lib/types";
```

```ts
const emptyScan: UnknownScan = {
  scanned: [{ path: "~/.local/bin", entries: 0 }],
  entries: [],
  attributed: 0,
  stopped: null,
};

function mockBackend(snap: Snapshot) {
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot" || cmd === "refresh") return Promise.resolve(snap);
    if (cmd === "get_settings") return Promise.resolve(defaultSettings);
    if (cmd === "list_operations") return Promise.resolve([]);
    if (cmd === "scan_unknown") return Promise.resolve(emptyScan);
    return Promise.resolve(undefined);
  });
}
```

and append inside `describe("App", …)`, before its closing `});` (`:95`):

```tsx
  it("switches to the Unknown page, which lives outside the snapshot's empty states", async () => {
    // A Mac with no source at all: SnapshotStatus shows "Nothing for
    // Banager to manage yet" for the Installed and Updates pages. That is
    // exactly where everything on the machine is unknown, so this page
    // must not be behind that gate.
    mockBackend({ ...snapshot, detect: "Missing", instances: [], artifacts: [] });
    const { getByRole, findByText, findByRole } = renderWithProviders(<App />);
    await findByText("Nothing for Banager to manage yet");

    fireEvent.click(getByRole("button", { name: "Unknown" }));

    expect(await findByRole("heading", { name: "Programs Banager can't place" })).toBeInTheDocument();
    expect(
      await findByText(
        "Nothing unexplained: every command-line program Banager found came from a source it knows.",
      ),
    ).toBeInTheDocument();
  });
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm typecheck`
Expected: FAIL — `src/store/ui.test.ts: error TS2345: Argument of type '"unknown"' is not assignable to parameter of type 'Page'`. (`pnpm test -- src/components/Sidebar src/App` would fail on the missing "Unknown" button.)

- [ ] **Step 3: Implement**

Modify `src/store/ui.ts:4`:

```ts
export type Page = "installed" | "updates" | "unknown" | "settings";
```

Modify `src/components/Sidebar.tsx:9`:

```ts
const PAGES: Page[] = ["installed", "updates", "unknown", "settings"];
```

Modify `src/App.tsx:4-6` and `:26-32`:

```tsx
import { InstalledPage } from "./pages/InstalledPage";
import { UpdatesPage } from "./pages/UpdatesPage";
import { UnknownPage } from "./pages/UnknownPage";
import { SettingsPage } from "./pages/SettingsPage";
```

```tsx
        <main className="flex-1 overflow-y-auto">
          {/* Settings and Unknown are not snapshot pages: Settings never
              was, and the unknown-source scan is judged against the
              snapshot but is not part of it -- on a Mac with no source at
              all, SnapshotStatus would replace it with "Nothing for
              Banager to manage yet", the one case where every program on
              the machine belongs on it. */}
          {page === "settings" ? (
            <SettingsPage />
          ) : page === "unknown" ? (
            <UnknownPage />
          ) : (
            <SnapshotStatus>
              {page === "installed" ? <InstalledPage /> : <UpdatesPage />}
            </SnapshotStatus>
          )}
        </main>
```

Modify `src/i18n/en.json:5-10`:

```json
  "nav": {
    "label": "Sections",
    "installed": "Installed",
    "updates": "Updates",
    "unknown": "Unknown",
    "settings": "Settings"
  },
```

Modify `src/i18n/zh-CN.json:5-10`:

```json
  "nav": {
    "label": "导航",
    "installed": "已安装",
    "updates": "更新",
    "unknown": "来源不明",
    "settings": "设置"
  },
```

Modify `src/i18n/completeness.test.ts:188-189`:

```ts
  // src/components/Sidebar.tsx: `t(\`nav.${p}\`)` over `PAGES`.
  nav: ["installed", "updates", "unknown", "settings"],
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm test`
Expected: green, including `Sidebar.test.tsx`'s order assertion, `App.test.tsx`'s new case, and `completeness.test.ts` (`nav.unknown` is now an enumerated tail of an interpolated head).

- [ ] **Step 5: Run the gates**

Run all five from Global Constraints. Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add src/store/ui.ts src/store/ui.test.ts src/components/Sidebar.tsx src/components/Sidebar.test.tsx src/App.tsx src/App.test.tsx src/i18n/completeness.test.ts src/i18n/en.json src/i18n/zh-CN.json
git commit -m "Put the Unknown page in the sidebar, outside the snapshot's empty states

Between Updates and Settings, and routed like Settings rather than
through SnapshotStatus: on a Mac with no source at all that gate shows
\"Nothing for Banager to manage yet\", which is the one case where every
program on the machine belongs on this page.

Co-Authored-By: <the executing session's attribution line>"
```

---

### Task 10: Docs — `what-we-run.md`, README, and the delivery note

**Files:**
- Modify: `docs/what-we-run.md` (append after `:78`)
- Modify: `README.md:12-13` and `:163-164` (test counts), after `:28` (the page in "What it manages"), `:53-57` and `:195-198` (the on-demand-refresh limitation, en and zh)
- Modify: `docs/superpowers/backlog.md:143-147` (the 「整个应用没有刷新按钮」 entry gains one sentence)

**Interfaces:** none — prose. Every sentence below is backed by a line in this plan's code: the directory list is `candidate_dirs` (Task 2), the reads are `examine`/`scan_dirs` (Task 2), the limits are `ScanBudget::default()` (Task 1), the rules are `Known::claimant` (Tasks 2–3), "no lock, no commit" is `Session::scan_unknown` (Task 4), and "when the page opens and on Scan again" is `useUnknownScan` + the page's `useEffect` (Tasks 6, 8).

- [ ] **Step 1: Append the scan's section to `docs/what-we-run.md`**

If step A has already merged and rewritten the file per source, the section goes after A's last per-source section; otherwise after the current last paragraph (`:78`). Either way it is self-contained:

```markdown

## Unknown-source scan (phase 4, step F): read-only, no command runs

The *Unknown* page lists command-line programs that none of the sources
above installed. Producing that list runs no command at all.
`scan_unknown` (`crates/banager-core/src/scan/mod.rs`) reads directory
entries and file metadata and nothing else:

| It looks at | How |
|---|---|
| `~/.local/bin`, `~/bin`, `/usr/local/bin`, `~/.cargo/bin` (and `$CARGO_HOME/bin` when that variable is set), `~/go/bin`, `~/.bun/bin`, `~/.deno/bin`, plus every `PATH` entry under your home folder | `read_dir`, one level deep -- a subdirectory is never entered; a directory that does not exist is skipped silently; two names for one directory are read once |
| each entry | `lstat`, `readlink`, `realpath`, `stat`: what kind of file it is, where a link points, its size and date, who owns it. A file with no execute bit is not listed. Nothing's *contents* are read, and `file(1)` is not run |

It stops after 2,000 entries or 10 seconds and says so on the page, with
the number it stopped at. It never runs, opens, moves or deletes
anything it finds. It takes no lock and is not part of a refresh: it
runs when the page opens, again when the sources' state changes while
the page is open, and when you press *Scan again* -- always against the
sources' last known state -- and its result is not stored.

A program is *not* listed when a known source accounts for it: it is a
source's own executable, or resolves to the same file one does
(`~/.cargo/bin/cargo` and rustup's other proxies all resolve to
`rustup`); it resolves under a path a source reported installing (a file
or a directory: a uv or pipx tool's shim resolves into that tool's
environment); or it resolves under a directory a source owns (Homebrew's
`Cellar`, `Caskroom` and `opt`; npm's `lib/node_modules` under its
global prefix; Ollama's `~/.ollama`). Everything else is listed, with where a broken
link pointed, the app a program runs inside, and whether an installer
with administrator rights put it there.
```

- [ ] **Step 2: README — the page, the limitation, the counts**

Insert after `README.md:28` (the Ollama row of the "What it manages" table), keeping the blank line before `Adding a source …`:

```markdown

Programs that none of these sources installed -- a tool's own installer dropped a binary into
`~/.local/bin`, an app put a helper into `/usr/local/bin`, a link whose target is gone -- are
listed, read-only, on the **Unknown** page. Banager never runs, moves or deletes anything there;
`docs/what-we-run.md` says exactly what it reads. A program a source installed but reported no
path for is listed there too (today that is uv's own `uvx`): the gap is the source's, and the
page says what it sees.
```

Replace `README.md:53-57` (the "No on-demand refresh" bullet) with:

```markdown
- **No on-demand refresh.** Banager checks at launch, after each operation, when a "Try again"
  button is pressed for something that already needs one — a failed refresh, or a Homebrew index
  Banager couldn't update — and on its own when a Homebrew index update left running in the
  background finishes (`ipc::refresh_on_background_change`, `src-tauri/src/lib.rs:31-34`). None
  of that is a standalone control you can press at any time. The Unknown page's *Scan again*
  button is the one exception, and it is scoped to that page: it re-runs only that page's scan of
  your bin folders, against the sources' last known state — it does not refresh the sources.
```

Replace `README.md:195-198` (the Chinese counterpart) with:

```markdown
尚未支持：搜索与软件目录、安装新东西、macOS 以外的平台。按需刷新也还没有——刷新只在启动、操作完成、
点了“重试”按钮（刷新失败，或者 Homebrew 的索引过期了，才会出现这个按钮），以及后台运行的 Homebrew
索引更新自行结束时（`ipc::refresh_on_background_change`，`src-tauri/src/lib.rs:31-34`，不需要用户
动手）这四种情况下发生，不是随时可按的独立刷新控件。唯一的例外是“来源不明”页上的“重新扫描”，
它只属于那一页：只重新扫描那一页看的几个 bin 文件夹，按各来源上次已知的状态判断——并不刷新各来源。
（来源装了却没报路径的程序也会列在那一页，目前只有 uv 自带的 `uvx`：缺口在来源那边，页面照实说。）
```

Update the two test counts. Get the numbers from the suites themselves, never by hand:

```bash
cargo test --workspace 2>&1 | grep -E '^test result' | awk '{ passed += $4 } END { print passed }'
pnpm test 2>&1 | grep -E '^\s*Tests\s'
```

The first prints the Rust total (the `#[ignore]`d `brew_live` pair is not in `passed` and the README already says so); the second prints `Tests  N passed (N)`. Put the Rust number where `396` is on `README.md:12` and `:163`, and the front-end number where `262` is on `:13` and `:164`.

- [ ] **Step 2b: backlog — the refresh-button entry stays true**

`docs/superpowers/backlog.md:143` opens with **「整个应用没有刷新按钮。」**, which is false the moment *Scan again* ships. Append one sentence to that entry, after `没有代为决定。` (`:147`):

```markdown
  阶段 4 步骤 F 给「来源不明」页加了它专属的「重新扫描」——只重跑那一页的扫描，不刷新各来源——各来源仍无常驻刷新控件，这条待拍板的问题不变。
```

- [ ] **Step 3: Run the gates**

Run all five from Global Constraints. Expected: all clean (docs only; the two guard tests do not read `.md` files).

- [ ] **Step 4: Commit**

```bash
git add docs/what-we-run.md README.md docs/superpowers/backlog.md
git commit -m "Say what the unknown-source scan reads, and that it reads only

The trust file gains the scan's section: which directories, which
metadata calls, the two limits, no command, no lock, no refresh, not
stored. The README names the page, records that its Scan again is the
one standing refresh control and is scoped to that page, and carries
the new test counts; the backlog's no-refresh-button entry records the
same page-scoped exception so it stays true.

Co-Authored-By: <the executing session's attribution line>"
```

- [ ] **Step 5: Delivery note (goes in the branch's PR description / handover; not a file)**

> **Step F: the Unknown page.** Sidebar → *Unknown* lists command-line programs none of the registered sources installed; read-only; *Scan again* re-runs it.
>
> What appears on this page **until later steps merge** — these are honest, not bugs:
> - **Until step E** fills `InstalledArtifact.path` for `cargo install`ed binaries, `~/.cargo/bin/hexyl`-style programs are listed here. `rustup` itself and its thirteen proxies are already claimed (they resolve to the same file the cargo instance's `cargo` does — rule 1); E's `path` lets rule 2 claim the crates' binaries (spec §8.3).
> - **Until step B** registers the standalone adapters, the native launchers `~/.local/bin/claude` (a link into `~/.local/share/claude/versions/…`), `~/.local/bin/agy` and — when `~/.grok/bin` is on `PATH`, which is the only way it is scanned — `~/.grok/bin/{grok,agent}` are listed here. B produces their instances (rules 0/1 claim the launchers) and adds their tool roots to `owned_roots` (ruling 1: those rows are not defined in F because nothing could produce their instances yet). A dangling `~/.local/bin/claude` from a stopped uninstall is likewise listed as a broken link until B's `LauncherOnly` instance exists for rule 0 to match.
> - **Requirement on step B's plan** (spec §十 row B does not say this, and §8.3 assigns the rows to this table, so it is recorded here as the hand-off): B must add the three `owned_roots` rows `standalone-claude` → `<prefix>` (`~/.local/share/claude`), `standalone-agy` → `<prefix>` (`~/.gemini/antigravity-cli`), `standalone-grok` → `<prefix>` (`~/.grok`) in the same change that first produces an instance with each id, with a unit test in `scan/mod.rs`'s `test_owned_roots_table` that `standalone-claude`'s prefix is an owned root and `standalone-rustup`'s is not. The comment in `owned_roots` says the same; when the spec is next edited, one clause in §十 row B should too.
> - **Until step D** adds rule 4 and the `globs` parameter, an `agy.<timestamp>.old` backup in `~/.local/bin` (transient, roughly half an hour after agy self-updates) is listed here.
> - **Until the second batch's uv recipe** (§十一), `~/.local/bin/uvx` — uv's second binary, not a link to `uv`, so rule 1 cannot see it; `uv` itself is the uv instance's `exe_path` and rule 0 has it — is listed here. pipx's shims are *not* listed: Task 3b fills their venv directory as `InstalledArtifact.path` and rule 2 claims them (ruling 11). npm's global CLIs under a home prefix are *not* listed: `owned_roots` gives npm `<prefix>/lib/node_modules` (ruling 10). The README's Unknown paragraph names `uvx` as the one known such program.
>
> **First standing refresh control.** *Scan again* is the app's first manual refresh button. It is scoped to this page: it re-runs the scan only, against the sources' last committed snapshot, and never refreshes the sources. The README's "No on-demand refresh" limitation (its en and zh sections both) and the backlog's 「整个应用没有刷新按钮」 entry (amended in Task 10 Step 2b) both record the page-scoped exception and remain true of the sources. The page also re-scans on its own when the snapshot's generation moves while it is open (ruling 9) — that is the page reacting to a refresh, not triggering one.
>
> **Rulings taken in this step** (see "Rulings this plan makes"): `~` abbreviated on the Rust side for `path`/`ScannedDir.path`, `resolved` left absolute; `$CARGO_HOME/bin` scanned when set; the page routed outside `SnapshotStatus`; 1000-based sizes; a broken link has no size and no date; `scan_dirs` as the test seam; "Scan again" disabled while a scan runs; one scan per snapshot generation while the page is open; npm's `lib/node_modules` as an owned root; pipx filling `InstalledArtifact.path`.
>
> **No fixture directory** was added: `scan` is not an adapter, and `fixtures_layout_test` keeps the fixture set equal to the registered adapter ids. All directory shapes are synthetic, built by the tests in temp directories.

---

## Self-review against the spec

Every requirement of §八 (8.1–8.5), the scan parts of §9.1/§9.2/§9.4, §十 row F, and the task brief, with the task that carries it. Gaps found while checking are marked and were fixed in the tasks above.

| Requirement | Where | Task |
|---|---|---|
| Not an `Adapter`; not in `Snapshot`; not in `refresh`; no lock (§8.1, §8.4, D10) | `scan/mod.rs` module doc; `Session::scan_unknown` clones and does not commit; the session test asserts `generation` unchanged | 1, 4 |
| `scan_unknown(env, instances, artifacts, budget) -> UnknownScan`, sync, pure fs (§8.1) — F's signature without `globs` | `scan/mod.rs` | 2 |
| `Session::scan_unknown(&self, env) -> UnknownScan`, clones instances/artifacts (§8.1) | `session/scan.rs` | 4 |
| IPC `scan_unknown` via `spawn_blocking`, registered in `lib.rs:37-48` (§8.1) | `ipc.rs`, `lib.rs` | 5 |
| `api.ts scanUnknown()`, `queries.ts useUnknownScan()` `enabled: false`, `queryKeys.unknown` (§8.1) | Task 6 | 6 |
| `UnknownPage`, `Page` union, `Sidebar.tsx:9`, `App.tsx` route (§8.1) | Tasks 8–9 | 8, 9 |
| Wire types exactly as §8.2, `ScanStop` with payloads, no `elapsed_ms`/`scanned_at`; every field's reader named | `scan/mod.rs` doc comments name the page element per field; TS mirror doc comments | 1, 6 |
| `EntryKind` via `Record` (tsc-exhaustive); `ScanStop` via `in` + `never` (§8.2) | `KIND_KEYS`, `stoppedText` | 8 |
| Directories: the seven ∪ `PATH` under home; canonical dedupe; missing skipped and not in `scanned`; depth 1; directory entries skipped; no-`x` files skipped (§8.3) | `candidate_dirs`, `scan_dirs`, `examine`; tests `…skipped_and_not_reported`, `…read_once`, `…skips_subdirectories…` | 2 |
| `symlink_metadata`; link → `canonicalize` (multi-hop); failure → `BrokenSymlink` with `link_target`; never an error; `nlink` not consulted (§8.3) | `examine`; tests `…two_hop…`, `…broken_symlink…` | 2 |
| Rule 0: raw `exe_path` equality, catches a dangling launcher (§8.3, §十三 #43) | `Known::claimant`; test `…rule_0…even_when_dangling` | 2 |
| Rule 1: canonical `exe_path` equality; 13 rustup proxies (§8.3) | test `…rule_1…` (14 attributed, `hexyl` listed) | 2 |
| Rule 2: `starts_with(canonicalize(artifact.path))`; uv's shim (§8.3, §十三 #35); pipx as the second producer of `path` (ruling 11) | test `…rule_2…`; pipx `parse_list` fills `path` from `app_paths`, fixture-asserted | 2, 3b |
| Rule 3: `owned_roots` table, **not** `prefix`; brew `Cellar/Caskroom/opt`, ollama `<prefix>`, npm `<prefix>/lib/node_modules` (ruling 10 — the spec's "empty" for npm rests on the `NotResponding` arm's prefix), cargo/uv/pipx/pip empty; longest root; the pip counter-example (§8.3, §十三 #1/#22) | `owned_roots`; tests `…rule_3_claims_a_link_into_homebrews_cellar…`, `…owns_outright`, `…npm_global_cli_under_a_home_prefix`, `…never_treats_a_parent_derived_prefix…`, `test_owned_roots_table`, `test_the_longest_owned_root_wins` | 3 |
| Standalone rows in the table: decided (ruling 1), justified by §十, comment in the table, delivery note | Task 3 preamble; Task 10 note | 3, 10 |
| `.app` component → `app_bundle` (§8.3) | `app_bundle`; tests `…dot_app…`, unit test | 2 |
| Budget: 2000 entries counted incl. claimed and skipped; 10 s checked before each `read_dir` and entry; partial results returned (§8.4) | `scan_dirs`; tests `…file_limit…` (2000 no stop, 2001 stop), `…zero_time_budget…` | 1, 2 |
| `owned_by_me = st_uid == euid` (§9.4: root case untestable, one line) | `examine`; asserted `true` in the plain-exe test | 2 |
| Page: `nav.unknown`; title + intro; `scanned` with counts; `attributed` sentence; `stopped` banner; rows via `ArtifactRow` with no action; name, `~` path, kind badge, broken-link sentence, "Part of", size · date, admin-owned; technical details → `resolved`; "Scan again" (§8.5) | `UnknownPage.tsx`; ten tests | 8, 9 |
| The page judges against the committed snapshot and does not go stale on it: one scan per `generation` while open (§8.1, Q11; ruling 9) | `UnknownPage`'s effect on `useSnapshot().data?.generation`; test "scans again when the sources' snapshot moves underneath it" | 8 |
| The session and IPC tests read nothing of the developer's home (ruling 3): `scan_unknown_impl(session, env)` takes the env | Task 4's `HostEnv` over a temp home; Task 5's over a non-existent one, asserting no `~` directory was read | 4, 5 |
| `formatBytes` (§9.2) | Task 7 | 7 |
| `INTERPOLATED_SUBTREES["nav"]` gains `unknown` (§9.2, §十三 #47) | `completeness.test.ts:189` | 9 |
| All `unknown.*` and `nav.unknown` copy, en + zh-CN, verbatim from §9.2 | Tasks 8, 9 | 8, 9 |
| `types.test.ts`: `ScanStop` two arms, `UnknownScan` shape (§9.4) | Task 6 | 6 |
| `UnknownPage.test.tsx`: rows, badges, broken link, `.app`, two stop banners with numbers, empty state, scan again (§9.4) | Task 8 | 8 |
| `Sidebar.test.tsx` new page (§9.4) | Task 9 | 9 |
| `tests/unknown_scan_test.rs` synthetic tree with every case §9.4 lists for F | Tasks 2–3 | 2, 3 |
| what-we-run.md's unknown-scan section (§9.5, §十) | Task 10 | 10 |
| Delivery note: hexyl until E; first refresh control scoped to this page (task brief) | Task 10 Step 5 | 10 |

Gaps found in the review and fixed in the tasks:

1. §8.5 asks for "technical details → a `resolved` line" without saying for which kinds; a plain `File`'s `resolved` is itself, so the line is for `Symlink` only (Task 8, tested in "shows where a link resolves only with technical details on").
2. §8.2 leaves `size_bytes`/`modified_at` unspecified for a broken link; ruling 7 gives both `None` a real producer, and the page renders neither (Task 2, Task 8).
3. The spec's per-field readers name the page but not the "Looked in" footer's plural — `unknown.dirCount_one/_other` (en) and `_other` (zh) are the keys; the plural stem is what `completeness.test.ts` matches (Task 8).
4. Nothing in the spec pins the sidebar position; Task 9 puts Unknown before Settings and tests the order.
5. The IPC command's only error is a `JoinError` (a panic inside the scan); `unknown.scanFailed {{message}}` renders it verbatim, as `emptyStates.loadFailed` renders a backend string (Task 5, Task 8 test "shows the backend's reason").

Spec points this step could not follow literally, each recorded as a ruling above: `$CARGO_HOME/bin` added to the fixed list (ruling 4); the standalone `owned_roots` rows deferred to B (ruling 1); rule 1 claims rustup's proxies for the **cargo** instance before B/E exist — the outcome (not listed) is what §8.3 describes, the claimant differs until `standalone-rustup` exists (Task 2's rule-1 test says so in its comment); §8.4's "连点两次跑两次" yields to a button disabled while a scan runs (ruling 8); §8.2's "path under technical details" yields to §8.5's row layout — the `~`-abbreviated path is always the row's first line, and only `resolved` is behind the technical-details switch (Task 8); §8.1's "页面打开与按需扫描" gains one more trigger, a change of the snapshot's `generation` while the page is open (ruling 9); §8.3's "npm → 空" becomes `<prefix>/lib/node_modules`, because the spec's premise about npm's prefix (`npm.rs:194-197`) describes only its `NotResponding` arm (ruling 10); §8.3's "其余今天没有能归属的东西" is made true for pipx by Task 3b filling `InstalledArtifact.path` (ruling 11); and §十 row F's "四个线类型" is five once `EntryKind`, `ScanStop`, `ScannedDir`, `UnknownEntry` and `UnknownScan` are counted — the shape test pins all five.

## Review log

Adversarial review of this plan, 2026-09-24, against HEAD `26bc640`. Every point was re-verified against the code and the spec (`grep -n`/`sed -n` on the cited files, the recorded pipx fixture, the research file) before being acted on. **Accepted: 22. Rejected: 0.** Where a point offered two fixes, the column says which was taken.

| # | Verdict | Reason and what changed |
|---|---|---|
| 1 | **accepted** | `npm.rs:194-197` is the `NotResponding` arm; a responding npm's prefix is `npm prefix -g`'s root (`:157-158`, `:240`) and packages live under `<prefix>/lib/node_modules` (`:50`). The spec inherited the same false premise. Ruling 10; `owned_roots` gains `"npm" => vec![prefix.join("lib").join("node_modules")]`; `test_owned_roots_table` asserts it; new integration test `test_rule_3_claims_an_npm_global_cli_under_a_home_prefix`; Task 3's interfaces, doc comment, commit message and the self-review row corrected. |
| 2 | **accepted, option (a)** | `pipx.rs:90` leaves `path: None` and the recorded fixture already carries `main_package.app_paths[].__Path__` (`list.json:11-16`), so the venv directory is two parents up and rule 2 claims the shim as it does uv's. New Task 3b (import, `PipxAppPath`, `app_paths` on `PipxMainPackage`, `path` in `parse_list`, fixture assertion by `ends_with`, an inline no-app case). `uvx` is the remaining unclaimable program: named in the delivery note and in the README's Unknown paragraph (en and zh). Ruling 11. |
| 3 | **accepted** | Counted the `#[test]` fns: Task 2 has 13, Task 3 appended 3 (now 4, with npm). Step 4s now say 13 and 17; Task 3 Step 2 says which of the four fail, on which assertion, and that the parent-prefix case already passes. |
| 4 | **accepted** | Verified with `grep -n`/`sed -n`: `open_ollama_app` closes at `ipc.rs:561`, `#[cfg(test)]` at `:563`; `state_with_fake_adapter` at `:700-703`; README counts at `:12-13`; the no-on-demand-refresh bullet at `:53-57`; `Sidebar.test.tsx`'s `describe` at `5-31`. All five anchors corrected (the ipc one before an executor could insert inside the function body). |
| 5 | **accepted** | Task 2 places `/// What the registered sources have said is theirs …` directly above `struct Known`; "insert directly above `struct Known`" would have split them. Task 3 Step 3 now says "directly above `Known`'s doc comment" and why. |
| 6 | **accepted, as "note it"** | `src/main.tsx:11` wraps the app in `StrictMode`; the mount effect does run twice in development. Chosen over `cancelRefetch: false` because point 7's fix keys the effect on `generation`, and a joined refetch would let a generation change settle on a scan still judging against the old snapshot. The page's comment and ruling 9 record the dev-only restart as accepted. |
| 7 | **accepted** | `events.ts:177` invalidates only the snapshot query; a page opened before the startup refresh commits listed every managed launcher until pressed. `UnknownPage` now reads `useSnapshot().data?.generation` and scans once per defined generation (mount included); new test "scans again when the sources' snapshot moves underneath it" (`queryClient.setQueryData` with a higher generation → a second `scan_unknown`); `get_snapshot` mocked in the page tests; `useUnknownScan`'s doc, what-we-run.md and the delivery note updated. Q11 intact: Rust's refresh never runs the scan. Ruling 9. |
| 8 | **accepted** | Trailer hard-coded to one model in all ten commit blocks. Replaced with the placeholder `Co-Authored-By: <the executing session's attribution line>` and a Global Constraints sentence that says to substitute, never paste. |
| 9 | **accepted** | The constraint as worded contradicted `claude`, `agy`, `ruff`, `hexyl`, `python3.12` and the proxies in the tests. Reworded: no author-machine paths, users or private app names; spec-named public tool names are fine. |
| 10 | **accepted** | Struct literals of an undeclared type are E0422; path uses are E0433. Task 1 Step 2 names both, with which literals produce which. |
| 11 | **accepted, in the hermetic form** | `scan_unknown_impl` now takes `env: &HostEnv` (the command passes `HostEnv::discover()`); the IPC test passes no `PATH` entries and a non-existent home and asserts no `~` directory was read, so only `/usr/local/bin` is touched; ruling 3 reworded to say exactly which tests read it. Core Interfaces, Task 5 and the File Structure updated. |
| 12 | **accepted, by moving the clock** | `started` was taken before `Known::index`. It now starts after indexing, with a comment, and `max_duration`'s doc says so — the budget bounds the walk, not the `canonicalize` per known path. The zero-budget test is unaffected (`elapsed() >= 0` is always true). |
| 13 | **accepted, by fixing the doc** | `examine` pushes `link_target` for every symlink, which is the more useful behaviour (an `.app` directory that is itself a link would otherwise hide the bundle). `app_bundle`'s field doc now reads "then on a link's own text (the only path a broken link has)", matching `app_bundle`'s own doc. |
| 14 | **accepted** | Same counts as point 3, plus: the ollama case fails on `entries.is_empty()` (line before `attributed`), and the "`Known::index` compiles / `claimant` returns `None`" paragraph described a state that never exists, since the unit-test target fails to compile as a whole on `owned_roots`. Task 3 Step 2 rewritten. |
| 15 | **accepted** | Line 233 was 102 columns (rewrapped); the three-line `session.refresh(…).await` chain fits in 63 (joined). An `awk` over every ```` ```rust ```` fence found no other over-width line outside the illustrative Core Interfaces block. Global Constraints now says to run `cargo fmt --all` before the `--check` gate. |
| 16 | **accepted** | Spec line 677 says pressing twice runs two scans; the plan's `disabled={scan.isFetching}` was an unrecorded deviation. Recorded as ruling 8 (kept, with the reason) and in the self-review's deviations paragraph. |
| 17 | **accepted** | §8.2 (`:634`) puts the full path under technical details; §8.5 (`:681`) puts the `~` path on the row. The plan chose §8.5; the deviations paragraph now says so. |
| 18 | **accepted** | "both READMEs" → "the README's en and zh sections"; `backlog.md:143` 「整个应用没有刷新按钮」 would be false once *Scan again* ships — Task 10 gains Step 2b appending one sentence to that entry (and `git add`s it); the grok line is qualified with "when `~/.grok/bin` is on `PATH`", since `candidate_dirs` adds only `PATH` entries under home. |
| 19 | **accepted** | `cargo.rs:133-136` *substitutes* `$CARGO_HOME` for `~/.cargo`; `candidate_dirs` *adds* it — the doc now says "resolved the way `CargoAdapter` resolves it, but added alongside". `InstalledArtifact.path` is a directory for uv (`uv.rs:65`) and now pipx — what-we-run.md says "under a path a source reported installing (a file or a directory)". |
| 20 | **accepted** | `state_with_fake_adapter` `:700-703` (Task 5); `ArtifactKey.instance_id` `model.rs:186` (Task 2); the root-owner note is spec §9.4 (`:926`), not §8.4. All three corrected; `InstalledArtifact`'s range in "What already exists" corrected to `:191-211` while there. |
| 21 | **accepted** | The TS mirror and the Rust shape test pin five types. "the four types" → "the five wire types" in the File Structure, the task table, Task 1's and Task 6's commit messages; the deviations paragraph notes §十 row F's "四个" is inherited. |
| 22 | **accepted** | §十 row B (`:952`) does not mention `owned_roots`; a B plan written from the spec alone would never add the three rows. The delivery note now states it as an explicit requirement on B's plan (three rows + a unit test in `test_owned_roots_table`), and asks for one clause in §十 row B when the spec is next edited. |

Remaining risk after this review: Task 3b and the point-7 effect are the two pieces of code added without running them here (this tree was being built by another process). Both are small — a `#[serde(rename)]` struct and a two-parent `Path` walk with a fixture assertion; a `useEffect` keyed on `generation` with one page test — and each carries its own failing-then-passing test, so an executor finds out in Step 2/Step 4 of that task, not later.
