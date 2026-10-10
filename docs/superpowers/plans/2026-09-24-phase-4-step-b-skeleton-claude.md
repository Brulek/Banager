# Phase 4 Step B: Standalone Skeleton and Claude Code Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Banager its first source that is not a package manager: a *Claude Code* group on the Installed page for the native install (`~/.local/bin/claude` → `~/.local/share/claude/versions/<v>`), an honest update badge read from the launcher's live version against Anthropic's channel pointer, a working `claude update` button, five notices that say which copy of `claude` runs when the user types its name (or that only a dangling launcher is left), and a row that truthfully says it cannot be uninstalled here yet — with the `StandaloneAdapter` skeleton every later standalone tool (agy, grok, rustup) is a data row for.

**Architecture:** One Rust type, `StandaloneAdapter`, driven by a `&'static Recipe`, registered once per tool under the adapter id `standalone-<tool>` (spec D1); the instance *is* the native install (`exe_path` = the launcher symlink, `prefix` = the tool root, one `ArtifactKind::Binary` artifact, spec D2). Detection checks the installer's fixed path, never `PATH` (D3); the newest version comes from a VERIFIED endpoint and a candidate exists only when remote > local by dotted-integer comparison (D4); a self-updating tool is badged as usual and its row says it usually updates itself (D5); upgrade is the tool's own `claude update` through the unchanged `run_plan` (D6); "which copy runs" is four payload-free `InstanceNote`s plus `LauncherOnly` (D7). No uninstall in this step: the artifact carries `UninstallBlocked::NoSafeMethod`, which the gate in `Session::issue_plan` refuses and both pages hide (spec §6.1, §十 row B).

**Tech Stack:** Rust (banager-core: `std::fs` symlink/canonicalize, `serde_json` for one settings key, the existing `CommandRunner`/`HttpClient` seams, no new crate), TypeScript 5 `strict`, React 19, i18next, vitest.

**Spec:** `docs/superpowers/2026-09-24-phase-4-standalone-spec.md` (authoritative; Chinese). This plan implements §十 row B and argues from §一 D1–D7, §二, §三 (3.1–3.6), §四 (4.1, 4.3, 4.4), §五, §6.1, §七, §9.1–9.5 and 附录 A. Raw research it cites: `~/dev/Banager/.superpowers/phase4/claude.md` (tool facts, VERIFIED/UNVERIFIED per line) and `architecture.md` (code map). This plan will live at `docs/superpowers/plans/2026-09-24-phase-4-step-b-skeleton-claude.md`.

## Baseline, and how to read the anchors below

Branch `feat/phase-4-standalone`. Originally written at `f574d9f`, 2026-09-24. Astra re-verified the findings on 2026-09-25 against `/Users/brulek/dev/Banager-phase4` at HEAD `d3890f4` plus uncommitted edits that were later abandoned (a pip-only `ManagerInstance.user_scripts_dir`; the controller removed every reference to it from this plan — the field does not exist). The worktree and spec were read only; this revision changes only this plan. Symbol anchors take precedence over historical line numbers. **Steps A and F land before this step executes**, so this plan treats their output as existing:

- **A** (`docs/superpowers/plans/2026-09-24-phase-4-step-a-trust-and-guards.md`) adds `ALLOWED_HTTPS_HOSTS` and `host_allowed` to `crates/banager-core/src/http/real.rs`, makes `warningKey`/`warningArgs` in `src/lib/warnings.ts` exhaustive, and rewrites `docs/what-we-run.md` as one section per source with `crates/banager-core/tests/what_we_run_test.rs` holding it to the code (a `## <meta.name>` section per registered adapter; every host in `ALLOWED_HTTPS_HOSTS` named).
- **F** (`docs/superpowers/plans/2026-09-24-phase-4-step-f-unknown-scan.md`) adds `crates/banager-core/src/scan/mod.rs` with `pub fn owned_roots(inst: &ManagerInstance) -> Vec<PathBuf>` whose standalone rows were explicitly deferred to this step, the Unknown page, `nav.unknown`, and pipx's `InstalledArtifact.path`.

In files A or F modify (`http/real.rs`, `scan/mod.rs`, `docs/what-we-run.md`, `src/i18n/*.json`, `src/lib/types.ts`, `README.md`, `crates/banager-core/src/lib.rs`), **every edit below is anchored by a symbol, function, type or quoted line, never by a line number alone; line numbers in those files may have moved.** In files neither touches, `file:line` is cited at `f574d9f`.

## Global Constraints

Copied verbatim from the spec's binding rules (spec lines 20–23):

> 产品规则一条不让（spec §1、§6）：每一步说人话；后台工作绝不问密码；执行前先看到确切命令；
> 结果诚实——版本没动是 `NeedsAttention(UnchangedAfterUpgrade)`，中途停止是 `Unconfirmed`，
> 没有证据绝不说成功；fixture 只收真机录制；Banager 不跑 shell、不把下载管进 `sh`；
> 界面绝不提供 Rust 会拒绝的操作；所有文案 en + zh-CN。

And from spec §十 ("每一步只带**该步有生产者**的变体与字段——「先定义、后面某步再用」正是本项目最常见的缺陷") and §2.2/§2.3 ("每个新字段点名生产读取方"), applied to this step:

- **Every new field, variant, constant or function names its production reader in the same task** (as a doc comment, and in the task's Interfaces block). What this step does *not* define because nothing in it produces it: `Recipe.uninstall`, `backup_globs`, `Uninstall`/`RemoveSpec`/`KeepSpec`/`Probe`/`Expect`/`Glob`, `PlanAction`, `Trasher`, `UpdateBlocked::SelfUpdatesOnly`, `Latest::{HttpJsonField, HttpTomlVersion, Command}`, `RouteKind::FlatFile`, `VersionParse::SecondToken`, `$CARGO_HOME/` path expansion, `Detected.{euid, cargo_home}`, `HostEnv.rustup_home` (deleted by the spec). Each arrives with its producer in C, D or E.
- **Honest outcomes.** Nothing here touches `run_operation`. `claude update` that exits 0 with the version unchanged is `NeedsAttention(UnchangedAfterUpgrade)` (Task 3, stage 8's end-to-end test); a stopped run is `Unconfirmed` through `run_plan` as for every source.
- **Fixtures come from real machines only.** The one fixture directory this step adds, `adapters/fixtures/standalone-claude/<version>/`, is recorded on the author's Mac by the read-only commands in Task 3, stage 9 and nothing else; `claude update` is never run to record anything. Synthetic layouts live in temp directories the tests build; inline strings in tests are not fixtures.
- **No shell.** `Plan.program` is only ever the instance's `exe_path` (the launcher); the recipe has no field that could name another program.
- **The UI never offers what Rust refuses.** `NoSafeMethod` is refused by `Session::issue_plan` (`plans.rs` `blocked_uninstall`) and hidden by `InstalledPage` from the same field; `plan(Uninstall)` refuses it again for a stale snapshot.
- **en + zh-CN for all copy.** Every new key in both `src/i18n/en.json` and `src/i18n/zh-CN.json`; `src/i18n/completeness.test.ts` requires each key to be looked up by a *literal* in non-test source (so lookups go through `Record`s of literal keys, never assembled strings); `src/i18n/no-literal-strings.test.ts` forbids English literals in JSX; zh-CN prose uses full-width `，：（）` between CJK characters.
- **No author-machine details in tests or source** beyond public tool names the spec itself uses; recorded fixtures carry what the commands printed (as `adapters/fixtures/uv/0.12.17/` already does).
- **The five gates**, from README.md "Tests — all five must pass before anything is committed" (the TypeScript gate is `pnpm typecheck`, two `tsc` programs):

  ```bash
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  pnpm test
  pnpm typecheck
  ```

  Run `cargo fmt --all` before the `--check` gate: the Rust below is written *for* rustfmt, and rustfmt decides line breaks.
- **Commits:** `git add <exact paths>` (never `-A`), an imperative subject in sentence case, a body that says why, a blank line, then the `Co-Authored-By:` attribution line **the executing session is instructed to use**. The complete commit blocks use `Co-Authored-By: Codex <noreply@openai.com>` for this revision; a later session with a different required attribution must replace that complete line with its actual attribution.

## Rulings this plan makes

Where the spec leaves a choice to the step, or where this step's slice of the spec's types would otherwise define something nothing produces, the decision is made here so no task has to.

1. **Non-optional where B has no `None` producer.** Spec §3.1 writes `latest: Option<Latest>` and `upgrade: Option<UpgradeCmd>`; the `None` of the first has no producer in the whole first batch (every recipe has an endpoint), and the `None` of the second is agy's `SelfUpdatesOnly` (step D). In B they are `latest: Latest` and `upgrade: UpgradeCmd`; D widens `upgrade` to `Option` with agy, and `latest` becomes `Option` with the first recipe that lacks an endpoint (none is known). Same rule for `RouteKind` (only `SymlinkIntoRoot`; `FlatFile` with agy in D), `VersionParse` (only `FirstToken`; `SecondToken` with grok in D), `Latest` (only `ClaudeChannel`), `route::expand` (only `~/`; `$CARGO_HOME/` with rustup in E).
2. **`Detected` holds only `home` in B.** Spec §3.2's `Detected { home, euid, cargo_home, launcher, real }` is the seat `plan()`/`execute()` read because they have no `HostEnv`. In B the one reader with no `HostEnv` that needs anything from detect is `check_updates` — it reads `~/.claude/settings.json`, so it needs `home`. `euid` (removal's check 3) arrives in C, `cargo_home` (rustup's locks) in E. `launcher` and `real` are not cached at all: `inventory` re-probes the disk from the instance's own `exe_path` and `prefix`, which detect already expanded (spec §3.6 says inventory "is not a cache of detect's result"), so caching them would be a second, staler copy.
3. **`check_updates` re-probes and reads the launcher's version now**, after refresh's inventory read, before comparing the endpoint. It never builds an actionable candidate from detect's stale `inst.version`. A missing or `LauncherOnly` route gets no request; a failed live version read yields an uncheckable candidate (the instance version is only a display fallback). This extra read uses the same environment and timeout. It fixes the detect=281, inventory=290, endpoint=290 race without adding snapshot caching or changing `Adapter`; an external update can still occur between any two reads.
4. **`plan(Uninstall)` refuses with `UninstallBlocked { NoSafeMethod }`; `plan(Install)` is `Unsupported`.** The gate refuses both first (`plans.rs`: `blocked_uninstall`; a missing instance is `SourceGone`); the adapter's answers are the late twins for a stale snapshot. `plan()` reads nothing from `Detected` in B, so spec §3.2's "`Refused` when `detected` is `None`" branch has no reader yet and arrives with C's removal.
5. **`owned_roots` gets the `standalone-claude` row only.** F's hand-off asks B for three rows (claude/agy/grok); §十's rule is the reason F itself deferred them, and it applies within B too: `standalone-agy` and `standalone-grok` rows join with the recipes that produce their instances (D).
6. **The fixture directory and the registration land in one task.** `tests/fixtures_layout_test.rs` requires the fixture directory set to equal the registered adapter ids *exactly*, so a recorded `adapters/fixtures/standalone-claude/` fails that test until `Session::new` registers the adapter, and registration fails it until the directory exists. Task 3, stage 9 does both, plus the `## Claude Code` section of `docs/what-we-run.md` that A's `what_we_run_test` requires for every registered id.
7. **`downloads.claude.ai` joins `ALLOWED_HTTPS_HOSTS` in the task that first contacts it** (Task 3, stage 7, `check_updates`), together with the host's row in `docs/what-we-run.md`'s network table — A's `test_what_we_run_names_every_allowed_https_host` requires the row in the same commit as the host.
8. **The two empty-state sentences name only Claude Code.** Spec §9.2's text lists "Claude Code, Antigravity, Grok, rustup"; in B only Claude Code is registered, and an empty state that promises three unregistered tools is false copy. D and E extend the list with their recipes.
9. **`UNINSTALL_BLOCKED_KEYS.NoSafeMethod.descriptionSourceUnavailable` is the same key as `description`.** Spec §9.2 writes "（同上）": the sentence promises nothing about when a button returns, so a silent source has nothing different to say, and one key is kept rather than two identical strings.
10. **`sourceNotice.shadowedByNpm.title` and `.shadowedByOther.title` are their own keys with the same text as `shadowedByHomebrew.title`** (spec: "同 shadowedByHomebrew.title"): each `sourceNoticesFor` branch names its own literal key, which is what `completeness.test.ts` checks.
11. **`parse_version` reads the first *non-empty* line's first token**, not literally "the first line" (spec §3.1): a leading blank line would otherwise make a perfectly working tool `NotResponding`. Recorded output has no such line; the rule costs nothing and is tested.
12. **The `--version` environment is asserted with a spec-recording runner** defined in the adapter's test module: `MockRunner` keys and records argv only, and `DISABLE_AUTOUPDATER=1` is the whole point of spec §3.4.
13. **Recorded values may differ from 2.1.281 on the recording day.** Every fixture-backed assertion derives its expectation from the meta file and the recorded files (Task 3, stage 9), and the recording instructions say what to rename if the installed version has moved. `2.1.281`, `/latest` → `2.1.281`, `/stable` → `2.1.273` are what this Mac answered on 2026-09-24 (re-checked read-only while writing this plan).
14. **`sourceNotice.launcherOnly.description` is B-true copy, not spec §9.2's sentence.** The spec's sentence says "an earlier uninstall stopped partway; they may be in the Trash" and "Uninstall removes the link", written for the phase with step C's uninstall. In B no Banager uninstall exists (the state can only come from a manual or third-party removal of `~/.local/share/claude`), the `LauncherOnly` row's artifact carries `NoSafeMethod` (Task 3, stage 6), the gate refuses and the Installed page shows no Uninstall button on that very row — so the spec's sentence would tell the user to press a button that is not there. B's sentence (Task 1) says the link is left, that Banager cannot remove it yet, and points at the official install and uninstall instructions; `sources.test.ts` asserts it promises no uninstall. Step C restores the spec's sentence together with the uninstall that makes it true. The Rust and TS doc comments on `LauncherOnly` say the same.
15. **`installed.blocked.NoSafeMethod.description` says "can't yet move its files to the Trash safely", not spec §9.2's "doesn't yet have a verified list of the files".** For Claude Code the two-path list *is* verified (claude.md §7); what B lacks is C's Trasher. The sentence B ships is true for both cases §6.1's "Neither" covers (a second-batch tool before its list is verified; Claude Code before C), keeps `{{source}}` and no `{{command}}`, and changes no test but the exact-sentence assertion in `InstalledPage.test.tsx`, which Task 2 writes against it.

## What already exists (do not rebuild)

- `Adapter` trait (`crates/banager-core/src/adapters/mod.rs:424-455`), `AdapterMeta::{from_toml, unverified_version}` (`:82-112`), `run_plan` (`:469-514`), `reconcile_from` (`:385-399`), `ensure_instance_match` (`:414-422`), `validate_package_name` (`:348-365`), `uncheckable_candidate` (`:269-284`), `CheckOptions`/`CheckOutcome` with `From<Vec<UpdateCandidate>>` (`:26-80`). `AdapterError::{UninstallBlocked, Unsupported, InvalidName, Refused}` (`:114-205`); `plan_operation_error` in `src-tauri/src/ipc.rs:183-216` already serialises `UninstallBlocked { reason }` as `{"kind":"uninstall_blocked","reason":<serde spelling>}`, so a new reason needs no IPC change.
- The single-instance adapter shape: `UvAdapter` (`adapters/uv.rs:101-161` detect with `instance_id(&self.meta.id, None)`, `:249-278` plan, `:300-354` the delegating `impl Adapter`). The detect-writes/plan-reads seat: `CargoAdapter.binstall: Mutex<Option<PathBuf>>` (`adapters/cargo.rs:98-106`, `:137`, `:318`). `include_str!` meta loading (`uv.rs:108-109`, four `../` from `src/adapters/`; five from `src/adapters/standalone/`).
- `HostEnv { path_dirs, home, euid, cargo_home, ollama_host }` and `resolve_exe` (`runner/path_env.rs:5-21`, `:87-95`); `CommandSpec`/`CommandOutput`/`OutputUse` (`runner/mod.rs:31-73`); `MockRunner::{new, respond, calls}` (`runner/mock.rs:16-49`); `HttpRequest`/`HttpResponse`/`HttpError` (`http/mod.rs:18-41`); `MockHttpClient::{new, respond, fail, calls}` (`http/mock.rs:16-41`); `crate::testing::manager_instance` (`testing.rs:44-56`).
- `Session::new`'s registration `vec![...]` (`session/mod.rs:257-273`) and `test_new_registers_all_seven_adapters` (`:499-515`); `Session::build`'s two `assert!`s (`:314-323`) that hold for `standalone-claude` (no `:`, not a duplicate).
- The gate: `blocked_uninstall` (`session/plans.rs:103-115`) refusing at `:186-188`; its test `test_issue_plan_refuses_an_uninstall_the_tool_will_refuse_for_that_package` (`:865-923`) with the helpers `FakeAdapter::new`, `set_artifacts`, `installed_on` (`:832-855`), `uninstall_on` (`:857-862`), `upgrade_on`.
- `merge_instance_notes` is `extend` (`session/refresh.rs:589-600`), so detect's notes survive `check_updates`'s `CheckOutcome.notes`.
- Front end: `ADAPTER_LABEL_KEYS` (`src/lib/sources.ts:18-26`) read at `InstalledPage.tsx:166`, `UpdatesPage.tsx:207`, `UpdatesPage.tsx:521`, `UninstallDialog.tsx:50`; `sourceNoticesFor`'s notes loop with its `never` (`sources.ts:198-225`); `UNINSTALL_BLOCKED_KEYS: Record<UninstallBlocked, UninstallBlockedCopy>` (`sources.ts:451-475`) read at `InstalledPage.tsx:144-146`, `:329-345` and `UninstallDialog.tsx:102-114`; `rowDescription` in `UpdatesPage.tsx:444-475` with `artifactsById` (`:335-341`), `isActionable` (`:287-288`), `sourceLabelFor` (`:518-523`); the description fallback `item.artifact.description ?? t("installed.noDescription")` at `InstalledPage.tsx:343`; `withCommand`/`COMMAND_SLOT` (`src/components/withCommand.tsx`: a sentence without the slot is returned unchanged, so a copy with no `{{command}}` renders as plain text); `updateStateOf` (`src/lib/updateState.ts:43-52`).
- Test harnesses: `src/lib/sources.test.ts` (`fakeT`, `instance()`), `src/pages/InstalledPage.test.tsx` (`snapshot`, `settings`, the virtualizer stubs in `beforeEach`), `src/pages/UpdatesPage.test.tsx` (`snapshot`, the `updates`/`instances`/`artifacts` knobs in `beforeEach`, `wholeSentence`), `crates/banager-core/tests/ops_upgrade_version_test.rs` (`ScriptedRunner::script`, `exited_0`, `upgrade(&runner, adapter, inst, kind, name) -> Outcome`).
- From A: `banager_core::http::real::{ALLOWED_HTTPS_HOSTS, host_allowed}`; `docs/what-we-run.md` with `## Homebrew` … `## Ollama`, `## Files Banager reads`, `## Network: Banager only connects to these hosts` (a table `| Host | What is fetched | By |`), `## What Banager never does`. From F: `owned_roots` in `scan/mod.rs` with `test_owned_roots_table` in its `mod tests`; `Known::index` reading `InstalledArtifact.path` (rule 2) and `ManagerInstance.exe_path` (rules 0/1).

## File Structure

```
adapters/meta/standalone-claude.toml                              NEW   AdapterMeta, seven fields, kind = "standalone" (Task 3, stage 4)
adapters/fixtures/standalone-claude/2.1.281/                      NEW   README.md, version.txt, latest.txt, stable.txt, layout.txt — recorded (Task 3, stage 9)
crates/banager-core/src/adapters/mod.rs                            MOD   `pub mod standalone;` (Task 3, stage 3)
crates/banager-core/src/adapters/standalone/mod.rs                 NEW   StandaloneAdapter, Detected, all(), impl Adapter, test support (Task 3, stages 3–9)
crates/banager-core/src/adapters/standalone/recipe.rs              NEW   Recipe, Route, RouteKind, VersionCmd, VersionParse, Latest, UpgradeCmd (Task 3, stage 3)
crates/banager-core/src/adapters/standalone/latest.rs              NEW   parse_version, is_dotted_version, compare_dotted, claude channel + body parsing (Task 3, stage 3)
crates/banager-core/src/adapters/standalone/recipes.rs             NEW   CLAUDE, RECIPES, the invariants tests (Task 3, stage 4)
crates/banager-core/src/adapters/standalone/route.rs               NEW   expand (Task 3, stage 4); probe, lexical_join, shadow_note (Task 3, stage 5)
crates/banager-core/src/model.rs                                   MOD   InstanceNote ×5 (Task 1), UninstallBlocked::NoSafeMethod (Task 2), shape tests
crates/banager-core/src/session/plans.rs                           MOD   one gate test (Task 2)
crates/banager-core/src/session/mod.rs                             MOD   Session::new registers standalone::all; the eight-adapter test (Task 3, stage 9)
crates/banager-core/src/http/real.rs                               MOD   ALLOWED_HTTPS_HOSTS += "downloads.claude.ai"; doc; one test (Task 3, stage 7)   [A's file]
crates/banager-core/src/scan/mod.rs                                MOD   owned_roots: standalone-claude row; test (Task 3, stage 9)                      [F's file]
crates/banager-core/src/lib.rs                                     MOD   one clause in the crate doc (Task 3, stage 9)                                    [F's file]
crates/banager-core/tests/ops_upgrade_version_test.rs              MOD   two Claude Code cases (Task 3, stage 8)
src/lib/types.ts, types.test.ts                                    MOD   InstanceNote (Task 1), UninstallBlocked (Task 2)                        [A's file]
src/lib/sources.ts, sources.test.ts                                MOD   five notice branches (1); label + NoSafeMethod copy (2); STANDALONE_SUMMARY_KEYS (10)
src/pages/InstalledPage.tsx, InstalledPage.test.tsx                MOD   NoSafeMethod row test (2); summary fallback (10)
src/pages/UpdatesPage.tsx, UpdatesPage.test.tsx                    MOD   selfUpdatingHint (10)
src/components/SnapshotStatus.test.tsx                             MOD   the two empty-state sentences (10)
src/i18n/en.json, zh-CN.json                                       MOD   sourceNotice.* (1), adapters + installed.blocked.NoSafeMethod (2), standalone.summary + updates.selfUpdatingHint + emptyStates (10)   [F's files]
docs/what-we-run.md                                                MOD   host row (7); `## Claude Code` section, intro, program-source paragraph, files-read bullet (9)   [A's + F's file]
README.md                                                          MOD   the source row; the two test counts (Task 11)                            [F's file]
```

Single responsibility: `recipe.rs` owns *what a tool is* (types only); `recipes.rs` owns *the tools* (data + the invariants over the data); `route.rs` owns *is this launcher this route's, and which copy runs*; `latest.rs` owns *versions* (parsing the installed one, the published one, comparing them); `mod.rs` owns *the `Adapter` contract* over those.

## Core Interfaces (authoritative names; complete definitions are in the tasks)

| Module | Names and production readers |
|---|---|
| `model.rs`, `types.ts` | `InstanceNote::{NotOnPath, ShadowedByHomebrew, ShadowedByNpm, ShadowedByOther, LauncherOnly}` → `sourceNoticesFor`; `UninstallBlocked::NoSafeMethod` → uninstall gate and copy record |
| `standalone/recipe.rs` | `Recipe`, `Route`, `RouteKind::SymlinkIntoRoot`, `VersionCmd`, `VersionParse::FirstToken`, `Latest::ClaudeChannel`, `UpgradeCmd` → the complete standalone adapter in the same core task |
| `standalone/latest.rs` | `is_dotted_version`, `parse_version`, `compare_dotted`, `CHANNEL_LATEST`, `CHANNEL_STABLE`, `claude_channel_from_json`, `claude_channel`, `parse_channel_body` → adapter version reads and update checks |
| `standalone/recipes.rs` | `CLAUDE`, `RECIPES` → `all`, detection, inventory, checks and plans |
| `standalone/route.rs` | `expand`, `Probe::{Absent, Present { real }, LauncherOnly}`, `probe`, `lexical_join`, private `canonicalize_existing_prefix`, `shadow_note` → detection, inventory and checks; helpers are consumed by `probe` |
| `standalone/mod.rs` | `Detected { home }`, `StandaloneAdapter::{new, detect, inventory, search, check_updates, plan, execute, reconcile}`, `all` → `Adapter` delegation and `Session::new` registration in the same core task |
| `sources.ts` | `StandaloneAdapterId`, `STANDALONE_SUMMARY_KEYS`, `standaloneSummaryKey` → `InstalledPage::installedDescription`, including blocked rows, in the same frontend task |
| `http/real.rs` | `ALLOWED_HTTPS_HOSTS` gains `downloads.claude.ai` with its production request and trust-file row |

## Task List

| Task | Deliverable |
|---|---|
| 1 | Five `InstanceNote` variants and their production notice readers, with both locales |
| 2 | `NoSafeMethod`, uninstall gate/copy readers, and standalone label |
| 3 | Complete standalone core in stages 3–9: types/parsing, recipe, routes, adapter, checks, operations, recording and registration; one commit with every new field read in production |
| 10 | Reachable summary beside the refusal, self-update hint and scoped empty states, with both locales |
| 11 | README source row and measured test counts |

Execute 1 → 2 → 3 (stages 3 → 4 → 5 → 6 → 7 → 8 → 9) → 10 → 11. Task numbers 10 and 11 are retained to keep the review and handover anchors stable; 4–9 are stages inside Task 3, not separate tasks or commits.

## Review Focus

Five inputs the spec implies but no test would otherwise exercise, most likely to bite first. Each has its test in the task named.

1. **`claude --version` prints a leading blank line or trailing whitespace** → the version must still parse, not turn the row into "Banager can't reach Claude Code" (Task 3, stage 3, `test_parse_version_skips_leading_blank_lines_and_trailing_space`).
2. **`claude --version` hangs** (a pre-2.1.214 build scanning a directory named `.zshrc`, or a locked keychain prompt) → the 30 s runner timeout makes the instance `NotResponding`, no refresh hangs (Task 3, stage 6, `test_detect_marks_a_timed_out_version_read_as_not_responding`).
3. **The channel endpoint answers with something that is not a version** (an HTML error page, a `v`-prefixed string) or `~/.claude/settings.json` carries `autoUpdatesChannel` as a non-string → an uncheckable row, resp. the `latest` channel; never a candidate built from garbage (Task 3, stage 3, `test_parse_channel_body_refuses_anything_that_is_not_a_version` and `test_claude_channel_from_json_defaults_to_latest_for_anything_but_stable`; Task 3, stage 7, `test_check_updates_marks_a_non_version_answer_uncheckable`).
4. **`$HOME` is itself reached through a symlink** (a home on another volume, `/Users/x` → `/Volumes/…`) → the launcher still resolves *into* the root, because both sides are canonicalised before comparison, and the instance's `exe_path`/`prefix` keep the non-canonical spelling `HostEnv.home` gave (which is what the Unknown page's rule 0 compares raw) (Task 3, stage 5, `test_probe_accepts_a_home_reached_through_a_symlink`). The half-uninstalled twin — the same home, the program directory gone, the link text spelled through the real home — must still be `LauncherOnly`, not a row that vanishes and reappears as a broken link on the Unknown page; neither side canonicalises whole then, so `probe` canonicalises the deepest surviving ancestor of each (Task 3, stage 5, `test_probe_reports_launcher_only_under_a_home_reached_through_a_symlink`).
5. **A `claude` earlier on `PATH` that is a symlink to this very launcher** (`~/bin/claude → ~/.local/bin/claude`) → no shadow notice: the same file runs (Task 3, stage 5, `test_shadow_note_is_silent_for_a_link_to_the_same_launcher`).

---

### Task 1: Five `InstanceNote` variants, their notices and copy

**Files:**
- Modify: `crates/banager-core/src/model.rs:92-105` (`InstanceNote`), `:843-887` (`test_instance_status_is_default_empty_and_bare_strings_on_the_wire`)
- Modify: `src/lib/types.ts` — the `InstanceNote` union (today `export type InstanceNote = "IndexMayBeStale" | "IndexUpdating";` under the comment `/** Mirrors \`InstanceNote\`; payload-free on purpose, so a bare string. */`)
- Modify: `src/lib/types.test.ts:138-163` (`spells InstanceStatus as an always-present object with bare-string variants`)
- Modify: `src/lib/sources.ts:198-225` (the notes loop of `sourceNoticesFor`)
- Modify: `src/lib/sources.test.ts` (append inside `describe("sourceNoticesFor", …)` before its closing `});` at `:220`; extend the list in `is the one rule hasSourceNotice answers from`, `:204-219`)
- Modify: `src/i18n/en.json` and `src/i18n/zh-CN.json` — the `sourceNotice` object (after its `indexUpdating` block)
- Test: `model.rs`'s shape test, `types.test.ts`, `sources.test.ts`, `completeness.test.ts` (both locales, every key referenced).

**Interfaces:**
- Consumes: `InstanceStatus { unavailable, notes: Vec<InstanceNote> }` (`model.rs:115-120`); `SourceNoticeSpec` (`sources.ts:89-100`); the `never` at the end of the notes loop (`sources.ts:221-224`), which is what makes an unhandled variant fail `tsc`.
- Produces (verbatim): `InstanceNote::{NotOnPath, ShadowedByHomebrew, ShadowedByNpm, ShadowedByOther, LauncherOnly}` (Rust) and the same five bare strings in `InstanceNote` (TS); five `sourceNoticesFor` branches with ids `<instance.id>:not-on-path`, `:shadowed-by-homebrew`, `:shadowed-by-npm`, `:shadowed-by-other`, `:launcher-only`, `axis: "state"`, the four PATH ones `variant: "info"`, `LauncherOnly` `variant: "warning"`, no action, `values: { source, command }` where `command` is the file name of `instance.exe_path`. Producer of the Rust variants: `StandaloneAdapter::detect` (Task 3, stage 6, through `route::shadow_note` in Task 3, stage 5 and the `LauncherOnly` probe result). Readers: `sourceNoticesFor` (both pages render it through `SourceNotices`, `InstalledPage.tsx:181`, `UpdatesPage.tsx:210`); `hasSourceNotice` (`SnapshotStatus`'s empty-state gate) follows from it.

- [ ] **Step 1: Write the failing tests**

In `crates/banager-core/src/model.rs`, inside `test_instance_status_is_default_empty_and_bare_strings_on_the_wire`, after the block that ends

```rust
        assert_eq!(
            serde_json::from_str::<InstanceStatus>(&json).expect("deserialize"),
            status
        );
    }
```

(the `IndexUpdating` check, the last statement of the test) and before the test's closing `}`, insert:

```rust

        // Phase 4's five standalone-installer notes are bare strings too,
        // spelled exactly as `src/lib/types.ts` mirrors them: the front
        // end's `sourceNoticesFor` matches these strings, and a spelling
        // that drifted would fall through every branch and show nothing.
        for (note, wire) in [
            (InstanceNote::NotOnPath, "NotOnPath"),
            (InstanceNote::ShadowedByHomebrew, "ShadowedByHomebrew"),
            (InstanceNote::ShadowedByNpm, "ShadowedByNpm"),
            (InstanceNote::ShadowedByOther, "ShadowedByOther"),
            (InstanceNote::LauncherOnly, "LauncherOnly"),
        ] {
            let status = InstanceStatus {
                unavailable: None,
                notes: vec![note],
            };
            let json = serde_json::to_string(&status).expect("serialize");
            assert_eq!(json, format!(r#"{{"unavailable":null,"notes":["{wire}"]}}"#));
            assert_eq!(
                serde_json::from_str::<InstanceStatus>(&json).expect("deserialize"),
                status
            );
        }
```

In `src/lib/types.test.ts`, inside `it("spells InstanceStatus as an always-present object with bare-string variants", …)`, after `expect(roundTrip(notResponding)).toEqual(notResponding);` and before the `});` that closes the `it`, insert:

```ts

    // The five notes a standalone tool's detect can add (phase 4): which
    // copy runs when its name is typed, or that only its launcher is left.
    const standalone: InstanceStatus = {
      unavailable: null,
      notes: ["NotOnPath", "ShadowedByHomebrew", "ShadowedByNpm", "ShadowedByOther", "LauncherOnly"],
    };
    expect(JSON.stringify(standalone)).toBe(
      '{"unavailable":null,"notes":["NotOnPath","ShadowedByHomebrew","ShadowedByNpm","ShadowedByOther","LauncherOnly"]}',
    );
    expect(roundTrip(standalone)).toEqual(standalone);
```

In `src/lib/sources.test.ts`, replace the list in `it("is the one rule hasSourceNotice answers from", …)` (the `it` is `:204-219`; the `for (const inst of [ … ])` array to replace is `:209-216`. Not `:99-112`, which is the stopped-Ollama test `gives a stopped source the same sentence…`) with:

```ts
    for (const inst of [
      instance(),
      instance({ read_only_reason: "ByDesign" }),
      instance({ status: { unavailable: "NotRunning", notes: [] } }),
      instance({ status: { unavailable: "RefusesAsRoot", notes: [] } }),
      instance({ status: { unavailable: null, notes: ["IndexMayBeStale"] } }),
      instance({ status: { unavailable: null, notes: ["IndexUpdating"] } }),
      instance({ status: { unavailable: null, notes: ["NotOnPath"] } }),
      instance({ status: { unavailable: null, notes: ["ShadowedByHomebrew"] } }),
      instance({ status: { unavailable: null, notes: ["ShadowedByNpm"] } }),
      instance({ status: { unavailable: null, notes: ["ShadowedByOther"] } }),
      instance({ status: { unavailable: null, notes: ["LauncherOnly"] } }),
    ]) {
```

and, after that `it` (before the `});` closing `describe("sourceNoticesFor", …)`), append:

```ts

  // A standalone tool's instance: the launcher is its `exe_path`, the tool
  // root its `prefix`, and the command the user types is the launcher's
  // file name.
  const claude = instance({
    id: "standalone-claude",
    adapter_id: "standalone-claude",
    exe_path: "/Users/someone/.local/bin/claude",
    prefix: "/Users/someone/.local/share/claude",
    version: "2.1.281",
  });

  it("tells a standalone tool's user which copy runs when they type its name", () => {
    // Four payload-free notes, four actionable sentences (spec §七): the
    // path of the winning copy is not in the notice -- the user this app
    // is for would not recognise it -- but the command name is, so the
    // sentence can say "when you type claude".
    for (const [note, id, key] of [
      ["NotOnPath", "not-on-path", "sourceNotice.notOnPath"],
      ["ShadowedByHomebrew", "shadowed-by-homebrew", "sourceNotice.shadowedByHomebrew"],
      ["ShadowedByNpm", "shadowed-by-npm", "sourceNotice.shadowedByNpm"],
      ["ShadowedByOther", "shadowed-by-other", "sourceNotice.shadowedByOther"],
    ] as const) {
      const notices = sourceNoticesFor(
        { ...claude, status: { unavailable: null, notes: [note] } },
        "Claude Code",
      );
      expect(notices).toEqual([
        {
          id: `standalone-claude:${id}`,
          axis: "state",
          variant: "info",
          titleKey: `${key}.title`,
          descriptionKey: `${key}.description`,
          values: { source: "Claude Code", command: "claude" },
        },
      ]);
    }
  });

  it("warns, and names the link, when only a standalone tool's launcher is left", () => {
    // The half-uninstalled state (program files gone, launcher dangling):
    // a warning because this launcher is broken; another PATH copy may work. No
    // button, and -- in this step -- no Uninstall on the row either (its
    // artifact carries NoSafeMethod until step C), so the sentence must
    // not promise one.
    const notices = sourceNoticesFor(
      { ...claude, status: { unavailable: null, notes: ["LauncherOnly"] } },
      "Claude Code",
    );
    expect(notices).toEqual([
      {
        id: "standalone-claude:launcher-only",
        axis: "state",
        variant: "warning",
        titleKey: "sourceNotice.launcherOnly.title",
        descriptionKey: "sourceNotice.launcherOnly.description",
        values: { source: "Claude Code", command: "claude" },
      },
    ]);
  });

  it("falls back to the whole exe_path as the command when it has no file name", () => {
    const notices = sourceNoticesFor(
      { ...claude, exe_path: "/", status: { unavailable: null, notes: ["NotOnPath"] } },
      "Claude Code",
    );
    expect(notices[0].values).toEqual({ source: "Claude Code", command: "/" });
  });

  it("puts the command and the source into every standalone notice's copy, in both locales", () => {
    for (const locale of [en, zhCN]) {
      for (const key of [
        "notOnPath",
        "shadowedByHomebrew",
        "shadowedByNpm",
        "shadowedByOther",
        "launcherOnly",
      ] as const) {
        expect(locale.sourceNotice[key].description).toContain("{{command}}");
      }
      // The three "another copy runs" notices share one title, and the
      // three descriptions each name the other copy differently.
      expect(locale.sourceNotice.shadowedByNpm.title).toBe(locale.sourceNotice.shadowedByHomebrew.title);
      expect(locale.sourceNotice.shadowedByOther.title).toBe(locale.sourceNotice.shadowedByHomebrew.title);
      expect(locale.sourceNotice.shadowedByHomebrew.description).toContain("Homebrew");
      expect(locale.sourceNotice.shadowedByNpm.description).toContain("npm");
      expect(locale.sourceNotice.launcherOnly.description).toContain("{{source}}");
      // Until step C the LauncherOnly row's artifact carries NoSafeMethod,
      // so the Installed page shows no Uninstall button on it: the notice
      // must not tell the user to press one (spec §9.2's sentence returns
      // with step C's uninstall).
      expect(locale.sourceNotice.launcherOnly.description).not.toMatch(/Uninstall removes|卸载会把/);
      expect(locale.sourceNotice.launcherOnly.description).not.toMatch(/typing .* in Terminal fails|输入 .* 会失败/);
      for (const key of ["shadowedByHomebrew", "shadowedByNpm"] as const) {
        expect(locale.sourceNotice[key].description).not.toMatch(/both are listed on this page|两份在这一页上都能找到/);
      }
    }
  });
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p banager-core --lib model::tests::test_instance_status_is_default_empty_and_bare_strings_on_the_wire`
Expected: FAIL to compile — `error[E0599]: no variant or associated item named \`NotOnPath\` found for enum \`InstanceNote\`` (and the four others).

Run: `pnpm typecheck`
Expected: FAIL — in `src/lib/types.test.ts`, `Type '"NotOnPath"' is not assignable to type 'InstanceNote'` (and in `sources.test.ts`, the same for each new note; `locale.sourceNotice.notOnPath` is `Property 'notOnPath' does not exist`).

- [ ] **Step 3: Add the variants, the mirror, the branches and the copy**

In `crates/banager-core/src/model.rs`, replace the `InstanceNote` enum (`:92-105`) with:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstanceNote {
    /// `brew update` failed, so the local catalogue may be behind and
    /// "no updates" may be wrong.
    IndexMayBeStale,
    /// `brew update` is still downloading, so the refresh did not read the
    /// catalogue it is rewriting (`AdapterError::IndexUpdating`). This
    /// source's update candidates are the previous snapshot's, and so are
    /// its installed packages unless the refresh read them before it
    /// started the update itself; with no previous snapshot, there are
    /// none. Nothing has failed. When the update ends the shell refreshes
    /// again (see `Session::background_change`), so this clears by itself.
    IndexUpdating,
    /// Typing this tool's name in Terminal would not find it: the
    /// directory its launcher lives in is not on the `PATH` Banager sees.
    /// Produced by `StandaloneAdapter::detect` (`route::shadow_note`) for
    /// a tool installed by its own installer; read by `sourceNoticesFor`
    /// in src/lib/sources.ts.
    NotOnPath,
    /// Typing the name runs a copy Homebrew installed instead of this one:
    /// the first executable of that name on `PATH` resolves under a
    /// `Cellar` or `Caskroom` directory. Same producer and reader as
    /// `NotOnPath`.
    ShadowedByHomebrew,
    /// As `ShadowedByHomebrew`, for a copy npm installed (it resolves under
    /// a `node_modules` directory).
    ShadowedByNpm,
    /// As `ShadowedByHomebrew`, for a copy Banager does not recognise; the
    /// Unknown page may show where it is.
    ShadowedByOther,
    /// The launcher is still there but points at program files that are
    /// gone: the program directory was removed by hand or by another
    /// tool (from step C on, also by a Banager uninstall that stopped
    /// after moving it and before moving the launcher -- C's removal
    /// order makes that the only such state). The row stays, with no
    /// version, so the state is visible. In this step its artifact still
    /// carries `UninstallBlocked::NoSafeMethod`, so the gate refuses an
    /// uninstall and the notice promises none; step C's path-list
    /// uninstall is what lets one through to remove the link. Produced by
    /// `StandaloneAdapter::detect` when `route::probe` answers
    /// `LauncherOnly`.
    LauncherOnly,
}
```

In `src/lib/types.ts`, replace the `InstanceNote` line and its comment with:

```ts
/**
 * Mirrors `InstanceNote` in crates/banager-core/src/model.rs; payload-free
 * on purpose, so a bare string. `sourceNoticesFor` in src/lib/sources.ts
 * ends its loop over these in a `never`, so a variant added here without
 * a branch there fails `tsc`. The last five are a standalone tool's
 * (phase 4): which copy runs when its name is typed, or that only its
 * launcher is left.
 */
export type InstanceNote =
  | "IndexMayBeStale"
  | "IndexUpdating"
  | "NotOnPath"
  | "ShadowedByHomebrew"
  | "ShadowedByNpm"
  | "ShadowedByOther"
  | "LauncherOnly";
```

In `src/lib/sources.ts`, replace the notes loop (`:198-225`, from `for (const note of instance.status.notes) {` to its closing `}`) with:

```ts
  for (const note of instance.status.notes) {
    if (note === "IndexMayBeStale") {
      notices.push({
        id: `${instance.id}:index-may-be-stale`,
        axis: "state",
        variant: "warning",
        titleKey: "sourceNotice.indexMayBeStale.title",
        descriptionKey: "sourceNotice.indexMayBeStale.description",
        action: { id: "retry", labelKey: "sourceNotice.indexMayBeStale.action" },
      });
    } else if (note === "IndexUpdating") {
      // Nothing has failed: the download is still going. So an "info"
      // notice, and no button -- there is nothing for the user to do, and
      // a "Try again" here could only wait on the same download. The core
      // refreshes by itself when it ends (`Session::background_change`),
      // which is what clears this.
      notices.push({
        id: `${instance.id}:index-updating`,
        axis: "state",
        variant: "info",
        titleKey: "sourceNotice.indexUpdating.title",
        descriptionKey: "sourceNotice.indexUpdating.description",
      });
    } else if (note === "NotOnPath") {
      // The four "which copy runs" notes of a standalone tool (spec §七).
      // Info, not warning: the install works, the user just needs to know
      // what typing its name does. `{{command}}` is the launcher's file
      // name -- the word the user types -- not its path, which this
      // app's audience would not recognise. Plain text: `SourceNotice`
      // renders `t(key, values)`, never `withCommand`.
      notices.push({
        id: `${instance.id}:not-on-path`,
        axis: "state",
        variant: "info",
        titleKey: "sourceNotice.notOnPath.title",
        descriptionKey: "sourceNotice.notOnPath.description",
        values: { source: sourceLabel, command: commandNameOf(instance) },
      });
    } else if (note === "ShadowedByHomebrew") {
      notices.push({
        id: `${instance.id}:shadowed-by-homebrew`,
        axis: "state",
        variant: "info",
        titleKey: "sourceNotice.shadowedByHomebrew.title",
        descriptionKey: "sourceNotice.shadowedByHomebrew.description",
        values: { source: sourceLabel, command: commandNameOf(instance) },
      });
    } else if (note === "ShadowedByNpm") {
      notices.push({
        id: `${instance.id}:shadowed-by-npm`,
        axis: "state",
        variant: "info",
        titleKey: "sourceNotice.shadowedByNpm.title",
        descriptionKey: "sourceNotice.shadowedByNpm.description",
        values: { source: sourceLabel, command: commandNameOf(instance) },
      });
    } else if (note === "ShadowedByOther") {
      notices.push({
        id: `${instance.id}:shadowed-by-other`,
        axis: "state",
        variant: "info",
        titleKey: "sourceNotice.shadowedByOther.title",
        descriptionKey: "sourceNotice.shadowedByOther.description",
        values: { source: sourceLabel, command: commandNameOf(instance) },
      });
    } else if (note === "LauncherOnly") {
      // The half-uninstalled state: typing the command now fails, so a
      // warning. No button, and no promise of one: until step C the row's
      // artifact carries NoSafeMethod, so the gate refuses an uninstall
      // and the Installed page shows none. C's path-list uninstall is
      // what finishes the job (spec §3.3, §6.1).
      notices.push({
        id: `${instance.id}:launcher-only`,
        axis: "state",
        variant: "warning",
        titleKey: "sourceNotice.launcherOnly.title",
        descriptionKey: "sourceNotice.launcherOnly.description",
        values: { source: sourceLabel, command: commandNameOf(instance) },
      });
    } else {
      const unhandled: never = note;
      void unhandled;
    }
  }
```

and add, directly above `export function sourceNoticesFor(` (after the doc comment block that precedes it ends is fine too, but keep this function's own doc with it):

```ts
/**
 * The word the user types to run a standalone tool: the file name of its
 * launcher (`~/.local/bin/claude` → `claude`). A standalone instance's
 * `exe_path` is the launcher, not the resolved binary
 * (`StandaloneAdapter::detect`). Falls back to the whole path for one
 * with no file name, which no adapter produces.
 */
function commandNameOf(instance: ManagerInstance): string {
  const name = instance.exe_path.split("/").pop();
  return name !== undefined && name.length > 0 ? name : instance.exe_path;
}

```

In `src/i18n/en.json`, inside `"sourceNotice": { … }`, after the `"indexUpdating": { … }` block (add a comma after its closing `}`), insert:

```json
    "notOnPath": {
      "title": "{{source}} isn't in your PATH",
      "description": "It's installed, but typing {{command}} in Terminal probably won't find it: the folder it lives in isn't in your shell's search path (PATH). Opening a new Terminal window sometimes fixes this; otherwise follow the tool's own install guide for adding it to PATH."
    },
    "shadowedByHomebrew": {
      "title": "Another copy runs when you type {{command}}",
      "description": "Banager found this native copy of {{source}} and an executable in a Homebrew directory earlier in its PATH. Typing {{command}} in Terminal will likely run that other copy. Check the Installed page for it; Banager may not list its installation."
    },
    "shadowedByNpm": {
      "title": "Another copy runs when you type {{command}}",
      "description": "Banager found this native copy of {{source}} and an executable in a npm directory earlier in its PATH. Typing {{command}} in Terminal will likely run that other copy. Check the Installed page for it; Banager may not list its installation."
    },
    "shadowedByOther": {
      "title": "Another copy runs when you type {{command}}",
      "description": "Banager found another executable earlier in its PATH than this copy of {{source}}. Typing {{command}} in Terminal will likely run that other copy. The Unknown page may show where it is."
    },
    "launcherOnly": {
      "title": "Only the {{command}} link is left",
      "description": "The program files targeted by this {{command}} link are missing, so this launcher cannot run. Another installation may still work in Terminal. Banager can't remove the link yet. To get {{source}} back, reinstall it following the official instructions on its website; to finish removing it, follow the uninstall steps on the same page."
    }
```

(Not spec §9.2's sentence, which says "an earlier uninstall stopped partway; they may be in the Trash" and "Uninstall removes the link": in this step no Banager uninstall exists, the row's artifact carries `NoSafeMethod`, and the Installed page shows no Uninstall button on it, so both clauses would be false. Step C restores the spec's sentence with the uninstall that makes it true -- ruling 14.)

In `src/i18n/zh-CN.json`, the same position:

```json
    "notOnPath": {
      "title": "{{source}} 不在 PATH 里",
      "description": "它装好了，但在「终端」里输入 {{command}} 多半会找不到：它所在的文件夹不在 shell 的搜索路径（PATH）里。新开一个终端窗口有时就好了；不行的话，按这个工具自己的安装说明把它加进 PATH。"
    },
    "shadowedByHomebrew": {
      "title": "输入 {{command}} 时运行的是另一份",
      "description": "Banager 找到了这份原生安装的 {{source}}，也在它的 PATH 中更靠前的 Homebrew 目录里找到了可执行文件。在「终端」里输入 {{command}} 时，多半运行的是另一份。可以在「已安装」页找找，但 Banager 不一定能列出那份安装。"
    },
    "shadowedByNpm": {
      "title": "输入 {{command}} 时运行的是另一份",
      "description": "Banager 找到了这份原生安装的 {{source}}，也在它的 PATH 中更靠前的 npm 目录里找到了可执行文件。在「终端」里输入 {{command}} 时，多半运行的是另一份。可以在「已安装」页找找，但 Banager 不一定能列出那份安装。"
    },
    "shadowedByOther": {
      "title": "输入 {{command}} 时运行的是另一份",
      "description": "Banager 在它的 PATH 中找到了比这份 {{source}} 更靠前的另一个可执行文件。在「终端」里输入 {{command}} 时，多半运行的是另一份。「来源不明」页可能能看到它在哪。"
    },
    "launcherOnly": {
      "title": "只剩下 {{command}} 这个链接了",
      "description": "这个 {{command}} 链接指向的程序文件已经不在了，这个启动器无法运行。另一份安装在「终端」里可能仍然可用。Banager 还不能移走这个链接。想把 {{source}} 装回来，按它网站上的官方说明重新安装；想彻底删掉，按同一页的卸载说明做。"
    }
```

(If F has not yet landed, the `unknown` sidebar entry the `shadowedByOther` copy mentions does not exist yet; the sentence is still true once F merges before this branch ships, and the spec's copy is used verbatim.)

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p banager-core --lib model::tests` and `pnpm typecheck && pnpm exec vitest run src/lib/types.test.ts src/lib/sources.test.ts src/i18n`
Expected: PASS. `completeness.test.ts` passes because every new `sourceNotice.*` key is a string literal in `sources.ts` and both locales carry it.

- [ ] **Step 5: Run the gates**

Run all five from Global Constraints. Expected: all clean. (The Rust variants have no producer until Task 3, stage 6; `pub` enum variants are not dead-code warnings.)

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/model.rs src/lib/types.ts src/lib/types.test.ts src/lib/sources.ts src/lib/sources.test.ts src/i18n/en.json src/i18n/zh-CN.json
git commit -m "$(cat <<'EOF'
Add the five notes a standalone tool's instance can carry, with their notices

NotOnPath, ShadowedByHomebrew, ShadowedByNpm, ShadowedByOther answer
"which copy runs when I type its name" for a tool installed by its own
installer beside a Homebrew or npm copy; LauncherOnly names the
half-uninstalled state where only the dangling launcher is left.
Payload-free, like the two brew notes, so the wire stays a bare string;
sourceNoticesFor words each with the command name and the source, in
both locales. The producer is the standalone adapter's detect, which
follows.

Co-Authored-By: Codex <noreply@openai.com>
EOF
)"
```

---

### Task 2: `UninstallBlocked::NoSafeMethod`, its gate test, its copy record, the `standalone-claude` label

**Files:**
- Modify: `crates/banager-core/src/model.rs:220-233` (`UninstallBlocked`), `:655-700` (`test_uninstall_blocked_is_a_bare_string_on_the_wire_and_null_when_absent`)
- Modify: `crates/banager-core/src/session/plans.rs` — one test appended after `test_issue_plan_refuses_an_uninstall_the_tool_will_refuse_for_that_package` (ends `:923`), before `test_submit_is_refused_once_the_package_became_uninstall_blocked` (`:924-925`)
- Modify: `src/lib/types.ts` — `export type UninstallBlocked = "Pinned";` and its doc comment
- Modify: `src/lib/types.test.ts:127-136` (`spells UninstallBlocked as a bare string, and a removable artifact as null`)
- Modify: `src/lib/sources.ts:18-26` (`ADAPTER_LABEL_KEYS`), `:451-475` (`UNINSTALL_BLOCKED_KEYS`)
- Modify: `src/lib/sources.test.ts` — `describe("UNINSTALL_BLOCKED_KEYS", …)` (`:579-615`) and `describe("parseUninstallBlocked", …)` (`:617-627`)
- Modify: `src/pages/InstalledPage.test.tsx` — one test appended after `promises Uninstall back on a silent source's pinned row only once the source answers` (`:361-397`), before `describe("the Update available badge", …)` (`:399`)
- Modify: `src/i18n/en.json`, `src/i18n/zh-CN.json` — `adapters` and `installed.blocked`
- Test: the Rust shape test, the gate test, `types.test.ts`, `sources.test.ts`, `InstalledPage.test.tsx`.

**Interfaces:**
- Consumes: `InstalledArtifact.uninstall_blocked: Option<UninstallBlocked>` (`model.rs:209`); `blocked_uninstall` (`plans.rs:103-115`) and its refusal (`:186-188`); `uninstall_blocked_json` (`src-tauri/src/ipc.rs:249-251`, serde spelling); `UninstallBlockedCopy { badge, description, descriptionSourceUnavailable, command, refused }` (`sources.ts:421-443`); `parseUninstallBlocked` (`:490-496`, `hasOwnProperty` over the record); `InstalledPage.tsx:144-146` (badge) and `:328-345` (description through `withCommand`, `wrapDescription`), `:352-370` (button hidden); `UninstallDialog.tsx:102-114` (`refusalText`).
- Produces (verbatim): `UninstallBlocked::NoSafeMethod` (Rust; producer `StandaloneAdapter::inventory`, Task 3, stage 6; readers: the gate, `plan_operation_error`, the pages); `"NoSafeMethod"` in the TS union; `UNINSTALL_BLOCKED_KEYS.NoSafeMethod = { badge: "installed.blocked.NoSafeMethod.badge", description: "installed.blocked.NoSafeMethod.description", descriptionSourceUnavailable: "installed.blocked.NoSafeMethod.description", command: () => "", refused: "installed.blocked.NoSafeMethod.refused" }`; `ADAPTER_LABEL_KEYS["standalone-claude"] = "adapters.standalone-claude"` (reader: the four label lookups listed under "What already exists"; producer of the id: `Session::new`, Task 3, stage 9).

- [ ] **Step 1: Write the failing tests**

In `crates/banager-core/src/model.rs`, inside `test_uninstall_blocked_is_a_bare_string_on_the_wire_and_null_when_absent`, after the final `assert_eq!(serde_json::from_str::<InstalledArtifact>(&json).expect("deserialize"), pinned);` and before the test's closing `}`, insert:

```rust

        // Phase 4: a tool with no uninstall command and no safe way yet
        // to remove its files (Claude Code until step C). Same wire
        // shape, a second spelling for `UNINSTALL_BLOCKED_KEYS` in
        // src/lib/sources.ts.
        let no_safe_method = InstalledArtifact {
            uninstall_blocked: Some(UninstallBlocked::NoSafeMethod),
            ..pinned.clone()
        };
        let json = serde_json::to_string(&no_safe_method).expect("serialize");
        assert!(
            json.contains("\"uninstall_blocked\":\"NoSafeMethod\""),
            "a reason is a bare string on the wire: {json}"
        );
        assert_eq!(
            serde_json::from_str::<InstalledArtifact>(&json).expect("deserialize"),
            no_safe_method
        );
```

In `crates/banager-core/src/session/plans.rs`, after the closing `}` of `test_issue_plan_refuses_an_uninstall_the_tool_will_refuse_for_that_package` (`:923`), insert:

```rust

    #[tokio::test]
    async fn test_issue_plan_refuses_an_uninstall_with_no_safe_method_but_plans_its_upgrade() {
        // `UninstallBlocked::NoSafeMethod`: the tool has no uninstall
        // command and Banager has no safe way yet to remove its files, so
        // its inventory entry carries the refusal (phase 4 step B: Claude
        // Code, whose two-path list is verified but which nothing can move
        // to the Trash until step C). Per package, like `Pinned`: the same
        // tool's upgrade still plans, since `blocked_uninstall` speaks only
        // for `Uninstall`.
        let adapter = FakeAdapter::new(vec![test_support::make_instance("fake", "fake:1")]);
        adapter.set_artifacts(vec![installed_on(
            "fake:1",
            ArtifactKind::Binary,
            "claude",
            Some(UninstallBlocked::NoSafeMethod),
        )]);
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![adapter], None);
        session
            .refresh(&test_support::non_root_env(), &CheckOptions::default())
            .await;

        match session
            .issue_plan(&uninstall_on("fake:1", ArtifactKind::Binary, "claude"))
            .await
        {
            Err(AdapterError::UninstallBlocked { reason }) => {
                assert_eq!(reason, UninstallBlocked::NoSafeMethod);
            }
            other => panic!("expected UninstallBlocked(NoSafeMethod) for claude, got {other:?}"),
        }
        session
            .issue_plan(&upgrade_on("fake:1", ArtifactKind::Binary, "claude"))
            .await
            .expect("no safe uninstall method does not refuse an upgrade");
        assert!(
            session.operations().is_empty(),
            "a refused plan must never reach the OperationManager"
        );
    }
```

In `src/lib/types.test.ts`, replace the body of `it("spells UninstallBlocked as a bare string, and a removable artifact as null", …)` (`:128-135`) with:

```ts
    // `Option<UninstallBlocked>` on `InstalledArtifact.uninstall_blocked`
    // in crates/banager-core/src/model.rs, whose
    // `test_uninstall_blocked_is_a_bare_string_on_the_wire_and_null_when_absent`
    // asserts these exact spellings from the Rust side.
    const reasons: UninstallBlocked[] = ["Pinned", "NoSafeMethod"];
    expect(JSON.stringify(reasons)).toBe('["Pinned","NoSafeMethod"]');
    const removable: UninstallBlocked | null = null;
    expect(roundTrip(removable)).toBeNull();
```

In `src/lib/sources.test.ts`, append inside `describe("UNINSTALL_BLOCKED_KEYS", …)` before its closing `});`:

```ts

  it("carries no command for a tool with no safe uninstall method: there is nothing to run first", () => {
    // Unlike a pin, nothing the user runs can make Banager able to
    // uninstall it; the sentence points at the tool's own instructions
    // and has no `{{command}}` slot, so `withCommand` renders it as plain
    // text and `InstalledPage` sets no `<code>`.
    const claude = instance({
      id: "standalone-claude",
      adapter_id: "standalone-claude",
      exe_path: "/Users/someone/.local/bin/claude",
    });
    const key: ArtifactKey = { instance_id: "standalone-claude", kind: "Binary", name: "claude" };
    expect(UNINSTALL_BLOCKED_KEYS.NoSafeMethod.command(key, claude)).toBe("");
    expect(UNINSTALL_BLOCKED_KEYS.NoSafeMethod.command(key, undefined)).toBe("");
    expect(UNINSTALL_BLOCKED_KEYS.NoSafeMethod.badge).toBe("installed.blocked.NoSafeMethod.badge");
    // Nothing about "when the button comes back" to say differently for a
    // silent source, so one sentence serves both.
    expect(UNINSTALL_BLOCKED_KEYS.NoSafeMethod.descriptionSourceUnavailable).toBe(
      UNINSTALL_BLOCKED_KEYS.NoSafeMethod.description,
    );
  });

  it("names the source and never a command in the no-safe-method sentences, in both locales", () => {
    for (const copy of [
      en.installed.blocked.NoSafeMethod.description,
      en.installed.blocked.NoSafeMethod.refused,
      zhCN.installed.blocked.NoSafeMethod.description,
      zhCN.installed.blocked.NoSafeMethod.refused,
    ]) {
      expect(copy).toContain("{{source}}");
      expect(copy).not.toContain("{{command}}");
    }
    expect(en.adapters["standalone-claude"]).toBe("Claude Code");
    expect(zhCN.adapters["standalone-claude"]).toBe("Claude Code");
  });
```

and, inside `describe("parseUninstallBlocked", …)`'s one `it`, after the line `expect(parseUninstallBlocked('{"kind":"uninstall_blocked","reason":"Pinned"}')).toBe("Pinned");`, insert:

```ts
    expect(parseUninstallBlocked('{"kind":"uninstall_blocked","reason":"NoSafeMethod"}')).toBe(
      "NoSafeMethod",
    );
```

In `src/pages/InstalledPage.test.tsx`, after the closing `});` of `it("promises Uninstall back on a silent source's pinned row only once the source answers", …)` and before `describe("the Update available badge", () => {`, insert:

```tsx
  it("offers no Uninstall on a tool with no safe uninstall method, and says so without a command", async () => {
    // `UninstallBlocked::NoSafeMethod` (phase 4): the tool has no
    // uninstall command and Banager has no safe way yet to remove its
    // files, so the row explains itself in place of its blurb and hides
    // the button -- and, unlike a pin, sets no command as code, because
    // there is nothing to run first. `Session::issue_plan` refuses it in
    // Rust too.
    const claudeSnapshot: Snapshot = {
      ...snapshot,
      instances: [
        {
          id: "standalone-claude",
          adapter_id: "standalone-claude",
          exe_path: "/Users/someone/.local/bin/claude",
          prefix: "/Users/someone/.local/share/claude",
          scope: "User",
          version: "2.1.281",
          status: { unavailable: null, notes: [] },
          unverified_version: null,
          read_only_reason: null,
        },
      ],
      artifacts: [
        {
          key: { instance_id: "standalone-claude", kind: "Binary", name: "claude" },
          display_name: "Claude Code",
          version: "2.1.281",
          reason: "Requested",
          description: null,
          homepage: "https://code.claude.com/docs/en/setup",
          size_bytes: null,
          installed_at: null,
          path: "/Users/someone/.local/share/claude/versions/2.1.281",
          auto_updates: true,
          uninstall_blocked: "NoSafeMethod",
        },
      ],
      updates: [],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(claudeSnapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      return Promise.resolve(undefined);
    });

    const { findByText, getByText, queryAllByRole, container } = renderWithProviders(
      <InstalledPage />,
    );

    await findByText("Can't uninstall here");
    expect(queryAllByRole("button", { name: "Uninstall" })).toHaveLength(0);
    expect(
      getByText(
        "Claude Code has no uninstall command, and Banager can't yet move its files to the Trash safely, so it doesn't offer to. The official instructions are on its website.",
      ),
    ).toBeInTheDocument();
    expect(container.querySelector("code")).toBeNull();
  });

```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p banager-core --lib model::tests::test_uninstall_blocked_is_a_bare_string_on_the_wire_and_null_when_absent`
Expected: FAIL to compile — `no variant or associated item named \`NoSafeMethod\` found for enum \`UninstallBlocked\`` (here and in `plans.rs`).

Run: `pnpm typecheck`
Expected: FAIL — `Type '"NoSafeMethod"' is not assignable to type 'UninstallBlocked'` (`types.test.ts`), `Property 'NoSafeMethod' does not exist on type 'Record<"Pinned", UninstallBlockedCopy>'` (`sources.test.ts`), `Property 'standalone-claude' does not exist` on `en.adapters` (`sources.test.ts`), and in `InstalledPage.test.tsx` the `uninstall_blocked: "NoSafeMethod"` literal.

- [ ] **Step 3: Add the variant, the mirror, the copy record and the label**

In `crates/banager-core/src/model.rs`, replace the `UninstallBlocked` enum (`:220-233`) with:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UninstallBlocked {
    /// `brew pin`, for a formula or a cask. Without `--force`, which
    /// Banager never passes, `brew uninstall` prints "Error: <name> is
    /// pinned. You must unpin it to uninstall." and skips it
    /// (`uninstall.rb:48-49`, `cask/uninstall.rb:40-44` in Homebrew 7.0.6).
    /// For a formula it still exits 0, because that message goes through
    /// `onoe`, not `ofail`, so without this Banager ran the command and
    /// then reported `StillInstalledAfterUninstall`. Read by
    /// `parse_info_installed` in `adapters/brew/parse.rs`, from the
    /// `pinned` key `brew info --installed --json=v2` writes for every
    /// formula (`formula.rb:3140`) and cask (`cask/cask.rb:574`).
    Pinned,
    /// The tool has no uninstall command, and Banager has no safe way yet
    /// to remove its files -- no verified list of them (a second-batch
    /// tool before verification), or a verified list but not yet the
    /// path-list uninstall that moves them to the Trash (Claude Code
    /// until step C) -- so it does not offer to. Per artifact, not the
    /// instance's `read_only_reason`: that would hide the upgrade too,
    /// which works. Produced by `StandaloneAdapter::inventory`
    /// (`adapters/standalone/mod.rs`) for a recipe without an uninstall
    /// method -- Claude Code in phase 4 step B, until step C's path-list
    /// uninstall replaces it; later Ollama.app. The gate refuses it
    /// (`blocked_uninstall` in session/plans.rs), the Installed page hides
    /// the button and says why (`UNINSTALL_BLOCKED_KEYS` in
    /// src/lib/sources.ts).
    NoSafeMethod,
}
```

In `src/lib/types.ts`, replace the `UninstallBlocked` doc comment and line with:

```ts
/**
 * Why the tool itself will refuse to uninstall this one package. Mirrors
 * `UninstallBlocked` in crates/banager-core/src/model.rs: bare-string unit
 * variants. `Pinned` is produced by brew's `parse_info_installed` (from
 * `brew info --installed --json=v2`'s `pinned: true`); `NoSafeMethod` by
 * the standalone adapter's inventory for a tool with no uninstall command
 * and no safe way yet to remove its files (Claude Code, phase 4 step B,
 * until step C). Read through
 * `UNINSTALL_BLOCKED_KEYS` in src/lib/sources.ts, a `Record` over this
 * union, so a variant added here without copy fails `tsc`.
 */
export type UninstallBlocked = "Pinned" | "NoSafeMethod";
```

In `src/lib/sources.ts`, replace `ADAPTER_LABEL_KEYS` (`:17-26`) with:

```ts
/** i18n key holding each adapter's human name. The `standalone-*` ids are
 *  the tools with their own installer (`standalone::all` in
 *  crates/banager-core/src/adapters/standalone/mod.rs), one per recipe. */
export const ADAPTER_LABEL_KEYS: Record<string, string> = {
  brew: "adapters.brew",
  npm: "adapters.npm",
  pipx: "adapters.pipx",
  uv: "adapters.uv",
  pip: "adapters.pip",
  cargo: "adapters.cargo",
  ollama: "adapters.ollama",
  "standalone-claude": "adapters.standalone-claude",
};
```

and, inside `UNINSTALL_BLOCKED_KEYS` (`:451-475`), after the `Pinned: { … },` entry and before the closing `};`, insert:

```ts
  NoSafeMethod: {
    badge: "installed.blocked.NoSafeMethod.badge",
    // No command: unlike a pin there is nothing the user can run to make
    // Banager able to uninstall it, so the sentence has no `{{command}}`
    // slot and `withCommand` returns it as plain text. It promises
    // nothing about when a button returns, so a silent source gets the
    // same sentence rather than a second key with the same words.
    description: "installed.blocked.NoSafeMethod.description",
    descriptionSourceUnavailable: "installed.blocked.NoSafeMethod.description",
    command: () => "",
    refused: "installed.blocked.NoSafeMethod.refused",
  },
```

In `src/i18n/en.json`: in `"adapters"`, after `"ollama": "Ollama"` add `,` and

```json
    "standalone-claude": "Claude Code"
```

and in `"installed"` → `"blocked"`, after the `"Pinned": { … }` block add `,` and

```json
      "NoSafeMethod": {
        "badge": "Can't uninstall here",
        "description": "{{source}} has no uninstall command, and Banager can't yet move its files to the Trash safely, so it doesn't offer to. The official instructions are on its website.",
        "refused": "Banager can't uninstall {{source}} yet, so it didn't. Nothing has been changed."
      }
```

(Not spec §9.2's sentence, which says "Banager doesn't yet have a verified list of the files it would need to remove": for Claude Code the two-path list *is* verified (claude.md §7), and what this step lacks is step C's way of moving them to the Trash. The sentence above is true for both cases §6.1's "Neither" covers -- a second-batch tool before its list is verified, and Claude Code before C -- and keeps `{{source}}` and no `{{command}}`, so the tests above hold. Ruling 15.)

In `src/i18n/zh-CN.json`, the same two positions:

```json
    "standalone-claude": "Claude Code"
```

```json
      "NoSafeMethod": {
        "badge": "无法在这里卸载",
        "description": "{{source}} 没有卸载命令，Banager 也还不能把它的文件安全地移到废纸篓，所以不提供卸载。官方说明在它的网站上。",
        "refused": "Banager 还不能卸载 {{source}}，所以没有动。什么都没有改动。"
      }
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p banager-core --lib model::tests` and `cargo test -p banager-core --lib session::plans::tests::test_issue_plan_refuses_an_uninstall_with_no_safe_method_but_plans_its_upgrade` and `pnpm typecheck && pnpm exec vitest run src/lib src/pages/InstalledPage.test.tsx src/i18n`
Expected: PASS. (`src-tauri`'s `plan_operation_error` needs no change: `uninstall_blocked_json` serialises the reason with serde, so the wire carries `"NoSafeMethod"`, which `parseUninstallBlocked` now recognises through the record.)

- [ ] **Step 5: Run the gates**

Run all five from Global Constraints. Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/model.rs crates/banager-core/src/session/plans.rs src/lib/types.ts src/lib/types.test.ts src/lib/sources.ts src/lib/sources.test.ts src/pages/InstalledPage.test.tsx src/i18n/en.json src/i18n/zh-CN.json
git commit -m "$(cat <<'EOF'
Add NoSafeMethod, the uninstall refusal for a tool with no verified way out

A tool with no uninstall command and no safe way yet to remove its
files cannot be uninstalled from Banager; the honest row says so and
offers no button, and the gate in Session::issue_plan refuses it as it
refuses a pinned package. Per artifact, not the instance's read-only
reason, so the same tool's upgrade still works. The first producer is
the standalone adapter's inventory for Claude Code, which follows; the
label for that source lands with the copy so the sentence names it.

Co-Authored-By: Codex <noreply@openai.com>
EOF
)"
```

---

### Task 3: Complete standalone core, recording and registration

Stages 3–9 below replace the former independently committed Tasks 3–9. They are **one implementation task and one commit**: the recipe fields and variants, route result, `Detected.home`, HTTP client, `all()` and allowlisted host all acquire their production readers before this task is complete. The stable stage numbers preserve the review's anchors. Run focused red/green checks while building; run the five gates only once the complete core and registration are present. No stubs and no temporary `allow(dead_code)`.

#### Stage 3: `recipe.rs` types and `latest.rs` pure functions

**Files:**
- Create: `crates/banager-core/src/adapters/standalone/mod.rs` (module doc and the two `pub mod` lines only; the adapter itself is Task 3, stage 6)
- Create: `crates/banager-core/src/adapters/standalone/recipe.rs`
- Create: `crates/banager-core/src/adapters/standalone/latest.rs`
- Modify: `crates/banager-core/src/adapters/mod.rs:15-21` (the `pub mod` list)
- Test: `latest.rs`'s `#[cfg(test)] mod tests`.

**Interfaces:**
- Consumes: `CancelPolicy` (`model.rs:355-370`); `serde_json::Value` (already a dependency).
- Produces (verbatim, from Core Interfaces): `Recipe`, `Route`, `RouteKind::SymlinkIntoRoot`, `VersionCmd`, `VersionParse::FirstToken`, `Latest::ClaudeChannel { base }`, `UpgradeCmd`; `is_dotted_version`, `parse_version`, `compare_dotted`, `CHANNEL_LATEST`, `CHANNEL_STABLE`, `claude_channel_from_json`, `claude_channel`, `parse_channel_body`. Readers: `recipes.rs` (Task 3, stage 4, the data), `route::probe` (Task 3, stage 5, `RouteKind`), `StandaloneAdapter::detect`/`inventory` (Task 3, stage 6, `VersionCmd`, `parse_version`), `check_updates` (Task 3, stage 7, `Latest`, `compare_dotted`, the channel functions), `plan` (Task 3, stage 8, `UpgradeCmd`). Each field's doc names its reader.

The submodules are public for the integration test's imports. All fields acquire production readers in this core task; public visibility is not a way to bypass the reader requirement.

- [ ] **Step 1: Write the failing tests**

Create `crates/banager-core/src/adapters/standalone/latest.rs` with only the test module for now (Step 3 adds the functions above it):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn test_is_dotted_version_accepts_integers_joined_by_dots_and_nothing_else() {
        for ok in ["2.1.281", "1.0.41", "2026.9.12", "7", "0.0.0"] {
            assert!(is_dotted_version(ok), "{ok:?} is a dotted version");
        }
        for bad in ["", "v2.1.281", "2.1.281-beta", "2..1", ".1", "1.", "abc", "2.1.281 (Claude Code)", "１.2"] {
            assert!(!is_dotted_version(bad), "{bad:?} is not a dotted version");
        }
    }

    #[test]
    fn test_parse_version_reads_the_first_token_of_claudes_recorded_line() {
        // `claude --version` on this Mac, 2026-09-24 (claude.md §1,
        // VERIFIED): `2.1.281 (Claude Code)`. The fixture file arrives with
        // Task 3, stage 9; the shape is pinned here from the research record.
        assert_eq!(
            parse_version("2.1.281 (Claude Code)\n", VersionParse::FirstToken),
            Some("2.1.281".to_string())
        );
    }

    #[test]
    fn test_parse_version_skips_leading_blank_lines_and_trailing_space() {
        // A tool that prints a blank line first, or a version with a
        // trailing tab, still has a version: the alternative is a working
        // install shown as "not responding".
        assert_eq!(
            parse_version("\n\n2.1.281 (Claude Code)  \n", VersionParse::FirstToken),
            Some("2.1.281".to_string())
        );
        assert_eq!(
            parse_version("2.1.281\t\n", VersionParse::FirstToken),
            Some("2.1.281".to_string())
        );
    }

    #[test]
    fn test_parse_version_is_none_only_when_no_token_is_available() {
        for stdout in ["", "\n", " \t\n"] {
            assert_eq!(
                parse_version(stdout, VersionParse::FirstToken),
                None,
                "{stdout:?}"
            );
        }
    }

    #[test]
    fn test_version_extraction_preserves_the_complete_token() {
        for token in ["2.1.281-beta", "2.1.281+build.7", "v2.1.281", "abc"] {
            assert_eq!(
                parse_version(&format!("{token} (Claude Code)\n"), VersionParse::FirstToken),
                Some(token.to_string())
            );
            assert_eq!(parse_channel_body(&format!(" {token}\n")), Ok(token.to_string()));
            assert_eq!(compare_dotted(token, "2.1.290"), None);
            assert_eq!(compare_dotted("2.1.281", token), None);
        }
    }

    #[test]
    fn test_compare_dotted_compares_integers_component_by_component() {
        // Spec §4.3's table, plus the two shapes a string comparison gets
        // wrong: `9` vs `12`, and a shorter version against a longer one.
        for (local, remote, expected) in [
            ("2.1.273", "2.1.281", Ordering::Less),
            ("2.1.281", "2.1.273", Ordering::Greater),
            ("1.0.41", "1.0.41", Ordering::Equal),
            ("2026.9.9", "2026.9.12", Ordering::Less),
            ("1.2.9", "1.2.10", Ordering::Less),
            ("1.2.10", "1.2.9", Ordering::Greater),
            ("1.2", "1.2.0", Ordering::Less),
            ("2.01", "2.1", Ordering::Equal),
        ] {
            assert_eq!(
                compare_dotted(local, remote),
                Some(expected),
                "{local} vs {remote}"
            );
        }
    }

    #[test]
    fn test_compare_dotted_is_none_when_either_side_is_not_a_version() {
        assert_eq!(compare_dotted("abc", "2.1.281"), None);
        assert_eq!(compare_dotted("2.1.281", "latest"), None);
        assert_eq!(compare_dotted("", ""), None);
        // A component too large for an integer is not a version Banager
        // will reason about either.
        assert_eq!(compare_dotted("1.99999999999999999999999", "2"), None);
    }

    #[test]
    fn test_claude_channel_from_json_reads_stable_and_defaults_to_latest() {
        // The two documented values (claude.md §5, VERIFIED): "latest"
        // (the default) and "stable".
        assert_eq!(
            claude_channel_from_json(r#"{"autoUpdatesChannel":"stable"}"#),
            CHANNEL_STABLE
        );
        assert_eq!(
            claude_channel_from_json(r#"{"autoUpdatesChannel":"latest"}"#),
            CHANNEL_LATEST
        );
        assert_eq!(
            claude_channel_from_json(r#"{"model":"opus","permissions":{}}"#),
            CHANNEL_LATEST
        );
    }

    #[test]
    fn test_claude_channel_from_json_defaults_to_latest_for_anything_but_stable() {
        // Malformed JSON, a value that is not a string, an unknown channel
        // name, a different case: never a guess, never an error -- the
        // failure mode of a wrong guess is a harmless `UnchangedAfterUpgrade`
        // (spec §3.1), and the default is what a fresh install has.
        for json in [
            "",
            "{",
            "[]",
            r#"{"autoUpdatesChannel":1}"#,
            r#"{"autoUpdatesChannel":null}"#,
            r#"{"autoUpdatesChannel":"Stable"}"#,
            r#"{"autoUpdatesChannel":"nightly"}"#,
        ] {
            assert_eq!(claude_channel_from_json(json), CHANNEL_LATEST, "{json:?}");
        }
    }

    #[test]
    fn test_claude_channel_reads_the_settings_file_under_home_and_defaults_when_absent() {
        let home = std::env::temp_dir().join(format!(
            "banager-standalone-channel-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(home.join(".claude")).expect("create .claude");
        assert_eq!(claude_channel(&home), CHANNEL_LATEST, "no settings.json yet");
        std::fs::write(
            home.join(".claude/settings.json"),
            r#"{"autoUpdatesChannel":"stable"}"#,
        )
        .expect("write settings.json");
        assert_eq!(claude_channel(&home), CHANNEL_STABLE);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn test_parse_channel_body_trims_and_accepts_a_bare_version() {
        // `curl -sS https://downloads.claude.ai/claude-code-releases/latest`
        // answers a bare version (claude.md §4, VERIFIED); whether or not it
        // ends in a newline must not matter.
        assert_eq!(parse_channel_body("2.1.281"), Ok("2.1.281".to_string()));
        assert_eq!(parse_channel_body("2.1.273\n"), Ok("2.1.273".to_string()));
        assert_eq!(parse_channel_body("  2.1.281 \r\n"), Ok("2.1.281".to_string()));
    }

    #[test]
    fn test_parse_channel_body_refuses_anything_that_is_not_a_version() {
        // A multi-token HTML error page or an empty body: an uncheckable
        // row with the reason, never a candidate built from it. The reason
        // quotes at most a few characters of the body, so a page of HTML
        // does not become the row's description.
        for body in ["", " \n", "<html><body>503 Service Unavailable</body></html>", "2.1.281 2.1.290"] {
            let err = parse_channel_body(body).expect_err(body);
            assert!(err.contains("did not answer with a version"), "{err}");
            assert!(err.len() < 120, "the reason stays short: {err}");
        }
    }
}
```

Create `crates/banager-core/src/adapters/standalone/mod.rs`:

```rust
//! Tools installed by their own installer rather than by a package
//! manager -- Claude Code's native install in this step; Antigravity CLI,
//! Grok Build and rustup in the steps after it (phase 4 spec, §一 D1-D7).
//!
//! One type, `StandaloneAdapter`, driven by one `&'static Recipe` per tool
//! and registered once per tool under the adapter id `standalone-<tool>`,
//! so everything keyed by adapter id today -- the front end's labels,
//! `verified_versions`, the fixture directory, the concurrent detect
//! fan-out -- works for each tool without a special case, and the logic
//! exists once. The instance *is* the native install: `exe_path` is the
//! launcher the installer wrote (`~/.local/bin/claude`, a symlink),
//! `prefix` the tool's own root, and the one artifact under it is the
//! tool itself (`ArtifactKind::Binary`).
//!
//! `recipe` holds the shape of a tool, `recipes` the tools, `route`
//! answers "is this launcher this route's, and which copy runs when its
//! name is typed", `latest` parses and compares versions.

pub mod latest;
pub mod recipe;
```

In `crates/banager-core/src/adapters/mod.rs`, change the module list

```rust
pub mod pip;
pub mod pipx;
pub mod uv;
```

to

```rust
pub mod pip;
pub mod pipx;
pub mod standalone;
pub mod uv;
```

Create `crates/banager-core/src/adapters/standalone/recipe.rs` (the types are needed for the tests to compile at all, so they are written in this step; there is nothing to test in a type with no behaviour):

```rust
//! One tool = one `Recipe`: all `'static` data, no trait objects, one
//! table to read. Every field's doc names the reader that consumes it; a
//! field without a reader is not added (phase 4 spec §3.1, §十).
//!
//! Only the shapes this step produces exist here. Step C adds the
//! uninstall method (`uninstall: Option<Uninstall>`), step D `backup_globs`,
//! a `FlatFile` route, a `SecondToken` version parse, the other `Latest`
//! sources and an optional `upgrade` (agy updates itself only), step E
//! `$CARGO_HOME` paths. A variant or field defined before anything
//! produces it is this project's most common defect (spec §十三 #41).

use crate::model::CancelPolicy;

/// A tool installed by its own installer, as data.
#[derive(Debug)]
pub struct Recipe {
    /// `"claude"`. The adapter id is `standalone-{id}`; `ArtifactKey.name`
    /// is this; and it is the command the user types, which
    /// `route::shadow_note` resolves on `PATH`. Every first-batch tool's
    /// command name equals its id; a `binary` field arrives with the first
    /// tool whose does not. Read by `StandaloneAdapter::new`, `detect`,
    /// `inventory`, `plan`.
    pub id: &'static str,
    /// `include_str!` of `adapters/meta/standalone-<id>.toml`, parsed by
    /// `AdapterMeta::from_toml` in `StandaloneAdapter::new`. The display
    /// name and homepage are `meta.name` / `meta.homepage`, never a second
    /// copy here (spec §十三 #45).
    pub meta_toml: &'static str,
    /// Where the installer puts the launcher and the tool's root. Read by
    /// `detect` (expanded against `HostEnv.home`) and, through the
    /// instance's `exe_path`/`prefix`, by `inventory`.
    pub route: Route,
    /// How the installed version is read. Read by `detect` and
    /// `inventory` (and so by `reconcile`).
    pub version: VersionCmd,
    /// Where the newest published version comes from. Read by
    /// `check_updates`.
    pub latest: Latest,
    /// Whether the tool updates itself in the background when its own
    /// updater is on (claude: yes, VERIFIED in claude.md §5). Read by
    /// `inventory`, into `InstalledArtifact.auto_updates`, whose reader is
    /// the Updates page's `selfUpdatingHint` sentence.
    pub self_updates: bool,
    /// The tool's own documented update command. Read by `plan(Upgrade)`.
    pub upgrade: UpgradeCmd,
}

/// The installer's fixed paths. Every path starts with `~/` and is expanded
/// by `route::expand` against `HostEnv.home` -- never `std::env::var("HOME")`,
/// which a Finder-launched app cannot be tested against (spec §3.1).
/// `recipes::tests::test_every_recipe_path_is_under_home` holds every
/// recipe to that.
#[derive(Debug)]
pub struct Route {
    pub kind: RouteKind,
    /// The launcher: `~/.local/bin/claude`. The instance's `exe_path` and
    /// the program every plan runs.
    pub launcher: &'static str,
    /// The tool's own root: `~/.local/share/claude`. The instance's
    /// `prefix`; what the launcher must resolve into.
    pub root: &'static str,
}

/// How a launcher is recognised as this route's and not a package
/// manager's copy. Read by `route::probe`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteKind {
    /// The launcher is a symbolic link that, fully resolved, lands under
    /// `root` (claude: `~/.local/bin/claude` → `~/.local/share/claude/
    /// versions/<v>`, VERIFIED on this Mac). Dangling, the link's own text
    /// decides, lexically normalised, whether this is the half-uninstalled
    /// `LauncherOnly` state (spec §3.3 step 2).
    SymlinkIntoRoot,
}

/// The read-only version command, run against the launcher.
#[derive(Debug)]
pub struct VersionCmd {
    pub args: &'static [&'static str],
    /// Environment added to the version read only -- never to the upgrade
    /// plan: Claude Code is documented to check for updates on startup and
    /// `DISABLE_AUTOUPDATER=1` to stop only that background check, so it
    /// goes on every version read whether or not a bare `--version` would
    /// reach the updater (not observed; spec §3.4). Upgrade adds no
    /// override; the runner still inherits ambient environment. Manual
    /// `claude update` is documented to work with this variable set.
    pub env: &'static [(&'static str, &'static str)],
    pub parse: VersionParse,
}

/// Which token of the version command's output is the version. Read by
/// `latest::parse_version`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionParse {
    /// The first whitespace-separated token of the first non-empty line:
    /// `2.1.281 (Claude Code)` → `2.1.281`.
    FirstToken,
}

/// Where the newest published version is read from. Only VERIFIED
/// endpoints (spec D4); every host here is on `ALLOWED_HTTPS_HOSTS`
/// (`recipes::tests::test_every_recipe_latest_url_is_an_allowed_https_host`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Latest {
    /// Claude Code only: `GET {base}/{channel}`, where `channel` is
    /// `latest` or `stable` as `~/.claude/settings.json`'s
    /// `autoUpdatesChannel` says (`latest::claude_channel`; anything but
    /// `"stable"` is `latest`). Both pointers answer one bare version
    /// (claude.md §4, VERIFIED). That the channel setting maps onto these
    /// two pointer URLs is inferred, not decompiled (claude.md §4,
    /// UNVERIFIED): a wrong inference costs a `stable` user a `latest`
    /// badge whose `claude update` then reports up to date --
    /// `UnchangedAfterUpgrade`, which is the truth. Tool-specific on
    /// purpose: one tool needs it, and a generic "read a JSON key" source
    /// would be a mechanism with one user.
    ClaudeChannel { base: &'static str },
}

/// The tool's own update command, run against the launcher through
/// `run_plan`. Read by `plan(Upgrade)`.
#[derive(Debug)]
pub struct UpgradeCmd {
    pub args: &'static [&'static str],
    pub timeout_secs: u64,
    pub cancel: CancelPolicy,
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib adapters::standalone::latest`
Expected: FAIL to compile — `error[E0425]: cannot find function \`is_dotted_version\` in this scope` (and `parse_version`, `compare_dotted`, `claude_channel_from_json`, `claude_channel`, `parse_channel_body`; `error[E0425]: cannot find value \`CHANNEL_STABLE\``, `CHANNEL_LATEST`; `error[E0433]` for `VersionParse` until `use super::recipe::VersionParse;` exists).

- [ ] **Step 3: Write the functions**

Prepend to `crates/banager-core/src/adapters/standalone/latest.rs` (above `#[cfg(test)]`):

```rust
//! Versions: the installed one out of a `--version` line, the published
//! one out of an endpoint's body, and how the two compare.
//!
//! Dotted integers, compared component by component. The existing
//! adapters compare with `!=`, because a package registry never reports a
//! version older than the installed one; a standalone tool's channel
//! pointer can (Claude Code's `stable` pointer was 2.1.273 while the
//! installed `latest` was 2.1.281, recorded in
//! `adapters/fixtures/standalone-claude/`), so only `remote > local` is an
//! update (phase 4 spec §4.3). No `semver` crate: these tools' versions
//! can include suffixes; those remain intact and uncheckable. Calver (`2026.9.12`) is right under this
//! rule where a string comparison is wrong.

use super::recipe::VersionParse;
use std::cmp::Ordering;
use std::path::Path;

/// `^\d+(\.\d+)*$`, by hand (the crate's `validate_package_name` sets the
/// precedent for not pulling in `regex` for one pattern).
pub fn is_dotted_version(s: &str) -> bool {
    !s.is_empty()
        && s.split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

/// The installed version out of a version command's stdout, per `parse`,
/// retaining the complete token, including prerelease/build suffixes.
/// Only an absent token is `None` (`NotResponding` in detect); only
/// `compare_dotted` decides comparability. Read the first non-empty line.
/// The command's output contract chooses the token; extraction does not
/// silently truncate or reinterpret a version it cannot compare.
pub fn parse_version(stdout: &str, parse: VersionParse) -> Option<String> {
    let line = stdout.lines().find(|line| !line.trim().is_empty())?;
    let token = match parse {
        VersionParse::FirstToken => line.split_whitespace().next()?,
    };
    Some(token.to_string())
}

/// `local` against `remote` as sequences of integers, shorter-is-less
/// (`1.2` < `1.2.0`); `None` when either is not a dotted version or has a
/// component too large to be an integer, which `check_updates` reports as
/// an uncheckable row naming both strings.
pub fn compare_dotted(local: &str, remote: &str) -> Option<Ordering> {
    fn components(s: &str) -> Option<Vec<u64>> {
        if !is_dotted_version(s) {
            return None;
        }
        s.split('.').map(|part| part.parse::<u64>().ok()).collect()
    }
    Some(components(local)?.cmp(&components(remote)?))
}

/// Claude Code's two release channels, as `autoUpdatesChannel` names them
/// and as the pointer URLs are spelled (claude.md §4, §5: VERIFIED for the
/// setting's values and for both URLs answering).
pub const CHANNEL_LATEST: &str = "latest";
pub const CHANNEL_STABLE: &str = "stable";

/// The channel out of `~/.claude/settings.json`'s text: `stable` when the
/// key `autoUpdatesChannel` is exactly the string `"stable"`, `latest` for
/// everything else -- a missing key, a value of another type, an unknown
/// name, or JSON that does not parse. Never an error: the default is what
/// a fresh install has, and a wrong channel costs a badge whose update
/// then reports `UnchangedAfterUpgrade` (spec §3.1).
pub fn claude_channel_from_json(json: &str) -> &'static str {
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(value)
            if value
                .get("autoUpdatesChannel")
                .and_then(serde_json::Value::as_str)
                == Some(CHANNEL_STABLE) =>
        {
            CHANNEL_STABLE
        }
        _ => CHANNEL_LATEST,
    }
}

/// `claude_channel_from_json` over `<home>/.claude/settings.json`, the one
/// file Banager reads for Claude Code (`docs/what-we-run.md`, "Files
/// Banager reads"): read-only, and `latest` when it cannot be read.
pub fn claude_channel(home: &Path) -> &'static str {
    match std::fs::read_to_string(home.join(".claude").join("settings.json")) {
        Ok(json) => claude_channel_from_json(&json),
        Err(_) => CHANNEL_LATEST,
    }
}

/// The version a channel pointer answered with, trimmed; `Err` with a
/// short reason for a body that is not one version, which becomes an
/// uncheckable row's description -- so the reason quotes only the first
/// few characters, never a page of HTML.
pub fn parse_channel_body(body: &str) -> Result<String, String> {
    let trimmed = body.trim();
    if !trimmed.is_empty() && !trimmed.chars().any(char::is_whitespace) {
        return Ok(trimmed.to_string());
    }
    let shown: String = trimmed.chars().take(40).collect();
    Err(format!(
        "the channel endpoint did not answer with a version (got {shown:?})"
    ))
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib adapters::standalone::latest`
Expected: PASS — all parser, comparison and channel tests, including intact unsupported tokens.

- [ ] **Step 5: Continue the core task**

Continue to the next stage. The complete core task runs `cargo fmt --all` and all five gates, then commits once, after stage 9. No partial type-only commit or dead-code exemption.

---

#### Stage 4: The `CLAUDE` recipe, its meta TOML, `route::expand`, the invariants tests

**Files:**
- Create: `adapters/meta/standalone-claude.toml`
- Create: `crates/banager-core/src/adapters/standalone/recipes.rs`
- Create: `crates/banager-core/src/adapters/standalone/route.rs` (`expand` only; `probe`/`shadow_note` are Task 3, stage 5)
- Modify: `crates/banager-core/src/adapters/standalone/mod.rs` (add `pub mod recipes;` and `pub mod route;`)
- Test: `recipes.rs`'s `#[cfg(test)] mod tests`, `route.rs`'s.

**Interfaces:**
- Consumes: the Task 3, stage 3 types; `AdapterMeta::from_toml` (`adapters/mod.rs:94-96`); `CancelPolicy::KillThenReconcile`.
- Produces (verbatim): `pub static CLAUDE: Recipe`, `pub static RECIPES: &[&Recipe]` (readers: `StandaloneAdapter::new`/`all()`, Task 3, stages 6 and 8; the invariants tests here); `pub fn expand(home: &Path, spec: &str) -> PathBuf` (reader: `detect`, Task 3, stage 6); `adapters/meta/standalone-claude.toml` (readers: `CLAUDE.meta_toml` via `include_str!`; A's `what_we_run_test` once the id is registered, Task 3, stage 9).

The recipe's every value and its source (claude.md, VERIFIED unless said): launcher `~/.local/bin/claude`, a symlink → `~/.local/share/claude/versions/2.1.281` (§2a, `ls -la`); root `~/.local/share/claude` (§2a); `--version` → `2.1.281 (Claude Code)` (§1); Anthropic documents that Claude Code checks for updates on startup, and `DISABLE_AUTOUPDATER=1` as stopping only that background check (§5, doc text) — whether a bare `--version` reaches the updater was not observed (the agy research found the opposite for that tool, spec §3.4), so the variable goes on every version read regardless; channel base `https://downloads.claude.ai/claude-code-releases`, `/latest` → `2.1.281`, `/stable` → `2.1.273`, both 200 without redirect (§4, and re-checked read-only on 2026-09-24 while writing this plan); self-updates in the background (§5, doc text); `claude update` is the documented updater, aliases `upgrade`, no options (§6); 1800 s is spec §4.1's install/upgrade budget and the binary is about 220 MB; `KillThenReconcile` stops the command and then reads current state, but a stopped upgrade stays `Unconfirmed` regardless of that reading: install.sh (read directly, §4) downloads each version to a new file under `versions/` and re-points the link only afterwards, but `claude update` is a compiled program whose steps were not read (§6 "No direct official statement", §8 open item), so Banager assumes nothing about interruption, the preview claims nothing, and the reading after (`--version`) gates success after exit 0; cancellation or timeout remains `Unconfirmed` as for every upgrade (spec §五 and `ops/mod.rs`).

- [ ] **Step 1: Write the failing tests**

Create `crates/banager-core/src/adapters/standalone/recipes.rs` with the test module only (Step 3 adds the data above it):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::AdapterMeta;
    use crate::model::CancelPolicy;
    use std::path::Path;

    #[test]
    fn test_every_recipe_path_is_under_home() {
        // `route::expand` joins a `~/` path onto `HostEnv.home` and nothing
        // else: a recipe path that does not start that way is a programming
        // error this test turns into a red build, not a runtime surprise.
        // (`$CARGO_HOME/` joins with rustup, step E.)
        for recipe in RECIPES {
            for path in [recipe.route.launcher, recipe.route.root] {
                assert!(
                    path.starts_with("~/"),
                    "{}: recipe path {path:?} must start with ~/",
                    recipe.id
                );
                assert!(
                    !path.contains("/../") && !path.ends_with("/.."),
                    "{}: recipe path {path:?} must not climb",
                    recipe.id
                );
            }
        }
    }

    #[test]
    fn test_every_recipe_launcher_is_named_after_its_id() {
        // `id` is the command the user types and the launcher's file name
        // (spec §3.1); a `binary` field arrives with the first tool where
        // the two differ.
        for recipe in RECIPES {
            assert_eq!(
                Path::new(recipe.route.launcher).file_name().and_then(|n| n.to_str()),
                Some(recipe.id),
                "{}: launcher {:?} must be named after the id",
                recipe.id,
                recipe.route.launcher
            );
        }
    }

    #[test]
    fn test_every_recipe_meta_parses_and_names_the_standalone_id() {
        for recipe in RECIPES {
            let meta = AdapterMeta::from_toml(recipe.meta_toml)
                .unwrap_or_else(|e| panic!("{}: meta toml: {e}", recipe.id));
            assert_eq!(meta.id, format!("standalone-{}", recipe.id));
            assert!(!meta.id.contains(':'), "Session::build asserts no ':'");
            assert_eq!(meta.kind, "standalone");
            assert!(!meta.name.is_empty());
            assert!(meta.homepage.starts_with("https://"));
            assert!(
                !meta.verified_versions.is_empty(),
                "{}: a recorded fixture backs verified_versions",
                recipe.id
            );
        }
    }

    #[test]
    fn test_claude_is_the_native_route_read_with_its_autoupdater_off() {
        assert_eq!(CLAUDE.id, "claude");
        assert_eq!(CLAUDE.route.kind, RouteKind::SymlinkIntoRoot);
        assert_eq!(CLAUDE.route.launcher, "~/.local/bin/claude");
        assert_eq!(CLAUDE.route.root, "~/.local/share/claude");
        assert_eq!(CLAUDE.version.args, &["--version"]);
        // Spec §3.4: the version read must not start a background update
        // check; the upgrade plan (Task 3, stage 8) must not carry this.
        assert_eq!(CLAUDE.version.env, &[("DISABLE_AUTOUPDATER", "1")]);
        assert_eq!(CLAUDE.version.parse, VersionParse::FirstToken);
        assert!(CLAUDE.self_updates);
    }

    #[test]
    fn test_claude_updates_with_its_own_updater() {
        assert_eq!(CLAUDE.upgrade.args, &["update"]);
        assert_eq!(CLAUDE.upgrade.timeout_secs, 1800);
        assert_eq!(CLAUDE.upgrade.cancel, CancelPolicy::KillThenReconcile);
        assert_eq!(
            CLAUDE.latest,
            Latest::ClaudeChannel {
                base: "https://downloads.claude.ai/claude-code-releases"
            }
        );
    }

    #[test]
    fn test_recipes_lists_claude_once() {
        assert_eq!(RECIPES.len(), 1);
        assert!(std::ptr::eq(RECIPES[0], &CLAUDE));
    }
}
```

Create `crates/banager-core/src/adapters/standalone/route.rs` with its test module only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // Named here as well as through `super::*`: in this step the file has
    // no imports of its own yet, and an explicit import beside a glob is
    // not a warning once it does.
    use std::path::{Path, PathBuf};

    #[test]
    fn test_expand_joins_a_tilde_path_onto_home() {
        let home = Path::new("/Users/someone");
        assert_eq!(
            expand(home, "~/.local/bin/claude"),
            PathBuf::from("/Users/someone/.local/bin/claude")
        );
        assert_eq!(
            expand(home, "~/.local/share/claude"),
            PathBuf::from("/Users/someone/.local/share/claude")
        );
    }

    #[test]
    fn test_expand_keeps_the_spelling_of_home_it_was_given() {
        // `HostEnv.home` is whatever `HOME` says; nothing here
        // canonicalises it. The Unknown page's rule 0 compares an
        // instance's raw `exe_path` with the raw directory entry it found
        // (scan/mod.rs), so the launcher path must be built from the same
        // spelling the scan uses.
        let home = Path::new("/Volumes/Data/homes/someone");
        assert_eq!(
            expand(home, "~/.local/bin/claude"),
            PathBuf::from("/Volumes/Data/homes/someone/.local/bin/claude")
        );
    }

    #[test]
    #[should_panic(expected = "must start with ~/")]
    fn test_expand_refuses_a_path_that_is_not_under_home() {
        // Unreachable from the shipped recipes
        // (`recipes::tests::test_every_recipe_path_is_under_home`); a
        // panic here is a programming error surfacing at the first test
        // run, not a state of anyone's Mac.
        let _ = expand(Path::new("/Users/someone"), "/usr/local/bin/claude");
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib adapters::standalone`
Expected: FAIL to compile — `error[E0425]: cannot find value \`RECIPES\`` / `\`CLAUDE\`` in `recipes.rs`; `cannot find function \`expand\`` in `route.rs`; and, until Step 3 adds the two `pub mod` lines, the two files are not compiled at all (so first the `mod.rs` edit, then the errors above).

- [ ] **Step 3: Write the recipe, the meta file and `expand`**

Create `adapters/meta/standalone-claude.toml`:

```toml
schema_version = 1
id = "standalone-claude"
name = "Claude Code"
kind = "standalone"
platforms = ["macos"]
homepage = "https://code.claude.com/docs/en/setup"
verified_versions = ["2.1.281"]
```

(`kind = "standalone"` is documentary: nothing branches on `kind` today, per spec §3.1; it tells a reader of `adapters/meta/` that this one is not a package manager. `verified_versions` is what Task 3, stage 9 records; if the recording day's `claude --version` differs, Task 3, stage 9 changes this line to match.)

Prepend to `crates/banager-core/src/adapters/standalone/recipes.rs`:

```rust
//! The tools, as data. One `pub static` per tool, `RECIPES` listing them
//! in registration order; `StandaloneAdapter::new` builds one adapter per
//! entry (`all()`). Adding a tool is one constant, one meta TOML and one
//! recorded fixture directory -- and the tests below hold every constant
//! to the invariants the code relies on.

use super::recipe::{Latest, Recipe, Route, RouteKind, UpgradeCmd, VersionCmd, VersionParse};
use crate::model::CancelPolicy;

/// Claude Code, the native install (`curl -fsSL https://claude.ai/install.sh
/// | bash`, run by the user; Banager never runs it).
///
/// Every value here is from `.superpowers/phase4/claude.md` (VERIFIED on
/// this Mac or in Anthropic's own documentation, 2026-09-24, unless
/// noted) and from the recording in
/// `adapters/fixtures/standalone-claude/<version>/`:
/// - the launcher `~/.local/bin/claude` is a symbolic link into
///   `~/.local/share/claude/versions/<version>`, one full executable per
///   installed version, kept after upgrades (§2a);
/// - `claude --version` prints `<version> (Claude Code)` (§1). Anthropic
///   documents that Claude Code checks for updates on startup and that
///   `DISABLE_AUTOUPDATER` stops only that background check (§5, doc
///   text); whether `--version` alone reaches the updater was not
///   observed, so the variable is set on every version read regardless,
///   while the upgrade plan adds no override (spec §3.4); the runner
///   inherits ambient environment, and manual updates still work with
///   `DISABLE_AUTOUPDATER=1` (§5);
/// - the newest published version is the channel pointer
///   `downloads.claude.ai/claude-code-releases/<latest|stable>`, a bare
///   version each, which install.sh itself reads (§4); the `stable` pointer
///   is behind `latest` (2.1.273 vs 2.1.281 when recorded), which is why
///   `check_updates` compares rather than tests inequality;
/// - it updates itself in the background when its updater is on (§5);
/// - `claude update` (alias `upgrade`, no options) is the documented
///   updater (§6). The install script downloads each version to a new
///   file under `versions/` and re-points the link only afterwards
///   (install.sh, read directly, §4); `claude update` itself is compiled
///   and its steps were not read (§6, §8), so Banager assumes nothing
///   about interruption: `KillThenReconcile`, no claim in the preview,
///   and stopped upgrades remain `Unconfirmed` even if the version
///   changes. After exit 0, a readable version gates success. 1800 s
///   is spec §4.1's upgrade budget; the binary is about 220 MB.
/// There is no `claude uninstall` subcommand (§2a, `claude --help`); the
/// documented uninstall is two paths, which step C's path-list removal
/// carries. Until then the artifact says `NoSafeMethod`.
pub static CLAUDE: Recipe = Recipe {
    id: "claude",
    meta_toml: include_str!("../../../../../adapters/meta/standalone-claude.toml"),
    route: Route {
        kind: RouteKind::SymlinkIntoRoot,
        launcher: "~/.local/bin/claude",
        root: "~/.local/share/claude",
    },
    version: VersionCmd {
        args: &["--version"],
        env: &[("DISABLE_AUTOUPDATER", "1")],
        parse: VersionParse::FirstToken,
    },
    latest: Latest::ClaudeChannel {
        base: "https://downloads.claude.ai/claude-code-releases",
    },
    self_updates: true,
    upgrade: UpgradeCmd {
        args: &["update"],
        timeout_secs: 1800,
        cancel: CancelPolicy::KillThenReconcile,
    },
};

/// Every tool this adapter type registers, in registration order. The
/// refresh fans out alphabetically by adapter id regardless
/// (`refresh_round`), so this order is only the reading order.
pub static RECIPES: &[&Recipe] = &[&CLAUDE];
```

Prepend to `crates/banager-core/src/adapters/standalone/route.rs`:

```rust
//! Where a tool's own installer put it, and whether what is there is that
//! route's: a launcher at the installer's fixed path, resolved and
//! fingerprinted (never a `claude` found through `PATH`, which on an
//! Intel Mac with `/opt/homebrew/bin` first would be Homebrew's copy and
//! make the native install invisible -- spec §3.3, D3); and, separately,
//! which copy runs when the user types the tool's name (spec §七).

use std::path::{Path, PathBuf};

/// A recipe path (`~/.local/bin/claude`) under `home`. `HostEnv.home` is
/// the `HOME` the login shell exported, not canonicalised: the Unknown
/// page (scan/mod.rs) compares an instance's raw `exe_path` with the raw
/// directory entries it reads, so both must come from the same spelling.
/// A path not starting with `~/` is a programming error in a recipe
/// constant; `recipes::tests::test_every_recipe_path_is_under_home`
/// catches it before this can.
pub fn expand(home: &Path, spec: &str) -> PathBuf {
    let rest = spec
        .strip_prefix("~/")
        .unwrap_or_else(|| panic!("recipe path {spec:?} must start with ~/"));
    home.join(rest)
}
```

In `crates/banager-core/src/adapters/standalone/mod.rs`, replace

```rust
pub mod latest;
pub mod recipe;
```

with

```rust
pub mod latest;
pub mod recipe;
pub mod recipes;
pub mod route;
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib adapters::standalone`
Expected: PASS — the `latest`, `recipes` and `route::expand` tests.

- [ ] **Step 5: Continue the core task**

Continue to the next stage. The complete core task runs `cargo fmt --all` and all five gates, then commits once, after stage 9. No partial type-only commit or dead-code exemption.

---

#### Stage 5: `route::probe`, `lexical_join`, `shadow_note` on synthetic trees

**Files:**
- Modify: `crates/banager-core/src/adapters/standalone/route.rs` (the three functions after `expand`; tests appended to `mod tests`)
- Modify: `crates/banager-core/src/adapters/standalone/mod.rs` (a `#[cfg(test)] mod testing` with the temp-home builder every later task's tests use)
- Test: `route.rs`'s `mod tests`.

**Interfaces:**
- Consumes: `RouteKind` (Task 3, stage 3), `InstanceNote` (Task 1), `HostEnv` (`runner/path_env.rs`), Unix executable permission bits (standalone PATH notices only), `std::fs::{symlink_metadata, canonicalize, read_link}`.
- Produces (verbatim): `pub enum Probe { Absent, Present { real: PathBuf }, LauncherOnly }`; `pub fn probe(kind: RouteKind, launcher: &Path, root: &Path) -> Probe` (readers: `detect` and `inventory`, Task 3, stage 6); `pub fn lexical_join(dir: &Path, target: &Path) -> PathBuf` (reader: `probe`); `fn canonicalize_existing_prefix(path: &Path) -> std::io::Result<PathBuf>` (private; reader: `probe`'s dangling branch); `pub fn shadow_note(command: &str, env: &HostEnv, real: &Path) -> Option<InstanceNote>` (reader: `detect`, Task 3, stage 6). Test support (this crate's tests only): `standalone::testing::{TempHome, ClaudeLayout, claude_layout}`.

Rules, from spec §3.3 and §七, with the deviations below: `lstat` identifies the launcher; a resolved package-manager marker excludes the route before the root fingerprint. Only `NotFound` can enter the dangling branch. Canonicalize the launcher parent before interpreting relative link text, then resolve existing components of target and root before folding a missing tail. A target inside the root with no package-manager marker is `LauncherOnly`. Loops, permission failures and unresolved intermediate symlinks conservatively return `Absent`, never a false claim that program files are missing. `shadow_note` scans PATH in order for a regular file with an executable bit, skipping non-executable namesakes; canonical identity and marker classification then select the existing four notes. This is a filesystem/PATH estimate, not shell alias/function resolution.

- [ ] **Step 1: Write the test support and the failing tests**

Append to `crates/banager-core/src/adapters/standalone/mod.rs` (after the `pub mod` lines):

```rust

/// Synthetic installs in a throwaway home, for this module's tests: real
/// links and real files on a real file system, since `route::probe`
/// answers from `lstat`/`realpath` and nothing else. Never a recorded
/// fixture (spec §9.3: fingerprint tests build their own layouts and must
/// not write under `adapters/fixtures/`).
#[cfg(test)]
pub(super) mod testing {
    use crate::runner::HostEnv;
    use std::path::{Path, PathBuf};

    /// A fresh, canonical directory under the system temp dir, removed on
    /// drop. Canonical, so paths built from it compare equal to what
    /// `canonicalize` answers (macOS's `/var/folders` is `/private/var/…`).
    pub struct TempHome(PathBuf);

    impl TempHome {
        pub fn new(tag: &str) -> TempHome {
            let raw = std::env::temp_dir().join(format!(
                "banager-standalone-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&raw).expect("create temp home");
            TempHome(std::fs::canonicalize(&raw).expect("canonical temp home"))
        }

        pub fn path(&self) -> &Path {
            &self.0
        }

        /// Creates `rel` (and its parents) under the home.
        pub fn dir(&self, rel: &str) -> PathBuf {
            let path = self.0.join(rel);
            std::fs::create_dir_all(&path).expect("create dir");
            path
        }

        /// Writes a small regular file at `rel` (parents created).
        pub fn file(&self, rel: &str) -> PathBuf {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("create parent");
            std::fs::write(&path, b"#!/bin/sh\n").expect("write file");
            path
        }

        /// An executable synthetic target; it is never actually spawned.
        pub fn executable(&self, rel: &str) -> PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let path = self.file(rel);
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("executable target");
            path
        }

        /// A symbolic link at `rel` whose text is `target` exactly --
        /// absolute or relative, existing or not (parents created).
        pub fn link(&self, rel: &str, target: &Path) -> PathBuf {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("create parent");
            std::os::unix::fs::symlink(target, &path).expect("symlink");
            path
        }

        /// A `HostEnv` whose home is this directory and whose `PATH` is
        /// `path_dirs`.
        pub fn env(&self, path_dirs: Vec<PathBuf>) -> HostEnv {
            HostEnv {
                path_dirs,
                home: self.0.clone(),
                euid: 501,
                cargo_home: None,
                ollama_host: None,
            }
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The native Claude Code layout, as `ls -la` shows it on a Mac with
    /// the installer's defaults: `~/.local/bin/claude` linking (absolute
    /// text) to `~/.local/share/claude/versions/<version>`.
    pub struct ClaudeLayout {
        pub launcher: PathBuf,
        pub root: PathBuf,
        pub real: PathBuf,
    }

    pub fn claude_layout(home: &TempHome, version: &str) -> ClaudeLayout {
        let real = home.executable(&format!(".local/share/claude/versions/{version}"));
        let launcher = home.link(".local/bin/claude", &real);
        ClaudeLayout {
            launcher,
            root: home.path().join(".local/share/claude"),
            real,
        }
    }
}
```

Append inside `mod tests` in `crates/banager-core/src/adapters/standalone/route.rs` (after `test_expand_refuses_a_path_that_is_not_under_home`):

```rust

    use super::super::recipe::RouteKind;
    use super::super::testing::{claude_layout, TempHome};
    use crate::model::InstanceNote;

    #[test]
    fn test_probe_finds_a_launcher_that_links_into_its_root() {
        let home = TempHome::new("probe-present");
        let layout = claude_layout(&home, "2.1.281");
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &layout.launcher, &layout.root),
            Probe::Present { real: layout.real }
        );
    }

    #[test]
    fn test_probe_follows_a_two_hop_link_into_the_root() {
        // `realpath`, not one `readlink`: a launcher that links to a
        // `current` link inside the root still resolves into it.
        let home = TempHome::new("probe-two-hop");
        let real = home.file(".local/share/claude/versions/2.1.281");
        let current = home.link(".local/share/claude/current", &real);
        let launcher = home.link(".local/bin/claude", &current);
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &home.path().join(".local/share/claude")),
            Probe::Present { real }
        );
    }

    #[test]
    fn test_probe_accepts_a_home_reached_through_a_symlink() {
        // `HostEnv.home` may be a symlink to the real home (a home on
        // another volume). The launcher and root the recipe expands under
        // it are then non-canonical spellings of the same files; both
        // sides are canonicalised before the fingerprint compares them, and
        // `real` comes back canonical.
        let home = TempHome::new("probe-linked-home");
        let real_home = home.dir("real-home");
        let real = home.file("real-home/.local/share/claude/versions/2.1.281");
        home.link("real-home/.local/bin/claude", &real);
        let linked_home = home.link("linked-home", &real_home);
        let launcher = linked_home.join(".local/bin/claude");
        let root = linked_home.join(".local/share/claude");
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &root),
            Probe::Present { real }
        );
    }

    #[test]
    fn test_probe_leaves_a_homebrew_or_npm_copy_to_its_own_source() {
        // Shared exclusion before the fingerprint (spec §3.3 step 2): a
        // `~/.local/bin/claude` that resolves under Caskroom, Cellar,
        // node_modules or corepack is a row brew or npm already lists.
        for tail in [
            "opt/homebrew/Caskroom/claude-code/2.1.267/claude",
            "opt/homebrew/Cellar/something/1.0/bin/claude",
            "opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js",
            "opt/homebrew/lib/node_modules/corepack/dist/claude.js",
        ] {
            let home = TempHome::new("probe-excluded");
            let target = home.file(tail);
            let launcher = home.link(".local/bin/claude", &target);
            assert_eq!(
                probe(RouteKind::SymlinkIntoRoot, &launcher, &home.path().join(".local/share/claude")),
                Probe::Absent,
                "{tail}"
            );
        }
    }

    #[test]
    fn test_probe_excludes_package_manager_markers_even_inside_the_native_root() {
        for marker in ["Cellar", "Caskroom", "node_modules", "corepack"] {
            let home = TempHome::new("probe-marker-inside-root");
            let root = home.dir(".local/share/claude");
            let target = home.executable(&format!(".local/share/claude/{marker}/claude"));
            let launcher = home.link(".local/bin/claude", &target);
            // Without the marker exclusion, the root fingerprint accepts
            // this real executable, so this test detects that deletion.
            assert_eq!(probe(RouteKind::SymlinkIntoRoot, &launcher, &root), Probe::Absent);
            std::fs::remove_file(&target).unwrap();
            assert_eq!(probe(RouteKind::SymlinkIntoRoot, &launcher, &root), Probe::Absent);
        }
    }

    #[test]
    fn test_probe_resolves_a_linked_bin_before_relative_dotdot() {
        let home = TempHome::new("probe-linked-bin");
        let other_bin = home.dir("other/bin");
        home.link(".local/bin", &other_bin);
        let launcher = home.link(".local/bin/claude", Path::new("../share/claude/versions/missing"));
        let native_root = home.path().join(".local/share/claude");
        assert_eq!(probe(RouteKind::SymlinkIntoRoot, &launcher, &native_root), Probe::Absent);
        // The very same link belongs to this root, proving the parent
        // resolution changes its meaning instead of rejecting all links.
        let actual_root = home.path().join("other/share/claude");
        assert_eq!(probe(RouteKind::SymlinkIntoRoot, &launcher, &actual_root), Probe::LauncherOnly);
    }

    #[test]
    fn test_probe_does_not_call_a_symlink_loop_launcher_only() {
        let home = TempHome::new("probe-loop");
        let root = home.dir(".local/share/claude");
        let target = root.join("loop");
        home.link(".local/share/claude/loop", &target);
        let launcher = home.link(".local/bin/claude", &target);
        assert_eq!(probe(RouteKind::SymlinkIntoRoot, &launcher, &root), Probe::Absent);
    }

    #[test]
    fn test_probe_rejects_a_plain_file_where_a_link_is_expected() {
        let home = TempHome::new("probe-plain-file");
        let launcher = home.file(".local/bin/claude");
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &home.path().join(".local/share/claude")),
            Probe::Absent
        );
    }

    #[test]
    fn test_probe_rejects_a_link_that_resolves_outside_the_root() {
        let home = TempHome::new("probe-outside-root");
        let elsewhere = home.file("elsewhere/claude");
        let launcher = home.link(".local/bin/claude", &elsewhere);
        home.dir(".local/share/claude");
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &home.path().join(".local/share/claude")),
            Probe::Absent
        );
    }

    #[test]
    fn test_probe_is_absent_when_there_is_no_launcher() {
        // "Not installed", not "not responding": no instance at all.
        let home = TempHome::new("probe-missing");
        assert_eq!(
            probe(
                RouteKind::SymlinkIntoRoot,
                &home.path().join(".local/bin/claude"),
                &home.path().join(".local/share/claude")
            ),
            Probe::Absent
        );
    }

    #[test]
    fn test_probe_reports_launcher_only_for_a_dangling_link_whose_text_points_into_the_root() {
        // The half-uninstalled state (spec §3.3 step 2, §十七 Q17): the
        // program directory is gone, the link is left. Absolute text, and
        // relative text as grok's installer writes it (`../…`).
        let home = TempHome::new("probe-dangling-absolute");
        let root = home.path().join(".local/share/claude");
        let launcher = home.link(".local/bin/claude", &root.join("versions/2.1.281"));
        assert_eq!(probe(RouteKind::SymlinkIntoRoot, &launcher, &root), Probe::LauncherOnly);

        let home = TempHome::new("probe-dangling-relative");
        let root = home.path().join(".local/share/claude");
        let launcher = home.link(
            ".local/bin/claude",
            Path::new("../share/claude/versions/2.1.281"),
        );
        assert_eq!(probe(RouteKind::SymlinkIntoRoot, &launcher, &root), Probe::LauncherOnly);
    }

    #[test]
    fn test_probe_is_absent_for_a_dangling_link_that_points_elsewhere() {
        // An old installer's leftover pointing somewhere else is the
        // Unknown page's broken link, not a Claude Code that stopped
        // answering (spec §十三 #33).
        let home = TempHome::new("probe-dangling-elsewhere");
        let launcher = home.link(
            ".local/bin/claude",
            &home.path().join("Applications/Old.app/Contents/MacOS/claude"),
        );
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &home.path().join(".local/share/claude")),
            Probe::Absent
        );
    }

    #[test]
    fn test_probe_reports_launcher_only_under_a_home_reached_through_a_symlink() {
        // The dangling twin of
        // `test_probe_accepts_a_home_reached_through_a_symlink`: the
        // installer spelled the link text through the real home,
        // `HostEnv.home` is the symlink to it, and the program directory
        // is gone. Neither the target nor the root canonicalises whole, so
        // `probe` canonicalises the deepest ancestor of each that still
        // exists and the two spellings agree; without that the row would
        // vanish from the Installed page and the link would be listed on
        // the Unknown page as a broken link instead.
        let home = TempHome::new("probe-linked-home-dangling");
        let real_home = home.dir("real-home");
        home.dir("real-home/.local/share");
        home.link(
            "real-home/.local/bin/claude",
            &real_home.join(".local/share/claude/versions/2.1.281"),
        );
        let linked_home = home.link("linked-home", &real_home);
        assert_eq!(
            probe(
                RouteKind::SymlinkIntoRoot,
                &linked_home.join(".local/bin/claude"),
                &linked_home.join(".local/share/claude")
            ),
            Probe::LauncherOnly
        );
    }

    #[test]
    fn test_lexical_join_normalises_dot_and_dotdot_without_touching_the_disk() {
        let dir = Path::new("/Users/someone/.local/bin");
        assert_eq!(
            lexical_join(dir, Path::new("../share/claude/versions/2.1.281")),
            PathBuf::from("/Users/someone/.local/share/claude/versions/2.1.281")
        );
        assert_eq!(
            lexical_join(dir, Path::new("./claude-real")),
            PathBuf::from("/Users/someone/.local/bin/claude-real")
        );
        assert_eq!(
            lexical_join(dir, Path::new("/Users/someone/.grok/downloads/grok-1.0.41")),
            PathBuf::from("/Users/someone/.grok/downloads/grok-1.0.41")
        );
        // Climbing past the root stays at the root.
        assert_eq!(lexical_join(Path::new("/a"), Path::new("../../../b")), PathBuf::from("/b"));
    }

    #[test]
    fn test_shadow_note_says_not_on_path_when_typing_the_name_finds_nothing() {
        let home = TempHome::new("shadow-not-on-path");
        let layout = claude_layout(&home, "2.1.281");
        let env = home.env(vec![home.dir("somewhere/else")]);
        assert_eq!(shadow_note("claude", &env, &layout.real), Some(InstanceNote::NotOnPath));
    }

    #[test]
    fn test_shadow_note_is_silent_when_path_finds_this_very_copy() {
        let home = TempHome::new("shadow-same");
        let layout = claude_layout(&home, "2.1.281");
        let env = home.env(vec![home.path().join(".local/bin")]);
        assert_eq!(shadow_note("claude", &env, &layout.real), None);
    }

    #[test]
    fn test_shadow_note_is_silent_for_a_link_to_the_same_launcher() {
        // `~/bin/claude → ~/.local/bin/claude` earlier on PATH runs the
        // same file: canonical paths are compared, not the names PATH
        // found.
        let home = TempHome::new("shadow-link-to-launcher");
        let layout = claude_layout(&home, "2.1.281");
        let bin = home.dir("bin");
        home.link("bin/claude", &layout.launcher);
        let env = home.env(vec![bin, home.path().join(".local/bin")]);
        assert_eq!(shadow_note("claude", &env, &layout.real), None);
    }

    #[test]
    fn test_shadow_note_skips_an_earlier_non_executable_namesake() {
        let home = TempHome::new("shadow-non-executable");
        let layout = claude_layout(&home, "2.1.281");
        let namesake = home.file("earlier/claude");
        std::fs::set_permissions(&namesake, std::fs::Permissions::from_mode(0o644)).unwrap();
        let env = home.env(vec![home.path().join("earlier"), home.path().join(".local/bin")]);
        assert_eq!(shadow_note("claude", &env, &layout.real), None);
    }

    #[test]
    fn test_shadow_note_classifies_the_copy_that_wins_on_path() {
        for (tail, expected) in [
            (
                "opt/homebrew/Caskroom/claude-code/2.1.267/claude",
                InstanceNote::ShadowedByHomebrew,
            ),
            ("opt/homebrew/Cellar/x/1/bin/claude", InstanceNote::ShadowedByHomebrew),
            (
                "opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js",
                InstanceNote::ShadowedByNpm,
            ),
            ("Applications/Some.app/Contents/MacOS/claude", InstanceNote::ShadowedByOther),
        ] {
            let home = TempHome::new("shadow-classify");
            let layout = claude_layout(&home, "2.1.281");
            let winner = home.executable(tail);
            let first = home.dir("first-on-path");
            home.link("first-on-path/claude", &winner);
            let env = home.env(vec![first, home.path().join(".local/bin")]);
            assert_eq!(shadow_note("claude", &env, &layout.real), Some(expected), "{tail}");
        }
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib adapters::standalone::route`
Expected: FAIL to compile — `cannot find function \`probe\``, `\`lexical_join\``, `\`shadow_note\``; `cannot find type \`Probe\``.

- [ ] **Step 3: Write the three functions**

Append to `crates/banager-core/src/adapters/standalone/route.rs` after `expand` (before `#[cfg(test)]`), and add `use crate::model::InstanceNote; use crate::runner::HostEnv; use super::recipe::RouteKind; use std::path::Component; use std::os::unix::fs::PermissionsExt;` to the file's imports:

```rust

/// What is at a recipe's launcher path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Probe {
    /// Nothing this adapter lists: no launcher, a launcher that is a
    /// package manager's copy or another route's shape, or a dangling
    /// link pointing somewhere else. "Not installed", never "not
    /// responding".
    Absent,
    /// The launcher is this route's; `real` is the canonical binary it
    /// resolves to (the artifact's `path`).
    Present { real: PathBuf },
    /// A dangling launcher whose own text points into the root: the
    /// program files are gone (removed by hand, or -- from step C -- by an
    /// uninstall that stopped partway), the link is left. Listed with no
    /// version and `InstanceNote::LauncherOnly` so the state is visible;
    /// in this step the artifact still carries `NoSafeMethod`, and step
    /// C's path-list uninstall is what removes the link.
    LauncherOnly,
}

/// Path components that mean "a package manager put this here". Checked
/// before the fingerprint, so a `~/.local/bin/claude` that some tool
/// linked into Homebrew's Caskroom is brew's row (or the Unknown page's),
/// never listed twice (spec §3.3 step 2, D3).
const PACKAGE_MANAGER_MARKERS: [&str; 4] = ["Cellar", "Caskroom", "node_modules", "corepack"];

fn has_component(path: &Path, names: &[&str]) -> bool {
    path.components().any(|component| match component {
        Component::Normal(name) => names.iter().any(|candidate| name == *candidate),
        _ => false,
    })
}

/// Resolve existing components before interpreting `..`; only genuinely
/// missing components may remain lexical. Errors (permissions, loops,
/// non-directories, or dangling intermediate symlinks) are not evidence of
/// missing program files. Private reader: `probe`'s NotFound branch.
fn canonicalize_existing_prefix(path: &Path) -> std::io::Result<PathBuf> {
    let mut resolved = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(name) => {
                let next = resolved.join(name);
                match std::fs::canonicalize(&next) {
                    Ok(real) => resolved = real,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        // An existing symlink with a missing destination
                        // cannot safely be treated as a missing directory.
                        match std::fs::symlink_metadata(&next) {
                            Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => {
                                resolved = next;
                            }
                            _ => return Err(error),
                        }
                    }
                    Err(error) => return Err(error),
                }
            }
            other => resolved.push(other.as_os_str()),
        }
    }
    Ok(resolved)
}

/// Whether `launcher` is this route's install of the tool whose root is
/// `root`, and if so which binary it runs (spec §3.3 steps 1-3).
pub fn probe(kind: RouteKind, launcher: &Path, root: &Path) -> Probe {
    // Step 1: `lstat`, not `stat` -- a dangling link is still a launcher.
    let Ok(meta) = std::fs::symlink_metadata(launcher) else {
        return Probe::Absent;
    };
    match std::fs::canonicalize(launcher) {
        Ok(real) => {
            // Step 2: shared exclusion.
            if has_component(&real, &PACKAGE_MANAGER_MARKERS) {
                return Probe::Absent;
            }
            // Step 3: the fingerprint.
            match kind {
                RouteKind::SymlinkIntoRoot => {
                    if !meta.file_type().is_symlink() {
                        return Probe::Absent;
                    }
                    let Ok(canonical_root) = std::fs::canonicalize(root) else {
                        return Probe::Absent;
                    };
                    if real.starts_with(&canonical_root) {
                        Probe::Present { real }
                    } else {
                        Probe::Absent
                    }
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if !meta.file_type().is_symlink() {
                return Probe::Absent;
            }
            let Ok(target) = std::fs::read_link(launcher) else {
                return Probe::Absent;
            };
            let Some(parent) = launcher.parent() else {
                return Probe::Absent;
            };
            // Resolve the launcher directory before any relative `..`.
            let Ok(parent) = std::fs::canonicalize(parent) else {
                return Probe::Absent;
            };
            let joined = if target.is_absolute() { target } else { parent.join(target) };
            let (Ok(target), Ok(root)) = (
                canonicalize_existing_prefix(&joined),
                canonicalize_existing_prefix(root),
            ) else {
                return Probe::Absent;
            };
            // `lexical_join` is safe only AFTER existing symlink parents
            // have been resolved; it folds the remaining missing tail.
            let target = lexical_join(Path::new("/"), &target);
            if !has_component(&target, &PACKAGE_MANAGER_MARKERS) && target.starts_with(&root) {
                Probe::LauncherOnly
            } else {
                Probe::Absent
            }
        }
        // A loop or permission error is not a dangling native install.
        Err(_) => Probe::Absent,
    }
}

/// `target` as seen from `dir`, with `.` and `..` folded away without
/// touching the file system: a relative link text (`../downloads/grok-…`)
/// becomes the absolute path it names; an absolute one is normalised as
/// it is. Climbing above the root stays at the root.
pub fn lexical_join(dir: &Path, target: &Path) -> PathBuf {
    let joined = if target.is_absolute() {
        target.to_path_buf()
    } else {
        dir.join(target)
    };
    let mut out = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Which copy runs when the user types `command`, given that this
/// instance's binary is `real` (spec §七): `None` when `PATH`'s first
/// `command` is this very file, otherwise one of the four payload-free
/// notes. Payload-free on purpose (spec §2.3's rule for `InstanceNote`);
/// the sentence names the command, which the user knows, not the winner's
/// path, which they would not.
pub fn shadow_note(command: &str, env: &HostEnv, real: &Path) -> Option<InstanceNote> {
    // Standalone-only lookup: changing the shared package-manager
    // discovery helper would broaden this step beyond its PATH notices.
    let first = env.path_dirs.iter().map(|dir| dir.join(command)).find(|path| {
        std::fs::metadata(path)
            .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    });
    let Some(first) = first else {
        return Some(InstanceNote::NotOnPath);
    };
    let Ok(first_real) = std::fs::canonicalize(&first) else {
        return Some(InstanceNote::ShadowedByOther);
    };
    if first_real == real {
        return None;
    }
    Some(if has_component(&first_real, &["Cellar", "Caskroom"]) {
        InstanceNote::ShadowedByHomebrew
    } else if has_component(&first_real, &["node_modules"]) {
        InstanceNote::ShadowedByNpm
    } else {
        InstanceNote::ShadowedByOther
    })
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib adapters::standalone::route`
Expected: PASS — all `expand` and route tests, including linked-bin, loop, in-root markers, and non-executable PATH regressions.

- [ ] **Step 5: Continue the core task**

Continue to the next stage. The complete core task runs `cargo fmt --all` and all five gates, then commits once, after stage 9. No partial type-only commit or dead-code exemption.

---

#### Stage 6: `StandaloneAdapter::{new, detect, inventory, reconcile, search}`

**Files:**
- Modify: `crates/banager-core/src/adapters/standalone/mod.rs` (the struct, `Detected`, the inherent methods, a `#[cfg(test)] mod tests`)
- Test: that `mod tests`.

**Interfaces:**
- Consumes: `Recipe`/`RECIPES`/`CLAUDE` (core stages 3–4), `route::{expand, probe, shadow_note, Probe}` (core stages 4–5), `latest::parse_version` (Task 3, stage 3), `AdapterMeta::{from_toml, unverified_version}`, `reconcile_from`, `CommandRunner`/`CommandSpec`/`OutputUse::Parsed`, `HttpClient` (held for Task 3, stage 7), `instance_id` (`model.rs:25-34`), `InstanceNote`, `UninstallBlocked::NoSafeMethod` (Tasks 1–2).
- Produces (verbatim, from Core Interfaces): `pub struct Detected { pub home: PathBuf }` (writer: `detect`; reader: `check_updates`, Task 3, stage 7 — the seat C and E widen); `StandaloneAdapter::new`, `detect`, `inventory`, `reconcile`, `search` as inherent methods (the `impl Adapter` and `all()` are Task 3, stage 8, once `check_updates`, `plan` and `execute` exist — an `impl Adapter` with a stub method would be a placeholder). The `http` and `Detected.home` readers are in stage 7 of this same task; no dead-code exemption is added. Nothing in this task's non-test code needs `#[async_trait]` (inherent `async fn`s take no attribute), so `async_trait` is imported by the test module for its `RecordingRunner` and by Task 3, stage 8 for the `impl Adapter`, never here — an unused import fails `-D warnings`. Per-field values of the instance and the artifact are spec §2.2 and §2.3, reader by reader in the doc comments below.

- [ ] **Step 1: Write the failing tests**

Append to `crates/banager-core/src/adapters/standalone/mod.rs`:

```rust

#[cfg(test)]
mod tests {
    use super::recipes::CLAUDE;
    use super::testing::{claude_layout, TempHome};
    use super::*;
    use crate::http::MockHttpClient;
    use crate::model::{ArtifactKind, InstallReason, InstanceNote, Unavailable, UninstallBlocked};
    use crate::runner::{CommandOutput, MockRunner, RunnerError};
    // For `RecordingRunner` below; the non-test code of this task has no
    // trait impl and must not import it (unused under `-D warnings`).
    use async_trait::async_trait;
    use std::sync::Mutex as StdMutex;

    fn exited_0(stdout: &str) -> CommandOutput {
        CommandOutput {
            exit_code: Some(0),
            stdout: stdout.to_string(),
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        }
    }

    fn adapter(runner: Arc<dyn CommandRunner>) -> StandaloneAdapter {
        StandaloneAdapter::new(&CLAUDE, runner, Arc::new(MockHttpClient::new()))
    }

    /// `MockRunner` keys and records argv only. This one records every
    /// `CommandSpec` it is handed, so a test can prove what environment,
    /// timeout and output use the version read carried -- the point of
    /// spec §3.4 is one environment variable.
    struct RecordingRunner {
        specs: StdMutex<Vec<CommandSpec>>,
        output: CommandOutput,
    }

    #[async_trait]
    impl CommandRunner for RecordingRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _on_line: Option<crate::runner::LineCallback>,
            _cancel: CancellationToken,
        ) -> Result<CommandOutput, RunnerError> {
            self.specs.lock().unwrap().push(spec);
            Ok(self.output.clone())
        }
    }

    #[test]
    fn test_new_takes_its_meta_from_the_recipe_and_names_the_standalone_id() {
        let adapter = adapter(Arc::new(MockRunner::new()));
        assert_eq!(adapter.meta.id, "standalone-claude");
        assert_eq!(adapter.meta.name, "Claude Code");
        assert!(adapter.detected.lock().unwrap().is_none(), "nothing detected yet");
    }

    #[tokio::test]
    async fn test_detect_lists_the_native_install_as_one_instance() {
        let home = TempHome::new("detect-present");
        let meta = AdapterMeta::from_toml(CLAUDE.meta_toml).expect("meta");
        let version = meta.verified_versions.first().expect("a verified version");
        let layout = claude_layout(&home, version);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0(&format!("{version} (Claude Code)\n")),
        );
        let adapter = adapter(runner);

        let instances = adapter.detect(&home.env(vec![home.path().join(".local/bin")])).await;

        assert_eq!(instances.len(), 1);
        let inst = &instances[0];
        assert_eq!(inst.id, "standalone-claude");
        assert_eq!(inst.adapter_id, "standalone-claude");
        // The launcher itself, not the binary it resolves to: the program
        // every plan runs and the path `sourceNoticesFor` takes the
        // command name from (spec §2.2).
        assert_eq!(inst.exe_path, layout.launcher);
        assert_eq!(inst.prefix, layout.root);
        assert_eq!(inst.scope, Scope::User);
        assert_eq!(inst.version, Some(version.clone()));
        assert_eq!(inst.unverified_version, None, "the metadata version is verified");
        assert_eq!(inst.read_only_reason, None);
        assert_eq!(inst.status.unavailable, None);
        assert!(inst.status.notes.is_empty(), "PATH finds this very copy: no note");
        assert_eq!(
            adapter.detected.lock().unwrap().as_ref().map(|d| d.home.clone()),
            Some(home.path().to_path_buf()),
            "detect seats home for check_updates"
        );
    }

    #[tokio::test]
    async fn test_detect_reads_the_version_with_the_autoupdater_off_and_a_thirty_second_timeout() {
        // Spec §3.4: Claude Code checks for updates on startup (doc text)
        // and a refresh is read-only, so the documented switch for that
        // background check goes on this read (and inventory's) whether or
        // not a bare `--version` would reach the updater -- never on the
        // upgrade plan.
        let home = TempHome::new("detect-env");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(RecordingRunner {
            specs: StdMutex::new(Vec::new()),
            output: exited_0("2.1.281 (Claude Code)\n"),
        });
        let adapter = adapter(runner.clone());

        adapter.detect(&home.env(vec![])).await;

        let specs = runner.specs.lock().unwrap();
        assert_eq!(specs.len(), 1);
        let spec = &specs[0];
        assert_eq!(spec.program, layout.launcher);
        assert_eq!(spec.args, vec!["--version".to_string()]);
        assert_eq!(
            spec.env,
            vec![("DISABLE_AUTOUPDATER".to_string(), "1".to_string())]
        );
        assert_eq!(spec.timeout, Duration::from_secs(30));
        assert_eq!(spec.output_use, OutputUse::Parsed);
        assert_eq!(spec.cwd, None);
    }

    #[tokio::test]
    async fn test_detect_flags_a_version_outside_the_verified_list() {
        let meta = AdapterMeta::from_toml(CLAUDE.meta_toml).expect("meta");
        let version = (0_u64..)
            .map(|major| format!("{major}.0.0"))
            .find(|version| !meta.verified_versions.contains(version))
            .expect("a version outside the finite recorded list");
        let home = TempHome::new("detect-unverified");
        let layout = claude_layout(&home, &version);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0(&format!("{version} (Claude Code)\n")),
        );
        let instances = adapter(runner).detect(&home.env(vec![])).await;
        assert_eq!(instances[0].version, Some(version.clone()));
        assert_eq!(instances[0].unverified_version, Some(version));
    }

    #[tokio::test]
    async fn test_detect_marks_a_failed_version_read_as_not_responding() {
        // The launcher is there and is this route's, but `--version` did
        // not answer: the state axis (`NotResponding`), like uv's rule.
        let home = TempHome::new("detect-not-responding");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            CommandOutput {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "dyld: Library not loaded\n".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let instances = adapter(runner).detect(&home.env(vec![])).await;
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].version, None);
        assert_eq!(instances[0].status.unavailable, Some(Unavailable::NotResponding));
        assert_eq!(instances[0].exe_path, layout.launcher);
    }

    #[tokio::test]
    async fn test_detect_marks_a_timed_out_version_read_as_not_responding() {
        // A `claude --version` that hangs (a pre-2.1.214 build scanning a
        // directory named like a shell rc file, claude.md §6) is stopped by
        // the runner at 30 s and reported as not answering; nothing hangs
        // and nothing crashes.
        let home = TempHome::new("detect-timed-out");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            CommandOutput {
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: true,
                cancelled: false,
            },
        );
        let instances = adapter(runner).detect(&home.env(vec![])).await;
        assert_eq!(instances[0].status.unavailable, Some(Unavailable::NotResponding));
    }

    #[tokio::test]
    async fn test_detect_marks_a_runner_error_as_not_responding() {
        // No canned answer is the mock's spawn failure; a real one is the
        // same shape (`RunnerError::Spawn`).
        let home = TempHome::new("detect-spawn-failed");
        claude_layout(&home, "2.1.281");
        let instances = adapter(Arc::new(MockRunner::new())).detect(&home.env(vec![])).await;
        assert_eq!(instances[0].status.unavailable, Some(Unavailable::NotResponding));
    }

    #[tokio::test]
    async fn test_detect_carries_the_path_note_when_another_copy_wins() {
        let home = TempHome::new("detect-shadowed");
        let layout = claude_layout(&home, "2.1.281");
        let cask = home.executable("opt/homebrew/Caskroom/claude-code/2.1.267/claude");
        let brew_bin = home.dir("opt/homebrew/bin");
        home.link("opt/homebrew/bin/claude", &cask);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let env = home.env(vec![brew_bin, home.path().join(".local/bin")]);
        let instances = adapter(runner).detect(&env).await;
        assert_eq!(instances[0].status.notes, vec![InstanceNote::ShadowedByHomebrew]);
        // And with nothing on PATH at all: NotOnPath.
        let instances = adapter(Arc::new(MockRunner::new())).detect(&home.env(vec![])).await;
        assert_eq!(instances[0].status.notes, vec![InstanceNote::NotOnPath]);
    }

    #[tokio::test]
    async fn test_detect_keeps_a_dangling_launcher_as_a_launcher_only_row() {
        // The half-uninstalled state: no version read at all (there is no
        // program to run); not unavailable (the source has not stopped
        // answering, and step C's uninstall must be allowed on this
        // instance); the note says what is left.
        let home = TempHome::new("detect-launcher-only");
        let root = home.path().join(".local/share/claude");
        let launcher = home.link(".local/bin/claude", &root.join("versions/2.1.281"));
        let runner = Arc::new(MockRunner::new());
        let adapter = adapter(runner.clone());

        let instances = adapter.detect(&home.env(vec![home.path().join(".local/bin")])).await;

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].exe_path, launcher);
        assert_eq!(instances[0].version, None);
        assert_eq!(instances[0].status.unavailable, None);
        assert_eq!(instances[0].status.notes, vec![InstanceNote::LauncherOnly]);
        assert!(runner.calls().is_empty(), "nothing to run");
    }

    #[tokio::test]
    async fn test_detect_finds_nothing_without_a_launcher_or_with_a_package_managers_copy() {
        let home = TempHome::new("detect-absent");
        let runner = Arc::new(MockRunner::new());
        assert!(adapter(runner.clone()).detect(&home.env(vec![])).await.is_empty());
        let cask = home.executable("opt/homebrew/Caskroom/claude-code/2.1.267/claude");
        home.link(".local/bin/claude", &cask);
        assert!(adapter(runner.clone()).detect(&home.env(vec![])).await.is_empty());
        assert!(runner.calls().is_empty(), "no `--version` for a row this adapter does not own");
    }

    fn instance_for(layout: &super::testing::ClaudeLayout, version: Option<&str>) -> ManagerInstance {
        ManagerInstance {
            exe_path: layout.launcher.clone(),
            prefix: layout.root.clone(),
            version: version.map(str::to_string),
            ..crate::testing::manager_instance("standalone-claude", "standalone-claude")
        }
    }

    #[tokio::test]
    async fn test_inventory_is_the_tool_itself_with_no_safe_uninstall_method() {
        let home = TempHome::new("inventory-present");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = adapter(runner);
        let inst = instance_for(&layout, Some("2.1.281"));

        let artifacts = adapter.inventory(&inst).await.expect("inventory");

        assert_eq!(artifacts.len(), 1);
        let a = &artifacts[0];
        assert_eq!(
            a.key,
            ArtifactKey {
                instance_id: "standalone-claude".to_string(),
                kind: ArtifactKind::Binary,
                name: "claude".to_string(),
            }
        );
        assert_eq!(a.display_name, "Claude Code");
        assert_eq!(a.version, "2.1.281");
        assert_eq!(a.reason, InstallReason::Requested);
        // Localised by the front end (`STANDALONE_SUMMARY_KEYS`), not a
        // bare English string here.
        assert_eq!(a.description, None);
        assert_eq!(
            a.homepage.as_deref(),
            Some("https://code.claude.com/docs/en/setup")
        );
        // The real binary, for the Unknown page's rule 2.
        assert_eq!(a.path, Some(layout.real.clone()));
        assert!(a.auto_updates, "claude updates itself in the background");
        assert_eq!(a.uninstall_blocked, Some(UninstallBlocked::NoSafeMethod));
        assert_eq!(a.size_bytes, None);
        assert_eq!(a.installed_at, None);
    }

    #[tokio::test]
    async fn test_inventory_reads_the_disk_again_rather_than_detects_answer() {
        // `refresh` calls inventory under the instance lock and
        // `run_operation`'s reconcile must see the disk as it is now (spec
        // §3.6): a launcher removed since detect means an empty inventory.
        let home = TempHome::new("inventory-fresh");
        let layout = claude_layout(&home, "2.1.281");
        let inst = instance_for(&layout, Some("2.1.281"));
        std::fs::remove_file(&layout.launcher).expect("remove launcher");
        let artifacts = adapter(Arc::new(MockRunner::new()))
            .inventory(&inst)
            .await
            .expect("inventory");
        assert!(artifacts.is_empty());
    }

    #[tokio::test]
    async fn test_inventory_of_a_launcher_only_install_has_no_version_and_no_path() {
        let home = TempHome::new("inventory-launcher-only");
        let root = home.path().join(".local/share/claude");
        let launcher = home.link(".local/bin/claude", &root.join("versions/2.1.281"));
        let inst = ManagerInstance {
            exe_path: launcher,
            prefix: root,
            version: None,
            ..crate::testing::manager_instance("standalone-claude", "standalone-claude")
        };
        let runner = Arc::new(MockRunner::new());
        let artifacts = adapter(runner.clone()).inventory(&inst).await.expect("inventory");
        assert_eq!(artifacts.len(), 1, "still a row: the link is still there, and the state must be visible");
        assert_eq!(artifacts[0].version, "");
        assert_eq!(artifacts[0].path, None);
        assert_eq!(artifacts[0].uninstall_blocked, Some(UninstallBlocked::NoSafeMethod));
        assert!(runner.calls().is_empty());
    }

    #[tokio::test]
    async fn test_reconcile_reports_present_with_the_live_version_and_absent_when_gone() {
        let home = TempHome::new("reconcile");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = adapter(runner);
        let inst = instance_for(&layout, Some("2.1.281"));
        let key = ArtifactKey {
            instance_id: inst.id.clone(),
            kind: ArtifactKind::Binary,
            name: "claude".to_string(),
        };

        let present = adapter.reconcile(&inst, &key).await.expect("reconcile");
        assert!(present.present);
        assert_eq!(present.version, Some("2.1.281".to_string()));

        // Another kind or name of the same instance is not this artifact.
        let other = ArtifactKey {
            kind: ArtifactKind::Tool,
            ..key.clone()
        };
        assert!(!adapter.reconcile(&inst, &other).await.expect("reconcile").present);

        std::fs::remove_file(&layout.launcher).expect("remove launcher");
        let absent = adapter.reconcile(&inst, &key).await.expect("reconcile");
        assert!(!absent.present);
        assert_eq!(absent.version, None);
    }

    #[tokio::test]
    async fn test_reconcile_rejects_an_unreadable_version_and_a_dangling_launcher() {
        let home = TempHome::new("reconcile-broken");
        let layout = claude_layout(&home, "2.1.281");
        let inst = instance_for(&layout, Some("2.1.281"));
        let adapter = adapter(Arc::new(MockRunner::new()));
        let key = adapter.artifact_key(&inst);
        assert!(matches!(adapter.reconcile(&inst, &key).await, Err(AdapterError::Parse(_))));
        std::fs::remove_file(&layout.real).unwrap();
        let artifacts = adapter.inventory(&inst).await.unwrap();
        assert_eq!(artifacts.len(), 1, "presence survives for step C");
        assert_eq!(artifacts[0].path, None);
        assert_eq!(artifacts[0].version, "");
        assert!(matches!(adapter.reconcile(&inst, &key).await, Err(AdapterError::Parse(_))));
    }

    #[tokio::test]
    async fn test_search_is_unsupported() {
        let home = TempHome::new("search");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let result = adapter.search(&instance_for(&layout, Some("2.1.281")), "claude").await;
        assert!(matches!(result, Err(AdapterError::Unsupported(_))));
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib adapters::standalone::tests`
Expected: FAIL to compile — `cannot find type \`StandaloneAdapter\` in this scope` (and `Detected`, `CommandSpec`, `OutputUse`, `Scope`, `ArtifactKey`, `ManagerInstance`, `AdapterError`, `CommandRunner`, `CancellationToken`, `Arc`, `Duration` — all brought in by Step 3's imports; `async_trait` the test module imports itself).

- [ ] **Step 3: Write the adapter**

In `crates/banager-core/src/adapters/standalone/mod.rs`, after the `pub mod` lines and before `#[cfg(test)] pub(super) mod testing`, insert:

```rust

use self::recipe::Recipe;
use self::route::Probe;
use crate::adapters::{reconcile_from, AdapterError, AdapterMeta};
use crate::http::HttpClient;
use crate::model::{
    ArtifactKey, ArtifactKind, InstallReason, InstalledArtifact, InstanceNote, InstanceStatus,
    ManagerInstance, Reconciled, Scope, SearchHit, Unavailable, UninstallBlocked,
};
use crate::runner::{CommandRunner, CommandSpec, HostEnv, OutputUse};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// How long a version read may take before the runner stops it and the
/// instance is reported as not responding (the same 30 s every adapter
/// gives `--version`).
const VERSION_TIMEOUT: Duration = Duration::from_secs(30);

/// What `detect` learned that the `Adapter` methods without a `HostEnv`
/// need later -- the same seat `CargoAdapter.binstall` is (detect writes,
/// later calls read; `Session` always detects before it asks anything
/// else of an instance). In this step: `home`, which `check_updates` needs
/// to find `~/.claude/settings.json`. Step C adds `euid` (the removal's
/// ownership check), step E `cargo_home` (rustup's cargo lock).
pub struct Detected {
    pub home: PathBuf,
}

/// One tool installed by its own installer, as the `Adapter` contract
/// sees it. Built once per `Recipe` by `all()`; the instance it detects
/// *is* the native install (spec D2).
pub struct StandaloneAdapter {
    recipe: &'static Recipe,
    meta: AdapterMeta,
    runner: Arc<dyn CommandRunner>,
    /// The channel pointer request in `check_updates` (stage 7 of this task).
    http: Arc<dyn HttpClient>,
    detected: Mutex<Option<Detected>>,
}

impl StandaloneAdapter {
    /// Panics on a meta file that does not parse or whose `id` is not
    /// `standalone-<recipe.id>`: both are compile-time data
    /// (`recipes::tests` holds every recipe to them), never a state of a
    /// user's Mac.
    pub fn new(
        recipe: &'static Recipe,
        runner: Arc<dyn CommandRunner>,
        http: Arc<dyn HttpClient>,
    ) -> StandaloneAdapter {
        let meta = AdapterMeta::from_toml(recipe.meta_toml).unwrap_or_else(|e| {
            panic!("adapters/meta/standalone-{}.toml must parse: {e}", recipe.id)
        });
        assert_eq!(
            meta.id,
            format!("standalone-{}", recipe.id),
            "the meta file's id must be the recipe's adapter id"
        );
        StandaloneAdapter {
            recipe,
            meta,
            runner,
            http,
            detected: Mutex::new(None),
        }
    }

    /// Spec §3.3, steps 1-6: the launcher at the installer's fixed path
    /// (never `resolve_exe`, spec D3), the fingerprint, the version read,
    /// the PATH note. One instance or none; never two.
    pub async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        let launcher = route::expand(&env.home, self.recipe.route.launcher);
        let root = route::expand(&env.home, self.recipe.route.root);
        let (version, unavailable, notes) =
            match route::probe(self.recipe.route.kind, &launcher, &root) {
                Probe::Absent => return Vec::new(),
                // No program to ask: no version read; not unavailable,
                // because nothing about the source has stopped answering --
                // the state is on the row's note, and from step C the
                // uninstall that finishes it must be allowed on this
                // instance (spec Q17).
                Probe::LauncherOnly => (None, None, vec![InstanceNote::LauncherOnly]),
                Probe::Present { real } => {
                    let version = self.read_version(&launcher).await;
                    // The state axis, exactly as uv's rule: the launcher is
                    // there and is ours, it just did not answer.
                    let unavailable = version.is_none().then_some(Unavailable::NotResponding);
                    let notes = route::shadow_note(self.recipe.id, env, &real)
                        .into_iter()
                        .collect();
                    (version, unavailable, notes)
                }
            };
        *self.detected.lock().unwrap() = Some(Detected {
            home: env.home.clone(),
        });
        let unverified_version = self.meta.unverified_version(&version);
        vec![ManagerInstance {
            // Bare adapter id: one native install per tool is the real
            // cardinality (spec §2.1), and this id is persisted in
            // `Settings.ignored_updates`.
            id: crate::model::instance_id(&self.meta.id, None),
            adapter_id: self.meta.id.clone(),
            // The launcher itself: the program every plan runs, the path
            // the notices take the command name from, the raw path the
            // Unknown page's rule 0 matches.
            exe_path: launcher,
            // The tool's own root: what the launcher must resolve into,
            // and (Task 3, stage 9) an owned root for the Unknown page's rule 3.
            prefix: root,
            scope: Scope::User,
            status: InstanceStatus { unavailable, notes },
            version,
            unverified_version,
            read_only_reason: None,
        }]
    }

    /// `<launcher> --version` with the recipe's environment (the updater
    /// switched off, spec §3.4), parsed per the recipe; `None` when it did
    /// not exit 0, timed out, could not be spawned, or printed no version.
    async fn read_version(&self, launcher: &Path) -> Option<String> {
        let cmd = &self.recipe.version;
        let output = self
            .runner
            .run(
                CommandSpec {
                    program: launcher.to_path_buf(),
                    args: cmd.args.iter().map(|a| a.to_string()).collect(),
                    env: cmd
                        .env
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect(),
                    cwd: None,
                    timeout: VERSION_TIMEOUT,
                    output_use: OutputUse::Parsed,
                },
                None,
                CancellationToken::new(),
            )
            .await;
        match output {
            Ok(o) if o.exit_code == Some(0) && !o.timed_out && !o.cancelled => {
                latest::parse_version(&o.stdout, cmd.parse)
            }
            _ => None,
        }
    }

    /// The one artifact's key: the tool id as the name (what `OpRequest`
    /// hands back and `reconcile_from` matches on), `Binary` as the kind
    /// (a standalone tool is a binary; `ArtifactKey` includes the instance
    /// id, so cargo's `Binary` artifacts never collide).
    fn artifact_key(&self, inst: &ManagerInstance) -> ArtifactKey {
        ArtifactKey {
            instance_id: inst.id.clone(),
            kind: ArtifactKind::Binary,
            name: self.recipe.id.to_string(),
        }
    }

    /// The tool itself, read from the disk again -- not detect's answer
    /// cached: `refresh` calls this under the instance lock and
    /// `run_operation`'s reconcile after an operation must see what is
    /// there now (spec §3.6). The launcher and root are the instance's own
    /// `exe_path` and `prefix`, which detect expanded.
    pub async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        let (version, path) = match route::probe(self.recipe.route.kind, &inst.exe_path, &inst.prefix) {
            Probe::Absent => return Ok(Vec::new()),
            Probe::LauncherOnly => (String::new(), None),
            Probe::Present { real } => (
                self.read_version(&inst.exe_path).await.unwrap_or_default(),
                Some(real),
            ),
        };
        Ok(vec![InstalledArtifact {
            key: self.artifact_key(inst),
            // From the meta TOML, not a second copy in the recipe.
            display_name: self.meta.name.clone(),
            version,
            // The user ran the installer themselves; `Dependency` would fold
            // the row behind "N components".
            reason: InstallReason::Requested,
            // A sentence has to be localised, and this field is a bare
            // string that does not know the UI language: the Installed
            // page reads `STANDALONE_SUMMARY_KEYS` by adapter id instead.
            description: None,
            homepage: Some(self.meta.homepage.clone()),
            size_bytes: None,
            installed_at: None,
            // The real binary: the Unknown page's rule 2.
            path,
            // Read by the Updates page's `selfUpdatingHint` sentence.
            auto_updates: self.recipe.self_updates,
            // No uninstall method in this step (spec §6.1 "Neither"): the
            // gate refuses, the page hides the button and says why. Step
            // C's path-list uninstall replaces this with `None`.
            uninstall_blocked: Some(UninstallBlocked::NoSafeMethod),
        }])
    }

    /// Discovery is phase 5; a standalone tool has nothing to search anyway.
    pub async fn search(
        &self,
        _inst: &ManagerInstance,
        _query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        Err(AdapterError::Unsupported(format!(
            "{} is one tool with nothing to search; installing tools is phase 5",
            self.meta.name
        )))
    }

    /// B only executes upgrades: an owned launcher without a readable
    /// version is not sufficient evidence that an upgrade succeeded.
    /// Inventory still preserves `LauncherOnly` presence for display and
    /// step C's removal. C must use that presence for uninstall verification
    /// while keeping this stricter upgrade check (see deviations).
    pub async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        let artifacts = self.inventory(inst).await?;
        let reconciled = reconcile_from(artifacts, key);
        if reconciled.present && reconciled.version.as_deref().is_none_or(str::is_empty) {
            return Err(AdapterError::Parse(
                "cannot verify the standalone launcher's installed version".to_string(),
            ));
        }
        Ok(reconciled)
    }
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib adapters::standalone`
Expected: the focused adapter tests pass. The completed task's clippy gate runs after stage 9, when `http`, `Detected.home`, all recipe fields and `all()` have production readers.

- [ ] **Step 5: Continue the core task**

Continue to the next stage. The complete core task runs `cargo fmt --all` and all five gates, then commits once, after stage 9. No partial type-only commit or dead-code exemption.

---

#### Stage 7: `check_updates`, the channel fetch, `downloads.claude.ai` on the allowlist

**Files:**
- Modify: `crates/banager-core/src/adapters/standalone/mod.rs` (`check_updates`, `latest_version`; tests)]` line on `http`, now that this task reads the field)
- Modify: `crates/banager-core/src/adapters/standalone/recipes.rs` (one test)
- Modify: `crates/banager-core/src/http/real.rs` — A's `ALLOWED_HTTPS_HOSTS` constant, its doc comment, and one new test in `mod tests`  [A's file: anchor by the constant and the test names]
- Modify: `docs/what-we-run.md` — the table under `## Network: Banager only connects to these hosts`  [A's file: anchor by the heading and the `registry.ollama.ai` row]
- Test: `mod tests` in `standalone/mod.rs`, `recipes.rs`, `real.rs`; A's `tests/what_we_run_test.rs` (`test_what_we_run_names_every_allowed_https_host`) keeps passing.

**Interfaces:**
- Consumes: `Latest::ClaudeChannel`, `latest::{claude_channel, CHANNEL_LATEST, parse_channel_body, compare_dotted}` (Task 3, stage 3), `Detected.home` (Task 3, stage 6), `HttpClient::send`/`HttpRequest` (`http/mod.rs:18-46`), `uncheckable_candidate` (`adapters/mod.rs:269-284`), `UpdateCandidate`/`UpdateChannel::Registry` (`model.rs:313-327`, `:235-240`), `CheckOptions`/`CheckOutcome` (`adapters/mod.rs:26-80`), A's `host_allowed` (`banager_core::http::real::host_allowed`).
- Produces (verbatim): `pub async fn check_updates(&self, inst: &ManagerInstance, opts: &CheckOptions) -> Result<CheckOutcome, AdapterError>` (reader: `impl Adapter`, Task 3, stage 8 → `Session::refresh`); `"downloads.claude.ai"` in `ALLOWED_HTTPS_HOSTS` (readers: A's `host_allowed` in `send`, the doc table, A's `what_we_run_test`, and `recipes::tests::test_every_recipe_latest_url_is_an_allowed_https_host` here).

Rules (spec §4.1, §4.3, §4.4, D4, D5): the request goes to `{base}/{channel}`, 30 s, `GET`, no headers of Banager's own; a candidate only when `compare_dotted(current, remote) == Less`, with `channel: Registry`, `checkable: true`, `warnings: []`, `blocked: None`, `key` equal to the artifact's; equal or older remote → no candidate (never "everything is up to date" from a `stable` pointer that is behind); a network failure, a non-200, a body that is not a version, or an incomparable pair → one `uncheckable_candidate` naming the reason (never an `Err`, which would mark the whole source stale); `CheckOptions.include_self_updating` is not read (that switch is Homebrew's `--greedy`; the badge here is read from the launcher's live version and is true either way); `notes` empty.

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` in `crates/banager-core/src/adapters/standalone/mod.rs` (before its closing `}`); also add `use crate::adapters::CheckOptions; use crate::http::HttpResponse; use crate::model::{UpdateChannel, Warning};` to that module's `use` lines:

```rust

    const LATEST_URL: &str = "https://downloads.claude.ai/claude-code-releases/latest";
    const STABLE_URL: &str = "https://downloads.claude.ai/claude-code-releases/stable";

    fn answer(body: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            body: body.to_string(),
        }
    }

    /// An adapter that has detected the layout in `home` (so `Detected`
    /// holds that home), over `http`.
    async fn detected_adapter(
        home: &TempHome,
        layout: &super::testing::ClaudeLayout,
        http: Arc<MockHttpClient>,
    ) -> (StandaloneAdapter, ManagerInstance) {
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = StandaloneAdapter::new(&CLAUDE, runner, http);
        let inst = adapter
            .detect(&home.env(vec![home.path().join(".local/bin")]))
            .await
            .remove(0);
        (adapter, inst)
    }

    #[tokio::test]
    async fn test_check_updates_uses_the_version_after_inventory_not_detects_version() {
        let home = TempHome::new("check-fresh");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        let argv = vec![layout.launcher.to_str().unwrap(), "--version"];
        runner.respond(argv.clone(), exited_0("2.1.281 (Claude Code)\n"));
        let http = Arc::new(MockHttpClient::new());
        http.respond(LATEST_URL, answer("2.1.290"));
        let adapter = StandaloneAdapter::new(&CLAUDE, runner.clone(), http);
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        runner.respond(argv, exited_0("2.1.290 (Claude Code)\n"));
        assert_eq!(adapter.inventory(&inst).await.unwrap()[0].version, "2.1.290");
        assert_eq!(inst.version.as_deref(), Some("2.1.281"));
        assert!(adapter.check_updates(&inst, &CheckOptions::default()).await.unwrap().candidates.is_empty());
        assert_eq!(runner.calls().len(), 3, "detect, inventory, then a fresh check read");
    }

    #[tokio::test]
    async fn test_prereleases_stay_available_and_incomparable_pairs_are_uncheckable() {
        for (local, remote) in [
            ("2.1.281-beta", "2.1.290"),
            ("2.1.281", "2.1.290-beta"),
            ("2.1.281+build.7", "2.1.290+build.8"),
        ] {
            let home = TempHome::new("check-prerelease");
            let layout = claude_layout(&home, local);
            let runner = Arc::new(MockRunner::new());
            runner.respond(vec![layout.launcher.to_str().unwrap(), "--version"], exited_0(&format!("{local} (Claude Code)\n")));
            let http = Arc::new(MockHttpClient::new());
            http.respond(LATEST_URL, answer(remote));
            let adapter = StandaloneAdapter::new(&CLAUDE, runner, http);
            let inst = adapter.detect(&home.env(vec![])).await.remove(0);
            assert_eq!(inst.status.unavailable, None);
            assert_eq!(inst.version.as_deref(), Some(local));
            assert_eq!(adapter.inventory(&inst).await.unwrap()[0].version, local);
            let out = adapter.check_updates(&inst, &CheckOptions::default()).await.unwrap();
            assert_eq!(out.candidates.len(), 1);
            let candidate = &out.candidates[0];
            assert!(!candidate.checkable);
            assert_eq!(candidate.current, local);
            assert_eq!(candidate.target, local, "existing uncheckable candidate convention");
            assert!(matches!(&candidate.warnings[..], [Warning::Message(message)]
                if message.contains("cannot compare") && message.contains(local) && message.contains(remote)));
        }
    }

    #[tokio::test]
    async fn test_check_updates_does_not_use_a_stale_version_after_a_failed_read() {
        let home = TempHome::new("check-failed-live-read");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        let argv = vec![layout.launcher.to_str().unwrap(), "--version"];
        runner.respond(argv.clone(), exited_0("2.1.281 (Claude Code)\n"));
        let http = Arc::new(MockHttpClient::new());
        http.respond(LATEST_URL, answer("2.1.290"));
        let adapter = StandaloneAdapter::new(&CLAUDE, runner.clone(), http.clone());
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        runner.respond(argv, exited_0(""));
        let out = adapter.check_updates(&inst, &CheckOptions::default()).await.unwrap();
        assert_eq!(out.candidates.len(), 1);
        assert!(!out.candidates[0].checkable);
        assert_eq!(out.candidates[0].current, "2.1.281");
        assert!(http.calls().is_empty());
    }

    #[tokio::test]
    async fn test_check_updates_lists_a_newer_published_version_as_one_candidate() {
        let home = TempHome::new("check-newer");
        let layout = claude_layout(&home, "2.1.281");
        let http = Arc::new(MockHttpClient::new());
        http.respond(LATEST_URL, answer("2.1.290\n"));
        let (adapter, inst) = detected_adapter(&home, &layout, http.clone()).await;

        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");

        assert!(out.notes.is_empty());
        assert_eq!(
            out.candidates,
            vec![UpdateCandidate {
                key: ArtifactKey {
                    instance_id: "standalone-claude".to_string(),
                    kind: ArtifactKind::Binary,
                    name: "claude".to_string(),
                },
                current: "2.1.281".to_string(),
                target: "2.1.290".to_string(),
                channel: UpdateChannel::Registry,
                checkable: true,
                warnings: Vec::new(),
                blocked: None,
            }]
        );
        assert_eq!(http.calls(), vec![LATEST_URL.to_string()]);
        let request = &http.requests()[0];
        assert_eq!(request.method, "GET");
        assert!(request.headers.is_empty(), "nothing of Banager's own but the client's UA");
        assert_eq!(request.timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_check_updates_lists_nothing_when_the_pointer_is_equal_or_behind() {
        // The `stable` pointer was 2.1.273 while 2.1.281 was installed
        // (recorded, Task 3, stage 9): "different" would badge a downgrade. Only
        // remote > local is an update (spec §4.3, D4).
        for body in ["2.1.281", "2.1.273\n", "2.0.999"] {
            let home = TempHome::new("check-not-newer");
            let layout = claude_layout(&home, "2.1.281");
            let http = Arc::new(MockHttpClient::new());
            http.respond(LATEST_URL, answer(body));
            let (adapter, inst) = detected_adapter(&home, &layout, http).await;
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("check_updates");
            assert!(out.candidates.is_empty(), "{body:?}");
        }
    }

    #[tokio::test]
    async fn test_check_updates_asks_the_stable_pointer_when_settings_say_so() {
        // Q7: `~/.claude/settings.json` `autoUpdatesChannel: "stable"`
        // picks the other pointer; missing or malformed is `latest`.
        let home = TempHome::new("check-stable");
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude");
        std::fs::write(
            home.path().join(".claude/settings.json"),
            r#"{"autoUpdatesChannel":"stable"}"#,
        )
        .expect("write settings");
        let http = Arc::new(MockHttpClient::new());
        http.respond(STABLE_URL, answer("2.1.273"));
        let (adapter, inst) = detected_adapter(&home, &layout, http.clone()).await;
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert!(out.candidates.is_empty());
        assert_eq!(http.calls(), vec![STABLE_URL.to_string()]);

        std::fs::write(home.path().join(".claude/settings.json"), "{ not json").expect("write");
        let http = Arc::new(MockHttpClient::new());
        http.respond(LATEST_URL, answer("2.1.281"));
        let (adapter, inst) = detected_adapter(&home, &layout, http.clone()).await;
        adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert_eq!(http.calls(), vec![LATEST_URL.to_string()]);
    }

    #[tokio::test]
    async fn test_check_updates_marks_a_failed_request_uncheckable_never_an_error() {
        // A network failure is "Banager could not find out", not a failed
        // source (which would hold the snapshot stale): one row at the
        // installed version, `checkable: false`, with the reason.
        let home = TempHome::new("check-network");
        let layout = claude_layout(&home, "2.1.281");
        let http = Arc::new(MockHttpClient::new());
        http.fail(LATEST_URL, "connection refused");
        let (adapter, inst) = detected_adapter(&home, &layout, http).await;
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("a failed lookup is not a source failure");
        assert_eq!(out.candidates.len(), 1);
        let c = &out.candidates[0];
        assert!(!c.checkable);
        assert_eq!(c.current, "2.1.281");
        assert_eq!(c.target, "2.1.281");
        assert_eq!(c.channel, UpdateChannel::Registry);
        assert!(matches!(&c.warnings[..], [Warning::Message(m)] if m.contains("connection refused")));
    }

    #[tokio::test]
    async fn test_check_updates_marks_a_non_200_uncheckable() {
        let home = TempHome::new("check-status");
        let layout = claude_layout(&home, "2.1.281");
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            LATEST_URL,
            HttpResponse {
                status: 503,
                body: "<html>busy</html>".to_string(),
            },
        );
        let (adapter, inst) = detected_adapter(&home, &layout, http).await;
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert_eq!(out.candidates.len(), 1);
        assert!(!out.candidates[0].checkable);
        assert!(matches!(&out.candidates[0].warnings[..], [Warning::Message(m)] if m.contains("503")));
    }

    #[tokio::test]
    async fn test_check_updates_marks_a_non_version_answer_uncheckable() {
        // An HTML page with status 200 (a captive portal), multiple tokens:
        // no candidate is built from it, and the reason quotes only a
        // little of the body.
        for body in ["<html><body>Sign in to the network</body></html>", "2.1.290 extra"] {
            let home = TempHome::new("check-garbage");
            let layout = claude_layout(&home, "2.1.281");
            let http = Arc::new(MockHttpClient::new());
            http.respond(LATEST_URL, answer(body));
            let (adapter, inst) = detected_adapter(&home, &layout, http).await;
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("check_updates");
            assert_eq!(out.candidates.len(), 1, "{body:?}");
            assert!(!out.candidates[0].checkable);
            assert!(
                matches!(&out.candidates[0].warnings[..], [Warning::Message(m)] if m.contains("did not answer with a version") && m.len() < 120),
                "{:?}",
                out.candidates[0].warnings
            );
        }
    }

    #[tokio::test]
    async fn test_check_updates_ignores_the_include_self_updating_switch() {
        // D5: the switch is Homebrew's `--greedy`, for casks whose live
        // version `brew outdated` cannot see. This badge is read from the
        // launcher's live version and is true whatever the switch says.
        for include_self_updating in [false, true] {
            let home = TempHome::new("check-greedy");
            let layout = claude_layout(&home, "2.1.281");
            let http = Arc::new(MockHttpClient::new());
            http.respond(LATEST_URL, answer("2.1.290"));
            let (adapter, inst) = detected_adapter(&home, &layout, http).await;
            let out = adapter
                .check_updates(
                    &inst,
                    &CheckOptions {
                        include_self_updating,
                        ..CheckOptions::default()
                    },
                )
                .await
                .expect("check_updates");
            assert_eq!(out.candidates.len(), 1, "include_self_updating={include_self_updating}");
        }
    }

    #[tokio::test]
    async fn test_check_updates_asks_nothing_for_a_launcher_only_row() {
        // No installed version to compare, so no request and no row: the
        // notice already says what is left.
        let home = TempHome::new("check-launcher-only");
        let root = home.path().join(".local/share/claude");
        let launcher = home.link(".local/bin/claude", &root.join("versions/2.1.281"));
        let http = Arc::new(MockHttpClient::new());
        let adapter = StandaloneAdapter::new(&CLAUDE, Arc::new(MockRunner::new()), http.clone());
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        assert_eq!(inst.exe_path, launcher);
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        assert!(out.candidates.is_empty());
        assert!(http.calls().is_empty());
    }
```

Append inside `mod tests` in `crates/banager-core/src/adapters/standalone/recipes.rs`:

```rust

    #[test]
    fn test_every_recipe_latest_url_is_an_allowed_https_host() {
        // Spec §4.2: a recipe that brings a new host and the allowlist
        // entry are one reviewed change; `RealHttpClient::send` refuses
        // anything else before connecting, so a recipe whose host is not
        // on the list would be a permanent "could not check" row.
        use crate::http::real::host_allowed;
        for recipe in RECIPES {
            let urls: Vec<String> = match recipe.latest {
                Latest::ClaudeChannel { base } => vec![
                    format!("{base}/{}", crate::adapters::standalone::latest::CHANNEL_LATEST),
                    format!("{base}/{}", crate::adapters::standalone::latest::CHANNEL_STABLE),
                ],
            };
            for url in urls {
                host_allowed(&url).unwrap_or_else(|e| panic!("{}: {url}: {e}", recipe.id));
            }
        }
    }
```

In `crates/banager-core/src/http/real.rs`, append inside `mod tests`, after A's `test_real_http_client_refuses_an_https_host_off_the_list_before_connecting` (the last test A added, ending with `other => panic!("expected a host-not-allowed error, got {other:?}"), } }`) and before the module's closing `}`:

```rust

    #[test]
    fn test_host_allowed_accepts_claude_codes_channel_pointers() {
        // The exact URLs `StandaloneAdapter::check_updates` builds for the
        // `CLAUDE` recipe (adapters/standalone/recipes.rs), phase 4 step B.
        host_allowed("https://downloads.claude.ai/claude-code-releases/latest")
            .expect("downloads.claude.ai, latest");
        host_allowed("https://downloads.claude.ai/claude-code-releases/stable")
            .expect("downloads.claude.ai, stable");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib adapters::standalone`
Expected: FAIL to compile — `no method named \`check_updates\` found for struct \`StandaloneAdapter\``. And `cargo test -p banager-core --lib http::real::tests::test_host_allowed_accepts_claude_codes_channel_pointers` FAILS: `host not allowed: "downloads.claude.ai" is not one of ["crates.io", "pypi.org", "registry.ollama.ai"]`. (`test_every_recipe_latest_url_is_an_allowed_https_host` fails the same way once the standalone module compiles.)

- [ ] **Step 3: Write `check_updates`, add the host, fix the comment, add the doc row**

In `crates/banager-core/src/adapters/standalone/mod.rs`, add to the imports `use self::recipe::Latest; use crate::adapters::{uncheckable_candidate, CheckOptions, CheckOutcome}; use crate::http::HttpRequest; use crate::model::{UpdateCandidate, UpdateChannel}; use std::cmp::Ordering;` (merge into the existing `use` lines), and insert after `search` (before `reconcile`):

```rust

    /// Spec §4.1/§4.3/D4/D5: a fresh installed version reading against
    /// the published one; a candidate only when the
    /// published one is greater, comparing dotted integers -- Claude
    /// Code's `stable` pointer sits behind its `latest`, so "different"
    /// would be a downgrade badge. Anything that stops the comparison (no
    /// network, a non-200, a body that is not a version, an incomparable
    /// pair) is one "could not check" row, never an `Err`: a failed lookup
    /// is not knowing, and an `Err` would hold the whole source stale.
    ///
    /// `include_self_updating` is not read: that is Homebrew's `--greedy`
    /// for casks whose live version `brew outdated` cannot see. This badge
    /// compares the launcher's live version and is true whatever the
    /// switch says; that the tool usually updates itself is said on the
    /// row (`selfUpdatingHint`), not hidden behind a setting.
    pub async fn check_updates(
        &self,
        inst: &ManagerInstance,
        _opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        // Detect's version predates refresh's inventory; an auto-update
        // can happen between them. Re-probe and read now, with the same
        // version command environment as detect/inventory/reconcile.
        match route::probe(self.recipe.route.kind, &inst.exe_path, &inst.prefix) {
            Probe::Absent | Probe::LauncherOnly => return Ok(CheckOutcome::default()),
            Probe::Present { .. } => {}
        }
        let key = self.artifact_key(inst);
        let Some(current) = self.read_version(&inst.exe_path).await else {
            return Ok(vec![uncheckable_candidate(
                key,
                inst.version.clone().unwrap_or_default(),
                UpdateChannel::Registry,
                "cannot read the installed version now".to_string(),
            )].into());
        };
        let remote = match self.latest_version().await {
            Ok(remote) => remote,
            Err(reason) => {
                return Ok(vec![uncheckable_candidate(
                    key,
                    current,
                    UpdateChannel::Registry,
                    reason,
                )]
                .into())
            }
        };
        Ok(match latest::compare_dotted(&current, &remote) {
            Some(Ordering::Less) => vec![UpdateCandidate {
                key,
                current,
                target: remote,
                channel: UpdateChannel::Registry,
                checkable: true,
                warnings: Vec::new(),
                blocked: None,
            }],
            Some(Ordering::Equal | Ordering::Greater) => Vec::new(),
            None => {
                let reason = format!(
                    "cannot compare the installed version {current:?} with the published {remote:?}"
                );
                vec![uncheckable_candidate(
                    key,
                    current,
                    UpdateChannel::Registry,
                    reason,
                )]
            }
        }
        .into())
    }

    /// The newest published version per the recipe's `Latest`, or the
    /// reason it could not be read (one line, for an uncheckable row).
    async fn latest_version(&self) -> Result<String, String> {
        match self.recipe.latest {
            Latest::ClaudeChannel { base } => {
                // `home` from detect's seat; before any detect (which
                // `Session` never does) the default channel is as good an
                // answer as any.
                let home = self
                    .detected
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|d| d.home.clone());
                let channel = match home {
                    Some(home) => latest::claude_channel(&home),
                    None => latest::CHANNEL_LATEST,
                };
                let url = format!("{base}/{channel}");
                let resp = self
                    .http
                    .send(HttpRequest {
                        method: "GET",
                        url,
                        headers: Vec::new(),
                        timeout: Duration::from_secs(30),
                    })
                    .await
                    .map_err(|e| format!("downloads.claude.ai request failed: {e}"))?;
                if resp.status != 200 {
                    return Err(format!("downloads.claude.ai returned status {}", resp.status));
                }
                latest::parse_channel_body(&resp.body)
            }
        }
    }
```

In `crates/banager-core/src/http/real.rs`, change A's constant

```rust
pub const ALLOWED_HTTPS_HOSTS: &[&str] = &["crates.io", "pypi.org", "registry.ollama.ai"];
```

to

```rust
pub const ALLOWED_HTTPS_HOSTS: &[&str] = &[
    "crates.io",
    "pypi.org",
    "registry.ollama.ai",
    "downloads.claude.ai",
];
```

and, in the doc comment above it, change the sentence that reads

```rust
/// Every https URL this crate builds names one of these: crates.io
/// (`CargoAdapter::latest_stable_version`), pypi.org
/// (`PipxAdapter::latest_pypi_version`) and registry.ollama.ai
/// (`OllamaAdapter::compare_digests`). `send` refuses any other https host
```

to

```rust
/// Every https URL this crate builds names one of these: crates.io
/// (`CargoAdapter::latest_stable_version`), pypi.org
/// (`PipxAdapter::latest_pypi_version`), registry.ollama.ai
/// (`OllamaAdapter::compare_digests`) and downloads.claude.ai
/// (`StandaloneAdapter::check_updates`, Claude Code's channel pointer).
/// `send` refuses any other https host
```

In `docs/what-we-run.md`, under `## Network: Banager only connects to these hosts`, after the table row that begins `| \`registry.ollama.ai\` |`, add:

```markdown
| `downloads.claude.ai` | `GET /claude-code-releases/latest` or `/stable` — the newest published Claude Code version on that channel, answered as one bare version number | Claude Code's `check_updates` (`StandaloneAdapter`) |
```

(Same "By" form as A's three rows, which name the adapter method and nothing else; A's `test_what_we_run_names_every_allowed_https_host` needs only the host string somewhere in the file. The `## Claude Code` section does not exist until Task 3, stage 9's commit, so the row must not point at it; Task 3, stage 9 changes nothing here.)

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib adapters::standalone` and `cargo test -p banager-core --lib http::real` and `cargo test -p banager-core --test what_we_run_test`
Expected: PASS — all `check_updates` tests, including fresh reads and incomparable versions, the recipes host test, the `real.rs` test; A's four document tests still green (the host is now named in the doc).

- [ ] **Step 5: Continue the core task**

Continue to the next stage. The complete core task runs `cargo fmt --all` and all five gates, then commits once, after stage 9. No partial type-only commit or dead-code exemption.

---

#### Stage 8: `plan`/`execute`, `impl Adapter`, `all()`, the end-to-end `UnchangedAfterUpgrade` case

**Files:**
- Modify: `crates/banager-core/src/adapters/standalone/mod.rs` (`plan`, `execute`, `impl Adapter for StandaloneAdapter`, `pub fn all`; tests)
- Modify: `crates/banager-core/tests/ops_upgrade_version_test.rs` (imports at `:21-35`; two tests and one helper appended at the end of the file)
- Test: both.

**Interfaces:**
- Consumes: `UpgradeCmd` (Task 3, stage 3), `ensure_instance_match`, `validate_package_name`, `run_plan` (`adapters/mod.rs:414-422`, `:348-365`, `:469-514`), `Plan`/`OpRequest`/`OpKind`/`ResourceLock` (`model.rs:337-387`), `AdapterError::{Unsupported, InvalidName, UninstallBlocked}`, `Adapter` (`adapters/mod.rs:424-455`), the `ScriptedRunner`/`exited_0`/`upgrade` helpers of `ops_upgrade_version_test.rs` (`:44-51`, `:81-126`, `:129-157`).
- Produces (verbatim): `pub async fn plan(...)`, `pub async fn execute(...)` (spec §五's table row for claude: `program` = the launcher, `args` = `["update"]`, `env` = `[]`, 1800 s, `KillThenReconcile`, `locks` = `[inst.id]`, `needs_password: false`, no warnings, no affected); `impl Adapter for StandaloneAdapter` (reader: `Session::build`'s `ops.register_adapter`, `refresh_round`, `issue_plan`); `pub fn all(runner, http) -> Vec<Arc<dyn Adapter>>` (reader: `Session::new`, Task 3, stage 9).

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` in `crates/banager-core/src/adapters/standalone/mod.rs`; add `use crate::adapters::Adapter; use crate::events::VecSink; use crate::model::{CancelPolicy, OpKind, OpRequest, Outcome, ResourceLock};` to its `use` lines:

```rust

    fn request(kind: OpKind, artifact_kind: ArtifactKind, name: &str) -> OpRequest {
        OpRequest {
            kind,
            instance_id: "standalone-claude".to_string(),
            artifact_kind,
            name: name.to_string(),
        }
    }

    #[tokio::test]
    async fn test_plan_upgrade_is_the_tools_own_update_command_without_the_version_env() {
        // Spec §五: `<launcher> update`, 1800 s, KillThenReconcile, the
        // instance's own lock, no password. Version reads add
        // `DISABLE_AUTOUPDATER=1`; upgrade adds no environment override.
        // RealRunner inherits ambient variables, including this one.
        let home = TempHome::new("plan-upgrade");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let inst = instance_for(&layout, Some("2.1.281"));

        let plan = adapter
            .plan(&inst, &request(OpKind::Upgrade, ArtifactKind::Binary, "claude"))
            .await
            .expect("plan");

        assert_eq!(plan.program, layout.launcher);
        assert_eq!(plan.args, vec!["update".to_string()]);
        assert!(plan.env.is_empty(), "upgrade adds no environment override");
        assert!(!plan.needs_password);
        assert_eq!(plan.locks, vec![ResourceLock("standalone-claude".to_string())]);
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);
        assert!(plan.warnings.is_empty());
        assert!(plan.affected.is_empty());
        assert_eq!(plan.timeout_secs, 1800);
        assert_eq!(plan.request.name, "claude");
    }

    #[tokio::test]
    async fn test_plan_refuses_a_request_for_another_instance() {
        let home = TempHome::new("plan-other-instance");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let req = OpRequest {
            instance_id: "standalone-grok".to_string(),
            ..request(OpKind::Upgrade, ArtifactKind::Binary, "claude")
        };
        assert!(matches!(
            adapter.plan(&instance_for(&layout, Some("2.1.281")), &req).await,
            Err(AdapterError::Refused(_))
        ));
    }

    #[tokio::test]
    async fn test_plan_refuses_a_name_or_kind_that_is_not_this_tool() {
        // The one artifact is `Binary`/`claude`; anything else is a request
        // this adapter cannot mean, refused before an argv exists.
        let home = TempHome::new("plan-wrong-name");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let inst = instance_for(&layout, Some("2.1.281"));
        for (kind, name) in [
            (ArtifactKind::Binary, "grok"),
            (ArtifactKind::Binary, "claude-code"),
            (ArtifactKind::Tool, "claude"),
        ] {
            assert!(
                matches!(
                    adapter.plan(&inst, &request(OpKind::Upgrade, kind, name)).await,
                    Err(AdapterError::InvalidName(_))
                ),
                "{kind:?} {name}"
            );
        }
        // A name that fails `validate_package_name` is refused as such
        // before the tool-name comparison.
        assert!(matches!(
            adapter.plan(&inst, &request(OpKind::Upgrade, ArtifactKind::Binary, "-rf")).await,
            Err(AdapterError::InvalidName(_))
        ));
    }

    #[tokio::test]
    async fn test_plan_refuses_install_as_unsupported_and_uninstall_as_no_safe_method() {
        // Both are refused earlier by the gate (`SourceGone` for a tool
        // that is not installed; `blocked_uninstall` for the artifact's
        // `NoSafeMethod`); these are the adapter's own answers for a stale
        // snapshot that reaches it anyway.
        let home = TempHome::new("plan-refusals");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let inst = instance_for(&layout, Some("2.1.281"));
        assert!(matches!(
            adapter.plan(&inst, &request(OpKind::Install, ArtifactKind::Binary, "claude")).await,
            Err(AdapterError::Unsupported(_))
        ));
        match adapter
            .plan(&inst, &request(OpKind::Uninstall, ArtifactKind::Binary, "claude"))
            .await
        {
            Err(AdapterError::UninstallBlocked { reason }) => {
                assert_eq!(reason, UninstallBlocked::NoSafeMethod)
            }
            other => panic!("expected UninstallBlocked(NoSafeMethod), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_execute_runs_the_plan_and_streams_its_output() {
        let home = TempHome::new("execute");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "update"],
            exited_0("Successfully updated from 2.1.281 to version 2.1.290\n"),
        );
        let adapter = adapter(runner);
        let inst = instance_for(&layout, Some("2.1.281"));
        let plan = adapter
            .plan(&inst, &request(OpKind::Upgrade, ArtifactKind::Binary, "claude"))
            .await
            .expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome = adapter
            .execute(&plan, sink.clone(), 1, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(sink.snapshot().len(), 1, "one log line, streamed");
    }

    #[tokio::test]
    async fn test_all_builds_one_adapter_per_recipe_under_its_standalone_id() {
        let adapters = all(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let ids: Vec<String> = adapters.iter().map(|a| a.meta().id.clone()).collect();
        assert_eq!(ids, vec!["standalone-claude".to_string()]);
        assert_eq!(adapters.len(), super::recipes::RECIPES.len());
    }

    #[tokio::test]
    async fn test_the_adapter_trait_delegates_to_the_inherent_methods() {
        // Through `dyn Adapter`, as Session sees it.
        let home = TempHome::new("trait");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter: Arc<dyn Adapter> =
            Arc::new(StandaloneAdapter::new(&CLAUDE, runner, Arc::new(MockHttpClient::new())));
        let instances = adapter.detect(&home.env(vec![])).await;
        assert_eq!(instances.len(), 1);
        let artifacts = adapter.inventory(&instances[0]).await.expect("inventory");
        assert_eq!(artifacts[0].display_name, "Claude Code");
        assert!(matches!(
            adapter.search(&instances[0], "x").await,
            Err(AdapterError::Unsupported(_))
        ));
    }
```

Append to `crates/banager-core/tests/ops_upgrade_version_test.rs` (at the end of the file), and add `use banager_core::adapters::standalone::recipes::CLAUDE; use banager_core::adapters::standalone::StandaloneAdapter;` to its imports (`:21-35`, alphabetical among the `banager_core::adapters::…` lines):

```rust

// --- Claude Code (standalone, phase 4 step B) ------------------------------

/// A native Claude Code layout in a temp home: `~/.local/bin/claude`
/// linking into `~/.local/share/claude/versions/<version>`. The adapter's
/// inventory probes the disk (a real link resolving into a real root), so
/// a scripted runner alone cannot stand in for the install; the runner
/// scripts only the two commands.
fn claude_home(version: &str) -> (PathBuf, ManagerInstance) {
    let home = std::env::temp_dir().join(format!(
        "banager-ops-claude-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = home.join(".local/share/claude");
    let real = root.join("versions").join(version);
    std::fs::create_dir_all(real.parent().unwrap()).expect("versions dir");
    std::fs::write(&real, b"#!/bin/sh\n").expect("real binary");
    let bin = home.join(".local/bin");
    std::fs::create_dir_all(&bin).expect("bin dir");
    let launcher = bin.join("claude");
    std::os::unix::fs::symlink(&real, &launcher).expect("launcher link");
    let inst = ManagerInstance {
        exe_path: launcher,
        prefix: root,
        version: Some(version.to_string()),
        ..banager_core::testing::manager_instance("standalone-claude", "standalone-claude")
    };
    (home, inst)
}

struct ClaudeMutationRunner {
    inner: Arc<ScriptedRunner>,
    remove_target_on_update: Option<PathBuf>,
}

#[async_trait]
impl CommandRunner for ClaudeMutationRunner {
    async fn run(
        &self,
        spec: CommandSpec,
        on_line: Option<LineCallback>,
        cancel: CancellationToken,
    ) -> Result<CommandOutput, RunnerError> {
        let is_update = spec.args == ["update"];
        let output = self.inner.run(spec, on_line, cancel).await?;
        if is_update {
            if let Some(target) = &self.remove_target_on_update {
                std::fs::remove_file(target).expect("update left launcher dangling");
            }
        }
        Ok(output)
    }
}

async fn claude_upgrade_outputs(
    update_output: CommandOutput,
    versions: Vec<CommandOutput>,
    dangling_after_update: bool,
) -> Outcome {
    let (home, inst) = claude_home("2.1.281");
    let launcher = inst.exe_path.to_string_lossy().to_string();
    let runner = Arc::new(ScriptedRunner::default());
    runner.script(&[launcher.as_str(), "update"], vec![update_output]);
    runner.script(&[launcher.as_str(), "--version"], versions);
    let mutating = Arc::new(ClaudeMutationRunner {
        inner: runner.clone(),
        remove_target_on_update: dangling_after_update
            .then(|| std::fs::canonicalize(&inst.exe_path).unwrap()),
    });
    let outcome = upgrade(
        &runner,
        Arc::new(StandaloneAdapter::new(
            &CLAUDE,
            mutating,
            Arc::new(MockHttpClient::new()),
        )),
        inst.clone(),
        ArtifactKind::Binary,
        "claude",
    )
    .await;
    if dangling_after_update {
        use banager_core::adapters::standalone::recipe::RouteKind;
        use banager_core::adapters::standalone::route::{probe, Probe};
        assert_eq!(probe(RouteKind::SymlinkIntoRoot, &inst.exe_path, &inst.prefix), Probe::LauncherOnly);
        let adapter = StandaloneAdapter::new(&CLAUDE, runner.clone(), Arc::new(MockHttpClient::new()));
        let artifacts = adapter.inventory(&inst).await.unwrap();
        assert_eq!(artifacts.len(), 1, "step C can still discover the remaining launcher");
        assert_eq!(artifacts[0].path, None);
    }
    let _ = std::fs::remove_dir_all(&home);
    outcome
}

async fn claude_upgrade(update_output: CommandOutput, versions: Vec<&str>) -> Outcome {
    claude_upgrade_outputs(
        update_output,
        versions.iter().map(|v| exited_0(&format!("{v} (Claude Code)\n"), "")).collect(),
        false,
    ).await
}

#[tokio::test]
async fn test_a_claude_update_exiting_zero_with_a_failed_version_read_is_unconfirmed() {
    let failed = CommandOutput {
        exit_code: Some(1),
        stdout: String::new(),
        stderr: "dyld: Library not loaded".to_string(),
        timed_out: false,
        cancelled: false,
    };
    for after in [failed, exited_0("", "")] {
        assert_eq!(claude_upgrade_outputs(
            exited_0("updated", ""),
            vec![exited_0("2.1.281 (Claude Code)\n", ""), after],
            false,
        ).await, Outcome::Unconfirmed);
    }
}

#[tokio::test]
async fn test_a_claude_update_exiting_zero_with_a_dangling_launcher_is_unconfirmed() {
    assert_eq!(claude_upgrade_outputs(
        exited_0("updated", ""),
        vec![exited_0("2.1.281 (Claude Code)\n", "")],
        true,
    ).await, Outcome::Unconfirmed);
}

#[tokio::test]
async fn test_a_stopped_claude_upgrade_stays_unconfirmed_even_if_the_version_moves() {
    for stop in [Stop::Cancel, Stop::Timeout] {
        assert_eq!(claude_upgrade(stop.output(), vec!["2.1.281", "2.1.290"]).await, Outcome::Unconfirmed);
    }
}

#[tokio::test]
async fn test_a_claude_update_that_reports_up_to_date_is_not_reported_as_updated() {
    // `claude update` prints `Claude Code is up to date (X)` and exits 0
    // when there is nothing to install (doc text, VERIFIED in
    // .superpowers/phase4/claude.md §6) -- which is also what the race
    // with its own background updater looks like from here. The version
    // read before and after is the same `--version`, so the outcome is
    // the honest one a skipped brew or pipx upgrade gets (spec §4.4 item
    // 5, D5).
    let outcome = claude_upgrade(
        exited_0("Claude Code is up to date (2.1.281)\n", ""),
        vec!["2.1.281", "2.1.281"],
    )
    .await;
    assert_eq!(
        outcome,
        Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade)
    );
}

#[tokio::test]
async fn test_a_claude_update_that_moved_the_version_succeeded() {
    // `Successfully updated from <old> to version <new>` (doc text,
    // VERIFIED, claude.md §6), and the launcher now answers the new
    // version.
    let outcome = claude_upgrade(
        exited_0("Successfully updated from 2.1.281 to version 2.1.290\n", ""),
        vec!["2.1.281", "2.1.290"],
    )
    .await;
    assert_eq!(outcome, Outcome::Succeeded);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib adapters::standalone::tests`
Expected: FAIL to compile — `no method named \`plan\`` / `\`execute\`` found for `StandaloneAdapter`; `cannot find function \`all\``; `the trait bound \`StandaloneAdapter: Adapter\` is not satisfied` at the `Arc<dyn Adapter>` coercion. `cargo test -p banager-core --test ops_upgrade_version_test` fails to compile for the same trait bound.

- [ ] **Step 3: Write `plan`, `execute`, the trait impl and `all()`**

In `crates/banager-core/src/adapters/standalone/mod.rs`, add to the imports `use crate::adapters::{ensure_instance_match, run_plan, validate_package_name, Adapter}; use crate::events::{EventSink, OpId}; use crate::model::{OpKind, OpRequest, Outcome, Plan, ResourceLock}; use async_trait::async_trait;` (merged into the existing `use` lines; the test module already imports `async_trait` for itself, and an explicit import beside the parent's is not a warning). `CancelPolicy` is *not* imported here: `plan()` copies `upgrade.cancel` without naming the type, and the test module has its own `use crate::model::{CancelPolicy, …}`, so a non-test import would be unused and fail `-D warnings`. Then insert after `search` (before `reconcile`):

```rust

    /// Spec §五: the tool's own documented update command, run against the
    /// launcher through `run_plan` unchanged. `Install` is `Unsupported`
    /// (the installer is Anthropic's and Banager never runs it; installing
    /// tools is phase 5). `Uninstall` is refused with the artifact's own
    /// reason -- the gate (`blocked_uninstall`) refuses it first; this is
    /// its late twin for a stale snapshot. The one artifact is
    /// `Binary`/`<recipe.id>`, so any other name or kind is a request this
    /// adapter cannot mean.
    pub async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        ensure_instance_match(req, inst)?;
        validate_package_name(&req.name)?;
        if req.name != self.recipe.id || req.artifact_kind != ArtifactKind::Binary {
            return Err(AdapterError::InvalidName(req.name.clone()));
        }
        match req.kind {
            OpKind::Install => Err(AdapterError::Unsupported(format!(
                "{} is installed by its own installer, which Banager never runs",
                self.meta.name
            ))),
            OpKind::Uninstall => Err(AdapterError::UninstallBlocked {
                reason: UninstallBlocked::NoSafeMethod,
            }),
            OpKind::Upgrade => {
                let upgrade = &self.recipe.upgrade;
                Ok(Plan {
                    request: req.clone(),
                    // The launcher, exactly as previewed: never a program
                    // the recipe could name (spec 附录 B).
                    program: inst.exe_path.clone(),
                    args: upgrade.args.iter().map(|a| a.to_string()).collect(),
                    // Not the version read's environment: `claude update`
                    // must not be told to stop updating (spec §3.4).
                    env: Vec::new(),
                    // Everything lives under $HOME (spec §五).
                    needs_password: false,
                    locks: vec![ResourceLock(inst.id.clone())],
                    cancel_policy: upgrade.cancel,
                    warnings: Vec::new(),
                    affected: Vec::new(),
                    timeout_secs: upgrade.timeout_secs,
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
        run_plan(&self.runner, plan, sink, op_id, cancel).await
    }
```

and, after the `impl StandaloneAdapter { … }` block (before `#[cfg(test)] pub(super) mod testing`):

```rust

/// One adapter per recipe in `recipes::RECIPES`, over the shared runner
/// and http client, for `Session::new`'s registration list.
pub fn all(runner: Arc<dyn CommandRunner>, http: Arc<dyn HttpClient>) -> Vec<Arc<dyn Adapter>> {
    recipes::RECIPES
        .iter()
        .map(|&recipe| {
            Arc::new(StandaloneAdapter::new(recipe, runner.clone(), http.clone()))
                as Arc<dyn Adapter>
        })
        .collect()
}

#[async_trait]
impl Adapter for StandaloneAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
    }

    async fn detect(&self, env: &HostEnv) -> Vec<ManagerInstance> {
        StandaloneAdapter::detect(self, env).await
    }

    async fn inventory(
        &self,
        inst: &ManagerInstance,
    ) -> Result<Vec<InstalledArtifact>, AdapterError> {
        StandaloneAdapter::inventory(self, inst).await
    }

    async fn check_updates(
        &self,
        inst: &ManagerInstance,
        opts: &CheckOptions,
    ) -> Result<CheckOutcome, AdapterError> {
        StandaloneAdapter::check_updates(self, inst, opts).await
    }

    async fn search(
        &self,
        inst: &ManagerInstance,
        query: &str,
    ) -> Result<Vec<SearchHit>, AdapterError> {
        StandaloneAdapter::search(self, inst, query).await
    }

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        StandaloneAdapter::plan(self, inst, req).await
    }

    async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        StandaloneAdapter::execute(self, plan, sink, op_id, cancel).await
    }

    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        StandaloneAdapter::reconcile(self, inst, key).await
    }
}
```

(In `mod tests`, calls written as `adapter.detect(...)` on a `StandaloneAdapter` value stay unambiguous: inherent methods win over trait methods of the same name, exactly as `uv.rs`'s tests rely on.)

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib adapters::standalone` and `cargo test -p banager-core --test ops_upgrade_version_test`
Expected: PASS — plan/execute unit tests and all Claude end-to-end cases: unchanged, changed, failed/empty version read, dangling after update, cancellation and timeout; existing integration cases remain green.

- [ ] **Step 5: Continue the core task**

Continue to the next stage. The complete core task runs `cargo fmt --all` and all five gates, then commits once, after stage 9. No partial type-only commit or dead-code exemption.

---

#### Stage 9: Recording, registration, the `owned_roots` row, `## Claude Code` in the trust file

**Files:**
- Create: `adapters/fixtures/standalone-claude/2.1.281/{README.md, version.txt, latest.txt, stable.txt, layout.txt}` — recorded, never typed (the directory is named after the recorded version; see Step 1)
- Modify: `adapters/meta/standalone-claude.toml` only if the recorded version is not `2.1.281`
- Modify: `crates/banager-core/src/adapters/standalone/mod.rs` (three fixture-backed tests in `mod tests`)
- Modify: `crates/banager-core/src/session/mod.rs:30-36` (imports), `:253-273` (`Session::new`), `:499-515` (`test_new_registers_all_seven_adapters`)
- Modify: `crates/banager-core/src/scan/mod.rs` — `owned_roots` and `test_owned_roots_table`  [F's file]
- Modify: `crates/banager-core/src/lib.rs` — the crate doc's list of sources  [F's file]
- Modify: `docs/what-we-run.md` — intro, "Where the program comes from", a `## Claude Code` section, "Files Banager reads"  [A's and F's file]
- Test: `fixtures_layout_test.rs`, `what_we_run_test.rs` (both existing), the session test, the three fixture tests, `scan::tests::test_owned_roots_table`.

**Interfaces:**
- Consumes: `standalone::all` (Task 3, stage 8), `Session::build` (`session/mod.rs:297-336`), F's `owned_roots` (`match inst.adapter_id.as_str() { "brew" => …, "ollama" => …, "npm" => …, _ => Vec::new() }`), A's document structure.
- Produces: the registered id `standalone-claude` (readers: `Session::refresh`'s fan-out, `fixtures_layout_test`, `what_we_run_test`, `ADAPTER_LABEL_KEYS`); the `"standalone-claude" => vec![inst.prefix.clone()]` row of `owned_roots` (reader: F's `Known::index`, rule 3); the `## Claude Code` section (reader: the person spec §12 wrote the file for; `what_we_run_test`).

Why one task: `tests/fixtures_layout_test.rs` asserts the fixture directory set equals the registered id set, and `tests/what_we_run_test.rs` asserts a `## <meta.name>` section per registered id — so the recording, the registration and the section cannot be green separately (ruling 6).

- [ ] **Step 1: Record the fixture on this Mac (read-only commands only)**

From the implementation checkout, record only read-only tool commands. This script writes the recording and its provenance README in the repository, and updates the metadata from the actual version output. It never updates Claude itself. It does not treat an unavailable Homebrew/npm command as proof that a copy is absent. The settings file is not copied. If the version directory already exists, review and reuse that recording or remove only that recording explicitly before re-recording; this script refuses to overwrite it.

```bash
python3 - <<'RECORD_CLAUDE'
import datetime
import json
import os
import platform
import re
import subprocess
from pathlib import Path

home = Path.home()
launcher = home / ".local/bin/claude"
root = home / ".local/share/claude"
version_env = dict(os.environ, DISABLE_AUTOUPDATER="1")

def capture(argv, env=None):
    return subprocess.run(argv, check=True, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, env=env).stdout

version_output = capture([str(launcher), "--version"], version_env)
version = version_output.decode().split()[0]
if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9.+_-]*", version):
    raise ValueError("version cannot safely name a fixture directory")

def pointer(channel):
    result = capture(["curl", "--fail", "--silent", "--show-error",
                      "--write-out", "\n%{http_code}",
                      f"https://downloads.claude.ai/claude-code-releases/{channel}"])
    body, status = result.rsplit(b"\n", 1)
    if status != b"200":
        raise ValueError(f"{channel}: expected direct HTTP 200, got {status!r}")
    return body

latest_output = pointer("latest")
stable_output = pointer("stable")
layout_output = capture(["ls", "-la", str(launcher), str(root / "versions")])
link_text = os.readlink(launcher)

def observation(argv):
    try:
        result = subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        return f"exit={result.returncode}; stdout={result.stdout.decode(errors='replace').strip()!r}"
    except OSError as error:
        return f"not observed: {error}"

brew = observation(["brew", "list", "--cask", "claude-code"])
npm = observation(["npm", "list", "-g", "--depth=0", "@anthropic-ai/claude-code"])
try:
    settings = json.loads((home / ".claude/settings.json").read_text())
    channel_key = "present" if isinstance(settings, dict) and "autoUpdatesChannel" in settings else "absent"
except (OSError, ValueError):
    channel_key = "not observed (unreadable or malformed settings)"

fixture_dir = Path("adapters/fixtures/standalone-claude") / version
fixture_dir.mkdir(parents=True, exist_ok=False)
for name, data in [("version.txt", version_output), ("latest.txt", latest_output),
                   ("stable.txt", stable_output), ("layout.txt", layout_output)]:
    (fixture_dir / name).write_bytes(data)
readme = "\n".join([
    f"# Claude Code {version} fixtures (native installer route, `standalone-claude`)",
    "",
    f"Recorded {datetime.date.today().isoformat()} on {platform.node()}, macOS {platform.mac_ver()[0]}, {platform.machine()}.",
    "Command output files are saved byte for byte. Claude was not upgraded.",
    "",
    "Commands:",
    "- `DISABLE_AUTOUPDATER=1 ~/.local/bin/claude --version` -> `version.txt`",
    "- `curl --fail --silent --show-error https://downloads.claude.ai/claude-code-releases/latest` -> `latest.txt`",
    "- `curl --fail --silent --show-error https://downloads.claude.ai/claude-code-releases/stable` -> `stable.txt`",
    "- `ls -la ~/.local/bin/claude ~/.local/share/claude/versions` -> `layout.txt`",
    "",
    "Both pointers answered direct HTTP 200; curl's checked status trailer is excluded from the saved bodies.",
    "Banager adds the documented background-check switch to version reads. Whether bare --version starts that check was not observed. Manual updates work with the switch set.",
    "No claude update, claude install, or bare claude was run.",
    f"Launcher link text: {link_text!r}.",
    f"brew list --cask claude-code: {brew}.",
    f"npm list -g --depth=0 @anthropic-ai/claude-code: {npm}.",
    "These command-scoped observations do not rule out other prefixes. Shared-exclusion and PATH cases use synthetic unit tests.",
    f"autoUpdatesChannel key: {channel_key}; settings contents are not recorded. Channel tests use inline JSON in a temporary home.",
    "The dotted comparison decides whether either pointer is newer; no channel ordering is assumed by this recording.",
    "",
])
(fixture_dir / "README.md").write_text(readme)
meta_path = Path("adapters/meta/standalone-claude.toml")
metadata, replacements = re.subn(r"(?m)^verified_versions\s*=\s*\[.*\]$",
                                 f"verified_versions = [{json.dumps(version)}]",
                                 meta_path.read_text())
if replacements != 1:
    raise ValueError("expected exactly one verified_versions assignment")
meta_path.write_text(metadata)
print(f"Recorded {fixture_dir}; verified version: {version}")
RECORD_CLAUDE
```

Use the metadata version in the trust section's “Verified against Claude Code” sentence when adding it in step 4. The synthetic verified-version test in stage 6 also reads this metadata; fixed comparison-table numbers remain independent of recording-day versions.

- [ ] **Step 2: Write the failing tests**

Append inside `mod tests` in `crates/banager-core/src/adapters/standalone/mod.rs`:

```rust

    /// The recorded fixture directory for the version the meta file
    /// verifies: `adapters/fixtures/standalone-claude/<verified>/`.
    fn fixture(name: &str) -> String {
        let adapter = adapter(Arc::new(MockRunner::new()));
        let version = adapter
            .meta
            .verified_versions
            .first()
            .expect("meta lists the recorded version")
            .clone();
        let path = format!("../../adapters/fixtures/standalone-claude/{version}/{name}");
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
    }

    #[test]
    fn test_the_recorded_version_line_parses_to_the_verified_version() {
        let verified = adapter(Arc::new(MockRunner::new()))
            .meta
            .verified_versions[0]
            .clone();
        assert_eq!(
            latest::parse_version(&fixture("version.txt"), CLAUDE.version.parse),
            Some(verified)
        );
    }

    #[test]
    fn test_the_recorded_channel_pointers_are_bare_versions() {
        let latest_pointer = latest::parse_channel_body(&fixture("latest.txt")).expect("latest");
        let stable_pointer = latest::parse_channel_body(&fixture("stable.txt")).expect("stable");
        // The recording's reason for existing: the stable pointer is not
        // ahead of the latest one.
        assert_ne!(
            latest::compare_dotted(&stable_pointer, &latest_pointer),
            Some(Ordering::Greater),
            "stable {stable_pointer} is not ahead of latest {latest_pointer}"
        );
    }

    #[tokio::test]
    async fn test_check_updates_over_the_recorded_pointers_lists_only_a_real_update() {
        // Fed each recorded pointer as the endpoint's body: the stable
        // pointer, behind or equal to the installed version, yields no
        // candidate; the latest pointer yields one exactly when it is
        // greater than the installed version -- both derived from the
        // recording, so a re-recording on a later day stays honest.
        let installed = adapter(Arc::new(MockRunner::new()))
            .meta
            .verified_versions[0]
            .clone();
        for (name, url) in [("stable.txt", STABLE_URL), ("latest.txt", LATEST_URL)] {
            let body = fixture(name);
            let pointer = latest::parse_channel_body(&body).expect(name);
            let home = TempHome::new("check-recorded");
            let layout = claude_layout(&home, &installed);
            if name == "stable.txt" {
                home.dir(".claude");
                std::fs::write(
                    home.path().join(".claude/settings.json"),
                    r#"{"autoUpdatesChannel":"stable"}"#,
                )
                .expect("write settings");
            }
            let http = Arc::new(MockHttpClient::new());
            http.respond(url, answer(&body));
            let runner = Arc::new(MockRunner::new());
            runner.respond(
                vec![layout.launcher.to_str().unwrap(), "--version"],
                exited_0(&format!("{installed} (Claude Code)\n")),
            );
            let adapter = StandaloneAdapter::new(&CLAUDE, runner, http);
            let inst = adapter.detect(&home.env(vec![])).await.remove(0);
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("check_updates");
            match latest::compare_dotted(&installed, &pointer) {
                Some(Ordering::Less) => {
                    assert_eq!(out.candidates.len(), 1, "{name}");
                    assert_eq!(out.candidates[0].target, pointer);
                    assert!(out.candidates[0].checkable);
                }
                Some(Ordering::Equal | Ordering::Greater) => assert!(out.candidates.is_empty()),
                None => {
                    assert_eq!(out.candidates.len(), 1, "{name}");
                    assert!(!out.candidates[0].checkable);
                    assert_eq!(out.candidates[0].current, installed);
                    assert_eq!(out.candidates[0].target, installed);
                }
            }
        }
    }
```

In `crates/banager-core/src/session/mod.rs`, replace `test_new_registers_all_seven_adapters` (`:499-515`) with:

```rust
    #[test]
    fn test_new_registers_all_eight_adapters() {
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
                "standalone-claude".to_string(),
                "uv".to_string(),
            ]
        );
    }
```

In `crates/banager-core/src/scan/mod.rs`, inside F's `test_owned_roots_table`, after the `assert_eq!(owned_roots(&npm), vec![PathBuf::from("/usr/local/lib/node_modules")]);` line and before the comment `// A \`parent()\`-derived prefix, or \`$CARGO_HOME\`, is never a root.`, insert:

```rust
        // A standalone tool owns its root: the launcher is the instance's
        // `exe_path` (rules 0/1), the `versions/<v>` store under the root
        // is this row's (phase 4 step B; agy and grok join with step D).
        let claude = ManagerInstance {
            prefix: PathBuf::from("/Users/someone/.local/share/claude"),
            ..crate::testing::manager_instance("standalone-claude", "standalone-claude")
        };
        assert_eq!(
            owned_roots(&claude),
            vec![PathBuf::from("/Users/someone/.local/share/claude")]
        );
```

and add to that test's "never a root" array (the `for (adapter, id, prefix) in [ … ]` list) one more tuple, after the `("pip", "pip:/usr/bin/python3", "/usr/bin"),` line:

```rust
            // No adapter with this id exists yet (rustup is step E); the
            // default arm answers for it as for any unknown id, and
            // everything of rustup's is rule 1's anyway.
            ("standalone-rustup", "standalone-rustup", "/Users/someone/.cargo"),
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p banager-core`
Expected: FAIL — `test_new_registers_all_eight_adapters` (left has seven ids, no `standalone-claude`); `fixtures_layout_test::test_every_registered_adapter_has_a_documented_fixture_directory` (`adapters/fixtures/*` now has a `standalone-claude` directory that no registered adapter matches: left `[…, "standalone-claude", …]`, right the seven ids); `scan::tests::test_owned_roots_table` (`owned_roots(&claude)` is `[]`). The three fixture tests PASS already (they read the recording through the adapter, which exists since Task 3, stage 8) — they are here because the recording is.

- [ ] **Step 4: Register, add the row, write the section**

In `crates/banager-core/src/session/mod.rs`, add to the imports, after `use crate::adapters::pipx::PipxAdapter;`:

```rust
use crate::adapters::standalone;
```

and replace `Session::new` (`:253-273`) with:

```rust
    /// Registers all eight adapters over a shared `RealRunner` and
    /// `RealHttpClient` (network-touching adapters only: pipx, cargo,
    /// ollama, and the standalone tools' update checks). `now_fn` exists
    /// so tests can pin `refreshed_at`; production passes `None`.
    pub fn new(sink: Arc<dyn EventSink>, now_fn: Option<fn() -> i64>) -> Arc<Session> {
        let runner: Arc<dyn CommandRunner> = Arc::new(RealRunner::new());
        let http: Arc<dyn HttpClient> = Arc::new(RealHttpClient::new());
        let background_change = Arc::new(tokio::sync::Notify::new());
        let mut adapters: Vec<Arc<dyn Adapter>> = vec![
            Arc::new(
                BrewAdapter::new(runner.clone()).with_background_change(background_change.clone()),
            ),
            Arc::new(NpmAdapter::new(runner.clone())),
            Arc::new(PipxAdapter::new(runner.clone(), http.clone())),
            Arc::new(UvAdapter::new(runner.clone())),
            Arc::new(PipAdapter::new(runner.clone())),
            Arc::new(CargoAdapter::new(runner.clone(), http.clone())),
            Arc::new(OllamaAdapter::new(runner.clone(), http.clone())),
        ];
        // The tools with their own installer: one adapter per recipe
        // (`standalone-claude` in phase 4 step B), over the same runner and
        // client. Listed after the seven package managers only as reading
        // order; the refresh fans out alphabetically by id regardless
        // (`refresh_round`).
        adapters.extend(standalone::all(runner, http));
        Session::build(sink, adapters, now_fn, background_change)
    }
```

In `crates/banager-core/src/scan/mod.rs`, inside F's `owned_roots`, replace the arm

```rust
        "npm" => vec![inst.prefix.join("lib").join("node_modules")],
```

and everything from there to the `_ => Vec::new(),` arm inclusive with:

```rust
        "npm" => vec![inst.prefix.join("lib").join("node_modules")],
        // A tool installed by its own installer owns its root
        // (`~/.local/share/claude`, the `versions/<v>` store its launcher
        // links into). The launcher itself is the instance's `exe_path`
        // and rules 0/1 have it; this row is for anything else that
        // resolves under the root. Added with the adapter that first
        // produces the instance (phase 4 step B); `standalone-agy` and
        // `standalone-grok` follow with their recipes in step D, and
        // `standalone-rustup` never joins: everything of rustup's resolves
        // to its launcher (rule 1).
        "standalone-claude" => vec![inst.prefix.clone()],
        // cargo: `$CARGO_HOME` holds `bin/`, the very directory being
        // scanned; rule 1 places the proxies and, from step E, rule 2
        // places `cargo install`ed binaries. uv and pipx: rule 2, through
        // the tool venv their artifacts carry. pip: a `parent()`-derived
        // prefix, never a root.
        _ => Vec::new(),
```

(keep F's comment text for the `_` arm as it is in the tree if it differs in wording; only the `standalone-claude` arm and its comment are new). Also, in F's doc comment on `owned_roots`, the paragraph beginning `/// The standalone adapters (phase 4 step B) add their tool roots --` describes what this change does; rewrite its first sentence to `/// The standalone adapters add their tool roots as their recipes land --` so the comment no longer says the rows are missing.

In `crates/banager-core/src/lib.rs`, in the crate doc, change the clause

```rust
//! after the things they installed from a terminal — Homebrew, npm, pipx,
//! uv, pip, cargo, Ollama. This crate is the part that does the work: the
```

to

```rust
//! after the things they installed from a terminal — Homebrew, npm, pipx,
//! uv, pip, cargo, Ollama, and tools that come with their own installer
//! (Claude Code). This crate is the part that does the work: the
```

(F may have reworded this sentence for the Unknown page; keep F's wording and add the same clause to it.)

In `docs/what-we-run.md`:

The two sentences below are quoted on one line each, but in the file A hard-wrapped them (`for the seven sources` / `it manages today: …` across lines 4-5; `Every source` / `but Homebrew finds …` across lines 54-55 at `eb254ef`), so match them by their words, not by an exact one-line string, and keep the file's wrapping style when rewriting.

(a) In the opening paragraph, change `for the seven sources it manages today: Homebrew, npm, pipx, uv, pip (read-only), Cargo and Ollama.` to `for the eight sources it manages today: Homebrew, npm, pipx, uv, pip (read-only), Cargo, Ollama, and Claude Code (a tool with its own installer).`

(b) Under `## How Banager runs anything`, in the paragraph `**Where the program comes from.**`, change the sentence `Every source but Homebrew finds its executable with \`resolve_exe\`: the first directory on that \`PATH\` containing a regular file of that name. Homebrew is looked for at three fixed paths instead (its section).` to `Every package manager but Homebrew finds its executable with \`resolve_exe\`: the first directory on that \`PATH\` containing a regular file of that name. Homebrew is looked for at three fixed paths instead (its section), and so is a tool with its own installer: Claude Code at the one path its installer writes (its section).`

(c) After the `## Ollama` section's last paragraph (the one beginning `**The Open Ollama button** runs \`/usr/bin/open -a Ollama\``), and before whatever follows it (`## Files Banager reads`, or F's `## Unknown-source scan …` section if it landed there), insert:

```markdown

## Claude Code

Adapter: `StandaloneAdapter` over the `CLAUDE` recipe in
`crates/banager-core/src/adapters/standalone/` (`recipes.rs` is the data,
`mod.rs` the behaviour, `route.rs` the recognition). Verified against
Claude Code 2.1.281 (the version in `adapters/meta/standalone-claude.toml`
and the name of the recorded fixture directory; write the recording day's
number here). The first
source that is not a package manager: the row is one tool, installed by
its own installer (`curl -fsSL https://claude.ai/install.sh | bash`, run by
the user — Banager never runs it), and the one item under it is the tool
itself.

**Detect.** Banager looks at the fixed path the installer writes,
`~/.local/bin/claude` — never a `claude` found through `PATH`, which on a
Mac with the Homebrew cask earlier on `PATH` would be that copy instead —
and checks with `lstat`, `readlink` and `realpath` that it is a symbolic
link resolving into `~/.local/share/claude` (the installer's
`versions/<version>` store). A `claude` there that resolves into a
`Cellar`, `Caskroom`, `node_modules` or `corepack` directory is Homebrew's
or npm's and is left to that source; a plain file at that path is not this
route and is not listed. A dangling link whose own text points into
`~/.local/share/claude` (the program files were removed by hand, or by an
uninstall that stopped partway) is listed with no version and a notice
saying so; in this step Banager cannot remove the link either (see the
write commands below). Then `<claude> --version` (30 s) with
`DISABLE_AUTOUPDATER=1` in its environment: Anthropic documents that Claude
Code checks for updates on startup, and the variable as stopping only that
background check (so `claude update` is unaffected); whether `--version`
alone triggers the check was not observed, and a refresh must never start
a download, so the variable is set on every version read regardless. The
version is the first token of the first non-empty line (`2.1.281 (Claude
Code)`).

Banager also asks where `claude` would run from if typed in Terminal
(the first regular file with executable bits in Banager's `PATH`) and, when that is not this copy, says
so under the source: not on `PATH`, or shadowed by a Homebrew, npm or
unknown copy. That is a notice, not a command.

**Environment Banager adds to version reads** (`CLAUDE.version.env`;
upgrade adds no override and inherits ambient variables):

    DISABLE_AUTOUPDATER=1

**Read-only commands and requests** (background checks; never need a
password):

| Purpose | Argv or request | Timeout |
|---|---|---|
| Detect, inventory, the fresh update check, and the reading before and after an update | `<claude> --version`, with `DISABLE_AUTOUPDATER=1` | 30 s |
| Newest published version (`check_updates`) | `GET https://downloads.claude.ai/claude-code-releases/latest` — or `/stable`, when `~/.claude/settings.json` sets `"autoUpdatesChannel": "stable"` | 30 s |

The pointer answers with one version number. An update is listed only when
that number is greater than the installed one, comparing the dot-separated
integers — the `stable` pointer is usually behind `latest`, so "different"
would be wrong; a request that fails, answers anything but 200, or answers
something that is not a version is listed as "could not check", never as an
error for the source. Claude Code updates itself in the background when its
own updater is on, so the row says it usually will; the update listed is
real either way, since it is read from the launcher's live version.

**Write commands** (only run after the user reviews and confirms a plan
preview):

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Upgrade | `<claude> update` | 1800 s | No |

Banager adds no environment override to `claude update`; the runner
inherits the app's ambient environment. `DISABLE_AUTOUPDATER=1` stops the
background check, and manual updates still work with it set. Anthropic's install script
downloads each version to a new file under
`~/.local/share/claude/versions/` and re-points the link only afterwards
(install.sh, read directly); `claude update` itself is a compiled program
whose steps were not read, so Banager assumes nothing about what a run
stopped partway leaves behind, and its preview promises nothing. Cancel:
allowed (`KillThenReconcile`) — the runner stops the process group, Banager
reads `<claude> --version` again, and the operation is reported as
unconfirmed regardless of that reading (the same rule as every stopped
upgrade). If it exits 0 but the launcher is dangling or its version cannot
be read, verification fails and the outcome is also unconfirmed. If it exits 0 and the
version did not move (Claude
Code already updated itself, or reports "up to date"), the operation is
reported as needing attention, as for every source. There is no install
(the installer is Anthropic's, not Banager's) and, in this step, no
uninstall: Claude Code has no uninstall command, and until Banager can move
its files to the Trash itself (phase 4 step C) the row says it cannot be
uninstalled here and offers no button — `Session::issue_plan` refuses it as
well.
```

(d) Under `## Files Banager reads`, after the `- Ollama: …` bullet, add:

```markdown
- Claude Code: whether `~/.local/bin/claude` exists and where it links to
  (`lstat`, `readlink`, `realpath`, also for `~/.local/share/claude`);
  `~/.claude/settings.json`, for the one key `autoUpdatesChannel` (read and
  discarded; a missing file or key means `latest`).
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p banager-core`
Expected: PASS — `test_new_registers_all_eight_adapters`, `fixtures_layout_test` (eight directories, eight ids, one README each), `what_we_run_test` (a `## Claude Code` section, the host named), `test_owned_roots_table`, the three fixture tests, and everything before.

- [ ] **Step 6: Run the gates**

Run all five from Global Constraints. Expected: all clean.

- [ ] **Step 7: Commit**

```bash
git add crates/banager-core/src/adapters/mod.rs crates/banager-core/src/adapters/standalone/mod.rs crates/banager-core/src/adapters/standalone/recipe.rs crates/banager-core/src/adapters/standalone/latest.rs adapters/meta/standalone-claude.toml crates/banager-core/src/adapters/standalone/recipes.rs crates/banager-core/src/adapters/standalone/route.rs crates/banager-core/src/http/real.rs docs/what-we-run.md crates/banager-core/tests/ops_upgrade_version_test.rs adapters/fixtures/standalone-claude crates/banager-core/src/session/mod.rs crates/banager-core/src/scan/mod.rs crates/banager-core/src/lib.rs
git commit -m "$(cat <<'EOF'
Add and register the complete Claude Code standalone adapter

The recipe, route checks, fresh version reads, update plans and strict
verification land with all their production readers. Session::new lists
the standalone adapters after the seven package
managers, so a native Claude Code install is a group on the Installed
page. The fixture directory the layout test demands is a real recording
from this Mac: the version line with the updater switched off, both
channel pointers (compared as recorded, without assuming their order), and the on-disk layout. The Unknown page's owned-roots table gains
the tool's root, and what-we-run.md gains the section that says every
command, request, file and variable this source involves.

Co-Authored-By: Codex <noreply@openai.com>
EOF
)"
```

---

### Task 10: Front end — the summary sentence, `selfUpdatingHint`, the two empty states

**Files:**
- Modify: `src/lib/sources.ts` (after `ADAPTER_LABEL_KEYS`: `StandaloneAdapterId`, `STANDALONE_SUMMARY_KEYS`, `standaloneSummaryKey`)
- Modify: `src/lib/sources.test.ts` (one `describe` appended at the end of the file)
- Modify: `src/pages/InstalledPage.tsx:1-18` (imports), the full `ArtifactRow.description` expression and the new `installedDescription` helper
- Modify: `src/pages/InstalledPage.test.tsx` (one test appended after Task 2's `offers no Uninstall on a tool with no safe uninstall method…`)
- Modify: `src/pages/UpdatesPage.tsx:444-475` (`rowDescription`)
- Modify: `src/pages/UpdatesPage.test.tsx` (two tests appended inside `describe("UpdatesPage", …)` before its closing `});`)
- Modify: `src/components/SnapshotStatus.test.tsx` (the two empty-state sentences, `:57` and `:242`)
- Modify: `src/i18n/en.json`, `src/i18n/zh-CN.json` (`standalone.summary`, `updates.selfUpdatingHint`, `emptyStates.noSources.description`, `emptyStates.nothingInstalled.description`)
- Test: all of the above test files; `completeness.test.ts`.

**Interfaces:**
- Consumes: `InstalledArtifact.description` (`null` for a standalone artifact, Task 3, stage 6) and `.auto_updates` (`true` for claude); `ManagerInstance.adapter_id`; `isActionable`, `artifactsById`, `instancesById`, `sourceLabelFor` in `UpdatesPage.tsx`; `withCommand` is not used by the hint (no command in it).
- Produces (verbatim): `export type StandaloneAdapterId = "standalone-claude"`, `export const STANDALONE_SUMMARY_KEYS: Record<StandaloneAdapterId, string>` (`"standalone-claude": "standalone.summary.standalone-claude"`), `export function standaloneSummaryKey(adapterId: string): string | null` (reader: `InstalledPage::installedDescription`, including the `NoSafeMethod` branch); the `updates.selfUpdatingHint` branch of `rowDescription` (reader: the Updates page row; this is `auto_updates`'s first reader outside the `Pinned` copy, spec 附录 A); the two rewritten `emptyStates` sentences (readers: `SnapshotStatus.tsx`'s two `t()` calls, unchanged).

- [ ] **Step 1: Write the failing tests**

Append to `src/lib/sources.test.ts`; add `standaloneSummaryKey` to the `import { … } from "./sources"` list:

```ts

describe("STANDALONE_SUMMARY_KEYS", () => {
  it("gives each standalone tool a sentence and every other source none", () => {
    // A standalone artifact's `description` is `null` on the wire (a bare
    // string could not be localised), so the Installed page reads the
    // sentence here by adapter id, and every package manager keeps
    // `installed.noDescription` for a package with no blurb.
    expect(standaloneSummaryKey("standalone-claude")).toBe("standalone.summary.standalone-claude");
    for (const id of ["brew", "npm", "pipx", "uv", "pip", "cargo", "ollama", "toString", ""]) {
      expect(standaloneSummaryKey(id)).toBeNull();
    }
  });

  it("has the sentence in both locales, naming the installer route", () => {
    expect(en.standalone.summary["standalone-claude"]).toBe(
      "Anthropic's coding assistant for the terminal. Installed with its own installer, not with Homebrew or npm.",
    );
    expect(zhCN.standalone.summary["standalone-claude"]).toBe(
      "Anthropic 的终端编程助手。用它自己的安装器装的，不是 Homebrew 或 npm。",
    );
  });
});
```

Append to `src/pages/InstalledPage.test.tsx`, after Task 2's test (before `describe("the Update available badge", () => {`):

```tsx
  it("shows the standalone summary alongside its real uninstall refusal", async () => {
    // A standalone artifact carries `description: null` (the sentence has
    // to be localised, so it lives in `STANDALONE_SUMMARY_KEYS`); a
    // Homebrew package with no blurb keeps "No description available".
    const mixed: Snapshot = {
      ...snapshot,
      instances: [
        snapshot.instances[0],
        {
          id: "standalone-claude",
          adapter_id: "standalone-claude",
          exe_path: "/Users/someone/.local/bin/claude",
          prefix: "/Users/someone/.local/share/claude",
          scope: "User",
          version: "2.1.281",
          status: { unavailable: null, notes: [] },
          unverified_version: null,
          read_only_reason: null,
        },
      ],
      artifacts: [
        { ...snapshot.artifacts[0], description: null },
        {
          key: { instance_id: "standalone-claude", kind: "Binary", name: "claude" },
          display_name: "Claude Code",
          version: "2.1.281",
          reason: "Requested",
          description: null,
          homepage: "https://code.claude.com/docs/en/setup",
          size_bytes: null,
          installed_at: null,
          path: "/Users/someone/.local/share/claude/versions/2.1.281",
          auto_updates: true,
          uninstall_blocked: "NoSafeMethod",
        },
      ],
      updates: [],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(mixed);
      if (cmd === "get_settings") return Promise.resolve(settings);
      return Promise.resolve(undefined);
    });

    const { findByText, getByText, queryAllByRole } = renderWithProviders(<InstalledPage />);

    expect(
      await findByText(
        "Anthropic's coding assistant for the terminal. Installed with its own installer, not with Homebrew or npm.",
      ),
    ).toBeInTheDocument();
    expect(getByText("No description available")).toBeInTheDocument();
    expect(getByText("Claude Code has no uninstall command, and Banager can't yet move its files to the Trash safely, so it doesn't offer to. The official instructions are on its website.")).toBeInTheDocument();
    // Only the Homebrew artifact may offer Uninstall; B's actual Claude
    // artifact is NoSafeMethod and must still show both sentences.
    expect(queryAllByRole("button", { name: "Uninstall" })).toHaveLength(1);
  });

```

Append inside `describe("UpdatesPage", …)` in `src/pages/UpdatesPage.test.tsx` (before its closing `});`):

```tsx

  const claudeKey: ArtifactKey = { instance_id: "standalone-claude", kind: "Binary", name: "claude" };
  const claudeInstance: Snapshot["instances"][number] = {
    id: "standalone-claude",
    adapter_id: "standalone-claude",
    exe_path: "/Users/someone/.local/bin/claude",
    prefix: "/Users/someone/.local/share/claude",
    scope: "User",
    version: "2.1.281",
    status: { unavailable: null, notes: [] },
    unverified_version: null,
    read_only_reason: null,
  };
  const claudeArtifact: Snapshot["artifacts"][number] = {
    key: claudeKey,
    display_name: "Claude Code",
    version: "2.1.281",
    reason: "Requested",
    description: null,
    homepage: "https://code.claude.com/docs/en/setup",
    size_bytes: null,
    installed_at: null,
    path: "/Users/someone/.local/share/claude/versions/2.1.281",
    auto_updates: true,
    uninstall_blocked: "NoSafeMethod",
  };
  const claudeUpdate: Snapshot["updates"][number] = {
    key: claudeKey,
    current: "2.1.281",
    target: "2.1.290",
    channel: "Registry",
    checkable: true,
    warnings: [],
    blocked: null,
  };

  it("says a self-updating standalone tool will probably update itself, and still offers the button", async () => {
    // Spec D5: the badge is real (read from the launcher's live version),
    // so the row keeps its Update button; the honest sentence says the
    // tool usually does this itself. `auto_updates`'s first reader beyond
    // the pinned copy.
    instances = [...snapshot.instances, claudeInstance];
    updates = [claudeUpdate];
    artifacts = [claudeArtifact];
    const { findByText, getAllByRole } = renderWithProviders(<UpdatesPage />);

    expect(
      await findByText(
        "This copy is behind (2.1.281 → 2.1.290). Claude Code usually updates itself the next time you run it; you can update it now with Banager, or just run it.",
      ),
    ).toBeInTheDocument();
    expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
  });

  it("keeps a self-updating Homebrew cask's own blurb: the hint is for tools that update themselves, not for --greedy", async () => {
    // A cask listed through `include_self_updating` carries
    // `auto_updates: true` too, but Homebrew, not the app, is what the
    // button drives; its row keeps its description.
    settings = { ...settings, include_self_updating: true };
    updates = [snapshot.updates[1]];
    artifacts = [
      {
        key: onyxKey,
        display_name: "OnyX",
        version: "5.0.2",
        reason: "Requested",
        description: "Verify system files structure",
        homepage: null,
        size_bytes: null,
        installed_at: null,
        path: null,
        auto_updates: true,
        uninstall_blocked: null,
      },
    ];
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    expect(await findByText("Verify system files structure")).toBeInTheDocument();
    expect(queryByText(/usually updates itself/)).toBeNull();
  });

  it("gives a standalone row that cannot be checked its reason, not the self-updating hint", async () => {
    instances = [...snapshot.instances, claudeInstance];
    updates = [
      {
        ...claudeUpdate,
        target: "2.1.281",
        checkable: false,
        warnings: [{ Message: "downloads.claude.ai request failed: network error: offline" }],
      },
    ];
    artifacts = [claudeArtifact];
    const { findByText, queryByText, queryAllByRole } = renderWithProviders(<UpdatesPage />);

    expect(await findByText("Banager couldn't check this one for updates just now.")).toBeInTheDocument();
    expect(queryByText(/usually updates itself/)).toBeNull();
    expect(queryAllByRole("button", { name: "Update" })).toHaveLength(0);
  });
```

In `src/components/SnapshotStatus.test.tsx`, replace the two expected sentences: at `:57`,

```ts
        "Banager works with Homebrew, npm, pipx, uv, pip, Cargo and Ollama. None of them are set up on this Mac yet — Homebrew is the easiest place to start.",
```

becomes

```ts
        "Banager works with Homebrew, npm, pipx, uv, pip, Cargo, Ollama, and Claude Code at its native installer's default location. None of them are set up on this Mac yet — Homebrew is the easiest place to start.",
```

and at `:242`,

```ts
        "Anything you install with Homebrew, npm, pipx, uv, pip, Cargo or Ollama will show up here.",
```

becomes

```ts
        "Items installed with Homebrew, npm, pipx, uv, pip, Cargo or Ollama appear here, along with Claude Code installed at its native installer's default location.",
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm typecheck`
Expected: FAIL — `Module '"./sources"' has no exported member 'standaloneSummaryKey'`; `Property 'standalone' does not exist on type` of `en`/`zhCN`. (`pnpm exec vitest run src/components/SnapshotStatus.test.tsx` would fail on both sentences; the page tests on the missing sentences.)

- [ ] **Step 3: Implement**

In `src/lib/sources.ts`, after `ADAPTER_LABEL_KEYS` (before the `READ_ONLY_NOTICE_KEYS` doc comment), insert:

```ts

/** The adapter ids of the tools with their own installer, one per recipe
 *  in `standalone::RECIPES` (crates/banager-core/src/adapters/standalone/
 *  recipes.rs). A union so `STANDALONE_SUMMARY_KEYS` is a `Record` over
 *  it: a tool added here without a sentence fails `tsc`. */
export type StandaloneAdapterId = "standalone-claude";

/**
 * One sentence per standalone tool, for the Installed page's description
 * slot: what the tool is and that its own installer put it there.
 * `InstalledArtifact.description` is a bare string that cannot be
 * localised, so the standalone adapter's inventory leaves it `null` and
 * the sentence lives here, in both locales.
 */
export const STANDALONE_SUMMARY_KEYS: Record<StandaloneAdapterId, string> = {
  "standalone-claude": "standalone.summary.standalone-claude",
};

/**
 * The summary key for `adapterId`, or `null` for a source that is not a
 * standalone tool (or one this build has no sentence for), which keeps
 * `installed.noDescription`. `hasOwnProperty`, not truthiness: an id like
 * "toString" finds a function on the prototype, not a key.
 */
export function standaloneSummaryKey(adapterId: string): string | null {
  return Object.prototype.hasOwnProperty.call(STANDALONE_SUMMARY_KEYS, adapterId)
    ? STANDALONE_SUMMARY_KEYS[adapterId as StandaloneAdapterId]
    : null;
}
```

In `src/pages/InstalledPage.tsx`, add `standaloneSummaryKey` to the imports from `../lib/sources` and `ReactNode` to a type import from `react`. Replace the entire `description={...}` expression on `ArtifactRow` with:

```tsx
                    description={installedDescription(item.artifact, item.instance, item.sourceLabel)}
```

Keep `wrapDescription={item.artifact.uninstall_blocked !== null}`. Replace the comment above `description` with:

```tsx
                    // Standalone rows show what the tool is alongside
                    // the refusal explaining why Uninstall is absent.
```

Add inside `InstalledPage`, after `installedBadge` and before `const items = useMemo<ListItem[]>(`:

```tsx
  function installedDescription(
    artifact: InstalledArtifact,
    instance: ManagerInstance,
    sourceLabel: string,
  ): ReactNode {
    const summaryKey = standaloneSummaryKey(instance.adapter_id);
    const blurb = artifact.description ?? (summaryKey === null ? null : t(summaryKey));
    if (artifact.uninstall_blocked !== null) {
      const copy = UNINSTALL_BLOCKED_KEYS[artifact.uninstall_blocked];
      const refusal = withCommand(
        t(isAvailable(instance) ? copy.description : copy.descriptionSourceUnavailable, {
          command: COMMAND_SLOT,
          source: sourceLabel,
        }),
        copy.command(artifact.key, instance),
      );
      return summaryKey === null ? refusal : (
        <>
          <span>{blurb}</span>{" "}<span>{refusal}</span>
        </>
      );
    }
    return blurb ?? t("installed.noDescription");
  }
```

The two spans are valid inside `ArtifactRow`'s existing `<p>` and retain its wrapping. The test uses `NoSafeMethod`, the artifact B actually produces; reverting to the old refusal-only branch makes the summary assertion fail.

In `src/pages/UpdatesPage.tsx`, in `rowDescription` (`:444-475`), replace its last line `return descriptionFor(candidate);` with:

```tsx
    // A tool that updates itself in the background (`auto_updates`, set
    // by the standalone adapter from its recipe): the row is real -- it
    // compares the launcher's live version with the published one -- and
    // keeps its button, but the honest sentence says the tool usually
    // does this itself and offers Banager's button as the other way
    // (spec D5). Only for the standalone adapters: a self-updating
    // Homebrew cask listed by --greedy keeps its blurb, since Homebrew,
    // not the app, is what the button drives. Only for an actionable
    // row: a blocked or uncheckable one has already said its piece above.
    const owner = instancesById.get(candidate.key.instance_id);
    if (
      isActionable(candidate) &&
      owner !== undefined &&
      owner.adapter_id.startsWith("standalone-") &&
      artifactsById.get(artifactKeyId(candidate.key))?.auto_updates === true
    ) {
      return t("updates.selfUpdatingHint", {
        current: candidate.current,
        target: candidate.target,
        source: sourceLabelFor(candidate.key.instance_id),
      });
    }
    return descriptionFor(candidate);
```

In `src/i18n/en.json`: in `"updates"`, after `"noneCheckable": "…",` add

```json
    "selfUpdatingHint": "This copy is behind ({{current}} → {{target}}). {{source}} usually updates itself the next time you run it; you can update it now with Banager, or just run it.",
```

(before the `"blocked": {` key); add a top-level object after `"warnings": { … },` (or anywhere at the top level):

```json
  "standalone": {
    "summary": {
      "standalone-claude": "Anthropic's coding assistant for the terminal. Installed with its own installer, not with Homebrew or npm."
    }
  },
```

and replace the two `emptyStates` descriptions:

```json
      "description": "Banager works with Homebrew, npm, pipx, uv, pip, Cargo, Ollama, and Claude Code at its native installer's default location. None of them are set up on this Mac yet — Homebrew is the easiest place to start."
```

```json
      "description": "Items installed with Homebrew, npm, pipx, uv, pip, Cargo or Ollama appear here, along with Claude Code installed at its native installer's default location."
```

In `src/i18n/zh-CN.json`, the same four places:

```json
    "selfUpdatingHint": "这份落后了（{{current}} → {{target}}）。{{source}} 通常在下次运行时会自己更新；可以现在用 Banager 更新，也可以直接运行它。",
```

```json
  "standalone": {
    "summary": {
      "standalone-claude": "Anthropic 的终端编程助手。用它自己的安装器装的，不是 Homebrew 或 npm。"
    }
  },
```

```json
      "description": "Banager 支持 Homebrew、npm、pipx、uv、pip、Cargo、Ollama，以及用 Claude Code 原生安装器装在默认位置的 Claude Code。这台 Mac 上一个都还没装，建议先从 Homebrew 开始。"
```

```json
      "description": "用 Homebrew、npm、pipx、uv、pip、Cargo、Ollama 装的东西，以及用 Claude Code 原生安装器装在默认位置的 Claude Code，会出现在这里。"
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm typecheck && pnpm test`
Expected: PASS — including `completeness.test.ts` (`standalone.summary.standalone-claude` is a literal in `sources.ts`; `updates.selfUpdatingHint` a literal in `UpdatesPage.tsx`) and `no-literal-strings.test.ts` (no JSX literal was added).

- [ ] **Step 5: Run the gates**

Run all five from Global Constraints. Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add src/lib/sources.ts src/lib/sources.test.ts src/pages/InstalledPage.tsx src/pages/InstalledPage.test.tsx src/pages/UpdatesPage.tsx src/pages/UpdatesPage.test.tsx src/components/SnapshotStatus.test.tsx src/i18n/en.json src/i18n/zh-CN.json
git commit -m "$(cat <<'EOF'
Say what a standalone tool is, and that it usually updates itself

A standalone artifact carries no description on the wire, since a bare
string cannot be localised; the Installed page now reads a sentence per
standalone source in both locales. On the Updates page a self-updating
standalone tool keeps its real button and gets the honest sentence --
it will probably do this itself the next time it runs -- which is
auto_updates' first reader outside the pinned-cask copy. The two empty
states name tools with their own installer, Claude Code being the one
registered.

Co-Authored-By: Codex <noreply@openai.com>
EOF
)"
```

---

### Task 11: README — the source row and the test counts

**Files:**
- Modify: `README.md` — the "What it manages" table (after the `| Ollama — models | yes | yes |` row) and the two test-count sentences (the `> **Status: pre-release.**` block near the top and its Chinese counterpart under `## 中文`)  [F's file: F also edits both count lines and inserts a paragraph after the Ollama row; anchor by text]
- Test: none new (prose); the five gates.

**Interfaces:** none — prose. The row's claims are Task 3, stage 6 (reads), Task 3, stage 8 (updates), Task 2/6 (no uninstall), and `docs/what-we-run.md`'s Claude Code section.

- [ ] **Step 1: Add the row**

Directly after the table row `| Ollama — models | yes | yes |` (and before F's paragraph about the Unknown page, if it follows the table), insert:

```markdown
| Claude Code — the native install, via its own installer | yes | updates yes; install no (the installer is Anthropic's, and Banager never runs it); uninstall not yet — the row says so and offers no button |
```

- [ ] **Step 2: Update the two test counts**

Get the numbers from the suites, never by hand:

```bash
cargo test --workspace 2>&1 | grep -E '^test result' | awk '{ passed += $4 } END { print passed }'
pnpm test 2>&1 | grep -E '^\s*Tests\s'
```

Put the Rust total where the status block says `covered by <N> Rust tests` and the Chinese block says `有 <N> 个 Rust 测试`, and the front-end total where both say `<M> front-end tests` / `<M> 个前端测试` (F's Task 10 changed the same four numbers; whatever they read now, replace them with today's).

- [ ] **Step 3: Run the gates**

Run all five from Global Constraints. Expected: all clean.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "$(cat <<'EOF'
List Claude Code among the sources, with the new test counts

Reads and updates, no install, and -- for now -- no uninstall, which the
row says rather than implies.

Co-Authored-By: Codex <noreply@openai.com>
EOF
)"
```

- [ ] **Step 5: Delivery note (goes in the branch's PR description / handover; not a file)**

> **Step B: the standalone skeleton and Claude Code.** A native Claude Code install (`~/.local/bin/claude` → `~/.local/share/claude/versions/<v>`) is a *Claude Code* group on the Installed page with one row; its update badge compares the launcher's live version with `downloads.claude.ai`'s channel pointer and lists an update only when the published version is greater; *Update* runs `claude update` and a no-op update is reported as needing attention; the row says it cannot be uninstalled here (no button; `NoSafeMethod`, refused by Rust too) until step C; five notices say which copy runs when `claude` is typed, or that only a dangling launcher is left.
>
> **What later steps change** — honest, not bugs: until **C**, no uninstall for Claude Code (the row explains). Until **D**, `agy` and `grok` are not sources (their launchers stay on the Unknown page), the `owned_roots` table has only the `standalone-claude` row, and the empty-state copy names only Claude Code. Until **E**, no rustup row.
>
> **Rulings taken** (see "Rulings this plan makes"): non-optional recipe fields where B has no `None` producer; `Detected { home }` only; `check_updates` reads a fresh version; `plan(Uninstall)` → `NoSafeMethod`, `plan(Install)` → `Unsupported`; one `owned_roots` row; fixtures and registration in one commit; the host added with its first request; one key for `NoSafeMethod`'s two descriptions; first non-empty line for `parse_version`; B-true copy for `launcherOnly` (no uninstall promised — **step C restores spec §9.2's sentence together with the uninstall**) and for `NoSafeMethod` ("can't yet move its files to the Trash safely", true for both of §6.1's "Neither" cases).
>
> **Recorded on this Mac**: `adapters/fixtures/standalone-claude/<version>/` with the version line, both channel pointers and the layout — read-only commands only; `claude update` was never run.

---

## Self-review against the spec

**1. Spec coverage.** Tasks 1–2 cover the wire variants, notices, uninstall gate, labels and both locales (§七, §6.1, §9.1–9.2). Task 3 stages 3–9 cover recipe shape, detection, inventory, full-token extraction, comparison, fresh checks, upgrade planning/execution/verification, actual recordings, registration and scan ownership (§2–5, §9.3–9.5). Task 10 supplies reachable summary/hint readers and accurately scoped empty states (§9.2); Task 11 updates the source matrix and measured counts. The deviations below identify where the spec's literal wording would produce an incorrect result.

**2. Completion checks.** Every changed implementation and regression case is supplied in full. The complete core lands in one task with registration; it introduces no temporary dead-code exemption. Recorded fixtures remain machine output, never synthetic data.

**3. Type consistency.** The authoritative interface map above names every new type and reader. The summary uses `installedDescription` and is rendered on the real `NoSafeMethod` artifact. Tests of unsupported version tokens retain both full strings and expect the existing uncheckable-candidate shape.

## Spec points this plan could not follow literally, and facts found while writing it

1. **§9.2's two `emptyStates` sentences name Antigravity, Grok and rustup.** In B only Claude Code is registered; copy that promises three unregistered tools is false. B's sentences name Claude Code; D and E extend them (ruling 8). §十 row B should say so.
2. **§3.1's `Option` fields have no `None` producer in B** (`latest`, `upgrade`), and `RouteKind::FlatFile`, `VersionParse::SecondToken`, `$CARGO_HOME/` have none either. By §十's own rule (and §十三 #41, which the spec applied to B once already) they are non-optional / absent in B and widened with their producers (ruling 1).
3. **§3.2's `Detected { home, euid, cargo_home, launcher, real }`**: in B only `home` has a reader (`check_updates`), and `launcher`/`real` are re-probed rather than cached (§3.6 says inventory is not detect's cache). `Detected { home }` in B, widened in C and E (ruling 2). Likewise §3.2's "`plan()` returns `Refused` when `detected` is `None`" has no B reader.
4. **F's hand-off asks B for three `owned_roots` rows**; §十 allows only the `standalone-claude` row in B (ruling 5). When the spec's §十 row B is next edited, one clause should name the row.
5. **`~/.claude/settings.json` on the author's Mac has no `autoUpdatesChannel` key** (checked read-only while writing this plan: the keys present are unrelated to updates). The channel therefore defaults to `latest` on this Mac; the fixture README says so, and the channel reader is exercised with inline JSON exactly as §9.3 prescribes.
6. **`claude doctor` did not print its channel line non-interactively within 20 s** while writing this plan (it seems to want a TTY); nothing in the plan depends on it — the spec cites its output only as corroboration.
7. **A's `docs/what-we-run.md` sentence "Every source but Homebrew finds its executable with `resolve_exe`" becomes false at B**; Task 3, stage 9 amends it. A's `ALLOWED_HTTPS_HOSTS` doc comment enumerating three hosts is amended in Task 3, stage 7. A's `test_host_allowed_accepts_the_three_https_urls_the_adapters_build` is left as it is (its name counts A's hosts) and a fourth test is added, rather than renaming A's.
8. **`tests/fixtures_layout_test.rs`'s set-equality and A's `what_we_run_test`'s section test force the recording, the registration and the trust-file section into one commit** (Task 3, stage 9). The spec's §十 row B lists them as separate items; they are one deliverable in practice.
9. **`MockRunner` records argv only**, so §3.4's one environment variable cannot be asserted through it; Task 3, stage 6 defines a small spec-recording runner in the test module (ruling 12).
10. **Line numbers**: the spec pins `8ba6f52`; HEAD is `f574d9f` (= `8ba6f52` + two commits touching only `docs/`), so every `file:line` the spec cites in files A and F do not touch was re-read at `f574d9f` and holds (`sources.ts:222`, `InstalledPage.tsx:343-344`, `UpdatesPage.tsx:444-475`, `session/mod.rs:500`, `model.rs:93-105`, `plans.rs:187`, `types.ts:122`, `refresh.rs:598`). At review time A and F had landed (`eb254ef`): their diff touches `brew/mod.rs`, `pipx.rs`, `http/real.rs`, `lib.rs`, `scan/mod.rs`, `tests/unknown_scan_test.rs`, `tests/what_we_run_test.rs`, `docs/what-we-run.md`, `UninstallDialog.{tsx,test.tsx}`, `types.{ts,test.ts}`, `warnings.{ts,test.ts}` — not yet `src/i18n/*.json`, `README.md` or `src/lib/sources.ts`, so the "F's file" cautions on those stay, and the `sources.test.ts` line numbers above were re-read at `eb254ef`.
11. **§9.2's `launcherOnly` sentence and §6.1's `NoSafeMethod` for pre-C Claude Code contradict each other within B.** The sentence promises an Uninstall on a row whose artifact §6.1 makes `NoSafeMethod`; §十 row B asks only that "半卸载态有一行会说话", and a sentence that names a missing button does not. B ships true copy and C restores the spec's (ruling 14). The spec's §9.2 entry should say the sentence is C's.
12. **§9.2's `NoSafeMethod` sentence ("no verified list of the files") is false for the tool B first produces it for**: claude.md §7 verifies Claude Code's two-path list. B's sentence names what is actually missing — a safe way to move the files — which holds for both cases §6.1's "Neither" covers (ruling 15). The spec's §9.2 entry should be reworded the same way, since C does not change it.
13. **claude.md §6 does not verify how `claude update` writes.** The research verifies install.sh's download-then-relink order and says outright that the compiled updater's steps were not read and that "Banager should not claim this is officially guaranteed"; 附录 B refuses UNVERIFIED facts in user-visible sentences. The `CLAUDE` doc, the Task 3, stage 4 provenance paragraph and the `## Claude Code` section therefore attribute the mechanism to install.sh only; the reading gates exit-0 success, and stopped upgrades remain unconfirmed, and the section states the cancel policy (`KillThenReconcile`) as §9.5 asks. Likewise nothing verifies that a bare `claude --version` reaches the updater (claude.md §5 quotes only the doc's startup sentence; spec §3.4 says only 启动时); the variable is set on every version read regardless, and the copy says so rather than the stronger claim.


14. **§3.1's numeric-only extraction contradicts §4.3's incomparable-pair behavior.** Keep the full selected token, including prerelease/build suffixes; `compare_dotted` alone decides whether the local/remote pair is comparable. A successful `--version` returning a prerelease is available, and an incomparable pair becomes the existing `uncheckable_candidate`, with both strings in its reason. No suffix stripping and no semver ordering is added. Endpoint parsing checks only that its body is one non-empty token.
15. **§3.6's unknown version cannot safely authorize an upgrade success.** The actual `reconcile_from` wraps the inventory string in `Some`, including an empty one, while `run_operation` permits `VersionChange::Unknown` after exit 0. B's standalone `reconcile` therefore returns `AdapterError::Parse` for an owned launcher without a readable version; the unchanged operation manager reports `Unconfirmed`. Detection and inventory still preserve a `LauncherOnly` row and its route presence. **Step C handoff:** before enabling uninstall, make verification operation-aware so uninstall reads that presence even without a version, while upgrades retain this strict check; do not restore the unsafe generic empty-version reconciliation. No operation-aware field or API is added before its step C reader exists.
16. **§七's `resolve_exe` recipe accepts non-executable namesakes.** The shared helper only calls `is_file()`. B's standalone PATH-notice lookup checks executable permission bits and does not alter package-manager discovery. Synthetic executable targets get mode `0755`; an earlier mode-`0644` namesake must not shadow the native launcher. This remains a PATH/filesystem estimate, not shell alias, ACL or effective-credential emulation.
17. **§3.3's lexical dangling rule needs filesystem parent semantics.** Canonicalize the launcher's parent before joining relative link text; resolve existing components before folding `..`. Only `NotFound` enters the dangling branch. Permission errors, loops and unresolved intermediate symlinks are conservatively unrecognized, not `LauncherOnly`; marker exclusion also applies to the normalized missing target. Tests cover a symlinked bin directory, a loop, and each package-manager marker inside the accepted root.
18. **§9.2's summary location is unreachable for B's `NoSafeMethod` artifacts.** Task 10 renders the localized summary alongside the refusal, using `installedDescription` and the real blocked artifact shape. The refusal and absence of Uninstall remain visible; no imaginary unblocked Claude row is used to exercise the summary.
19. **Same-task production readers require a complete core task.** Former Tasks 3–9 are stages of one Task 3 and one commit. Types, recipe data, route helpers, the complete `Adapter` implementation and registration land together; there is no dead-code allowance or public-visibility workaround for fields awaiting a later task.
20. **(Withdrawn.)** An earlier revision added `user_scripts_dir` to every `ManagerInstance` literal because an unfinished, since-abandoned fix had put that field in the worktree. The field does not exist; every literal in this plan omits it.

## Regression verification for the revised plan

During implementation, first run each named test with the relevant pre-fix behavior still present, then apply the supplied correction and rerun the focused suite. At the bare pre-B baseline, missing standalone symbols cause compile failures; those alone do **not** prove the semantic regressions. Once the complete core compiles, the following before/after assertions distinguish the faulty behavior without unrelated failures:

| Finding | Regression and failure before the correction |
|---|---|
| 1 | `test_a_claude_update_exiting_zero_with_a_failed_version_read_is_unconfirmed` and `test_a_claude_update_exiting_zero_with_a_dangling_launcher_is_unconfirmed`: the old empty-version reconciliation returns `Succeeded`, not `Unconfirmed`; the latter mutates disk during the update command and confirms the launcher remains discoverable. |
| 2 | `cargo test --help` exposes one `[TESTNAME]`; each repaired command has one filter and reaches the intended tests instead of argument parsing failure. |
| 3 | With a newly recorded metadata version, `test_detect_lists_the_native_install_as_one_instance` now selects that version; the old fixed-input verified assertion fails because `unverified_version` becomes `Some`. The unverified test also selects a value outside the recorded list. |
| 4 | `test_version_extraction_preserves_the_complete_token` and `test_prereleases_stay_available_and_incomparable_pairs_are_uncheckable`: old extraction returns `None`/`NotResponding` for a local suffix, and old endpoint parsing bypasses the both-string comparison reason for a remote suffix. |
| 5 | `test_shadow_note_skips_an_earlier_non_executable_namesake`: `is_file()` lookup reports `ShadowedByOther`; executable-aware lookup reports no note for the native winner. |
| 6 | `test_probe_resolves_a_linked_bin_before_relative_dotdot` and `test_probe_does_not_call_a_symlink_loop_launcher_only`: old normalization and catch-all error handling falsely return `LauncherOnly`. |
| 7 | `test_check_updates_uses_the_version_after_inventory_not_detects_version`: detect reads 281, inventory reads 290, endpoint answers 290; using `inst.version` creates a false 281-to-290 candidate. The failed-live-read case also refuses an actionable candidate from that stale version. |
| 8 | InstalledPage's `shows the standalone summary alongside its real uninstall refusal`: the old refusal-only branch hides the summary with `NoSafeMethod`; the test also requires the refusal and only the other artifact's Uninstall button. |
| 9 | Task 1's bilingual notice assertions reject the old “both on this page” and unconditional terminal-failure text. |
| 10 | `test_probe_excludes_package_manager_markers_even_inside_the_native_root`: with marker exclusion removed, each existing target passes the root fingerprint and is wrongly `Present`; missing-target cases protect the dangling branch too. |
| 11 | Documentation correction to match unchanged runtime contracts: the recording runner and plan-env assertions distinguish added overrides from inheritance; `test_a_stopped_claude_upgrade_stays_unconfirmed_even_if_the_version_moves` guards the existing cancel/timeout behavior. Those existing semantics are not claimed to fail before a prose-only correction. |
| 12 | Updated `SnapshotStatus` expected sentences fail against the old broad empty-state copy; both locales explicitly restrict standalone support to Claude Code's native default location. |

After the complete core task, run `cargo fmt --all` before all five gates. Run Task 10's page, locale and type checks before its commit. Preserve explicit `git add` paths and the final blank line plus `Co-Authored-By` trailer in every commit message. This plan revision performed static source/spec checks and syntax parsing only; it did not execute these implementation tests or any recorder command.

## Earlier review log (historical task numbering)

This is the prior pass, retained for traceability. Its one-commit dead-code allowance, old dangling normalization, and cancellation wording are superseded by the revised instructions above and the Astra log below.


Adversarial review of this plan, 2026-09-25, verified against the worktree at `eb254ef` (A and F landed), the spec, claude.md and the A/F plans. Every point was checked before it was acted on; "accepted" means the plan was changed as recorded.

| # | Verdict | Reason (one line) | Where the plan changed |
|---|---|---|---|
| 1 | accepted | Task 6's only `#[async_trait]` was in `mod tests` (via `use super::*`, which does pick up a parent's private `use`), so the lib target had an unused import; `uv.rs` imports it only because its `impl Adapter` is in the same file; `dead_code` covers the unread `http` field | Task 6: import removed from Step 3, added to the test module; `#[allow(dead_code)]` written on `http` with its reader named; Interfaces and Step 2/4 say so; Task 7 deletes it |
| 2 | accepted | `inventory` sets `NoSafeMethod` on every row incl. `LauncherOnly` and `blocked_uninstall` (`plans.rs:103-115`) refuses on that field, so "Uninstall removes the link" named a button the page hides; spec §6.1 makes pre-C claude `NoSafeMethod`, §十 row B wants the row to "say" something true | Task 1: en/zh `launcherOnly.description` rewritten (no uninstall promised, points at the official install/uninstall instructions), `sources.test.ts` asserts it; Rust doc, `sources.ts` comment, Task 5 `Probe` doc, Task 6 detect/test/reconcile comments, Task 9 section, commit bodies; ruling 14; deviation 11 |
| 3 | accepted (premise qualified) | The dangling branch compared `lexical_join(parent, text)` against `root` as spelled only, so a home reached through a symlink whose link text is spelled through the real home answered `Absent`; whether install.sh spells the text through the real home is itself unverified, but the hole exists whenever the two spellings differ, and Review Focus #4 claimed the case without covering it | Task 5: `canonicalize_existing_prefix` helper, the branch compares both spellings, `test_probe_reports_launcher_only_under_a_home_reached_through_a_symlink` added, count 14→15, rules paragraph, Interfaces, commit body; Review Focus #4 |
| 4 | accepted | `CheckOutcome` derives `Default` (`adapters/mod.rs:56`); no `Vec::new().into()` anywhere in the crate; inference through `From<Vec<UpdateCandidate>>` is fragile for no gain | Task 7: `Ok(CheckOutcome::default())` |
| 5 | accepted | `plan()` copies `upgrade.cancel` without naming `CancelPolicy`; the test module imports it itself; an unused lib import fails `-D warnings` | Task 8: `CancelPolicy` removed from the non-test import list, the hedge replaced by the reason |
| 6 | accepted | `sources.test.ts:204-219` is the `hasSourceNotice` `it`, its array `:209-216`; `:99-112` is the stopped-Ollama test; `what-we-run.md` wraps both quoted sentences (lines 4-5, 54-55) | Task 1 Step 1 line reference corrected; Task 9 Step 4 says the sentences wrap and to match by words |
| 7 | accepted | Task 4's route.rs test module used `Path::new` with only `use super::*; use std::path::PathBuf;` in a file that had no imports yet | Task 4: test module imports `std::path::{Path, PathBuf}` |
| 8 | accepted (same defect as 2, more evidence) | As 2; also the state is reachable in B by a manual `rm -rf ~/.local/share/claude`, and the "earlier uninstall / Trash" cause was false in B | As 2 |
| 9 | accepted | claude.md §6: "No direct official statement was found", "UNVERIFIED as an explicit official guarantee", "Banager should not claim this is officially guaranteed"; §8 lists the compiled updater as not decompiled; 附录 B refuses UNVERIFIED facts in user-visible sentences; the plan cited install.sh as if it were `claude update` | Task 4 provenance paragraph and `CLAUDE` doc; Task 9 `## Claude Code` write paragraph; deviation 13 |
| 10 | accepted | The README asserted three recording-day facts (no channel key, no cask, no npm copy) that none of the five recording commands observes; they came from the 2026-09-24 research and the plan author's check | Task 9 Step 1: three read-only commands added (`grep -c`, `brew list --cask`, `ls "$(npm root -g)/@anthropic-ai"`); README brackets `[CASK]`, `[NPM]`, `[CHANNEL_KEY_COUNT]` with two wordings; self-review counts seven values |
| 11 | accepted | Spec §9.5 requires each standalone section to state "升级 argv 与取消策略"; the section had argv/timeout/password only; A's sections established no cancel column, so B is the first the spec asks it of | Task 9 section: a Cancel sentence under the write table (`KillThenReconcile`, `--version` re-read, `Unconfirmed` unless settled) |
| 12 | accepted | claude.md §5 quotes only the doc's startup sentence and spec §3.4 says only 启动时; nothing observed `--version` reaching the updater (the agy research found the opposite for that tool) | Task 4 provenance paragraph and `CLAUDE` doc; Task 9 Detect paragraph: "whether `--version` alone triggers the check was not observed; set on every version read regardless" |
| 13 | accepted | The row said "(its section)" two commits before the section exists; A's own "By" column names only the adapter method; A's test needs only the host string | Task 7 row now `Claude Code's \`check_updates\` (\`StandaloneAdapter\`)`, with a note; Task 9 changes nothing there |
| 14 | accepted (duplicate of 6a) | Files list said `:204-219`, Step 1 said `:101-108`; an executor following Step 1 would overwrite the Ollama test | Task 1 Step 1: `:204-219` / array `:209-216`, with the wrong test named so it is not touched |
| 15 | accepted (duplicate of 6b) | Both trust-file sentences are hard-wrapped in the landed file | Task 9 Step 4 preamble |
| 16 | accepted | `parse_version` and ruling 11 read the first *non-empty* line; the trust file must describe the code | Task 9 Detect paragraph: "first token of the first non-empty line" |
| 17 | accepted | "Verified against Claude Code 2.1.281" was hard-coded while Step 1 named only the TOML as the place to update | Task 9 Step 1 comment lists the section as the third place; the section says which number to write |
| 18 | accepted (the `http` half of 1) | Step 4's note thought aloud and left the executor to choose | Task 6 Step 3 carries the attribute; Step 4 states the rule; Interfaces say so |
| 19 | accepted | claude.md §7 verifies the two-path list, so "no verified list" was false for the one tool B produces `NoSafeMethod` for; the spec's sentence covers only half of §6.1's "Neither" | Task 2: en/zh `NoSafeMethod.description` → "can't yet move its files to the Trash safely", the InstalledPage test sentence, the Rust/TS docs and comments; ruling 15; deviation 12 |

Rejected: none. Points 8, 14, 15 and 18 restate 2, 6a, 6b and 1 with more evidence and were folded into those fixes.

## Review log (Astra, 2026-09-25)

Re-verified all 12 findings from `astra/review-plan-b.md` against the actual `/Users/brulek/dev/Banager-phase4` worktree (HEAD `d3890f4` plus current uncommitted edits), the standalone spec, and the cited local research. **Accepted: 12. Rejected: 0.** Stage numbers retain the original core task numbers.

| # | Finding | Decision | One-line reason |
|---|---|---|---|
| 1 | Exit 0 with a broken installation reports success | accepted | `adapters::reconcile_from` and `ops::run_operation` accept an unknown version after exit 0; stage 6 rejects unverifiable standalone versions, stage 8 covers both broken outcomes, and detection/inventory preserve step C presence (§3.6, D6). |
| 2 | Two positional cargo test filters | accepted | Cargo accepts one TESTNAME; Task 2 and stage 7 now run separate filtered invocations, preserving the spec's test gates (§9.4). |
| 3 | New recordings invalidate a synthetic verified-version test | accepted | `AdapterMeta::unverified_version` tests actual metadata membership; stage 6 derives both verified and unverified test inputs from metadata (§2.1, §9.3). |
| 4 | Prereleases become NotResponding | accepted | `refresh_round` skips unavailable instances; stages 3/7 preserve full tokens and emit uncheckable pairs, explicitly resolving §3.1 versus §4.3. |
| 5 | PATH lookup accepts non-executable files | accepted | `runner/path_env.rs::resolve_exe` checks only `is_file`; stage 5 adds executable-aware standalone lookup and executable test targets, deviating from §七's literal helper choice. |
| 6 | Dangling normalization loses symlink-parent semantics | accepted | Relative targets are interpreted under the resolved parent; stage 5 fixes that ordering and distinguishes NotFound from loops/other errors in §3.3's route decision. |
| 7 | Badge compares stale detect version | accepted | `refresh_round` inventories before calling `check_updates` and accepts its successful candidates directly; stage 7 reads the live launcher and tests 281/290/290 (§3.6, §4.3). |
| 8 | Summary has no reachable B reader | accepted | `InstalledPage` selects refusal copy for every B Claude artifact; Task 10 now renders summary plus refusal and tests `NoSafeMethod` (§6.1, §9.2). |
| 9 | PATH copy promises unsupported page coverage and terminal failure | accepted | UpdatesPage groups candidates and other prefixes may be unlisted; Task 1 scopes both locales to detected executables and allows another working copy (§七, §9.2). |
| 10 | Exclusion tests do not isolate the marker rule | accepted | Outside-root targets fail the fingerprint without markers; stage 5 tests each marker inside the accepted native root so removing the exclusion fails (§3.3, §9.4). |
| 11 | Updater/environment/cancellation prose contradicts code | accepted | Research and §3.4 say manual updates still work; RealRunner inherits env and stopped upgrades stay Unconfirmed, now stated consistently in stages 4/8/9 (§五, §9.5). |
| 12 | Empty state promises every own-installer tool | accepted | Only Claude's default native route is registered in B; Task 10 limits both locales and their expectations to that supported route (§3.3, §十 row B). |
