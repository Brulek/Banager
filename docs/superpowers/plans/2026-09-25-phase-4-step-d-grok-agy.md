# Phase 4 Step D: Grok Build and Antigravity CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put the last two first-batch AI CLIs on the Installed page as data rows of the `StandaloneAdapter` steps B, C and E built — *Antigravity CLI* (`~/.local/bin/agy`, a flat file that updates itself and offers no update command Canager may run) and *Grok Build* (`~/.grok/bin/grok`, a relative link into `~/.grok` whose own read-only `update --check --json` says whether a newer version exists) — each with an honest update row, Grok's `grok update` button, Antigravity's "Updates itself" badge with no button, a path-list uninstall for both that moves only what the installer put there and says what stays, the backup-file pattern check the spec calls check 5, and the Unknown page's rule 4 that stops a fresh `agy.<time>.old` from being listed as a stranger.

**Architecture:** Two more `pub static Recipe` rows (`AGY`, `GROK`) and the shapes they are the first to need: `Latest::HttpJsonField` (agy's version manifest, Apple silicon only — an Intel Mac gets an honest "could not check"), `Latest::Command` (grok's own check, trusted as it answers), `Recipe.upgrade: Option<UpgradeCmd>` whose `None` makes every candidate `UpdateBlocked::SelfUpdatesOnly` (gate, `updateStateOf`, copy record), `Recipe.backup_globs: &[Glob]` with `Glob` living in `scan/` so the Unknown page's rule 4 and the removal's check 5 read one type, `Expect::File`, `RemovedWhat::Backups` and five `KeptWhat`s, and the `NotOurs` skip for an optional path Canager cannot confirm is the tool's. Everything runs through C's `removal.rs` and `Trasher` unchanged in contract: `plan_removal` learns to list backup files before the launcher and to keep-and-say instead of refusing for an optional path; `take_turn` looks items up the same way. Recordings are read-only (`--version`, `update --check --json`, `curl`, `ls`), with the home folder abbreviated to `~` and numeric owners, and the version-read-does-not-update observation spec §3.4 demands for agy written into the fixture README.

**Tech Stack:** Rust (canager-core: `serde_json` for two JSON bodies, `std::fs` for the checks and the glob, the existing `CommandRunner`/`HttpClient`/`Trasher` seams; no new crate), TypeScript 5 `strict`, React 19, i18next, vitest.

**Spec:** `docs/superpowers/2026-09-24-phase-4-standalone-spec.md` (authoritative; Chinese). This plan implements §十 row D and argues from §3.1, §3.3–§3.5 (the agy and grok rows), §4.1/§4.4 (self-updating semantics, `SelfUpdatesOnly`), §6.1–§6.3 (the `Paths` uninstall, the agy and grok rows of the §6.3 table), §8.3 rule 4, §9, 附录 A. Raw research it cites: `~/dev/Canager/.superpowers/phase4/agy.md`, `grok.md`, `architecture.md`, `unknown-scan.md` (VERIFIED/UNVERIFIED per line). This plan will live at `docs/superpowers/plans/2026-09-25-phase-4-step-d-grok-agy.md`.

## Baseline, and how to read the anchors below

Branch `feat/phase-4-standalone`, worktree `~/dev/Canager-phase4`, first read at HEAD `dc3a979` and re-read at HEAD `db42e79` on 2026-09-25 (A, F and B landed; C's plan committed as `ea30cfb`; C's Tasks 1–7 landed as `d019908`…`db42e79`, so `tests/standalone_uninstall_test.rs` exists; C's Task 8 and all of E not yet landed — `session/mod.rs` still has `test_new_registers_all_eight_adapters`). **The order is A → F → B → C → E → D**, so when this plan executes, all of C (`docs/superpowers/plans/2026-09-25-phase-4-step-c-trash-uninstall.md`) and all of E (`~/dev/Canager/.superpowers/phase4/plan-step-e-rustup.md`, to be committed as `docs/superpowers/plans/2026-09-25-phase-4-step-e-rustup.md`) have landed. **Every shape C and E produce is treated here as existing with the exact names of those two plans' "Core Interfaces" sections**, and every place this plan touches one of them is a row of the "C/E dependency checklist" below, which the executor re-verifies against the landed tree **before Task 1 and again before Task 4** (the first task that edits `removal.rs`). Where the spec and plans C/E disagree, this plan follows C and E (newer, reviewed twice each) and says so in "Rulings" and "Deviations".

The confirm-grep, run at the start of execution and pasted into the branch's handover:

```bash
git log --oneline | head -20
grep -n "enum PlanAction\|pub uninstall:\|pub extra_locks\|pub upgrade:\|pub euid\|pub cargo_home\|pub rustup_home\|pub zdotdir\|fn seated_detected_for\|fn locks(\|fn detected_or_refuse\|pub fn new(\|pub fn with_trash_gap\|pub fn all(\|fn reconcile_after_uninstall\|fn probe_strict\|pub fn expand_route\|pub fn no_extra_locks\|Command(CommandUninstall)\|pub const SHARED_FOLDERS\|pub enum Expect" crates/canager-core/src/adapters/standalone/{mod,recipe,route,recipes}.rs
grep -n "pub struct Job\|pub struct Removal\|pub fn plan_removal\|pub async fn execute_removal\|fn take_turn\|fn check_item\|fn kept_places\|fn disturbed\|fn spelled\|fn shown\|fn is_shared_folder\|struct Look\|struct Kept\|struct Refusal\|pub const TIMEOUT_SECS\|pub const PUT_BACK_SETTLE\|pub struct Confirmed\|pub struct Pacing" crates/canager-core/src/adapters/standalone/removal.rs
grep -n "pub fn scan_dirs\|pub fn scan_unknown\|fn index(\|fn claimant(\|pub(crate) fn display_path\|pub fn owned_roots" crates/canager-core/src/scan/mod.rs
grep -rn "Recipe {$\|Job {$\|Detected {$" crates/canager-core/src/adapters/standalone/
grep -n "fn test_new_registers_all_" crates/canager-core/src/session/mod.rs
ls crates/canager-core/tests/standalone_uninstall_test.rs && grep -n "async fn outcome_of" crates/canager-core/tests/standalone_uninstall_test.rs
grep -n "^## " docs/what-we-run.md
```

**Stop rule:** if the `test_new_registers_all_` grep prints `eight` (E not landed), or `ls` says `tests/standalone_uninstall_test.rs` does not exist (C's Task 7 not landed), stop: C and E land first (Task 4 Step 4 runs that test file, checklist row 24 copies its `outcome_of`, and checklist row 18 renames E's `nine`). Do not start Task 1.

In files A, F, B, C or E touch — which is every file this plan edits except the two new fixture directories, the two new meta files and the new integration test — **every edit below is anchored by a symbol, function, type or quoted line, never by a line number alone.** Line numbers, where given, are hints at `db42e79`. Where the landed, rustfmt'd text differs from a quoted anchor, match by the words and the symbol.

## C/E dependency checklist

Every point where this plan meets C or E. A row whose landed shape differs from what is written here is resolved by taking the landed spelling wherever this plan uses it; the row says how far that reaches.

| # | C/E shape | Where D touches it | What D does if it was spelled differently |
|---|---|---|---|
| 1 | `PlanAction::{Command { program, args, env }, TrashPaths { paths, previewed }}`, `Plan.action` (C Task 1, stage 6e) | Tasks 5–6 tests destructure `TrashPaths`; Task 4 leaves `plan_removal`/`execute_removal`'s contract alone | Use the landed field names |
| 2 | `Recipe { id, meta_toml, route, version, latest, self_updates, upgrade, uninstall, extra_locks }` (B, C, E Task 4) | Task 3 adds `backup_globs`; Task 5 widens `upgrade` to `Option<UpgradeCmd>`; every `Recipe {` literal (`CLAUDE`, `RUSTUP`, C's test-only `NO_UNINSTALL`, any E test recipe) gains the field and `Some(…)` | `grep -rn "Recipe {$" crates/canager-core/src/adapters/standalone/` before Tasks 3 and 5; `missing field` names any the grep missed |
| 3 | `Uninstall::Paths { remove, keep }` (C stage 6b) and `Uninstall::Command(CommandUninstall)` (E Task 6); every RECIPES-wide test destructuring `Paths` with `else { continue }` (E Task 10) | Task 4 rewrites three of those tests; agy and grok are `Paths`, so the skips stay for rustup | Match by test name; keep E's skip form |
| 4 | `RemoveSpec { path, expect, what, optional }`, `KeepSpec { path, what }`, `Expect::{SymlinkIntoRoot, Dir}`, `pub const SHARED_FOLDERS: [&str; 5]` (C stage 6b) | Task 4 adds `Expect::File`, lets a `KeepSpec` with `what: OutsideHome` name an absolute path | If C's field names differ, use C's |
| 5 | `removal::{Job { recipe, detected, remove, keep }, Removal { paths, identities, warnings }, plan_removal, execute_removal, Confirmed, Pacing, TIMEOUT_SECS, PUT_BACK_SETTLE}` and its private `check_item(look, kept, spec, path)`, `kept_places`, `disturbed`, `take_turn`, `spelled`, `shown`, `is_shared_folder`, `identity_of`, `Look { job, canonical_home, launcher, root }` (`root` is `route::expand(home, recipe.route.root)`, landed at `removal.rs:174-190`), `Kept`, `Refusal { path, reason }`, `Turn` (C stages 6c–6d) | Task 4 adds `Job.globs`, `Item`, `listed_items`, `keeps_instead`, `outside_home_keeps(look)`, `points_into`; changes `check_item`'s signature, `plan_removal`, `take_turn`, `kept_places` | Edit the landed function of the same role; the code below quotes C's plan text as the anchor |
| 6 | C's `removal.rs` test helpers `detected(home)` (E adds three fields), `claude_job`, `only`, `trash`, `keep`, `refused`, `identity`, `run`, `no_gap`, `path_changed`, `moved`; `test_plan_removal_refuses_an_optional_path_of_the_wrong_shape`; `test_plan_removal_refuses_a_path_reached_through_a_linked_folder_inside_home` (landed at `removal.rs:859-899`, two halves: `~/.claude -> ~/Documents` refusing the optional `~/.claude/downloads`, and a dotfiles-linked `~/.local/bin` refusing the launcher) | Task 4 reuses the helpers, gives `claude_job` a `globs: &[]`, replaces the first test, and rewrites the second to its launcher half (its first half becomes a keep, folded into a new test) | If a helper has another name, use it |
| 7 | `route::{expand(home, spec), expand_route(home, cargo_home, spec), probe, probe_strict, lexical_join, shadow_note, Probe::{Absent, Present { real }, LauncherOnly}}` (B, C, E) | Read only. `Glob::dir_under` (Task 3) mirrors `expand`'s `~/` rule for the scan, which must not depend on `adapters` | — |
| 8 | `#[derive(Clone, Debug)] Detected { home, euid, cargo_home, rustup_home, zdotdir }`; `seated_detected_for(inst)`, `locks(inst, detected)`, `detected_or_refuse()` (C, E Task 4) | Task 5's `plan(Upgrade)` edit keeps E's `let detected = self.seated_detected_for(inst)?;` line and changes only the `upgrade` binding | If E routed C's `Paths` arm through `seated_detected_for` (E checklist row 16), keep that |
| 9 | `StandaloneAdapter { recipe, meta, runner, http, trasher, trash_gap, detected }`, `new(recipe, runner, http, trasher)`, `with_trash_gap`, `all(runner, http, trasher)`, `canager_core::trash::MockTrasher` (C stage 6e) | Task 5 adds `arch: &'static str` and `with_arch`; every D test passes `Arc::new(MockTrasher::new())` | If `new` has another arity, match it |
| 10 | `Adapter::reconcile_after_uninstall` with `StandaloneAdapter`'s override over `probe_strict` (C Task 2, stage 6e) | Tasks 5–6 tests read presence after an agy/grok uninstall | — |
| 11 | `RouteKind::FlatFile`, `VersionParse::SecondToken`, `Latest::HttpTomlVersion { url }`, `latest::parse_release_stable_toml`, `latest_version`'s two arms (E Tasks 3–4) | Task 5 folds `latest_version` into `published`, keeping both arms word for word | If E named the function differently, fold that one |
| 12 | `Recipe.extra_locks: fn(&Detected) -> Vec<ResourceLock>`, `pub fn no_extra_locks` (E Task 4) | `AGY` and `GROK` carry `extra_locks: no_extra_locks` | — |
| 13 | `HostEnv { path_dirs, home, euid, cargo_home, rustup_home, zdotdir, ollama_host }` (E Task 1); `testing::TempHome::env` (B) | Task 6's integration test writes one literal | Drop a field E did not add |
| 14 | `UninstallUnsafeReason::{OutsideHome, SharedFolder, Missing, NotOwnedByYou, NotWhatInstructionsExpect, OverlapsKept}` (C Task 5) | Task 4's `keeps_instead` names three | — |
| 15 | `Warning::{WillTrash { path, what }, WillKeep { path, what }, AlreadyGone { path }}`, `RemovedWhat { Launcher, Program, Cache }`, `KeptWhat { Settings, SettingsAndHistory }`, `REMOVED_WHAT_KEYS`/`KEPT_WHAT_KEYS` in `warnings.ts` (C Task 3) | Task 2 extends both enums and both `Record`s | — |
| 16 | `scan::{owned_roots, scan_dirs(dirs, env, instances, artifacts, budget), scan_unknown(env, instances, artifacts, budget), Known::{index(instances, artifacts), claimant(raw, resolved)}, display_path (pub(crate) since C)}` (F, C) | Task 3 adds a `globs` parameter to both functions, `home` and `globs` to `index`, `dir` and `kind` to `claimant`; Task 6 adds two `owned_roots` rows | The compiler lists every call site |
| 17 | `Session::scan_unknown` calling `scan::scan_unknown(env, &instances, &artifacts, ScanBudget::default())` (F) | Task 3 passes `&recipes::backup_globs()` | — |
| 18 | `test_new_registers_all_nine_adapters` in `session/mod.rs` (E Task 10) | Task 6 → eleven | If still `eight` (E not landed), stop: E lands first |
| 19 | `recipes::tests::{test_every_recipe_path_is_under_home_or_the_cargo_home, test_a_paths_recipe_names_only_home_paths, test_recipes_lists_each_registered_tool_once_in_reading_order, test_every_recipe_latest_url_is_an_allowed_https_host}` (B, E) and C's `test_every_uninstall_path_is_under_home_and_not_in_a_shared_folder`, `test_every_paths_recipe_moves_its_launcher_last_and_lists_no_path_inside_another` (landed at `recipes.rs:304-345`: `last.path == recipe.route.launcher`, `Launcher`, `!optional`, no `expect` assertion) | Task 4 rewrites three (the launcher-last test keeps C's `last.path == route.launcher` and adds `expect` per `RouteKind`); Task 5 adds arms to the allowlist test; Task 6 changes the count test | Match by name |
| 20 | `UPDATE_BLOCKED_KEYS: Record<UpdateBlocked, UpdateBlockedCopy>`, `unpinCommand`, `displayToken` (per-package actionability, B) | Task 1 adds the `SelfUpdatesOnly` row and `launcherCommand` | — |
| 21 | `ADAPTER_LABEL_KEYS`, `StandaloneAdapterId = "standalone-claude" \| "standalone-rustup"`, `STANDALONE_SUMMARY_KEYS`, `uninstallBlockedCopy`/`UNINSTALL_BLOCKED_OVERRIDES` (B, E Task 11) | Task 7 adds two rows to each of the first three; the overrides are untouched (agy and grok never produce `NoSafeMethod`) | — |
| 22 | The two `emptyStates` sentences as E worded them (E Task 11) | Task 7 replaces them; E's text is the anchor | Match by words |
| 23 | `docs/what-we-run.md`: the intro sentence, "Where the program comes from", `## Claude Code` (its check paragraph, landed at lines 585-599: "so a `~/.local/bin` kept as a link to a dotfiles folder refuses the uninstall, and so does a `~/.claude` that is a link when the download cache is inside it" and "If any check fails, the whole uninstall is refused"), `## rustup`, `## Files Canager writes`, "Moving files to the Trash", the never-list bullets C rewrote (the "Never deletes a file" and "Never moves anything outside the home folder" bullets, lines 856-867), the network table and its last paragraph (A, B, C, E) | Tasks 3, 4, 5, 6 edit by quoted words | Match by words |
| 24 | `tests/standalone_uninstall_test.rs`'s `outcome_of(session, op_id)` (C Task 7) | Task 6's new integration test copies it (a `tests/` file is its own crate) | Copy whatever the landed helper is |
| 25 | `rowDescription`'s blocked branch in `UpdatesPage.tsx`: `t(description, { command: COMMAND_SLOT, source: … })` (B) | Task 1 adds `current`/`target` to that one `t()` call | — |

## Global Constraints

Copied verbatim from the spec's binding rules (spec lines 20–23):

> 产品规则一条不让（spec §1、§6）：每一步说人话；后台工作绝不问密码；执行前先看到确切命令；
> 结果诚实——版本没动是 `NeedsAttention(UnchangedAfterUpgrade)`，中途停止是 `Unconfirmed`，
> 没有证据绝不说成功；fixture 只收真机录制；Canager 不跑 shell、不把下载管进 `sh`；
> 界面绝不提供 Rust 会拒绝的操作；所有文案 en + zh-CN。

And from spec §十 ("每一步只带**该步有生产者**的变体与字段——「先定义、后面某步再用」正是本项目最常见的缺陷") and §2.2/§2.3 ("每个新字段点名生产读取方"), applied to this step:

- **Every new field, variant, constant or function names its production reader in the same task** (doc comment and Interfaces block), and that reader lands within this step. Wire variants whose *producer* is the core task land with their front-end reader in their own task first, as B's Tasks 1–2 and C's Tasks 3–5 did: `UpdateBlocked::SelfUpdatesOnly` (Task 1; producer `check_updates`, Task 5), `RemovedWhat::Backups` and the five `KeptWhat`s (Task 2; producers Tasks 4–5). Declared deferrals inside the step: `Recipe.backup_globs` and `Glob` (Task 3; the first non-empty producer is `AGY`, Task 5; the readers — rule 4 and check 5 — are live from Tasks 3 and 4), `Expect::File` (Task 4; producers `AGY`/`GROK`, Task 5). Nothing here is defined for a later step.
- **Read-only recordings only.** The recording commands (Task 6) are `AGY_CLI_DISABLE_AUTO_UPDATE=true ~/.local/bin/agy --version` (once), `~/.grok/bin/grok --version` (once), `~/.grok/bin/grok update --check --json` (its `--help` says "Check for updates without installing", grok.md §4, VERIFIED), one `curl` of the agy manifest, `cat` of one state file, `stat`, `readlink`, `ps`, `diff`, and `ls` of the tools' own paths — with `$HOME` replaced by `~` and owners numeric (`ls -lan … | sed "s|$HOME|~|g"`; the README states that exact transformation, since the file is then not byte-for-byte). **Never run, in a recording or a test: `grok update` (without `--check`), `agy update`, bare `agy`, bare `grok`.** No recording or test on this Mac touches the author's installs; every layout a test needs is synthetic, in a temp directory.
- **Honest outcomes.** Nothing here touches `run_operation`. A `TrashPaths` uninstall keeps C's contract (`Succeeded` only when every path moved, `Failed` with macOS's words, `Unconfirmed` between items, `CanagerFailed(PathChanged)` for anything that changed since the preview). agy's newer version is a real candidate with no button (`SelfUpdatesOnly`), never "up to date" and never `checkable: false`. grok's candidate is what grok itself answered.
- **No shell.** `PlanAction::Command.program` is only ever the instance's `exe_path`; `Latest::Command` runs the launcher with fixed argv and no shell; a recipe has no field that could name another program.
- **The UI never offers what Rust refuses.** `SelfUpdatesOnly` is refused by `Session::issue_plan` (`blocked_upgrade`, generic over `UpdateBlocked`) and by `plan(Upgrade)` for a recipe with no `upgrade`; `updateStateOf` hides the button and the checkbox for it.
- **en + zh-CN for all copy.** Every new key in both `src/i18n/en.json` and `src/i18n/zh-CN.json`; `completeness.test.ts` requires each key to be looked up by a *literal* in non-test source (lookups go through `Record`s of literal keys); `no-literal-strings.test.ts` forbids English literals in JSX; zh-CN prose uses full-width `，：（）——` between CJK characters and carries only `_other` for a plural key.
- **No author-machine details in tests or source** beyond public tool names; every test path is under a temp directory or a made-up `/Users/someone`; the fixtures carry `~` and numeric owners.
- **The five gates**, from README.md "Tests — all five must pass before anything is committed":

  ```bash
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  pnpm test
  pnpm typecheck
  ```

  Run `cargo fmt --all` before the `--check` gate: the Rust below is written *for* rustfmt, and rustfmt decides line breaks.
- **Commits:** `git add <exact paths>` (never `-A`, never `.`), an imperative subject in sentence case, a body that says why, a blank line, then the attribution line. The commit blocks below end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`, the line this plan's author was told to use; an executing session told to use a different attribution replaces that whole line with its own and never adds a second.
- **Read-only worktree rule for the plan's author** (not the executor): this plan was written without creating, editing or deleting anything under `~/dev/Canager-phase4`, where C's executors work.

## Rulings this plan makes

Where the spec leaves a choice to the step, or where C's or E's landed shapes changed what the spec drew, the decision is made here so no task has to.

1. **agy's `~/.cache/antigravity` is kept and said, not moved.** Spec §6.3 lists it as `Dir · Cache · optional`; C's check 1 (ruling 5) refuses any path whose resolved folder is `~/.cache` (`recipe::SHARED_FOLDERS`, reason `SharedFolder`), and `recipes::tests` refuses it in the constant. C left the decision to D (backlog): a tested exception, or the list changes. **The list changes.** The exception would have to say "a folder directly in `~/.cache` may be moved when it is named after the tool" — a second rule the never-list exists to not have, for a folder that is empty on this Mac (`~/.cache/antigravity/staging`, 0 entries, 2026-09-25) and at most one interrupted download when it is not. Instead the folder is a `KeepSpec` with a new `KeptWhat::InstallerCache`, listed only when it exists (C ruling 6), whose sentence says what it is, that Canager moves nothing that sits directly in `~/.cache`, and that the user may delete it. **Author-facing consequence:** after an Antigravity uninstall `~/.cache/antigravity` stays (usually empty; up to ~180 MB if an update was interrupted mid-download), the dialog says so, and the Unknown page never shows it (it is not a bin directory). Recorded in the backlog entry C opened (Task 8).
2. **agy's launcher is `RemovedWhat::Launcher`, not the spec's `Program`.** Spec §6.3 writes `~/.local/bin/agy · File · Program`; C's invariants test requires the last listed path to be `Launcher`, and "the command itself" is exactly what the 176 MB file is. The sentence the user reads is "Moves to the Trash: ~/.local/bin/agy (the command itself)".
3. **grok's optional fallback links move first, not after the program folders — a precaution, not a correctness fix.** Spec §6.3 orders `~/.local/bin/grok` and `~/.local/bin/agent` after `~/.grok/downloads`. Either order works with C's code: `take_turn` re-runs `check_item` before each move, check 4 for a `SymlinkIntoRoot` path is `route::probe`, and `probe_strict`'s dangling branch (`route.rs:164-175`) reads the link's own text with `one_hop` and folds only the *existing* prefix of the hop's folder (`canonicalize_existing_prefix`, `route.rs:70`) — so once `~/.grok/downloads` is in the Trash, a fallback whose text names `~/.grok/bin/grok` (two hops) still lands under the root while `~/.grok/bin` exists, and a fallback whose text names the download directly lands there lexically: both answer `LauncherOnly`, which `check_item` accepts (`removal.rs:327-334`). The order is chosen for a different reason: the installer's link text is UNVERIFIED (grok.md §2 says only that it links "into" `~/.local/bin` or `/usr/local/bin`; one hop or two is not known), so the two links go while every folder their text could pass through is still on the disk — check 4 then answers from a link that *resolves* (`Present`), never from its text alone — and a run that stops after them leaves an intact program with no dangling, foreign-looking `~/.local/bin/grok` behind it. The launcher-last rule (spec §6.2) holds: `~/.grok/bin/grok` is the last item (ruling 4), a stopped run leaves it dangling into `~/.grok`, the launcher-only state a second run finishes. `recipes::tests` pins the order. (An earlier draft of this ruling claimed the spec's order would make `probe` answer `Absent` and the turn read as `PathChanged`; that was wrong, and the fixture README, the trust file and the backlog say the reason given here.)
4. **grok's `~/.grok/bin` is not moved as a folder; its two links are the items, `grok` last.** Spec §6.3 lists `~/.grok/bin · Dir · Launcher` as the last item. The installer puts `~/.grok/bin` on `PATH` (`export PATH="$HOME/.grok/bin:$PATH"`, grok.md §2), so a user's own script dropped in there is a realistic state, and a folder move under the sentence "(the command itself)" would take it without saying so — the "see the exact thing before it moves" rule (spec §1) under strain. So the list names what the installer put there: `~/.grok/bin/agent` (`SymlinkIntoRoot · Launcher · optional`, the second name of the same command) and then `~/.grok/bin/grok` (`SymlinkIntoRoot · Launcher`, last, `route.launcher` itself), and the emptied `~/.grok/bin` stays inside the kept `~/.grok` (harmless: the `PATH` line the installer left points at an existing, empty folder). C's launcher-last invariant (`last.path == route.launcher`) is kept as C wrote it; Task 4 adds only the `expect` per `RouteKind` that agy's `File` needs. **Author-facing consequence:** after a grok uninstall an empty `~/.grok/bin` folder remains inside `~/.grok`; the preview lists two links, not a folder. Recorded as deviation 14.
5. **An optional path Canager cannot confirm is the tool's is kept and said (`KeptWhat::NotOurs`), for three of the six refusal reasons, not one.** Spec §6.3 check 4 says "指纹不符 → 跳过并发 WillKeep { NotOurs }" (a foreign `~/.local/bin/agent` must not make grok un-uninstallable, §十三 #27). C's `check_item` reports the wrong shape, a link elsewhere and a linked folder on the way all as `NotWhatInstructionsExpect`, and a folder leading out of the home folder or into a shared one as `OutsideHome`/`SharedFolder`. For an *optional* path all three mean the same thing to the user — Canager will not touch it and the uninstall goes on — so all three become the skip (`removal::keeps_instead`); `NotOwnedByYou` and `OverlapsKept` still refuse the whole list, because they are about what the move would do. A dotfiles-linked `~/.config` therefore keeps grok's optional `grok.fish` rather than refusing grok's uninstall, which is the outcome §十三 #27 wanted for the analogous `agent`. The `NotOurs` sentence is widened to cover both cases (deviation 5). Backup files matched by a glob are optional in this sense too.
6. **A kept path outside the home folder is reported, never protected — and reported only when it is demonstrably this tool's link.** Spec §6.3 lists `/usr/local/bin/grok` and `/usr/local/bin/agent` as `WillKeep { OutsideHome }` "若存在". C's `kept_places` expands every `KeepSpec` with `route::expand` (which panics on a non-`~/` path) and feeds it to `disturbed`, which refuses a listed path that a kept one *leads into* — and a `/usr/local/bin/grok` link leads into `~/.grok/downloads`, so treating it as kept would refuse every grok uninstall it exists for. So a `KeepSpec` whose `what` is `OutsideHome` may (and must) name an absolute path, `kept_places` skips it, and `removal::outside_home_keeps` turns it into a `WillKeep { OutsideHome }` sentence, after the recipe's other kept paths. **Existence is not enough for the sentence.** Its copy says "after uninstalling it's a dead link you can delete yourself", which is true only of a link into this tool's root: on an Intel Mac Homebrew's prefix is `/usr/local`, so with the `grok-build` cask installed `/usr/local/bin/grok` is Homebrew's live link into its Caskroom, and `/usr/local/bin/agent` is a generic name any CLI may own — telling the user to delete either would be a false, safety-relevant sentence. So `outside_home_keeps` reports a path only when `removal::points_into(path, root)` says it is a symbolic link whose target lies under the recipe's root as expanded for this home (`Look.root`): resolved (`canonicalize`) when it resolves, or, dangling, by its own text folded from its folder (`route::lexical_join`). A regular file, a folder, a link elsewhere, or nothing: no sentence. This also keeps every test off the machine's real `/usr/local/bin`: a link there can never point into a test's temp home. `recipes::tests` holds every other `keep` path to `~/`.
7. **Backup files are listed before the last item, in name order, and are optional.** `Recipe.backup_globs` is separate from `remove` (spec §3.1), so `removal::listed_items` places every match after all listed paths but the last (the launcher) — the launcher stays last (spec §6.2). A match is a regular file (never a link) directly in the pattern's folder whose name is `prefix` + at least one character + `suffix`; `Glob::matches_name` says so, so `agy..old` is not one. A match Canager cannot confirm (ruling 5) is kept and said; one that appears between the preview and the click is `PathChanged` (C's list comparison, unchanged). `take_turn` finds each item through the same `listed_items`, so a backup that vanished by its turn is `Changed`, as C treats every other vanished item.
8. **`Glob` lives in `scan/mod.rs`**, as spec §3.1 says: the scan reads it (rule 4) and `adapters` depends on `scan`, never the reverse. `Glob::dir_under(home)` joins `~/` exactly as `route::expand` does (the scan compares raw spellings, F's rule), and `recipes::tests` holds every glob's `dir` to `~/`.
9. **Rule 4 claims by name, in the pattern's own directory, for a regular file, while the tool is installed.** `Known::index` indexes each instance's patterns (`globs` keyed by adapter id, filled by `Session::scan_unknown` from `recipes::backup_globs()`), the directory canonical; `claimant` gets the entry's canonical directory and its `EntryKind` and applies rule 4 after rule 3. Without an instance the pattern is not indexed and a leftover `agy.<time>.old` is listed — the spec's own words (§8.3).
10. **`Latest::Command` trusts the tool's `updateAvailable` and never compares** (spec §4.3): `check_updates` builds a `Native` candidate whose `target` is `latestVersion` as printed when `updateAvailable` is `true`, and nothing when it is `false` — **unless the tool also reports an error.** grok's answer carries an `error` field (`"error":null` on success, grok.md §3, VERIFIED); a check that exits 0 with `updateAvailable:false` and a non-null `error` (the plausible offline shape: it cannot know a newer version exists) would otherwise leave the row saying "up to date" while the check failed, which the honesty rule forbids ("没有证据绝不说成功"). So `Latest::Command` carries `error_field: Option<&'static str>` (grok: `Some("error")`), and `latest::parse_update_check` answers `Err("the update check reported: <text>")` when that field is present and not `null`; the row is then "could not check" with grok's own reason. A recipe may name a `Command` only when its argv carries `--check` and `--json` (`recipes::tests::test_every_command_latest_source_only_checks`), because a `Latest::Command` runs on every refresh.
11. **agy's manifest is fetched on Apple silicon only.** `latest::manifest_arch_allowed(arch)` answers `Err` with the row's reason for anything but `aarch64` (`std::env::consts::ARCH`'s spelling; a universal build under Rosetta reports `x86_64` and falls on the safe side, spec §3.1). The adapter carries `arch: &'static str` (set from `std::env::consts::ARCH` in `new`) and a test seam `with_arch`, so both branches are tested on any runner.
12. **`Recipe.upgrade` becomes `Option<UpgradeCmd>`** (B's ruling 1 anticipated this: "D widens `upgrade` to `Option` with agy"). `None` puts `UpdateBlocked::SelfUpdatesOnly` on every candidate the recipe produces and makes `plan(Upgrade)` refuse with the same reason (the late twin for a stale snapshot). `CLAUDE`, `GROK`, `RUSTUP` and every test recipe say `Some(…)`.
13. **`SelfUpdatesOnly`'s sentence names the versions**, as spec §9.2 writes it (`({{current}} → {{target}})`); `rowDescription`'s blocked branch interpolates `command` and `source` only, so Task 1 adds `current` and `target` to that one `t()` call (harmless for `Pinned`, whose sentences do not use them).
14. **`SelfUpdatesOnly`'s command is the launcher, bare** (`instance.exe_path`, through `displayToken` — quoted when the path has a space), never `<launcher> --version`: on 1.2.10 `--version` does not reach the updater (spec §3.4, §十三 #31); opening the tool does. The sentence says to open it once and quit, and that it checks at most every 15 minutes (agy.md §4, VERIFIED).
15. **`ShellConfigLines` says "any lines its installer added"**, since a `KeepSpec` is listed whenever the file exists (C ruling 6) and Canager does not read the file to find the marker: a `~/.zshrc` with no such line still gets a true sentence. Not reading the file keeps grok's `~/.zshrc` and agy's `~/.zshrc`/`~/.zprofile` out of "Files Canager reads".
16. **The version-read observation is per recording (spec §3.4), for both tools, and the README carries it.** Task 6's agy recording records the count of `~/.gemini/antigravity-cli/log` files and the mtime of `updater/update_status.json` before and after the one `--version` read that is recorded, and that no new `agy` process appeared (a before/after diff of `ps -axo pid,ppid,comm`, not a name grep — the Antigravity desktop app is also called `antigravity`). If any of the three changes, the executor stops and reports: the recipe's version read must then change (spec §3.4 names the shapes: read the updater's own record, or `version: None` with a sentence), which is the author's call, not this plan's. grok gets the same rule, not a weaker one: whether `grok --version` runs grok's launch-time updater is UNVERIFIED, and whether that updater *installs* or only checks is UNVERIFIED too (grok.md §5, open question 2), while Canager's refresh runs `grok --version` three to four times. So the recording snapshots `~/.grok/bin` and `~/.grok/downloads` (`ls -lan`), `readlink ~/.grok/bin/grok` and `~/.grok/version.json`'s mtime *before and after each* grok invocation. A moved `version.json` mtime after `--version` is recorded as evidence that the launch-time path is reachable from a version read (an open question in `## Grok Build`, not a pre-decided "changes no recipe"); a changed link target or a new entry in `downloads/` is a stop: report, do not commit, the author decides the recipe's version read. After `update --check --json` a moved `version.json` mtime is expected (its `checked_at`) and is recorded as the one write grok makes on every Canager refresh (`## Grok Build`, `## Files Canager writes`, the never-list).
17. **Fixture files carry `~` and numeric owners, and the README says so** (follow-ups-after-C item 4: B's `layout.txt` leaked the home path and hostname). `ls -lan <tool's own paths> | sed "s|$HOME|~|g"`; the version lines, the manifest and the update check are the commands' bytes unchanged. The README names the hostname as `the author's MacBook`, not the machine's name.
18. **The recorded version is whatever the tools print on the recording day**, and every fixture-backed assertion derives its expectation from the meta file. agy self-updates: this Mac's `~/.local/bin/agy` moved from 1.2.9 (2026-09-24) to 1.2.10 (spec) to a file dated 2026-09-25 12:25 (186,406,752 bytes) while this plan was written. The fixture directory, `verified_versions` and the trust file's "Verified against" sentence must agree, and the recording instructions say what to rename.
19. **Eleven adapters after this step**: `standalone-agy` and `standalone-grok` sort into the fan-out between `pipx` and `standalone-claude` / after it; the test is renamed `test_new_registers_all_eleven_adapters`.
20. **The empty-state sentences name all four tools**, in E's wording (E ruling 14), not the spec's: "Claude Code, Antigravity CLI, Grok Build and rustup at their own installers' default locations".
21. **Both new sources own their roots for the Unknown page** (`owned_roots`: `standalone-agy` → `~/.gemini/antigravity-cli`, `standalone-grok` → `~/.grok`), as F's comment promised.
22. **The `grok update` / `claude update` non-interactive recording is the author's, on CI, after 2026-10-01, and is written up as its own section** ("The author's pre-merge verification"). Until it has run, `docs/what-we-run.md`'s Grok section says so in one sentence; nothing in this plan runs either command anywhere.
23. **grok's upgrade env is empty** (spec §3.4: version-read variables never go on the upgrade plan; grok has none anyway), its `--version` has no env (none documented), and its check command has none.
24. **`KeptWhat::ToolState`'s path is agy's root `~/.gemini/antigravity-cli`**, with the spec's sentence (its conversations, history and working files; some of the program's own files too). `~/.gemini` itself is never listed, moved or mentioned as a path: it is shared with Gemini CLI (agy.md §2, §5).

## The author's pre-merge verification (spec §五, §十 row D — for the author on CI, never for the executor on this Mac)

Spec §五 makes merging this step conditional on recording, on a CI runner with stdin closed, how `grok update` and `claude update` behave when nothing can answer a prompt: neither has ever been run by this project ("两个升级命令都没有在调研中被执行过"), `RealRunner` gives a child `stdin(Stdio::null())` (`runner/real.rs`, the `Stdio::null()` line), so a confirmation prompt gets EOF at once — and how each tool takes EOF (exits non-zero, treats it as "no", ignores it, or hangs until Canager's 1800 s timeout) is what decides whether the *Update* button works. **CI minutes for this repository are exhausted until 2026-10-01**, so this runs after that date, and it never runs on the author's Mac: both commands change the machine's installs.

**What blocks the merge, stated once:** both commands are recorded in the one workflow run below. **grok's result gates this merge** — `GROK.upgrade` is new here, and `docs/what-we-run.md`'s `## Grok Build` says "the author records it on a CI runner before this step merges", which must be literally true. **claude's result does not gate this merge**: B's row already ships the *Update* button, so a prompting or hanging `claude update` is a regression filed for the release (Step 3), recorded here because the run is free once it exists. This is a deviation from spec §五's "步骤 D 之前…各录一次" (both before D), recorded as deviation 13.

**Step 1 — the probe workflow.** The author adds `.github/workflows/standalone-upgrade-probe.yml`, dispatch-only (adding the file costs no minutes; a run does), and dispatches it once from the Actions tab:

```yaml
name: Probe standalone upgrades (manual)

on:
  workflow_dispatch:
    inputs:
      grok_older:
        description: "An older Grok Build version to install first (x.ai/cli/install.sh | bash -s <v>); empty = current"
        default: "1.0.34"
      claude_older:
        description: "An older Claude Code version to install first; empty = current"
        default: ""

jobs:
  probe:
    runs-on: macos-latest
    timeout-minutes: 45
    steps:
      - name: throwaway home
        run: |
          echo "PROBE_HOME=$(mktemp -d)" >> "$GITHUB_ENV"
      - name: install grok (the older version when given)
        env: { HOME: ${{ env.PROBE_HOME }} }
        run: |
          if [ -n "${{ inputs.grok_older }}" ]; then
            curl -fsSL https://x.ai/cli/install.sh | bash -s "${{ inputs.grok_older }}"
          else
            curl -fsSL https://x.ai/cli/install.sh | bash
          fi
          "$HOME/.grok/bin/grok" --version
          "$HOME/.grok/bin/grok" update --check --json | tee grok-check-before.json
      - name: grok update with stdin closed
        env: { HOME: ${{ env.PROBE_HOME }} }
        run: |
          set +e
          # macOS has no `timeout`; the runner image's coreutils are
          # g-prefixed. A missing command would be exit 127, misread as a
          # prompt.
          TIMEOUT=$(command -v gtimeout || command -v timeout) || { echo "no timeout command" | tee grok-update.exit; exit 0; }
          start=$(date +%s)
          "$TIMEOUT" 1800 "$HOME/.grok/bin/grok" update </dev/null >grok-update.stdout 2>grok-update.stderr
          echo "exit=$? seconds=$(( $(date +%s) - start ))" | tee grok-update.exit
          "$HOME/.grok/bin/grok" --version | tee grok-version-after.txt
          ls -la "$HOME/.grok/bin" "$HOME/.grok/downloads" | sed "s|$HOME|~|g" | tee grok-layout-after.txt
          cat "$HOME/.grok/config.toml" | tee grok-config-after.toml
      - name: install claude (the older version when given)
        env: { HOME: ${{ env.PROBE_HOME }} }
        run: |
          curl -fsSL https://claude.ai/install.sh | head -80 | tee claude-install-head.txt
          if [ -n "${{ inputs.claude_older }}" ]; then
            curl -fsSL https://claude.ai/install.sh | bash -s "${{ inputs.claude_older }}"
          else
            curl -fsSL https://claude.ai/install.sh | bash
          fi
          DISABLE_AUTOUPDATER=1 "$HOME/.local/bin/claude" --version
      - name: claude update with stdin closed
        env: { HOME: ${{ env.PROBE_HOME }} }
        run: |
          set +e
          TIMEOUT=$(command -v gtimeout || command -v timeout) || { echo "no timeout command" | tee claude-update.exit; exit 0; }
          start=$(date +%s)
          "$TIMEOUT" 1800 "$HOME/.local/bin/claude" update </dev/null >claude-update.stdout 2>claude-update.stderr
          echo "exit=$? seconds=$(( $(date +%s) - start ))" | tee claude-update.exit
          DISABLE_AUTOUPDATER=1 "$HOME/.local/bin/claude" --version | tee claude-version-after.txt
      - uses: actions/upload-artifact@v4
        with:
          name: standalone-upgrade-probe
          path: |
            grok-*.json
            grok-*.txt
            grok-*.stdout
            grok-*.stderr
            grok-*.exit
            grok-*.toml
            claude-*.txt
            claude-*.stdout
            claude-*.stderr
            claude-*.exit
```

(`bash -s <version>` is documented for grok's installer — grok.md §2, `bash -s 0.1.42` — and 1.0.34 is a version this Mac's `~/.grok/downloads` still holds. Whether Claude Code's `install.sh` takes a version argument is read from its own usage text, captured in `claude-install-head.txt`; if it does not, leave `claude_older` empty and record the up-to-date path only, saying so.)

**Step 2 — what to look for**, per tool, in the artifact:

| Read | Meaning |
|---|---|
| `*-update.exit`: `exit=0`, seconds well under 1800, and `*-version-after.txt` shows the newer version | The command runs unattended and updates. The recipe stays as it is. |
| `exit=0`, version unchanged | The tool judged itself current (only possible when no older version was installed). Says nothing about a prompt; re-run with an older version. |
| `exit≠0`, and `*-update.stdout`/`.stderr` contains a question (`?`, `[y/N]`, `Continue`, `Proceed`, `Press`) | The command prompts and took EOF as "no". Canager's run would be `Failed` with that text — honest, but a button that can never work. |
| `exit=124` (timeout's own code), seconds ≈ 1800 | The command waited for an answer that never came. Canager's run would be `Unconfirmed` after 30 minutes. |
| `exit=127`, or `*-update.exit` reads `no timeout command` | The probe itself did not run (`gtimeout`/`timeout` missing on the image, or the launcher not where the installer was expected to put it). Says nothing about the tool; fix the workflow and re-run. |
| `*-update.stderr` says stdin is not a terminal / TTY | Same as a prompt: the tool refuses unattended. |
| `grok-layout-after.txt`: a `*.old` beside `grok`, or a new `downloads/grok-<v>-…` and a re-pointed link | What a grok upgrade leaves behind; `grok-config-after.toml` unchanged means `grok update` does not rewrite the config. Record both in the Grok section. |

**Step 3 — how the result changes the recipes** (an author decision each; none is the executor's):

- **Both run unattended (row 1):** no code change. Add one sentence with the date, the runner image and the exit codes to each tool's paragraph in `docs/what-we-run.md` (`## Grok Build`'s "Write commands" paragraph replaces its "not yet observed" sentence — Task 6 writes that sentence so it can be replaced by words; `## Claude Code`'s "Write commands" paragraph gains one), and save the artifact's `grok-update.exit`, `grok-update.stdout` and `grok-version-after.txt` as `adapters/fixtures/standalone-grok/<version>/ci-update-noninteractive.{exit,stdout,version}` with a README paragraph naming the runner and the date — a CI runner is a real machine (spec §9.3's second-batch rule).
- **grok prompts or hangs (rows 3–5):** `grok update --help` at 1.0.41 lists no `--yes`/`-y`/`--non-interactive` (grok.md §4: `--check`, `--json`, `--force-reinstall`, `--version`, `--alpha`, `--stable`). Then either (a) `GROK.upgrade = None` — but `SelfUpdatesOnly`'s sentence ("installs updates itself") would be false for grok (its self-update is UNVERIFIED, spec §4.4), so the honest shape is a **new** `UpdateBlocked::NeedsTerminal` with its own copy record ("run `grok update` in Terminal yourself") — a small follow-up plan; or (b) keep the button and let the `Failed` outcome quote grok's prompt text. (a) is the recommendation: a button that always fails is the thing the gate exists to hide.
- **claude prompts or hangs:** the same choice for `CLAUDE.upgrade`; `claude update` has no documented non-interactive flag either (claude.md §6). B's row already offers the button, so this is a regression to fix before the release, not before this merge (the header above says so once; the backlog entry Task 8 writes names it).

**Step 4 — the record.** The delivery note of this step (Task 8) lists grok's recording as the one open item before merge and claude's as recorded alongside; the author's PR description closes it with the artifact's numbers.

## What already exists (do not rebuild)

- **From B** (in the tree): `StandaloneAdapter` with `new`, `detect`, `read_version`, `artifact_key`, `inventory`, `search`, `check_updates`, `plan`, `execute`, `reconcile`, `all`; `Detected`; `Recipe`, `Route`, `RouteKind`, `VersionCmd`, `VersionParse`, `Latest`, `UpgradeCmd`; `recipes::{CLAUDE, RECIPES}` and its invariants tests; `route::{expand, probe, lexical_join, shadow_note, Probe}`; `latest::{is_dotted_version, parse_version, compare_dotted, claude_channel, claude_channel_from_json, parse_channel_body, CHANNEL_LATEST, CHANNEL_STABLE}`; `testing::{TempHome { new, path, dir, file, executable, link, env }, ClaudeLayout, claude_layout}`; the test helpers `exited_0`, `adapter`, `RecordingRunner`, `instance_for`, `request`, `answer`, `detected_adapter`, `fixture`; `InstanceNote::{NotOnPath, ShadowedByHomebrew, ShadowedByNpm, ShadowedByOther, LauncherOnly}`; `UninstallBlocked::NoSafeMethod`; `ADAPTER_LABEL_KEYS`, `StandaloneAdapterId`, `STANDALONE_SUMMARY_KEYS`, `standaloneSummaryKey`, `UNINSTALL_BLOCKED_KEYS`, `UPDATE_BLOCKED_KEYS`, `unpinCommand`; `updates.selfUpdatingHint*`; the `## Claude Code` section of `docs/what-we-run.md`; the README row.
- **From C** (see the checklist): `PlanAction`, `Plan.action`, `Recipe.uninstall`, `Uninstall::Paths`, `RemoveSpec`, `KeepSpec`, `Expect`, `SHARED_FOLDERS`, `Detected.euid`, `Warning::{WillTrash, WillKeep, AlreadyGone}`, `RemovedWhat`, `KeptWhat`, `Fault::PathChanged`, `LogNote::{MovedToTrash, TrashFailed}`, `AdapterError::UninstallUnsafe`, `UninstallUnsafeReason`, `trash::{Trasher, RealTrasher, MockTrasher, TrashError}`, `ItemKind`, `ItemIdentity`, `removal.rs` whole, `route::probe_strict`, `Adapter::reconcile_after_uninstall`, `StandaloneAdapter::{with_trash_gap, reconcile_after_uninstall, detected_or_refuse}`, `all(runner, http, trasher)`, `scan::display_path` as `pub(crate)`, `CommandPreview`'s `TrashPaths` branch, `warningKey`'s `WillTrash`/`WillKeep`/`AlreadyGone` branches, `REMOVED_WHAT_KEYS`/`KEPT_WHAT_KEYS`, `UNINSTALL_UNSAFE_KEYS`, `tests/standalone_uninstall_test.rs`, the "Moving files to the Trash" section.
- **From E** (see the checklist): `HostEnv.{rustup_home, zdotdir}`, `path_env::tool_home`, `cargo::{cargo_home_of, instance_id_for, parse_crates2_bins, RUSTUP_AUTO_INSTALL_OFF}`, `RouteKind::FlatFile`, `VersionParse::SecondToken`, `Latest::HttpTomlVersion`, `latest::parse_release_stable_toml`, `route::expand_route`, `Recipe.extra_locks`, `no_extra_locks`, `Detected.{cargo_home, rustup_home, zdotdir}`, `seated_detected_for`, `locks`, `Uninstall::Command(CommandUninstall)`, `rustup.rs`, `RUSTUP`, `RECIPES = &[&CLAUDE, &RUSTUP]`, `OperationManager::locks_held`, the refresh skip, `NoCancel`'s producer and `operations.noCancelHint`, `uninstallBlockedCopy`, `adapters.standalone-rustup`, `standalone.summary.standalone-rustup`, the `## rustup` section, `static.rust-lang.org` on the allowlist, `test_new_registers_all_nine_adapters`.
- **From A and F**: `ALLOWED_HTTPS_HOSTS`, `host_allowed`, `tests/what_we_run_test.rs` (`read_doc`, `is_heading_for`, `has_section`, `section_body`, the six tests), `scan/mod.rs` (`ScanBudget`, `ScanStop`, `ScannedDir`, `EntryKind`, `UnknownEntry`, `UnknownScan`, `candidate_dirs`, `display_path`, `app_bundle`, `owned_roots`, `Known`, `examine`, `scan_dirs`, `scan_unknown`), `Session::scan_unknown`, `tests/unknown_scan_test.rs` (`Home`, `exe`, `plain`, `link`, `artifact`, `tilde`), the Unknown page.
- **Engine and harnesses**: `MockRunner::{respond, delay, calls}`, `MockHttpClient::{respond, fail, calls, requests}`, `crate::testing::manager_instance`, `Session::{with_adapters, refresh, issue_plan, submit, operations, snapshot}`, `IssuedPlan { id, plan, issued_at }`, `OpSummary { id, status, outcome, … }`, `OpStatus::Done`; `src/pages/UpdatesPage.test.tsx`'s knobs (`settings`, `updates`, `instances`, `artifacts`, `wholeSentence`, `renderWithProviders`) and its `claudeKey`/`claudeInstance`/`claudeArtifact`/`claudeUpdate` fixtures; `src/lib/sources.test.ts`'s `fakeT`, `instance()`, `en`, `zhCN`; `src/lib/types.test.ts`'s `roundTrip`; `src/components/SnapshotStatus.test.tsx`'s two empty-state sentences.

## File Structure

```
adapters/meta/standalone-agy.toml                                  NEW   AdapterMeta, seven fields (5b)
adapters/meta/standalone-grok.toml                                 NEW   same (5b)
adapters/fixtures/standalone-agy/<version>/                        NEW   README.md, version.txt, manifest-darwin_arm64.json, update_status.json, layout.txt — recorded (6)
adapters/fixtures/standalone-grok/<version>/                       NEW   README.md, version.txt, update-check.json, layout.txt — recorded (6)
crates/canager-core/src/model.rs                                   MOD   UpdateBlocked::SelfUpdatesOnly + wire test (1); RemovedWhat::Backups, KeptWhat ×5 + shape test (2)                    [B's/C's file]
crates/canager-core/src/scan/mod.rs                                MOD   Glob (+ dir_under, matches_name), Known.backups, rule 4, globs parameter on scan_dirs/scan_unknown, claimant(raw, dir, resolved, kind) (3); owned_roots rows + test rows (6)   [F's file]
crates/canager-core/src/session/scan.rs                            MOD   passes recipes::backup_globs() (3)                                                                                  [F's file]
crates/canager-core/src/adapters/standalone/recipe.rs              MOD   Recipe.backup_globs (3); Expect::File, KeepSpec doc (4); Latest::{HttpJsonField, Command}, upgrade: Option (5a)     [B's/C's/E's file]
crates/canager-core/src/adapters/standalone/latest.rs              MOD   parse_json_field, UpdateCheck, parse_update_check, MANIFEST_VERIFIED_ARCHES, manifest_arch_allowed (5a)          [B's/E's file]
crates/canager-core/src/adapters/standalone/removal.rs             MOD   Job.globs, Item, listed_items, check_item(rel, expect), keeps_instead, outside_home_keeps(look), points_into, plan_removal, take_turn, kept_places; tests (4)   [C's file]
crates/canager-core/src/adapters/standalone/mod.rs                 MOD   Job literals gain globs (4); arch, with_arch, Published, published, check_updates, plan(Upgrade) (5a); testing::{agy_layout, grok_layout}; tests (5c); fixture tests (6)   [B's/C's/E's file]
crates/canager-core/src/adapters/standalone/recipes.rs             MOD   backup_globs() + tests (3); three invariants tests rewritten (4); AGY, GROK, tests, allowlist-test arms, --check test (5); RECIPES, count test (6)   [B's/C's/E's file]
crates/canager-core/src/http/real.rs                               MOD   ALLOWED_HTTPS_HOSTS += the agy manifest host; doc (5b)                                                              [A's/B's/E's file]
crates/canager-core/src/session/mod.rs                             MOD   the eleven-adapter test (6)                                                                                         [B's/E's file]
crates/canager-core/src/lib.rs                                     MOD   crate-doc clause (6)                                                                                                [B's/E's file]
crates/canager-core/tests/unknown_scan_test.rs                     MOD   every scan_dirs call gains `&[]`; the rule-4 test (3)                                                               [F's file]
crates/canager-core/tests/what_we_run_test.rs                      MOD   the read-only-check-command test (6)                                                                                [A's file]
crates/canager-core/tests/standalone_agy_grok_test.rs              NEW   through Session: agy's gate refusal, grok's uninstall end to end (6)
src/lib/types.ts, types.test.ts                                    MOD   UpdateBlocked (1); RemovedWhat, KeptWhat (2)                                                                        [A's/C's file]
src/lib/sources.ts, sources.test.ts                                MOD   UPDATE_BLOCKED_KEYS.SelfUpdatesOnly, launcherCommand (1); labels, StandaloneAdapterId, summaries (7)                [B's/E's file]
src/lib/warnings.ts, warnings.test.ts                              MOD   REMOVED_WHAT_KEYS.Backups, five KEPT_WHAT_KEYS rows (2)                                                             [A's/C's/E's file]
src/pages/UpdatesPage.tsx, UpdatesPage.test.tsx                    MOD   current/target on the blocked branch's t(); the SelfUpdatesOnly row test (1)                                       [B's/C's/E's file]
src/components/SnapshotStatus.test.tsx                             MOD   the two empty-state sentences (7)                                                                                   [B's/E's file]
src/i18n/en.json, zh-CN.json                                       MOD   updates.blocked.SelfUpdatesOnly.* (1); warnings.willTrash.Backups, warnings.willKeep.* ×5 (2); adapters.*, standalone.summary.*, emptyStates.* (7)   [everyone's file]
docs/what-we-run.md                                                MOD   the scan section's rule-4 sentence (3); two sentences of the Claude Code check paragraph (4); the network row (5b); intro, program-source paragraph, `## Antigravity CLI`, `## Grok Build`, files read, files written, never-list, the Trash section's one clause, the network paragraph (6)   [A's/B's/C's/E's file]
docs/superpowers/backlog.md                                        MOD   the agy-cache entry closed; two new entries (8)                                                                     [C's file]
README.md                                                          MOD   two source rows; the two test counts (8)                                                                            [B's/C's/E's file]
```

Single responsibility, unchanged from B/C/E: `recipe.rs` the shapes, `recipes.rs` the data and its invariants, `latest.rs` versions and endpoint bodies, `route.rs` recognition, `removal.rs` the checks and the order, `mod.rs` the `Adapter` contract, `scan/mod.rs` the Unknown page's reading of the disk (and now the one pattern type both sides read).

## Core Interfaces (authoritative — every task uses these names verbatim)

```rust
// crates/canager-core/src/model.rs
pub enum UpdateBlocked { Pinned, SelfUpdatesOnly }
pub enum RemovedWhat { Launcher, Program, Cache, Backups }
pub enum KeptWhat { Settings, SettingsAndHistory, ToolState, ShellConfigLines, OutsideHome, NotOurs, InstallerCache }

// crates/canager-core/src/scan/mod.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glob { pub dir: &'static str, pub prefix: &'static str, pub suffix: &'static str, pub what: RemovedWhat }
impl Glob { pub fn dir_under(&self, home: &Path) -> PathBuf; pub fn matches_name(&self, name: &str) -> bool; }
pub fn scan_dirs(dirs: &[PathBuf], env: &HostEnv, instances: &[ManagerInstance], artifacts: &[InstalledArtifact], globs: &[(String, &'static [Glob])], budget: ScanBudget) -> UnknownScan;
pub fn scan_unknown(env: &HostEnv, instances: &[ManagerInstance], artifacts: &[InstalledArtifact], globs: &[(String, &'static [Glob])], budget: ScanBudget) -> UnknownScan;
// private: Known { exe_raw, exe_canonical, artifact_roots, owned, backups: Vec<(PathBuf, Glob, InstanceId)> }
//          Known::index(instances, artifacts, globs, home) ; Known::claimant(&self, raw: &Path, dir: &Path, resolved: Option<&Path>, kind: EntryKind) -> Option<&InstanceId>
pub fn owned_roots(inst: &ManagerInstance) -> Vec<PathBuf>;   // + "standalone-agy" | "standalone-grok" => vec![inst.prefix.clone()]

// crates/canager-core/src/adapters/standalone/recipe.rs
pub struct Recipe { …B/C/E's nine fields with `upgrade: Option<UpgradeCmd>`…, pub backup_globs: &'static [Glob] }
pub enum Latest { ClaudeChannel { base }, HttpTomlVersion { url },
                  HttpJsonField { url: &'static str, field: &'static str },
                  Command { args: &'static [&'static str], timeout_secs: u64, latest_field: &'static str, available_field: &'static str, error_field: Option<&'static str> } }
pub enum Expect { SymlinkIntoRoot, Dir, File }
// KeepSpec.path: `~/…`, or an absolute path when `what == KeptWhat::OutsideHome` (report-only, and only when it links into the root)

// crates/canager-core/src/adapters/standalone/latest.rs
pub fn parse_json_field(body: &str, field: &str) -> Result<String, String>;
#[derive(Clone, Debug, PartialEq, Eq)] pub struct UpdateCheck { pub latest: String, pub available: bool }
pub fn parse_update_check(stdout: &str, latest_field: &str, available_field: &str, error_field: Option<&str>) -> Result<UpdateCheck, String>;
pub const MANIFEST_VERIFIED_ARCHES: [&str; 1] = ["aarch64"];
pub fn manifest_arch_allowed(arch: &str) -> Result<(), String>;

// crates/canager-core/src/adapters/standalone/removal.rs
pub struct Job { pub recipe: &'static Recipe, pub detected: Detected, pub remove: &'static [RemoveSpec], pub keep: &'static [KeepSpec], pub globs: &'static [Glob] }
// private: struct Item { rel: PathBuf, path: PathBuf, expect: Expect, what: RemovedWhat, optional: bool }
//          fn listed_items(job: &Job) -> Vec<Item>
//          fn check_item(look: &Look<'_>, kept: &[Kept], rel: &Path, expect: Expect, path: &Path) -> Result<ItemIdentity, Refusal>
//          fn keeps_instead(reason: UninstallUnsafeReason) -> bool
//          fn outside_home_keeps(look: &Look<'_>) -> Vec<Warning>
//          fn points_into(link: &Path, root: &Path) -> bool

// crates/canager-core/src/adapters/standalone/recipes.rs
pub static AGY: Recipe;  pub static GROK: Recipe;
pub static RECIPES: &[&Recipe] = &[&CLAUDE, &AGY, &GROK, &RUSTUP];
pub fn backup_globs() -> Vec<(String, &'static [Glob])>;

// crates/canager-core/src/adapters/standalone/mod.rs
pub struct StandaloneAdapter { …C's seven fields…, arch: &'static str }
impl StandaloneAdapter { pub fn with_arch(self, arch: &'static str) -> StandaloneAdapter; /* private */ async fn published(&self, launcher: &Path) -> Result<Published, String>; }
// private: enum Published { Version(String), ToolSays(latest::UpdateCheck) }
pub(super) mod testing { pub struct AgyLayout { pub launcher: PathBuf, pub root: PathBuf } pub fn agy_layout(home: &TempHome) -> AgyLayout;
                         pub struct GrokLayout { pub launcher: PathBuf, pub agent: PathBuf, pub root: PathBuf, pub real: PathBuf } pub fn grok_layout(home: &TempHome, version: &str) -> GrokLayout; }

// crates/canager-core/src/http/real.rs
pub const ALLOWED_HTTPS_HOSTS: &[&str] = &["crates.io", "pypi.org", "registry.ollama.ai", "downloads.claude.ai", "static.rust-lang.org",
                                          "antigravity-cli-auto-updater-974169037036.us-central1.run.app"];
```

```ts
// src/lib/types.ts
export type UpdateBlocked = "Pinned" | "SelfUpdatesOnly";
export type RemovedWhat = "Launcher" | "Program" | "Cache" | "Backups";
export type KeptWhat = "Settings" | "SettingsAndHistory" | "ToolState" | "ShellConfigLines" | "OutsideHome" | "NotOurs" | "InstallerCache";
// src/lib/sources.ts
// UPDATE_BLOCKED_KEYS.SelfUpdatesOnly: { badge, description, descriptionSourceUnavailable, selfUpdatingDescription: null, selfUpdatingDescriptionSourceUnavailable: null, command: launcherCommand, refused }
export type StandaloneAdapterId = "standalone-claude" | "standalone-rustup" | "standalone-agy" | "standalone-grok";
// ADAPTER_LABEL_KEYS / STANDALONE_SUMMARY_KEYS gain "standalone-agy" and "standalone-grok"
// i18n keys: updates.blocked.SelfUpdatesOnly.{badge,description,descriptionSourceUnavailable,refused};
//            warnings.willTrash.Backups; warnings.willKeep.{ToolState,ShellConfigLines,OutsideHome,NotOurs,InstallerCache};
//            adapters.standalone-agy, adapters.standalone-grok; standalone.summary.standalone-agy, standalone.summary.standalone-grok;
//            emptyStates.noSources.description, emptyStates.nothingInstalled.description (changed)
```

## Task List

| # | Task | Deliverable |
|---|---|---|
| 1 | `UpdateBlocked::SelfUpdatesOnly`, its wire mirror, copy record and page test | the badge, the sentence and the missing button a self-updating tool's row needs, both locales |
| 2 | `RemovedWhat::Backups`, five `KeptWhat`s, their mirror, keys and copy | every sentence the two new uninstall lists will show, both locales |
| 3 | `Glob`, `Recipe.backup_globs`, the Unknown page's rule 4 | a backup the updater left is the tool's, not a stranger |
| 4 | `removal.rs`: check 5, `Expect::File`, the `NotOurs` skip, report-only outside-home keeps (only a link into the root), the trust-file sentences those change | the checks the two lists need, on Claude Code's list and synthetic ones |
| 5 | The two recipes and the adapter: `HttpJsonField`, `Command`, `upgrade: Option`, `arch`, `AGY`, `GROK`, their meta, the host (one commit, stages 5a–5d) | agy and grok as recipes: detect, badge, the blocked upgrade, grok's own check, both uninstalls, tested on synthetic layouts |
| 6 | Recording, registration, `owned_roots`, the trust file, the end-to-end test | eleven sources; fixtures; `## Antigravity CLI` and `## Grok Build` |
| 7 | Front end: labels, summaries, empty states | the pages name both tools |
| 8 | README, backlog, delivery note | the documents that only lagged catch up; the open CI item named |

Order: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8. Tasks 1 and 2 are independent of each other; 3 precedes 4 (check 5 reads `Glob`); 5 needs 1–4; 6 needs 5; 7 needs 6's ids; 8 is last because it counts tests.

## Review Focus

Eight inputs the spec implies, or the research found, that a person is most likely to hit, most likely first. Each has its test in the task named.

1. **Antigravity updates itself between the uninstall preview and the click** — its updater replaces `~/.local/bin/agy` with a new file (this Mac's moved 1.2.9 → 1.2.10 → a 2026-09-25 build inside two days) → `CanagerFailed(PathChanged)` naming `~/.local/bin/agy`, nothing moved, the user previews again (Task 5, `test_execute_for_agy_refuses_a_launcher_its_updater_replaced_after_the_preview`).
2. **A `~/.local/bin/agent` that is another CLI's** (the research Mac has several agent CLIs; grok's installer writes that name only as a fallback) → kept with `NotOurs`, grok's uninstall goes ahead (Task 4 `test_plan_removal_keeps_an_optional_path_it_cannot_confirm_is_the_tools_and_says_so`; Task 5 `test_plan_uninstall_for_grok_keeps_a_foreign_agent_link_and_moves_its_own_fallback_links_first`).
3. **An Intel Mac, or a universal build under Rosetta** → agy's row says the check is not yet verified there, and no request leaves the machine (Task 5, `test_check_updates_for_agy_is_uncheckable_on_an_intel_mac_without_a_request`).
4. **grok's own check fails, prints something that is not its JSON, or is not grok's format at all** (exit 1, an HTML captive-portal page in stdout, a field renamed) → one uncheckable row with a short reason, never an `Err` that would hold the source stale (Task 5, `test_check_updates_for_grok_lists_nothing_when_it_says_no_update_and_is_uncheckable_when_it_fails`).
5. **A backup `agy.<time>.old` appears between the preview and the click** (the updater at work) → the fresh list differs, `PathChanged` names it, nothing moved (Task 4, `test_execute_removal_moves_a_backup_the_preview_listed_and_stops_when_one_appears_after_it`).
6. **A grok uninstall stops partway** (macOS refuses a folder, or Cancel) → the row comes back as launcher-only (`~/.grok/bin/grok` dangling into `~/.grok`), and a second uninstall lists the moved folders as already gone and finishes with `~/.grok/bin` (Task 5, `test_a_stopped_grok_uninstall_leaves_a_launcher_only_row_that_a_second_uninstall_finishes`).
7. **The optional paths are simply not there** (`~/.cache/antigravity` never made, no `~/.zprofile`, no fish, no fallback links) → no sentence for any of them, the list still adds up (Task 5, both `plan_uninstall` tests run once with everything present and once with the minimum). A `/usr/local/bin/grok` on the machine running the tests, if any, is not a link into the test's temp home, so it produces no sentence either (ruling 6): no test depends on the host.
8. **`/usr/local/bin/grok` is Homebrew's** (an Intel Mac with the `grok-build` cask: a live link into `/usr/local/Caskroom`), or `/usr/local/bin/agent` is another CLI's → no "dead link you can delete" sentence, since neither links into `~/.grok` (Task 4, `test_plan_removal_lists_a_kept_path_outside_the_home_folder_as_a_sentence_only`, its elsewhere and regular-file cases).

(Ownership and the kept-path overlap rule keep C's tests; Task 4 adds one showing they still refuse for an *optional* path — the skip is only for "not ours", never for "not yours" or "would take what stays".)

---

### Task 1: `UpdateBlocked::SelfUpdatesOnly`, its wire mirror, copy record and page test

**Files:**
- Modify: `crates/canager-core/src/model.rs` — `pub enum UpdateBlocked`; `test_update_blocked_is_a_bare_string_on_the_wire_and_null_when_absent`
- Modify: `src/lib/types.ts` — `UpdateBlocked` and its doc; `src/lib/types.test.ts` — `it("spells UpdateBlocked as a bare string, …")`
- Modify: `src/lib/sources.ts` — `launcherCommand`, `UPDATE_BLOCKED_KEYS`; `src/lib/sources.test.ts` — `describe("UPDATE_BLOCKED_KEYS", …)`
- Modify: `src/pages/UpdatesPage.tsx` — `rowDescription`'s blocked branch; `src/pages/UpdatesPage.test.tsx` — one test after `gives no self-updating hint to a standalone row whose source did not answer`
- Modify: `src/i18n/en.json`, `src/i18n/zh-CN.json` — `updates.blocked.SelfUpdatesOnly.*`
- Test: the files above.

**Interfaces:**
- Consumes: `UpdateBlocked::Pinned` and its wire test (model.rs); `UpdateBlockedCopy`, `UPDATE_BLOCKED_KEYS`, `displayToken` (sources.ts); `rowDescription`, `updateStateOf`, `isActionable` (UpdatesPage.tsx, updateState.ts); the `claudeInstance`/`claudeArtifact`/`claudeUpdate` fixtures and `wholeSentence` (UpdatesPage.test.tsx).
- Produces (verbatim): `UpdateBlocked::SelfUpdatesOnly` (Rust; serialised `"SelfUpdatesOnly"`; producer `StandaloneAdapter::check_updates`, Task 5; readers landing here: the gate `blocked_upgrade` in `session/plans.rs` — generic over the enum, no edit — `updateStateOf`, `parseUpdateBlocked`, `UPDATE_BLOCKED_KEYS.SelfUpdatesOnly`); `export type UpdateBlocked = "Pinned" | "SelfUpdatesOnly"`; `function launcherCommand(key: ArtifactKey, instance: ManagerInstance | undefined): string` (reader: the copy record's `command`); the four locale keys.

- [ ] **Step 1: Write the failing tests**

In `crates/canager-core/src/model.rs`, inside `test_update_blocked_is_a_bare_string_on_the_wire_and_null_when_absent`, after its last assertion (`assert_eq!(serde_json::from_str::<UpdateCandidate>(&json).expect("deserialize"), pinned);`) and before the test's closing `}`, add:

```rust
        // Phase 4 step D: the second reason, a tool that installs its updates
        // itself and offers no command Canager may run
        // (`StandaloneAdapter::check_updates` for a recipe with no `upgrade`).
        // `UPDATE_BLOCKED_KEYS.SelfUpdatesOnly` in src/lib/sources.ts indexes
        // this spelling.
        assert_eq!(
            serde_json::to_string(&UpdateBlocked::SelfUpdatesOnly).unwrap(),
            r#""SelfUpdatesOnly""#
        );
```

In `src/lib/types.test.ts`, in `it("spells UpdateBlocked as a bare string, and an updatable candidate as null", …)`, replace

```ts
    const reasons: UpdateBlocked[] = ["Pinned"];
    expect(JSON.stringify(reasons)).toBe('["Pinned"]');
```

with

```ts
    const reasons: UpdateBlocked[] = ["Pinned", "SelfUpdatesOnly"];
    expect(JSON.stringify(reasons)).toBe('["Pinned","SelfUpdatesOnly"]');
```

In `src/lib/sources.test.ts`, inside `describe("UPDATE_BLOCKED_KEYS", () => { … })`, after its last test (`it("does not say in Chinese that Homebrew is the one who pinned it", …)`), add:

```ts
  it("names the tool's own launcher, quoted when its path has a space, as what a self-updating tool is opened with", () => {
    // Spec §4.4 / §十三 #31: the sentence says to open the tool once (not
    // `<launcher> --version`, which on agy 1.2.10 never reaches its
    // updater), so the command is the launcher itself, bare. A missing
    // instance gives the bare name, which `refresh` never produces.
    const agy = instance({
      id: "standalone-agy",
      adapter_id: "standalone-agy",
      exe_path: "/Users/Alice Smith/.local/bin/agy",
      prefix: "/Users/Alice Smith/.gemini/antigravity-cli",
    });
    const key = { instance_id: "standalone-agy", kind: "Binary", name: "agy" } satisfies ArtifactKey;
    expect(UPDATE_BLOCKED_KEYS.SelfUpdatesOnly.command(key, agy)).toBe(
      "'/Users/Alice Smith/.local/bin/agy'",
    );
    expect(UPDATE_BLOCKED_KEYS.SelfUpdatesOnly.command(key, undefined)).toBe("agy");
    // The reason itself is "it updates itself": no separate sentence for a
    // self-updating package.
    expect(UPDATE_BLOCKED_KEYS.SelfUpdatesOnly.selfUpdatingDescription).toBeNull();
    expect(UPDATE_BLOCKED_KEYS.SelfUpdatesOnly.selfUpdatingDescriptionSourceUnavailable).toBeNull();
  });

  it("tells a self-updating tool's user to open it once, that it checks at most every 15 minutes, and names the versions", () => {
    // agy.md §4 (VERIFIED): a 15-minute debounce on its background check --
    // without that number, "I opened it and nothing happened" is certain.
    for (const copy of [
      en.updates.blocked.SelfUpdatesOnly.description,
      en.updates.blocked.SelfUpdatesOnly.descriptionSourceUnavailable,
    ]) {
      expect(copy).toContain("{{source}}");
      expect(copy).toContain("{{command}}");
      expect(copy).toMatch(/Open it once/);
      expect(copy).toMatch(/15 minutes/);
    }
    for (const copy of [
      zhCN.updates.blocked.SelfUpdatesOnly.description,
      zhCN.updates.blocked.SelfUpdatesOnly.descriptionSourceUnavailable,
    ]) {
      expect(copy).toContain("{{source}}");
      expect(copy).toContain("{{command}}");
      expect(copy).toMatch(/打开它一次/);
      expect(copy).toMatch(/15 分钟/);
    }
    // The available-source sentence names the two versions (spec §9.2);
    // the unavailable one cannot promise a current target and does not.
    expect(en.updates.blocked.SelfUpdatesOnly.description).toContain("{{current}} → {{target}}");
    expect(zhCN.updates.blocked.SelfUpdatesOnly.description).toContain("{{current}} → {{target}}");
    expect(en.updates.blocked.SelfUpdatesOnly.descriptionSourceUnavailable).not.toContain("{{target}}");
    // `refused` gets only the source's label.
    expect(en.updates.blocked.SelfUpdatesOnly.refused).not.toContain("{{command}}");
    expect(zhCN.updates.blocked.SelfUpdatesOnly.refused).not.toContain("{{command}}");
    expect(en.updates.blocked.SelfUpdatesOnly.badge).toBe("Updates itself");
    expect(zhCN.updates.blocked.SelfUpdatesOnly.badge).toBe("自己更新");
  });
```

(`instance`, `en`, `zhCN`, `ArtifactKey` and `UPDATE_BLOCKED_KEYS` are already imported in that file for B's tests.)

In `src/pages/UpdatesPage.test.tsx`, after the test `it("gives no self-updating hint to a standalone row whose source did not answer: it has no button to offer", …)` (its closing `});`) and before the comment `// The four PATH notes (spec §七)`, add:

```tsx
  it("offers no Update button for a tool that updates itself, and says to open it once", async () => {
    // Spec §4.4, D5 item 4: the newer version is real (read from the
    // launcher's live version), so the row stays and is counted with what
    // Canager cannot update; the tool has no update command Canager could
    // run, so there is no button and no checkbox, and the sentence says
    // what does work -- opening the tool, which checks at most every 15
    // minutes -- with the launcher set apart as code. The claude fixtures
    // stand in for agy here: the copy record is per reason, not per tool.
    instances = [...snapshot.instances, claudeInstance];
    updates = [{ ...claudeUpdate, blocked: "SelfUpdatesOnly" }, snapshot.updates[1]];
    artifacts = [...snapshot.artifacts, claudeArtifact];
    const { findByText, getAllByRole, getByText, queryByText } = renderWithProviders(<UpdatesPage />);

    await findByText("claude");
    // Only onyx's.
    expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
    expect(getAllByRole("checkbox")).toHaveLength(1);
    expect(getAllByRole("checkbox")[0]).toHaveAccessibleName("Select onyx for update");
    expect(getByText("Updates itself")).toBeInTheDocument();
    expect(
      getByText(
        wholeSentence(
          "A newer version of Claude Code is out (2.1.281 → 2.1.290), and Claude Code installs updates itself in the background — Canager doesn't have a safe way to do it for you. Open it once (run /Users/someone/.local/bin/claude in Terminal, then quit it): it checks for updates when it starts, at most once every 15 minutes, and installs the new version in the background.",
        ),
      ),
    ).toBeInTheDocument();
    expect(getByText("/Users/someone/.local/bin/claude").tagName).toBe("CODE");
    // Not the actionable row's hint: this row has no button to point at.
    expect(queryByText(/usually updates itself/)).toBeNull();
    await findByText("1 update available");
    await findByText("1 more can't be updated here");
  });
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p canager-core --lib model::tests::test_update_blocked_is_a_bare_string_on_the_wire_and_null_when_absent`
Expected: FAIL to compile — `no variant or associated item named \`SelfUpdatesOnly\` found for enum \`UpdateBlocked\``.

Run: `pnpm typecheck`
Expected: FAIL — `Type '"SelfUpdatesOnly"' is not assignable to type 'UpdateBlocked'` (types.test.ts, UpdatesPage.test.tsx); `Property 'SelfUpdatesOnly' does not exist on type 'Record<"Pinned", UpdateBlockedCopy>'` (sources.test.ts); `Property 'SelfUpdatesOnly' does not exist` on `en.updates.blocked`.

- [ ] **Step 3: Write the variant, the mirror, the copy record and the copy**

In `crates/canager-core/src/model.rs`, in `pub enum UpdateBlocked`, after the `Pinned,` variant (its doc ends `Read by \`parse_outdated\` in \`adapters/pipx.rs\`.`), add:

```rust
    /// The tool installs its updates itself and has no update command
    /// Canager may run for it, so a newer version is listed with no
    /// button. One producer: `StandaloneAdapter::check_updates`
    /// (`adapters/standalone/mod.rs`) for a recipe whose `upgrade` is
    /// `None` -- Antigravity CLI, whose `agy update` is undocumented, takes
    /// no options and has never been run (agy.md §4; spec §4.4). Not "no
    /// candidate": the Installed row would then say "up to date", which is
    /// false while 1.2.11 exists; not `checkable: false`: Canager did
    /// check. Read by the gate (`blocked_upgrade` in session/plans.rs,
    /// generic over this enum), by `updateStateOf` in
    /// src/lib/updateState.ts (no button, no checkbox) and by
    /// `UPDATE_BLOCKED_KEYS.SelfUpdatesOnly` in src/lib/sources.ts, whose
    /// sentence tells the user to open the tool once and that it checks at
    /// most every 15 minutes.
    SelfUpdatesOnly,
```

In `src/lib/types.ts`, replace the `UpdateBlocked` type and its doc comment (from `/**` through `export type UpdateBlocked = "Pinned";`) with:

```ts
/**
 * Why the tool itself will refuse to update this one package, although its
 * source is writable and answering. Mirrors `UpdateBlocked` in
 * crates/canager-core/src/model.rs: bare-string unit variants. `Pinned` is
 * produced by brew's `parse_outdated` (from `brew outdated`'s
 * `pinned: true`) and pipx's (from `pipx list --outdated`'s
 * `name [pinned]:`); `SelfUpdatesOnly` by the standalone adapter's
 * `check_updates` for a tool that installs its updates itself and has no
 * update command Canager may run (Antigravity CLI, phase 4 step D). Read
 * through `UPDATE_BLOCKED_KEYS` in src/lib/sources.ts, a `Record` over
 * this union, so a variant added here without copy fails `tsc` rather
 * than rendering nothing.
 */
export type UpdateBlocked = "Pinned" | "SelfUpdatesOnly";
```

In `src/lib/sources.ts`, after `unpinCommand`'s closing `}` (before `/** What \`UPDATE_BLOCKED_KEYS\` holds for one reason. */`), add:

```ts

/**
 * The command a `SelfUpdatesOnly` row's sentence tells the user to run
 * once: the tool itself -- its launcher, which is the standalone
 * instance's `exe_path` (`StandaloneAdapter::detect`) -- with no
 * arguments. Opening it is what makes it check for updates (spec §4.4);
 * `<launcher> --version` would not (agy 1.2.10 never reaches its updater
 * from `--version`, spec §3.4). Quoted by `displayToken` when the path has
 * a space, like the unpin commands. The bare name when the snapshot lacks
 * the instance, which `refresh` never produces.
 */
function launcherCommand(key: ArtifactKey, instance: ManagerInstance | undefined): string {
  return displayToken(instance?.exe_path ?? key.name);
}
```

and in `UPDATE_BLOCKED_KEYS`, after the `Pinned: { … },` entry (its last line `refused: "updates.blocked.Pinned.refused",` and the closing `},`), add:

```ts
  SelfUpdatesOnly: {
    badge: "updates.blocked.SelfUpdatesOnly.badge",
    // The tool installs its updates itself (agy: a 15-minute debounce on
    // its background check, agy.md §4) and offers no command Canager may
    // run, so the sentence says what does work: open it once, then quit.
    // The available sentence names the versions the row compared; the
    // unavailable one cannot promise a current target and says only that
    // a newer version was seen.
    description: "updates.blocked.SelfUpdatesOnly.description",
    descriptionSourceUnavailable: "updates.blocked.SelfUpdatesOnly.descriptionSourceUnavailable",
    // The reason *is* "it updates itself": no separate sentence exists for
    // a self-updating package, and `rowDescription` falls back to
    // `description` (`copy.selfUpdatingDescription ?? copy.description`).
    selfUpdatingDescription: null,
    selfUpdatingDescriptionSourceUnavailable: null,
    command: launcherCommand,
    refused: "updates.blocked.SelfUpdatesOnly.refused",
  },
```

In `src/pages/UpdatesPage.tsx`, inside `rowDescription`'s `if (candidate.blocked !== null) { … }` branch, replace

```tsx
      return withCommand(
        t(description, {
          command: COMMAND_SLOT,
          source: sourceLabelFor(candidate.key.instance_id),
        }),
        copy.command(candidate.key, instance),
      );
```

with

```tsx
      return withCommand(
        t(description, {
          command: COMMAND_SLOT,
          source: sourceLabelFor(candidate.key.instance_id),
          // `SelfUpdatesOnly`'s sentence names the versions the row
          // compared (spec §9.2); `Pinned`'s do not use them.
          current: candidate.current,
          target: candidate.target,
        }),
        copy.command(candidate.key, instance),
      );
```

In `src/i18n/en.json`, under `"updates"` → `"blocked"`, after the `"Pinned": { … }` object (its last key `"refused": "This package has been pinned in {{source}}, so Canager didn't update it. Nothing has been changed."` and its closing `}`), add `,` and:

```json
      "SelfUpdatesOnly": {
        "badge": "Updates itself",
        "description": "A newer version of {{source}} is out ({{current}} → {{target}}), and {{source}} installs updates itself in the background — Canager doesn't have a safe way to do it for you. Open it once (run {{command}} in Terminal, then quit it): it checks for updates when it starts, at most once every 15 minutes, and installs the new version in the background.",
        "descriptionSourceUnavailable": "A newer version of {{source}} was seen the last time it answered, and {{source}} installs updates itself in the background — Canager doesn't have a safe way to do it for you. Open it once (run {{command}} in Terminal, then quit it): it checks for updates when it starts, at most once every 15 minutes, and installs the new version in the background.",
        "refused": "{{source}} updates itself, so Canager didn't try to update it. Nothing has been changed."
      }
```

In `src/i18n/zh-CN.json`, the same place (after `"Pinned"`'s `"refused": "这个软件在 {{source}} 里被固定（pin）了，所以 Canager 没有更新它。什么都没有改动。"` and its closing `}`), add `,` and:

```json
      "SelfUpdatesOnly": {
        "badge": "自己更新",
        "description": "{{source}} 出了新版本（{{current}} → {{target}}），它会在后台自己安装更新——Canager 没有安全的办法替你做这件事。打开它一次（在「终端」里运行 {{command}}，然后退出）：它启动时会检查更新，最多每 15 分钟一次，并在后台自己装好新版本。",
        "descriptionSourceUnavailable": "上次 {{source}} 应答时就已经有新版本了，它会在后台自己安装更新——Canager 没有安全的办法替你做这件事。打开它一次（在「终端」里运行 {{command}}，然后退出）：它启动时会检查更新，最多每 15 分钟一次，并在后台自己装好新版本。",
        "refused": "{{source}} 会自己更新，所以 Canager 没有去更新它。什么都没有改动。"
      }
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p canager-core --lib model::tests` and `pnpm exec vitest run src/lib/types.test.ts src/lib/sources.test.ts src/pages/UpdatesPage.test.tsx src/i18n`
Expected: PASS — the wire test, the two new sources tests, the new page test, and `completeness.test.ts` (the four keys are looked up through the `Record`'s literals). Then `pnpm typecheck`: clean (the `Record<UpdateBlocked, …>` is complete again).

- [ ] **Step 5: Format, gates, commit**

Run `cargo fmt --all`, then the five gates from Global Constraints. Expected: all clean.

```bash
git add crates/canager-core/src/model.rs src/lib/types.ts src/lib/types.test.ts src/lib/sources.ts src/lib/sources.test.ts src/pages/UpdatesPage.tsx src/pages/UpdatesPage.test.tsx src/i18n/en.json src/i18n/zh-CN.json
git commit -m "$(cat <<'EOF'
Add SelfUpdatesOnly, the update reason for a tool that updates itself

A tool with no update command Canager may run still has a newer version
worth showing: the row keeps the badge and loses the button, and its
sentence says to open the tool once, since that is what makes it check
(at most every 15 minutes). The gate and updateStateOf already refuse any
blocked candidate; this gives the reason its wire spelling and its words.
Antigravity CLI produces it in the step's core task.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

### Task 2: `RemovedWhat::Backups`, five `KeptWhat`s, their mirror, keys and copy

**Files:**
- Modify: `crates/canager-core/src/model.rs` — `pub enum RemovedWhat`, `pub enum KeptWhat` and their docs; the shape test `test_warning_wire_shapes_match_the_hand_written_ts_mirror` (its two `for what in […]` loops)
- Modify: `src/lib/types.ts` — `RemovedWhat`, `KeptWhat`; `src/lib/types.test.ts` — the `removed`/`kept` arrays
- Modify: `src/lib/warnings.ts` — `REMOVED_WHAT_KEYS`, `KEPT_WHAT_KEYS`; `src/lib/warnings.test.ts` — `it("gives each fixed warning its own key", …)`
- Modify: `src/i18n/en.json`, `src/i18n/zh-CN.json` — `warnings.willTrash.Backups`, `warnings.willKeep.{ToolState,ShellConfigLines,OutsideHome,NotOurs,InstallerCache}`
- Test: the files above.

**Interfaces:**
- Consumes: C's `RemovedWhat { Launcher, Program, Cache }`, `KeptWhat { Settings, SettingsAndHistory }`, `Warning::{WillTrash, WillKeep}`, `REMOVED_WHAT_KEYS`/`KEPT_WHAT_KEYS` (`Record`s over the mirrors), `warningKey`/`warningArgs`.
- Produces (verbatim): `RemovedWhat::Backups` (producer: `removal::listed_items`'s glob matches with `Glob.what`, Task 4, from `AGY.backup_globs`, Task 5; reader `REMOVED_WHAT_KEYS.Backups`); `KeptWhat::ToolState`, `KeptWhat::ShellConfigLines`, `KeptWhat::InstallerCache` (producers: `AGY`/`GROK`'s `keep` lists through `kept_places`, Task 5), `KeptWhat::OutsideHome` (producer: `removal::outside_home_keeps`, Task 4, from `GROK.keep`), `KeptWhat::NotOurs` (producer: `plan_removal`'s optional-path skip, Task 4); readers `KEPT_WHAT_KEYS.*`; the six locale keys.

- [ ] **Step 1: Write the failing tests**

In `crates/canager-core/src/model.rs`, inside `test_warning_wire_shapes_match_the_hand_written_ts_mirror`, replace the two loops

```rust
        for what in [
            RemovedWhat::Launcher,
            RemovedWhat::Program,
            RemovedWhat::Cache,
        ] {
            assert_eq!(
                serde_json::to_string(&what).unwrap(),
                format!("\"{what:?}\"")
            );
        }
        for what in [KeptWhat::Settings, KeptWhat::SettingsAndHistory] {
            assert_eq!(
                serde_json::to_string(&what).unwrap(),
                format!("\"{what:?}\"")
            );
        }
```

with

```rust
        // Every kind, as `REMOVED_WHAT_KEYS`/`KEPT_WHAT_KEYS` in
        // src/lib/warnings.ts spell them (step C's three and two, step D's
        // `Backups` and five more).
        for what in [
            RemovedWhat::Launcher,
            RemovedWhat::Program,
            RemovedWhat::Cache,
            RemovedWhat::Backups,
        ] {
            assert_eq!(
                serde_json::to_string(&what).unwrap(),
                format!("\"{what:?}\"")
            );
        }
        for what in [
            KeptWhat::Settings,
            KeptWhat::SettingsAndHistory,
            KeptWhat::ToolState,
            KeptWhat::ShellConfigLines,
            KeptWhat::OutsideHome,
            KeptWhat::NotOurs,
            KeptWhat::InstallerCache,
        ] {
            assert_eq!(
                serde_json::to_string(&what).unwrap(),
                format!("\"{what:?}\"")
            );
        }
```

In `src/lib/types.test.ts`, replace

```ts
    const removed: RemovedWhat[] = ["Launcher", "Program", "Cache"];
    const kept: KeptWhat[] = ["Settings", "SettingsAndHistory"];
    expect(JSON.stringify(removed)).toBe('["Launcher","Program","Cache"]');
    expect(JSON.stringify(kept)).toBe('["Settings","SettingsAndHistory"]');
```

with

```ts
    const removed: RemovedWhat[] = ["Launcher", "Program", "Cache", "Backups"];
    const kept: KeptWhat[] = [
      "Settings",
      "SettingsAndHistory",
      "ToolState",
      "ShellConfigLines",
      "OutsideHome",
      "NotOurs",
      "InstallerCache",
    ];
    expect(JSON.stringify(removed)).toBe('["Launcher","Program","Cache","Backups"]');
    expect(JSON.stringify(kept)).toBe(
      '["Settings","SettingsAndHistory","ToolState","ShellConfigLines","OutsideHome","NotOurs","InstallerCache"]',
    );
```

In `src/lib/warnings.test.ts`, inside `it("gives each fixed warning its own key", …)`, after the `expect(warningKey({ WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } })).toBe("warnings.willKeep.SettingsAndHistory");` assertion, add:

```ts
    // Step D's kinds: the backup a self-updater leaves, and the five things
    // the Antigravity and Grok lists keep.
    expect(warningKey({ WillTrash: { path: "~/.local/bin/agy.1727000000.old", what: "Backups" } })).toBe(
      "warnings.willTrash.Backups",
    );
    expect(warningKey({ WillKeep: { path: "~/.gemini/antigravity-cli", what: "ToolState" } })).toBe(
      "warnings.willKeep.ToolState",
    );
    expect(warningKey({ WillKeep: { path: "~/.zshrc", what: "ShellConfigLines" } })).toBe(
      "warnings.willKeep.ShellConfigLines",
    );
    expect(warningKey({ WillKeep: { path: "/usr/local/bin/grok", what: "OutsideHome" } })).toBe(
      "warnings.willKeep.OutsideHome",
    );
    expect(warningKey({ WillKeep: { path: "~/.local/bin/agent", what: "NotOurs" } })).toBe(
      "warnings.willKeep.NotOurs",
    );
    expect(warningKey({ WillKeep: { path: "~/.cache/antigravity", what: "InstallerCache" } })).toBe(
      "warnings.willKeep.InstallerCache",
    );
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p canager-core --lib model::tests::test_warning_wire_shapes_match_the_hand_written_ts_mirror`
Expected: FAIL to compile — `no variant or associated item named \`Backups\` found for enum \`RemovedWhat\``, and the five `KeptWhat` names.

Run: `pnpm typecheck`
Expected: FAIL — `Type '"Backups"' is not assignable to type 'RemovedWhat'` and the five `KeptWhat` literals, in types.test.ts and warnings.test.ts.

- [ ] **Step 3: Write the variants, the mirror, the keys and the copy**

In `crates/canager-core/src/model.rs`, replace the doc comment and enum `RemovedWhat` (from `/// What one path a path-list uninstall moves to the Trash is, for the` through the enum's closing `}`) with:

```rust
/// What one path a path-list uninstall moves to the Trash is, for the
/// sentence that lists it. Payload of `Warning::WillTrash`; produced by
/// `removal::plan_removal` from the recipe's `RemoveSpec.what`, or from a
/// `Glob.what` for a backup file (`removal::listed_items`, check 5), read
/// by `REMOVED_WHAT_KEYS` in src/lib/warnings.ts, a `Record` over the
/// mirror, so a variant added here without copy fails `tsc`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemovedWhat {
    /// The launcher: the command itself (`~/.local/bin/claude`;
    /// `~/.local/bin/agy`, which is the whole program; grok's
    /// `~/.grok/bin/grok` and `~/.grok/bin/agent`, two links to one
    /// download, and its optional fallback links in `~/.local/bin`).
    Launcher,
    /// The program's files (`~/.local/share/claude`, `~/.grok/downloads`).
    Program,
    /// Downloaded files the tool re-creates (`~/.claude/downloads`).
    Cache,
    /// A backup copy the tool's own updater left beside its launcher
    /// (`~/.local/bin/agy.<time>.old`, agy.md/spec §3.5), found through the
    /// recipe's `backup_globs`.
    Backups,
}
```

and replace the doc comment and enum `KeptWhat` (from `/// What one path a path-list uninstall leaves where it is, for the` through its closing `}`) with:

```rust
/// What one path a path-list uninstall leaves where it is, for the
/// sentence that lists it. Payload of `Warning::WillKeep`; produced by
/// `removal::plan_removal` from the recipe's `KeepSpec.what` (through
/// `kept_places` and, for `OutsideHome`, `outside_home_keeps`) and from its
/// optional-path skip (`NotOurs`); read by `KEPT_WHAT_KEYS` in
/// src/lib/warnings.ts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeptWhat {
    /// A settings file (`~/.claude.json`).
    Settings,
    /// Settings, login, history and working files, shared with other
    /// apps (`~/.claude`, which the tool's editor extensions and desktop
    /// app use too; `~/.grok`, with `config.toml`, `auth.json`, sessions
    /// and memory).
    SettingsAndHistory,
    /// The tool's own root, where its conversations, history and working
    /// files sit beside some of the program's own files, with no vendor
    /// list saying which could go alone (`~/.gemini/antigravity-cli`;
    /// spec §十三 #24).
    ToolState,
    /// A shell startup file the installer added lines to (`~/.zshrc`,
    /// `~/.zprofile`): Canager never edits one (spec §6.8), and does not
    /// read it to find the lines, so the sentence says "any lines".
    ShellConfigLines,
    /// A link outside the home folder the installer may have made into the
    /// tool's root (`/usr/local/bin/grok`): never touched, reported so the
    /// user knows it becomes a dead link. Reported only when it is a link
    /// into the root (`removal::points_into`): a `/usr/local/bin/grok` that
    /// is Homebrew's, or an `agent` that is another CLI's, gets no sentence.
    /// Report-only: `kept_places` does not protect it (a link into the
    /// program folder would otherwise refuse the uninstall it exists for).
    OutsideHome,
    /// An optional listed path that is there but Canager could not confirm
    /// is this install's -- the wrong shape, a link elsewhere, a folder on
    /// the way that is a link, or a place Canager never moves from
    /// (`~/.local/bin/agent` when another CLI owns it; spec §十三 #27) --
    /// so it stays and the uninstall goes on.
    NotOurs,
    /// The installer's download staging folder, directly in `~/.cache`
    /// (`~/.cache/antigravity`): Canager moves nothing that sits directly in
    /// a shared folder (check 1's never-list), so it stays, usually empty,
    /// and the user may delete it (phase 4 step D plan, ruling 1).
    InstallerCache,
}
```

In `src/lib/types.ts`, replace

```ts
export type RemovedWhat = "Launcher" | "Program" | "Cache";
```

with

```ts
export type RemovedWhat = "Launcher" | "Program" | "Cache" | "Backups";
```

and

```ts
export type KeptWhat = "Settings" | "SettingsAndHistory";
```

with

```ts
export type KeptWhat =
  | "Settings"
  | "SettingsAndHistory"
  | "ToolState"
  | "ShellConfigLines"
  | "OutsideHome"
  | "NotOurs"
  | "InstallerCache";
```

In `src/lib/warnings.ts`, replace the two `Record`s with:

```ts
/** The sentence for each kind of path a path-list uninstall moves; a
 *  `Record` over `RemovedWhat`, so a kind without copy fails `tsc`. */
const REMOVED_WHAT_KEYS: Record<RemovedWhat, string> = {
  Launcher: "warnings.willTrash.Launcher",
  Program: "warnings.willTrash.Program",
  Cache: "warnings.willTrash.Cache",
  Backups: "warnings.willTrash.Backups",
};

/** The sentence for each kind of path a path-list uninstall keeps. */
const KEPT_WHAT_KEYS: Record<KeptWhat, string> = {
  Settings: "warnings.willKeep.Settings",
  SettingsAndHistory: "warnings.willKeep.SettingsAndHistory",
  ToolState: "warnings.willKeep.ToolState",
  ShellConfigLines: "warnings.willKeep.ShellConfigLines",
  OutsideHome: "warnings.willKeep.OutsideHome",
  NotOurs: "warnings.willKeep.NotOurs",
  InstallerCache: "warnings.willKeep.InstallerCache",
};
```

In `src/i18n/en.json`, under `"warnings"` → `"willTrash"`, after `"Cache": "Moves to the Trash: {{path}} (downloaded files it can re-create)"` add `,` and

```json
      "Backups": "Moves to the Trash: {{path}} (an old copy the updater left behind)"
```

and under `"willKeep"`, after `"SettingsAndHistory": "Keeps: {{path}} (your settings, login, history and working files — other apps may use it too)"` add `,` and

```json
      "ToolState": "Keeps: {{path}} (its conversations, history and working files; some of the program's own files are in there too)",
      "ShellConfigLines": "Keeps: {{path}} (any lines its installer added there — harmless, and Canager never edits that file)",
      "OutsideHome": "Keeps: {{path}} (outside your home folder, so Canager won't touch it; after uninstalling it's a dead link you can delete yourself)",
      "NotOurs": "Keeps: {{path}} (Canager couldn't confirm it's part of this install — something else may have put it there — so it stays)",
      "InstallerCache": "Keeps: {{path}} (the installer's download staging folder, usually empty — Canager moves nothing that sits directly in ~/.cache; you can delete it yourself)"
```

In `src/i18n/zh-CN.json`, the same two places: after `"Cache": "移到废纸篓：{{path}}（可重新下载的缓存）"` add `,` and

```json
      "Backups": "移到废纸篓：{{path}}（更新程序留下的旧副本）"
```

and after `"SettingsAndHistory": "保留：{{path}}（你的设置、登录信息、历史记录和工作文件，其它应用也可能在用）"` add `,` and

```json
      "ToolState": "保留：{{path}}（它的对话、历史和工作文件；程序自己的一些文件也在里面）",
      "ShellConfigLines": "保留：{{path}}（安装程序加进去的那几行，如果有的话——无害；Canager 从不改这个文件）",
      "OutsideHome": "保留：{{path}}（不在你的个人文件夹里，Canager 不会碰它；卸载后它是个失效的链接，你可以自己删）",
      "NotOurs": "保留：{{path}}（Canager 无法确认它属于这次安装——可能是别的东西放在那里的——所以不动它）",
      "InstallerCache": "保留：{{path}}（安装器的下载暂存文件夹，通常是空的——Canager 不会移动直接放在 ~/.cache 里的东西，你可以自己删）"
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p canager-core --lib model::tests` and `pnpm exec vitest run src/lib/types.test.ts src/lib/warnings.test.ts src/i18n`
Expected: PASS (the six keys are referenced through the two `Record`s' literals, so `completeness.test.ts` finds them). `pnpm typecheck`: clean.

- [ ] **Step 5: Format, gates, commit**

Run `cargo fmt --all`, then the five gates. Expected: all clean.

```bash
git add crates/canager-core/src/model.rs src/lib/types.ts src/lib/types.test.ts src/lib/warnings.ts src/lib/warnings.test.ts src/i18n/en.json src/i18n/zh-CN.json
git commit -m "$(cat <<'EOF'
Add the kinds the Antigravity and Grok uninstall lists move and keep

Backups (a copy the updater left), a tool's own root, a shell startup
file, a path outside the home folder, an optional path Canager could not
confirm is the tool's, and an installer's staging folder in ~/.cache --
each with its sentence in both languages, so the dialog can say them
when the two recipes land in this step's core task.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

### Task 3: `Glob`, `Recipe.backup_globs`, the Unknown page's rule 4

**Files:**
- Modify: `crates/canager-core/src/scan/mod.rs` — imports; new `Glob`; `Known` (field, `index`, `claimant`); `scan_dirs`, `scan_unknown`; `test_the_longest_owned_root_wins`; two new unit tests
- Modify: `crates/canager-core/src/session/scan.rs` — the one call
- Modify: `crates/canager-core/src/adapters/standalone/recipe.rs` — `Recipe.backup_globs`; module doc
- Modify: `crates/canager-core/src/adapters/standalone/recipes.rs` — `backup_globs()`; every `Recipe {` literal; two tests
- Modify: `crates/canager-core/src/adapters/standalone/mod.rs` — C's test-only `NO_UNINSTALL` recipe literal (and any other `Recipe {` literal the grep finds)
- Modify: `crates/canager-core/tests/unknown_scan_test.rs` — every `scan_dirs(` call; one new test
- Modify: `docs/what-we-run.md` — the `## Unknown-source scan` section's attribution paragraph
- Test: the files above.

**Interfaces:**
- Consumes: F's `Known`, `scan_dirs`, `scan_unknown`, `examine`, `EntryKind`, `display_path`; F's `Home`/`exe`/`link`/`tilde` in `unknown_scan_test.rs`; B/C/E's `Recipe` and `RECIPES`; `RemovedWhat` (model.rs).
- Produces (verbatim): `pub struct Glob { pub dir, pub prefix, pub suffix, pub what: RemovedWhat }` with `pub fn dir_under(&self, home: &Path) -> PathBuf` and `pub fn matches_name(&self, name: &str) -> bool` (readers: `Known::index`/`claimant` here; `removal::listed_items`, Task 4); `Recipe.backup_globs: &'static [Glob]` (readers: `recipes::backup_globs()` here, `StandaloneAdapter::plan`/`execute` through `Job.globs`, Task 4; producers: every literal with `&[]` here, `AGY` with one pattern, Task 5); `pub fn backup_globs() -> Vec<(String, &'static [Glob])>` (reader: `Session::scan_unknown`); `scan_dirs`/`scan_unknown` with a `globs: &[(String, &'static [Glob])]` parameter before `budget` (readers: `Session::scan_unknown`, the tests); private `Known::index(instances, artifacts, globs, home)` and `Known::claimant(raw, dir, resolved, kind)`.

Rule 4 (spec §8.3): after rules 0–3, an entry that is a regular file (`EntryKind::File`) in a directory that is some *installed* tool's pattern directory, whose name is `prefix` + at least one character + `suffix`, is that tool's. A tool with no instance contributes no pattern, so its leftover backup is listed — the spec's own words ("实例不存在（agy 已卸载）时留下的 .old 是真的来源不明，照列").

- [ ] **Step 1: Write the failing tests**

In `crates/canager-core/src/scan/mod.rs`, inside `mod tests`, after `test_app_bundle_takes_the_first_candidate_with_a_dot_app_component` add:

```rust
    #[test]
    fn test_glob_matches_a_prefix_something_and_a_suffix_on_the_name_alone() {
        // agy's updater leaves `agy.<time>.old` beside the launcher
        // (spec §3.5). Something has to sit between prefix and suffix, so
        // `agy..old` and `agy.old` are not matches; the name is all that is
        // looked at here -- the kind of file is the caller's (`claimant`,
        // `removal::listed_items`).
        let glob = Glob {
            dir: "~/.local/bin",
            prefix: "agy.",
            suffix: ".old",
            what: RemovedWhat::Backups,
        };
        for name in ["agy.1727000000.old", "agy.2026-09-25T12-25-00.old", "agy.x.old"] {
            assert!(glob.matches_name(name), "{name}");
        }
        for name in ["agy", "agy.old", "agy..old", "agy.1727000000.old.bak", "xagy.1.old", "agy.1.OLD"] {
            assert!(!glob.matches_name(name), "{name}");
        }
    }

    #[test]
    fn test_glob_dir_under_joins_like_a_recipe_path() {
        // The scan compares raw spellings (F's rule 0, `route::expand`'s
        // doc), so the pattern's directory is spelled off the same home.
        let glob = Glob {
            dir: "~/.local/bin",
            prefix: "agy.",
            suffix: ".old",
            what: RemovedWhat::Backups,
        };
        assert_eq!(
            glob.dir_under(Path::new("/Users/someone")),
            PathBuf::from("/Users/someone/.local/bin")
        );
        assert_eq!(
            glob.dir_under(Path::new("/Volumes/Data/homes/someone")),
            PathBuf::from("/Volumes/Data/homes/someone/.local/bin")
        );
    }
```

and in `test_the_longest_owned_root_wins`, replace

```rust
        let known = Known::index(&[outer_inst, inner_inst], &[]);
        assert_eq!(
            known.claimant(&entry, Some(&entry)).map(String::as_str),
            Some("ollama:http://inner:11434")
        );
```

with

```rust
        let known = Known::index(&[outer_inst, inner_inst], &[], &[], &tmp);
        assert_eq!(
            known
                .claimant(&entry, &inner_bin, Some(&entry), EntryKind::File)
                .map(String::as_str),
            Some("ollama:http://inner:11434")
        );
```

In `crates/canager-core/tests/unknown_scan_test.rs`, add `RemovedWhat` to the `use canager_core::model::{…}` list and `Glob` to the `use canager_core::scan::{…}` list, and after `test_rule_3_never_treats_a_parent_derived_prefix_as_owned` add:

```rust

/// agy's pattern, as its recipe declares it (spec §3.5): a `'static` slice
/// because the scan reads recipes' own constants.
static AGY_GLOBS: [Glob; 1] = [Glob {
    dir: "~/.local/bin",
    prefix: "agy.",
    suffix: ".old",
    what: RemovedWhat::Backups,
}];

#[test]
fn test_rule_4_claims_a_backup_the_updaters_pattern_names_only_while_the_tool_is_installed() {
    // Spec §8.3 rule 4: `agy.<time>.old` beside an installed agy is the
    // updater's leftover, not a stranger -- by name, in that directory, for
    // a regular file. A link with such a name is not (the pattern describes
    // the updater's copies, which are files) -- pointed at a file no rule
    // claims, so that it is rule 4's `File` guard and not rule 1 (a link
    // resolving to agy's own exe_path) that decides; a name without the
    // middle is not; and once agy is gone the pattern is gone with its
    // instance, so a leftover `.old` is listed, which is the truth.
    let home = Home::new("rule-4");
    let bin = home.dir(".local/bin");
    let agy = exe(&bin, "agy", b"x");
    exe(&bin, "agy.1727000000.old", b"x");
    exe(&bin, "agy.old", b"x");
    let elsewhere = home.dir("elsewhere");
    let other = exe(&elsewhere, "other-tool", b"y");
    link(&bin, "agy.2.old", &other);
    let instance = ManagerInstance {
        exe_path: agy.clone(),
        prefix: home.path().join(".gemini/antigravity-cli"),
        ..manager_instance("standalone-agy", "standalone-agy")
    };
    let globs = vec![("standalone-agy".to_string(), &AGY_GLOBS[..])];

    let scan = scan_dirs(
        &[bin.clone()],
        &home.env(vec![]),
        std::slice::from_ref(&instance),
        &[],
        &globs,
        ScanBudget::default(),
    );

    let listed: Vec<PathBuf> = scan.entries.iter().map(|e| e.path.clone()).collect();
    assert_eq!(
        listed,
        vec![tilde(".local/bin/agy.2.old"), tilde(".local/bin/agy.old")],
        "{:?}",
        scan.entries
    );
    assert_eq!(scan.attributed, 2, "agy (rule 0) and its backup (rule 4)");

    // agy uninstalled: no instance, no pattern; the backup is a stranger.
    let scan = scan_dirs(&[bin], &home.env(vec![]), &[], &[], &globs, ScanBudget::default());
    let listed: Vec<PathBuf> = scan.entries.iter().map(|e| e.path.clone()).collect();
    assert!(
        listed.contains(&tilde(".local/bin/agy.1727000000.old")),
        "{listed:?}"
    );
    assert_eq!(scan.attributed, 0);
}
```

Every other `scan_dirs(` call in that file gains a `&[],` argument between the `artifacts` argument and `ScanBudget::default()` (F wrote them all as `scan_dirs(&[…], &home.env(…), &[…], &[…], ScanBudget::default())`; the compiler names each one: `this function takes 6 arguments but 5 arguments were supplied`).

In `crates/canager-core/src/adapters/standalone/recipes.rs`, inside `mod tests`, add `use crate::scan::Glob;` to the imports and append two tests before the module's closing `}`:

```rust
    #[test]
    fn test_every_backup_glob_is_under_home_and_names_a_pattern() {
        // `Glob::dir_under` joins `~/` and nothing else, and check 1 refuses
        // a match whose folder is a shared one, so a pattern's folder must
        // be under home and deeper than the never-list; a pattern with an
        // empty prefix or suffix would match every file in the folder.
        for recipe in RECIPES {
            for glob in recipe.backup_globs {
                let rest = glob
                    .dir
                    .strip_prefix("~/")
                    .unwrap_or_else(|| panic!("{}: glob dir {:?} must start with ~/", recipe.id, glob.dir));
                assert!(
                    !rest.is_empty() && !rest.ends_with('/') && !rest.contains(".."),
                    "{}: glob dir {:?} must name one plain folder",
                    recipe.id,
                    glob.dir
                );
                assert!(
                    !SHARED_FOLDERS.iter().any(|shared| rest == *shared),
                    "{}: glob dir {:?} is a shared folder; check 1 would refuse every match",
                    recipe.id,
                    glob.dir
                );
                assert!(!glob.prefix.is_empty(), "{}: an empty prefix matches everything", recipe.id);
                assert!(!glob.suffix.is_empty(), "{}: an empty suffix matches everything", recipe.id);
            }
        }
    }

    #[test]
    fn test_backup_globs_lists_every_recipe_under_its_adapter_id() {
        // `Session::scan_unknown` hands this to the scan's rule 4, keyed the
        // way the scan keys instances: by adapter id.
        let listed = backup_globs();
        assert_eq!(listed.len(), RECIPES.len());
        for (recipe, (id, globs)) in RECIPES.iter().zip(&listed) {
            assert_eq!(id, &format!("standalone-{}", recipe.id));
            assert!(std::ptr::eq(*globs, recipe.backup_globs));
        }
    }
```

(`SHARED_FOLDERS` reaches the module through C's `use super::super::recipe::{Expect, Uninstall, SHARED_FOLDERS};` line in `mod tests`; if C imported it elsewhere, use that path.)

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p canager-core --lib scan::tests` and `cargo test -p canager-core --test unknown_scan_test`
Expected: FAIL to compile — `cannot find type \`Glob\` in this scope`; `this method takes 2 arguments but 4 arguments were supplied` (`Known::index`), `this method takes 2 arguments but 4 arguments were supplied` (`claimant`); `unresolved import \`canager_core::scan::Glob\``.

Run: `cargo test -p canager-core --lib adapters::standalone::recipes`
Expected: FAIL to compile — `no field \`backup_globs\` on type \`&Recipe\``; `cannot find function \`backup_globs\``.

- [ ] **Step 3: Write `Glob`, the field, the rule and the wiring**

In `crates/canager-core/src/scan/mod.rs`, change the import `use crate::model::{InstalledArtifact, InstanceId, ManagerInstance};` to `use crate::model::{InstalledArtifact, InstanceId, ManagerInstance, RemovedWhat};`, and after `ScannedDir`'s definition (before `/// What one listed entry is.`) insert:

```rust

/// A file-name pattern for the backup copies a tool's own updater leaves
/// beside its launcher -- `~/.local/bin/agy.<time>.old` (agy.md §4, spec
/// §3.5) -- as `prefix` + something + `suffix` on a regular file directly
/// in `dir`; no glob crate. Defined here rather than beside the recipes
/// because this scan reads it (rule 4) and `adapters` depends on `scan`,
/// never the reverse (spec §3.1). Read by `Known::index`/`claimant` (rule 4,
/// through `Recipe.backup_globs` via `Session::scan_unknown`) and by
/// `adapters::standalone::removal::listed_items` (check 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glob {
    /// `~/…`: the one directory the pattern applies to, never recursed.
    pub dir: &'static str,
    pub prefix: &'static str,
    pub suffix: &'static str,
    /// What a match is, for the uninstall preview's sentence
    /// (`Warning::WillTrash`).
    pub what: RemovedWhat,
}

impl Glob {
    /// `dir` under `home`, joined the way a recipe path is
    /// (`adapters::standalone::route::expand`): the same spelling rule, so
    /// an instance's raw `exe_path` and a pattern's directory come from
    /// one home. A `dir` not starting with `~/` is a programming error in a
    /// recipe constant, which
    /// `recipes::tests::test_every_backup_glob_is_under_home_and_names_a_pattern`
    /// catches before this can.
    pub fn dir_under(&self, home: &Path) -> PathBuf {
        let rest = self
            .dir
            .strip_prefix("~/")
            .unwrap_or_else(|| panic!("glob dir {:?} must start with ~/", self.dir));
        home.join(rest)
    }

    /// Whether a file named `name` is one of this pattern's: `prefix`
    /// first, `suffix` last, and at least one character between -- so a
    /// name that is exactly `prefix + suffix` (`agy..old`) is not.
    pub fn matches_name(&self, name: &str) -> bool {
        name.len() > self.prefix.len() + self.suffix.len()
            && name.starts_with(self.prefix)
            && name.ends_with(self.suffix)
    }
}
```

In `struct Known`, after the `owned` field add:

```rust
    /// Rule 4: for every instance whose adapter declares backup-file
    /// patterns (`Recipe.backup_globs`, handed in by `Session::scan_unknown`
    /// keyed by adapter id), each pattern's directory, canonical, with the
    /// pattern and the instance. A directory that does not exist is simply
    /// absent; a tool with no instance contributes nothing, so its leftover
    /// backup is listed.
    backups: Vec<(PathBuf, Glob, InstanceId)>,
```

In the doc comment above `struct Known` (the numbered rules), after rule 3's paragraph add:

```rust
/// 4. The entry is a regular file in a directory an *installed* tool's
///    `backup_globs` name, and its name matches one of them (`agy.<time>.old`
///    in `~/.local/bin`): the tool's own updater left it. Read from
///    `Recipe.backup_globs` (phase 4 step D); without the instance, listed.
```

Replace `Known::index`'s signature and the `Known { … }` it returns:

```rust
    fn index(
        instances: &[ManagerInstance],
        artifacts: &[InstalledArtifact],
        globs: &[(String, &'static [Glob])],
        home: &Path,
    ) -> Known {
```

and, before the `Known { exe_raw, … }` literal at the end of `index`, add:

```rust
        let backups = instances
            .iter()
            .flat_map(|inst| {
                globs
                    .iter()
                    .filter(move |(adapter_id, _)| *adapter_id == inst.adapter_id)
                    .flat_map(move |(_, patterns)| {
                        patterns.iter().filter_map(move |glob| {
                            let dir = std::fs::canonicalize(glob.dir_under(home)).ok()?;
                            Some((dir, *glob, inst.id.clone()))
                        })
                    })
            })
            .collect();
```

and add `backups,` to that literal. Replace `claimant` (from its doc comment `/// The source that put \`raw\` (real path \`resolved\`; \`None\` for a broken` through its closing `}`) with:

```rust
    /// The source that put `raw` (in the canonical directory `dir`; real
    /// path `resolved`, `None` for a broken link; `kind` what it is) there,
    /// by the first rule that matches -- or `None`: unknown.
    fn claimant(
        &self,
        raw: &Path,
        dir: &Path,
        resolved: Option<&Path>,
        kind: EntryKind,
    ) -> Option<&InstanceId> {
        if let Some((_, id)) = self.exe_raw.iter().find(|(exe, _)| exe == raw) {
            return Some(id);
        }
        if let Some(resolved) = resolved {
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
            if let Some((_, id)) = self
                .owned
                .iter()
                .filter(|(root, _)| resolved.starts_with(root))
                .max_by_key(|(root, _)| root.as_os_str().len())
            {
                return Some(id);
            }
        }
        // Rule 4: a backup the tool's own updater left, by name, in the
        // pattern's directory, a regular file -- a link of that name is
        // somebody's link, not the updater's copy.
        if kind == EntryKind::File {
            if let Some(name) = raw.file_name().and_then(|name| name.to_str()) {
                if let Some((_, _, id)) = self
                    .backups
                    .iter()
                    .find(|(glob_dir, glob, _)| glob_dir == dir && glob.matches_name(name))
                {
                    return Some(id);
                }
            }
        }
        None
    }
```

In `scan_dirs`: add the parameter `globs: &[(String, &'static [Glob])],` between `artifacts: &[InstalledArtifact],` and `budget: ScanBudget,`; change `let known = Known::index(instances, artifacts);` to `let known = Known::index(instances, artifacts, globs, &env.home);`; change `seen.push(canonical);` to `seen.push(canonical.clone());`; and change `match known.claimant(&raw, entry.resolved.as_deref()) {` to `match known.claimant(&raw, &canonical, entry.resolved.as_deref(), entry.kind) {`. In `scan_unknown`: the same parameter in the same place, passed through: `scan_dirs(&candidate_dirs(env), env, instances, artifacts, globs, budget)`. In both functions' doc comments add the sentence `\`globs\` are the installed tools' backup-file patterns by adapter id (rule 4).`

In `crates/canager-core/src/session/scan.rs`, replace `scan::scan_unknown(env, &instances, &artifacts, ScanBudget::default())` with:

```rust
        scan::scan_unknown(
            env,
            &instances,
            &artifacts,
            &crate::adapters::standalone::recipes::backup_globs(),
            ScanBudget::default(),
        )
```

and add to the method's doc comment: `The backup-file patterns of every standalone recipe (\`recipes::backup_globs\`) are handed in for rule 4; only the ones with an instance in the snapshot claim anything.`

In `crates/canager-core/src/adapters/standalone/recipe.rs`, add `use crate::scan::Glob;` to the imports, and inside `Recipe`, after E's `extra_locks` field, add:

```rust
    /// The file-name patterns of the backup copies the tool's own updater
    /// leaves beside its launcher (`~/.local/bin/agy.<time>.old`; spec
    /// §3.5), empty for a tool whose updater leaves none. Read by the
    /// path-list uninstall (`removal::listed_items`, check 5: each match
    /// is moved before the launcher and listed in the preview) and by the
    /// Unknown page's rule 4 (`recipes::backup_globs` →
    /// `Session::scan_unknown`), so a fresh backup is the tool's and not a
    /// stranger while the tool is installed.
    pub backup_globs: &'static [Glob],
```

In the module doc, change the sentence beginning `step D adds \`backup_globs\`` (C's wording: "step D adds `backup_globs`, a `FlatFile` route, `Expect::File`, a `SecondToken` version parse, the other `Latest` sources and an optional `upgrade` (agy updates itself only)") to `step D added \`backup_globs\`, \`Expect::File\`, the other \`Latest\` sources and an optional \`upgrade\` (agy updates itself only)` — the `FlatFile` and `SecondToken` clauses went with E.

In `crates/canager-core/src/adapters/standalone/recipes.rs`, add `use crate::scan::Glob;` to the imports, add `backup_globs: &[],` as the last field of `CLAUDE` and of `RUSTUP` (after E's `extra_locks: …,`), and after `RECIPES` add:

```rust

/// Every registered tool's backup-file patterns, keyed by its adapter id
/// (`standalone-<id>`), for the Unknown page's rule 4
/// (`Session::scan_unknown` → `scan::scan_unknown`). A tool with none
/// contributes an empty slice, which claims nothing.
pub fn backup_globs() -> Vec<(String, &'static [Glob])> {
    RECIPES
        .iter()
        .map(|recipe| (format!("standalone-{}", recipe.id), recipe.backup_globs))
        .collect()
}
```

Every other `Recipe {` literal gains `backup_globs: &[],` as its last field: `grep -rn "Recipe {$" crates/canager-core/src/adapters/standalone/` lists them (C's test-only `NO_UNINSTALL` in `mod.rs`'s tests; any E test recipe); the build stops with `missing field \`backup_globs\`` at any the grep missed.

In `docs/what-we-run.md`, in the `## Unknown-source scan` section's paragraph beginning `A program is *not* listed when a known source accounts for it`, after the clause ending `Ollama's \`~/.ollama\`; Claude Code's \`~/.local/share/claude\`).` (C/E may have extended that list; append after its closing parenthesis and before `Everything else is listed`), insert the sentence:

```
A regular file in a tool's own bin directory whose name is one of the
backup patterns that tool's recipe declares — `agy.<time>.old` in
`~/.local/bin`, the copies Antigravity's updater leaves — is that tool's
while the tool is installed (`Recipe.backup_globs`, rule 4); once the tool
is gone the pattern goes with it and such a file is listed.
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p canager-core --lib scan`, `cargo test -p canager-core --test unknown_scan_test`, `cargo test -p canager-core --lib adapters::standalone::recipes`, `cargo test -p canager-core --lib session::scan`
Expected: PASS — the two new `scan` unit tests and F's (with the two amended calls); the rule-4 test and F's twenty-odd with their `&[]`; `test_every_backup_glob_is_under_home_and_names_a_pattern` (nothing to check yet: every slice is empty) and `test_backup_globs_lists_every_recipe_under_its_adapter_id`; F's session test.

- [ ] **Step 5: Format, gates, commit**

Run `cargo fmt --all`, then the five gates. Expected: all clean.

```bash
git add crates/canager-core/src/scan/mod.rs crates/canager-core/src/session/scan.rs crates/canager-core/src/adapters/standalone/recipe.rs crates/canager-core/src/adapters/standalone/recipes.rs crates/canager-core/src/adapters/standalone/mod.rs crates/canager-core/tests/unknown_scan_test.rs docs/what-we-run.md
git commit -m "$(cat <<'EOF'
Claim a backup copy a tool's own updater left beside its launcher

Antigravity's updater leaves agy.<time>.old next to agy for a while
(spec §3.5). Without a rule for it the Unknown page would list the
updater's own leftover as a stranger. A recipe now declares the pattern
once (Recipe.backup_globs, a Glob the scan owns), the scan's rule 4
claims a matching regular file while the tool is installed, and the
uninstall's check 5 reads the same patterns in the next commit. Every
recipe declares none yet; agy declares its own with its recipe.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

(Plus any file the `Recipe {` grep made you edit that is not listed above: `git status --short` names it; add it by path.)

### Task 4: `removal.rs` — check 5, `Expect::File`, the `NotOurs` skip, report-only outside-home keeps

**Files:**
- Modify: `crates/canager-core/src/adapters/standalone/recipe.rs` — `Expect::File`; `KeepSpec.path`'s doc
- Modify: `crates/canager-core/src/adapters/standalone/removal.rs` — imports; `Job.globs`; `Item`, `listed_items`, `keeps_instead`, `outside_home_keeps`, `points_into`; `check_item`, `kept_places`, `plan_removal`, `take_turn`; tests (two of C's replaced or cut down, six new)
- Modify: `crates/canager-core/src/adapters/standalone/mod.rs` — the two `removal::Job { … }` literals in `plan` and `execute`
- Modify: `crates/canager-core/src/adapters/standalone/recipes.rs` — three invariants tests rewritten
- Modify: `docs/what-we-run.md` — two sentences of the `## Claude Code` check paragraph that this task makes false (C's rule: a document changes in the commit that changes what it describes)
- Test: `cargo test -p canager-core --lib adapters::standalone`, `cargo test -p canager-core --test what_we_run_test`

**Interfaces:**
- Consumes: C's `Job`, `Removal`, `Look`, `Kept`, `Refusal { path, reason }`, `check_item`, `kept_places`, `disturbed`, `is_shared_folder`, `spelled`, `shown`, `identity_of`, `take_turn`, `Turn`, `execute_removal`, `plan_removal`, the test helpers of `removal.rs`'s `mod tests` (`detected`, `claude_job`, `only`, `trash`, `keep`, `refused`, `identity`, `run`, `no_gap`, `path_changed`, `moved`); `Glob` (Task 3); `KeptWhat::{NotOurs, OutsideHome}`, `RemovedWhat::Backups` (Task 2); `UninstallUnsafeReason`.
- Produces (verbatim): `Expect::File` (reader: `check_item`'s kind match; producers `AGY`/`GROK`, Task 5, and every glob match here); `Job.globs: &'static [Glob]` (writers: `StandaloneAdapter::plan`/`execute` from `recipe.backup_globs`; reader `listed_items`); private `struct Item { rel: PathBuf, path: PathBuf, expect: Expect, what: RemovedWhat, optional: bool }`, `fn listed_items(job: &Job) -> Vec<Item>` (readers: `plan_removal`, `take_turn`), `fn check_item(look: &Look<'_>, kept: &[Kept], rel: &Path, expect: Expect, path: &Path) -> Result<ItemIdentity, Refusal>` (the same two callers), `fn keeps_instead(reason: UninstallUnsafeReason) -> bool` (reader: `plan_removal`), `fn outside_home_keeps(look: &Look<'_>) -> Vec<Warning>` (reader: `plan_removal`), `fn points_into(link: &Path, root: &Path) -> bool` (reader: `outside_home_keeps`); a `KeepSpec` whose `what` is `KeptWhat::OutsideHome` may name an absolute path (producer: `GROK.keep`, Task 5; readers: `kept_places` skips it, `outside_home_keeps` reports it when it links into the root).

Rules (spec §6.3, as C implemented it, plus this step's three): **check 5** — every match of a `Job.globs` pattern (a regular file directly in the pattern's folder, named `prefix`+something+`suffix`, in name order) is an item placed after every listed path but the last, so the launcher still goes last (spec §6.2), and passes checks 1, 3 and 4 as a `File`. **The `NotOurs` skip** — an *optional* item that is there but fails check 1 (`OutsideHome`, `SharedFolder`) or check 4 (`NotWhatInstructionsExpect`, which C also answers for a linked folder on the way) is kept and said (`WillKeep { NotOurs }`) after the moves; `NotOwnedByYou` and `OverlapsKept` still refuse the whole list (ruling 5). Glob matches are optional. **Outside-home keeps** — a `KeepSpec { what: OutsideHome }` is absolute, skipped by `kept_places`, and reported by `outside_home_keeps` only when `points_into` finds a symbolic link there whose target lies under the recipe's root for this home (ruling 6): a regular file, a folder, a link elsewhere (Homebrew's `/usr/local/bin/grok` on an Intel Mac) or nothing gives no sentence. **`take_turn`** finds each confirmed path among `listed_items(job)` (a glob match that vanished is `Changed`, like any other vanished item).

- [ ] **Step 1: Write the failing tests**

In `crates/canager-core/src/adapters/standalone/removal.rs`, in `mod tests`:

(a) Add `use crate::scan::Glob;` to the imports, and add `RemoveSpec`'s neighbour `KeepSpec` (already imported by C: `use super::super::recipe::{Expect, KeepSpec, RemoveSpec, Uninstall};`).

(b) In `claude_job`, add `globs: &[],` as the `Job` literal's last field. Every other `Job {` literal in the test module (the `only(…)`-based ones in `test_plan_removal_refuses_a_path_whose_folder_is_home_or_shared`) gains the same line; the compiler names them.

(c) After `only`, add two more leaked-slice helpers:

```rust
    /// A one-pattern glob list, `'static` like a recipe's.
    fn only_glob(glob: Glob) -> &'static [Glob] {
        Box::leak(Box::new([glob]))
    }

    /// A one-path keep list, `'static` like a recipe's; `path` is leaked
    /// too, so a test can keep a path under its own temp directory.
    fn only_keep(path: String, what: KeptWhat) -> &'static [KeepSpec] {
        let path: &'static str = Box::leak(path.into_boxed_str());
        Box::leak(Box::new([KeepSpec { path, what }]))
    }
```

(d) Replace C's `test_plan_removal_refuses_an_optional_path_of_the_wrong_shape` (from its `#[test]` through its closing `}`) with:

```rust
    #[test]
    fn test_plan_removal_keeps_an_optional_path_it_cannot_confirm_is_the_tools_and_says_so() {
        // Spec §6.3 check 4 (§十三 #27): an optional path that is there but
        // not what the list describes -- `~/.claude/downloads` as a link
        // elsewhere, or as a file -- is not this install's to move. C
        // refused the whole uninstall for it; now it stays, the preview
        // says so after the moves, and the uninstall goes on. (The array
        // is typed as fn pointers: two closures never share a type, and
        // the coercion does not reach inside a tuple.)
        let cases: [(&str, fn(&TempHome)); 2] = [
            ("removal-optional-link", |home| {
                let elsewhere = home.dir("elsewhere/downloads");
                home.link(".claude/downloads", &elsewhere);
            }),
            ("removal-optional-file", |home| {
                home.file(".claude/downloads");
            }),
        ];
        for (tag, make) in cases {
            let home = TempHome::new(tag);
            let layout = claude_layout(&home, "2.1.281");
            make(&home);
            let d = detected(home.path());

            let removal = plan_removal(&claude_job(&d)).expect("a plan");

            assert_eq!(
                removal.paths,
                vec![
                    home.path().join(".local/share/claude"),
                    layout.launcher.clone()
                ],
                "{tag}"
            );
            assert_eq!(
                removal.warnings,
                vec![
                    trash("~/.local/share/claude", RemovedWhat::Program),
                    trash("~/.local/bin/claude", RemovedWhat::Launcher),
                    keep("~/.claude/downloads", KeptWhat::NotOurs),
                    keep("~/.claude", KeptWhat::SettingsAndHistory),
                ],
                "{tag}"
            );
        }
    }

    #[test]
    fn test_plan_removal_keeps_an_optional_path_whose_folder_leads_elsewhere() {
        // The other reasons the skip covers (ruling 5): an optional path
        // whose folder is a link inside the home folder (C's ruling 24 case,
        // `~/.claude -> ~/Documents`, which C refused as
        // `NotWhatInstructionsExpect` and now keeps), to another volume
        // (`OutsideHome`), or into a shared folder (`SharedFolder`). None is
        // the tool's to move; a grok whose `~/.config` is a dotfiles link
        // keeps its fish completion rather than becoming impossible to
        // uninstall.
        let home = TempHome::new("removal-optional-linked-inside");
        let _layout = claude_layout(&home, "2.1.281");
        let documents = home.dir("Documents");
        home.dir("Documents/downloads");
        home.link(".claude", &documents);
        let d = detected(home.path());
        let removal = plan_removal(&claude_job(&d)).expect("a plan, not C's refusal");
        assert_eq!(
            removal.warnings,
            vec![
                trash("~/.local/share/claude", RemovedWhat::Program),
                trash("~/.local/bin/claude", RemovedWhat::Launcher),
                keep("~/.claude/downloads", KeptWhat::NotOurs),
                keep("~/.claude", KeptWhat::SettingsAndHistory),
            ]
        );
        assert!(home.path().join("Documents/downloads").is_dir());

        let outside = TempHome::new("removal-optional-outside");
        let home = TempHome::new("removal-optional-folder-away");
        let _layout = claude_layout(&home, "2.1.281");
        let away = outside.dir("claude-state");
        outside.dir("claude-state/downloads");
        home.link(".claude", &away);
        let d = detected(home.path());
        let removal = plan_removal(&claude_job(&d)).expect("a plan");
        assert!(removal
            .warnings
            .contains(&keep("~/.claude/downloads", KeptWhat::NotOurs)));
        assert_eq!(removal.paths.len(), 2);

        let home = TempHome::new("removal-optional-shared");
        let _layout = claude_layout(&home, "2.1.281");
        home.dir(".cache/thing");
        let d = detected(home.path());
        let job = Job {
            recipe: &CLAUDE,
            detected: d.clone(),
            remove: Box::leak(Box::new([
                RemoveSpec {
                    path: "~/.cache/thing",
                    expect: Expect::Dir,
                    what: RemovedWhat::Cache,
                    optional: true,
                },
                RemoveSpec {
                    path: "~/.local/bin/claude",
                    expect: Expect::SymlinkIntoRoot,
                    what: RemovedWhat::Launcher,
                    optional: false,
                },
            ])),
            keep: &[],
            globs: &[],
        };
        let removal = plan_removal(&job).expect("a plan");
        assert_eq!(
            removal.warnings,
            vec![
                trash("~/.local/bin/claude", RemovedWhat::Launcher),
                keep("~/.cache/thing", KeptWhat::NotOurs),
            ]
        );
    }

    #[test]
    fn test_plan_removal_still_refuses_an_optional_path_that_is_not_yours_or_overlaps_a_kept_one() {
        // The skip is for "not ours", never for "not yours" or "would take
        // what stays": those two refuse for an optional path exactly as for
        // a required one.
        let home = TempHome::new("removal-optional-owner");
        let _layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        let d = Detected {
            euid: detected(home.path()).euid + 1,
            ..detected(home.path())
        };
        let job = Job {
            recipe: &CLAUDE,
            detected: d,
            remove: only(RemoveSpec {
                path: "~/.claude/downloads",
                expect: Expect::Dir,
                what: RemovedWhat::Cache,
                optional: true,
            }),
            keep: &[],
            globs: &[],
        };
        let (path, reason) = refused(plan_removal(&job));
        assert_eq!(
            (path.as_str(), reason),
            ("~/.claude/downloads", UninstallUnsafeReason::NotOwnedByYou)
        );

        let home = TempHome::new("removal-optional-overlap");
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        // The kept settings file is a link into the optional cache folder.
        home.link(".claude.json", &home.path().join(".claude/downloads"));
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            ("~/.claude.json", UninstallUnsafeReason::OverlapsKept)
        );
        assert!(layout.root.is_dir());
    }

    #[test]
    fn test_plan_removal_moves_backup_files_a_pattern_names_before_the_launcher() {
        // Check 5 (spec §6.3): a regular file in the pattern's folder named
        // prefix+something+suffix is moved, listed as a backup, before the
        // last listed path; a link of that name and a name without the
        // middle are not matches. Name order, so the preview is stable.
        let home = TempHome::new("removal-globs");
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        home.file(".local/bin/claude.1727000000.old");
        home.file(".local/bin/claude.1726000000.old");
        home.file(".local/bin/claude.old");
        home.link(".local/bin/claude.9.old", &layout.real);
        let d = detected(home.path());
        let (remove, keep) = claude_lists();
        let job = Job {
            recipe: &CLAUDE,
            detected: d,
            remove,
            keep,
            globs: only_glob(Glob {
                dir: "~/.local/bin",
                prefix: "claude.",
                suffix: ".old",
                what: RemovedWhat::Backups,
            }),
        };

        let removal = plan_removal(&job).expect("a plan");

        assert_eq!(
            removal.paths,
            vec![
                home.path().join(".local/share/claude"),
                home.path().join(".claude/downloads"),
                home.path().join(".local/bin/claude.1726000000.old"),
                home.path().join(".local/bin/claude.1727000000.old"),
                layout.launcher.clone(),
            ]
        );
        assert_eq!(removal.identities[2].kind, ItemKind::File);
        assert_eq!(
            removal.warnings[2],
            trash("~/.local/bin/claude.1726000000.old", RemovedWhat::Backups)
        );
        assert_eq!(
            removal.warnings[4],
            trash("~/.local/bin/claude", RemovedWhat::Launcher)
        );
        // With the launcher as the only listed path, the backups still come
        // before it.
        let job = Job {
            remove: only(RemoveSpec {
                path: "~/.local/bin/claude",
                expect: Expect::SymlinkIntoRoot,
                what: RemovedWhat::Launcher,
                optional: false,
            }),
            ..job
        };
        let removal = plan_removal(&job).expect("a plan");
        assert_eq!(removal.paths.last(), Some(&layout.launcher));
        assert_eq!(removal.paths.len(), 3);
    }

    #[test]
    fn test_plan_removal_lists_a_kept_path_outside_the_home_folder_as_a_sentence_only() {
        // Ruling 6: grok's installer may leave `/usr/local/bin/grok`, a link
        // into `~/.grok/downloads`. It is reported (it becomes a dead link),
        // never protected -- as a kept path it would refuse the very
        // uninstall it exists for (`OverlapsKept`) -- and reported only when
        // it is a link into this tool's root: the same path may be
        // Homebrew's live link into its Caskroom (an Intel Mac), another
        // CLI's `agent`, or a plain file, and "a dead link you can delete"
        // would then be a false sentence. Stood in for by paths under
        // another temp directory, since a test cannot write to
        // /usr/local/bin.
        let outside = TempHome::new("removal-outside-keep");
        let home = TempHome::new("removal-outside-keep-home");
        let layout = claude_layout(&home, "2.1.281");
        let fallback = outside.link("bin/claude", &layout.real);
        let caskroom = outside.executable("Caskroom/claude-code/2.1.281/claude");
        let brews = outside.link("bin/claude-brew", &caskroom);
        let plain = outside.file("bin/claude-file");
        let (remove, _) = claude_lists();
        let job_for = |kept: &Path| Job {
            recipe: &CLAUDE,
            detected: detected(home.path()),
            remove,
            keep: only_keep(kept.display().to_string(), KeptWhat::OutsideHome),
            globs: &[],
        };

        // A link into the root: reported, after the moves.
        let removal = plan_removal(&job_for(&fallback)).expect("a plan, not OverlapsKept");
        assert_eq!(
            removal.warnings,
            vec![
                trash("~/.local/share/claude", RemovedWhat::Program),
                trash("~/.local/bin/claude", RemovedWhat::Launcher),
                Warning::WillKeep {
                    path: fallback.display().to_string(),
                    what: KeptWhat::OutsideHome,
                },
            ]
        );
        // A link elsewhere (Homebrew's), a regular file, and nothing at all:
        // no sentence, and no refusal.
        for (what, kept) in [("a link elsewhere", &brews), ("a regular file", &plain)] {
            let removal = plan_removal(&job_for(kept)).expect("a plan");
            assert_eq!(removal.warnings.len(), 2, "{what}: {:?}", removal.warnings);
        }
        std::fs::remove_file(&fallback).unwrap();
        let removal = plan_removal(&job_for(&fallback)).expect("a plan");
        assert_eq!(removal.warnings.len(), 2);
    }

    #[test]
    fn test_points_into_reads_a_links_target_resolved_or_by_its_own_text() {
        // The one question `outside_home_keeps` asks: is this a symbolic
        // link into the root? Resolved when it resolves; by its own text,
        // folded from its folder, when it dangles (the second run of a
        // stopped uninstall, `downloads/` already in the Trash); never for
        // a file or a folder.
        let home = TempHome::new("removal-points-into");
        let root = home.dir(".grok");
        let real = home.executable(".grok/downloads/grok-1.0.41-macos-aarch64");
        let outside = TempHome::new("removal-points-into-outside");
        let resolving = outside.link("bin/grok", &real);
        let dangling = outside.link("bin/grok-gone", &home.path().join(".grok/downloads/grok-1.0.40-macos-aarch64"));
        let relative_dangling = outside.link(
            "bin/grok-rel",
            Path::new(&format!("../../{}/.grok/bin/grok", home.path().file_name().unwrap().to_str().unwrap())),
        );
        let elsewhere = outside.link("bin/other", &outside.executable("Caskroom/x/grok"));
        let file = outside.file("bin/file");
        let folder = outside.dir("bin/folder");
        assert!(points_into(&resolving, &root));
        assert!(points_into(&dangling, &root));
        // `../../<home-name>/.grok/bin/grok` from `<outside>/bin`: the two
        // temp homes are siblings, so the text lands under the root.
        assert!(points_into(&relative_dangling, &root), "{relative_dangling:?}");
        assert!(!points_into(&elsewhere, &root));
        assert!(!points_into(&file, &root));
        assert!(!points_into(&folder, &root));
        assert!(!points_into(&outside.path().join("bin/missing"), &root));
    }

    #[tokio::test]
    async fn test_execute_removal_moves_a_backup_the_preview_listed_and_stops_when_one_appears_after_it() {
        // Check 5 at run time: the backup the preview listed is moved in its
        // place; a second backup appearing between the preview and the
        // click makes the fresh list differ, and nothing moves.
        let home = TempHome::new("removal-exec-globs");
        let layout = claude_layout(&home, "2.1.281");
        home.file(".local/bin/claude.1727000000.old");
        let (remove, keep) = claude_lists();
        let job = Job {
            recipe: &CLAUDE,
            detected: detected(home.path()),
            remove,
            keep,
            globs: only_glob(Glob {
                dir: "~/.local/bin",
                prefix: "claude.",
                suffix: ".old",
                what: RemovedWhat::Backups,
            }),
        };
        let preview = plan_removal(&job).unwrap();
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();

        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(mock.calls(), preview.paths);
        assert_eq!(mock.kinds()[1], ItemKind::File, "the backup, as a file");

        let home = TempHome::new("removal-exec-glob-appeared");
        let _layout = claude_layout(&home, "2.1.281");
        home.file(".local/bin/claude.1727000000.old");
        let job = Job {
            detected: detected(home.path()),
            ..job
        };
        let preview = plan_removal(&job).unwrap();
        home.file(".local/bin/claude.1727000600.old");
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();

        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.local/bin/claude.1727000600.old"));
        assert!(mock.calls().is_empty());
    }
```

(`Job` derives `Clone`, so `..job` struct-update works; `path_changed(path)` is C's helper building `CanagerFailed(PathChanged { path })`; `MockTrasher::kinds()` is C's. `TempHome::new` creates its directory under `std::env::temp_dir()` and canonicalises it (`mod.rs:717-727`), so two homes are siblings and `home.path().file_name()` is the leaf — which is what the relative-text case of `test_points_into_…` relies on.)

(e) Cut C's `test_plan_removal_refuses_a_path_reached_through_a_linked_folder_inside_home` down to its second half. Its first half (`removal-linked-cache-parent`: `~/.claude -> ~/Documents`, expecting `~/.claude/downloads` refused as `NotWhatInstructionsExpect`) describes an *optional* path, which this task keeps instead; that case now lives in `test_plan_removal_keeps_an_optional_path_whose_folder_leads_elsewhere` above with the opposite expectation. The launcher half still refuses (the launcher is never optional). Replace the whole test, from its `#[test]` through its closing `}`, with:

```rust
    #[test]
    fn test_plan_removal_refuses_a_launcher_reached_through_a_linked_folder_inside_home() {
        // Ruling 24 of the step C plan: every folder between the home
        // folder and a listed path must be a real folder -- inside the home
        // folder or not -- so a launcher whose `~/.local/bin` is kept as a
        // link to a dotfiles folder is refused. (C's other half, an
        // optional `~/.claude/downloads` behind a linked `~/.claude`, is
        // kept and said since step D: see
        // `test_plan_removal_keeps_an_optional_path_whose_folder_leads_elsewhere`.)
        let home = TempHome::new("removal-linked-bin-inside");
        let real = home.executable(".local/share/claude/versions/2.1.281");
        let dotfiles_bin = home.dir("dotfiles/bin");
        std::os::unix::fs::symlink(real, dotfiles_bin.join("claude")).unwrap();
        home.link(".local/bin", &dotfiles_bin);
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            (
                "~/.local/bin/claude",
                UninstallUnsafeReason::NotWhatInstructionsExpect
            )
        );
    }
```

In `crates/canager-core/src/adapters/standalone/recipes.rs`, inside `mod tests`, replace C's `test_every_uninstall_path_is_under_home_and_not_in_a_shared_folder` and `test_every_paths_recipe_moves_its_launcher_last_and_lists_no_path_inside_another`, and E's `test_a_paths_recipe_names_only_home_paths`, each from its `#[test]` through its closing `}`, with:

```rust
    /// The kept paths of a `Paths` recipe that must live under home (every
    /// one but the report-only `OutsideHome` ones, ruling 6 of the step D
    /// plan).
    fn home_keeps(keep: &'static [KeepSpec]) -> impl Iterator<Item = &'static str> {
        keep.iter()
            .filter(|spec| spec.what != KeptWhat::OutsideHome)
            .map(|spec| spec.path)
    }

    #[test]
    fn test_every_uninstall_path_is_under_home_and_not_in_a_shared_folder() {
        // `route::expand` panics on a path that does not start with `~/`,
        // and the removal's check 1 refuses a path whose folder is the home
        // folder or one of `SHARED_FOLDERS` -- a recipe listing one would
        // refuse every uninstall, and the never-list exists so no recipe
        // can quietly move `~/.local/bin` whole. The one exception is a
        // kept path outside the home folder, which is never expanded:
        // absolute, never `~/`, never under a user's home.
        for recipe in RECIPES {
            let (remove, keep) = path_lists(recipe);
            let Some(Uninstall::Paths { keep: keep_specs, .. }) = &recipe.uninstall else {
                continue;
            };
            for path in remove.iter().copied().chain(home_keeps(keep_specs)) {
                let rest = path
                    .strip_prefix("~/")
                    .unwrap_or_else(|| panic!("{}: {path:?} must start with ~/", recipe.id));
                assert!(
                    !rest.is_empty() && !rest.ends_with('/') && !rest.contains("..") && !rest.contains("/./"),
                    "{}: {path:?} must name one plain path",
                    recipe.id
                );
            }
            for spec in keep_specs.iter().filter(|spec| spec.what == KeptWhat::OutsideHome) {
                assert!(
                    spec.path.starts_with('/') && !spec.path.starts_with("/Users/"),
                    "{}: an OutsideHome keep names an absolute path outside every home, got {:?}",
                    recipe.id,
                    spec.path
                );
            }
            assert_eq!(keep.len(), keep_specs.len());
            for path in &remove {
                let folder = Path::new(path.strip_prefix("~/").unwrap())
                    .parent()
                    .unwrap_or(Path::new(""));
                assert!(
                    folder != Path::new("")
                        && !SHARED_FOLDERS.iter().any(|shared| folder == Path::new(shared)),
                    "{}: {path:?} sits directly in the home folder or in a shared folder; check 1 would refuse it",
                    recipe.id
                );
            }
        }
    }

    #[test]
    fn test_a_paths_recipe_names_only_home_paths() {
        // The path-list uninstall (`removal.rs`) expands its recipe's route
        // and every remove/keep spec with B's two-argument `route::expand`,
        // which knows `~/` and nothing else -- except the report-only
        // `OutsideHome` keeps, which it never expands. A `$CARGO_HOME` tool
        // (rustup) uninstalls with its own command.
        for recipe in RECIPES {
            let Some(Uninstall::Paths { remove, keep }) = &recipe.uninstall else {
                continue;
            };
            let mut paths = vec![recipe.route.launcher, recipe.route.root];
            paths.extend(remove.iter().map(|spec| spec.path));
            paths.extend(home_keeps(keep));
            paths.extend(recipe.backup_globs.iter().map(|glob| glob.dir));
            for path in paths {
                assert!(
                    path.starts_with("~/"),
                    "{}: a Paths recipe may only name ~/ paths, got {path:?}",
                    recipe.id
                );
            }
        }
    }

    #[test]
    fn test_every_paths_recipe_moves_its_launcher_last_and_lists_no_path_inside_another() {
        // Spec §6.2: the launcher itself last (claude, agy, grok's
        // `~/.grok/bin/grok`; never a folder holding it, ruling 4 of the
        // step D plan), so a run that stops partway leaves exactly the
        // launcher-only state a second run finishes -- and what it is must
        // match the route: a link for `SymlinkIntoRoot`, a file for
        // `FlatFile`. Spec §6.3's former check 7: no removed path is inside
        // another removed path, and no kept path is inside a removed one
        // -- properties of the constant, not of the Mac.
        for recipe in RECIPES {
            let Some(Uninstall::Paths { remove, keep }) = &recipe.uninstall else {
                continue;
            };
            let last = remove
                .last()
                .unwrap_or_else(|| panic!("{}: an empty remove list", recipe.id));
            assert_eq!(
                last.path, recipe.route.launcher,
                "{}: the last path must be the launcher",
                recipe.id
            );
            assert_eq!(last.what, RemovedWhat::Launcher, "{}", recipe.id);
            assert!(!last.optional, "{}: the launcher is never optional", recipe.id);
            let expected = match recipe.route.kind {
                RouteKind::SymlinkIntoRoot => Expect::SymlinkIntoRoot,
                RouteKind::FlatFile => Expect::File,
            };
            assert_eq!(last.expect, expected, "{}", recipe.id);
            let removed: Vec<&str> = remove.iter().map(|spec| spec.path).collect();
            for a in &removed {
                for b in &removed {
                    assert!(
                        a == b || !b.starts_with(&format!("{a}/")),
                        "{}: {b:?} is inside {a:?}",
                        recipe.id
                    );
                }
                for kept in home_keeps(keep) {
                    assert!(
                        kept != *a && !kept.starts_with(&format!("{a}/")),
                        "{}: kept {kept:?} is inside removed {a:?}",
                        recipe.id
                    );
                }
            }
        }
    }
```

(C's `path_lists(recipe)` helper stays and is used as before; `KeptWhat` and `RouteKind` reach the module through C's `use crate::model::{KeptWhat, RemovedWhat};` and B's `use super::*;`.)

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p canager-core --lib adapters::standalone`
Expected: FAIL to compile — `struct \`Job\` has no field named \`globs\``; `no variant or associated item named \`File\` found for enum \`Expect\``; `cannot find function \`points_into\``; `no variant or associated item named \`NotOurs\`` is *not* raised (Task 2 added it); `unresolved import \`crate::scan::Glob\`` is *not* raised (Task 3 added it).

- [ ] **Step 3: Write `Expect::File`, the items, the skip and the outside-home report**

In `crates/canager-core/src/adapters/standalone/recipe.rs`:

(a) In `pub enum Expect`, after the `Dir,` variant add:

```rust
    /// A regular file, not a link: Antigravity's launcher (`~/.local/bin/agy`,
    /// the whole program), grok's fish completion file, and every backup
    /// copy a `Glob` matches (`removal::listed_items`).
    File,
```

and in the enum's doc comment delete the sentence `Only the kinds Claude Code's list has exist in this step; \`File\` (Antigravity's launcher, a plain executable) arrives with step D.`

(b) Replace `KeepSpec`'s doc comment (`/// One path a path-list uninstall leaves alone, named in the preview so` … `/// when it exists.`) with:

```rust
/// One path a path-list uninstall leaves alone, named in the preview so
/// the user knows their settings stay (`Warning::WillKeep`); listed only
/// when it exists. `path` is `~/…` and protected by the checks
/// (`removal::kept_places`, `disturbed`) -- except when `what` is
/// `KeptWhat::OutsideHome`: then it is an absolute path outside the home
/// folder (`/usr/local/bin/grok`), reported when it exists and never
/// protected (`removal::outside_home_keeps`): a fallback link *into* the
/// program folder would otherwise refuse the uninstall it exists for
/// (`recipes::tests` hold the two spellings apart).
```

In `crates/canager-core/src/adapters/standalone/removal.rs`:

(c) Add `use crate::scan::Glob;` to the imports, and add `KeptWhat` and `RemovedWhat` to the `use crate::model::{…}` list (C imports `ItemIdentity, ItemKind, UninstallUnsafeReason, Warning` and, from stage 6d, `Fault` and `Outcome`; keep those).

(d) In `pub struct Job`, after `pub keep: &'static [KeepSpec],` add:

```rust
    /// The recipe's backup-file patterns (`Recipe.backup_globs`): check 5.
    pub globs: &'static [Glob],
```

and in `Job`'s doc comment change `and the recipe's two lists` to `and the recipe's two lists and its backup-file patterns`.

(e) After `identity_of`'s closing `}` (before `/// One look at the disk`), insert:

```rust

/// One thing the list may move, as the checks see it: a listed
/// `RemoveSpec`, or a backup file one of the recipe's `backup_globs`
/// matched (check 5). `rel` is how the recipe spells it under the home
/// folder -- for a match, the pattern's folder joined with the file's name
/// (`.local/bin/agy.1727000000.old`) -- which the ancestry rule compares
/// against; `path` where it is. Built by `listed_items`; read by
/// `plan_removal` and `take_turn`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Item {
    rel: PathBuf,
    path: PathBuf,
    expect: Expect,
    what: RemovedWhat,
    optional: bool,
}

/// The list as it stands on the disk now, in execution order: every listed
/// path, with the backup files the recipe's patterns match (`Job.globs`,
/// spec §6.3 check 5) placed before the last listed path -- the launcher
/// -- so it still goes last (spec §6.2). A
/// match is a regular file (never a link) directly in the pattern's folder
/// whose name is prefix + something + suffix (`Glob::matches_name`), in
/// name order so the preview is stable; a folder that cannot be read
/// matches nothing (the launcher's own check speaks for that folder).
/// Every match is optional: it may be gone by its turn, and one Canager
/// cannot confirm is the tool's is kept and said, like an optional listed
/// path.
fn listed_items(job: &Job) -> Vec<Item> {
    let home = job.detected.home.as_path();
    let listed = |spec: &RemoveSpec| Item {
        rel: spelled(spec.path).to_path_buf(),
        path: route::expand(home, spec.path),
        expect: spec.expect,
        what: spec.what,
        optional: spec.optional,
    };
    let Some((last, before)) = job.remove.split_last() else {
        return Vec::new();
    };
    let mut items: Vec<Item> = before.iter().map(listed).collect();
    for glob in job.globs {
        let dir = glob.dir_under(home);
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut names: Vec<String> = read
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| glob.matches_name(name))
            .filter(|name| {
                std::fs::symlink_metadata(dir.join(name))
                    .is_ok_and(|meta| meta.file_type().is_file())
            })
            .collect();
        names.sort();
        items.extend(names.into_iter().map(|name| Item {
            rel: spelled(glob.dir).join(&name),
            path: dir.join(&name),
            expect: Expect::File,
            what: glob.what,
            optional: true,
        }));
    }
    items.push(listed(last));
    items
}

/// Which of a check's refusals mean, for an *optional* path, "not this
/// install's, leave it" rather than "stop": the wrong shape, a link
/// elsewhere or a folder on the way that is a link
/// (`NotWhatInstructionsExpect`), and a folder that leads out of the home
/// folder or into a shared one (`OutsideHome`, `SharedFolder`) -- all
/// places Canager will not move from, none of them a reason to leave the
/// tool uninstallable (spec §十三 #27). `NotOwnedByYou` and `OverlapsKept`
/// still stop the whole list: they are about what the move would do, not
/// about whose the path is. Read by `plan_removal`.
fn keeps_instead(reason: UninstallUnsafeReason) -> bool {
    matches!(
        reason,
        UninstallUnsafeReason::NotWhatInstructionsExpect
            | UninstallUnsafeReason::OutsideHome
            | UninstallUnsafeReason::SharedFolder
    )
}

/// The kept paths outside the home folder that are this tool's links --
/// grok's installer may put `/usr/local/bin/grok` and `/usr/local/bin/agent`
/// there when `~/.grok/bin` is not on PATH (grok.md §2) -- each as a
/// `WillKeep` sentence: Canager never moves anything outside the home
/// folder (spec §6.3), so after the uninstall they are dead links the user
/// can delete. Only a link *into the root* (`points_into`) gets the
/// sentence: the same path may be Homebrew's live link into its Caskroom
/// (an Intel Mac's `/usr/local`), another CLI's `agent`, or a file, and
/// "a dead link you can delete" would then be false. Spelled absolute in
/// the recipe (`recipes::tests` hold them to `/`, never `~/`), so nothing
/// to expand; never protected (`kept_places` skips them: a link into the
/// program folder would otherwise refuse the uninstall). Read by
/// `plan_removal`.
fn outside_home_keeps(look: &Look<'_>) -> Vec<Warning> {
    look.job
        .keep
        .iter()
        .filter(|spec| spec.what == KeptWhat::OutsideHome)
        .filter(|spec| points_into(Path::new(spec.path), &look.root))
        .map(|spec| Warning::WillKeep {
            path: spec.path.to_string(),
            what: KeptWhat::OutsideHome,
        })
        .collect()
}

/// Whether `link` is a symbolic link whose target lies under `root` (the
/// recipe's root as expanded for this home): where it resolves when it
/// resolves, or -- dangling, as on the second run of a stopped uninstall
/// whose `downloads/` is already in the Trash -- where its own text points,
/// folded from the link's folder without touching the disk
/// (`route::lexical_join`), compared against the root as spelled and as
/// canonical. Anything that is not a symbolic link, or is not there, is
/// not the installer's fallback link: `false`. Read by `outside_home_keeps`.
fn points_into(link: &Path, root: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(link) else {
        return false;
    };
    if !meta.file_type().is_symlink() {
        return false;
    }
    let canonical_root = std::fs::canonicalize(root).ok();
    match std::fs::canonicalize(link) {
        Ok(real) => canonical_root.is_some_and(|root| real.starts_with(root)),
        Err(_) => {
            let Ok(text) = std::fs::read_link(link) else {
                return false;
            };
            let folder = link.parent().unwrap_or(Path::new("/"));
            let named = route::lexical_join(folder, &text);
            named.starts_with(root) || canonical_root.is_some_and(|root| named.starts_with(root))
        }
    }
}
```

(f) In `kept_places`, at the top of `for spec in look.job.keep {`, before `let path = route::expand(home, spec.path);`, insert:

```rust
        // Reported, not protected (`outside_home_keeps`): absolute, outside
        // the home folder, and possibly a link *into* the program folder,
        // which `disturbed` would otherwise call `OverlapsKept`.
        if spec.what == KeptWhat::OutsideHome {
            continue;
        }
```

(g) Change `check_item`'s signature and its first lines. C's

```rust
fn check_item(
    look: &Look<'_>,
    kept: &[Kept],
    spec: &'static RemoveSpec,
    path: &Path,
) -> Result<ItemIdentity, Refusal> {
    use UninstallUnsafeReason::{
        Missing, NotOwnedByYou, NotWhatInstructionsExpect, OutsideHome, OverlapsKept, SharedFolder,
    };
    let refuse = |reason| Refusal::new(path, reason);
    let rel = spelled(spec.path);
```

becomes

```rust
fn check_item(
    look: &Look<'_>,
    kept: &[Kept],
    rel: &Path,
    expect: Expect,
    path: &Path,
) -> Result<ItemIdentity, Refusal> {
    use UninstallUnsafeReason::{
        Missing, NotOwnedByYou, NotWhatInstructionsExpect, OutsideHome, OverlapsKept, SharedFolder,
    };
    let refuse = |reason| Refusal::new(path, reason);
```

and, further down in the same function, `if spec.expect == Expect::SymlinkIntoRoot` becomes `if expect == Expect::SymlinkIntoRoot`, and

```rust
    let expected = match spec.expect {
        Expect::Dir => ItemKind::Dir,
        Expect::SymlinkIntoRoot => ItemKind::Symlink,
    };
```

becomes

```rust
    let expected = match expect {
        Expect::Dir => ItemKind::Dir,
        Expect::SymlinkIntoRoot => ItemKind::Symlink,
        Expect::File => ItemKind::File,
    };
```

In `check_item`'s doc comment, change the parenthesis in the last bullet — `a real directory for \`Dir\`, a link for \`SymlinkIntoRoot\`` — to `a real directory for \`Dir\`, a link for \`SymlinkIntoRoot\`, a regular file for \`File\``, and add a first sentence: `\`rel\` is the recipe's spelling of the path under the home folder (a glob match's is its pattern's folder plus its name), \`expect\` what must be there.`

(h) Replace `plan_removal` (from its doc comment `/// Spec §6.3 on every path the recipe lists (check 5, the backup-file` through its closing `}`) with:

```rust
/// Spec §6.3 on every item of the list -- the recipe's paths and, before
/// the last of them, the backup files its patterns match (check 5,
/// `listed_items`) -- then the kept paths. Check 2 (is it there?) goes
/// first, because the others need something to look at: a missing optional
/// item is skipped, a missing program directory of a launcher-only install
/// is `AlreadyGone` (re-probed from the disk now, ruling 4 of the step C
/// plan), anything else missing refuses. Then `check_item`; an optional
/// item it cannot confirm is the tool's is kept and said (`WillKeep {
/// NotOurs }`, after the moves; `keeps_instead`). Any other failure refuses
/// the whole list with nothing moved (`AdapterError::UninstallUnsafe`, one
/// of six reasons); an empty list is a plain `Refused` (unreachable while
/// the launcher is listed and the row exists), and so is a home folder
/// that cannot be resolved. The kept paths come last: the recipe's under
/// the home folder, then the ones outside it (`outside_home_keeps`).
pub fn plan_removal(job: &Job) -> Result<Removal, AdapterError> {
    let home = job.detected.home.as_path();
    let look = Look::new(job).map_err(|e| {
        AdapterError::Refused(format!(
            "cannot resolve the home folder {}: {e}",
            home.display()
        ))
    })?;
    let kept = kept_places(&look).map_err(|refusal| refusal.into_error(home))?;
    let launcher_only =
        route::probe(job.recipe.route.kind, &look.launcher, &look.root) == Probe::LauncherOnly;

    let mut paths = Vec::new();
    let mut identities = Vec::new();
    let mut warnings = Vec::new();
    let mut not_ours = Vec::new();
    for item in listed_items(job) {
        // Check 2: is it there? `lstat`, so a dangling launcher counts.
        match std::fs::symlink_metadata(&item.path) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if item.optional {
                    continue;
                }
                if launcher_only && item.path != look.launcher {
                    warnings.push(Warning::AlreadyGone {
                        path: shown(home, &item.path),
                    });
                    continue;
                }
                return Err(
                    Refusal::new(&item.path, UninstallUnsafeReason::Missing).into_error(home),
                );
            }
            // Unreadable (a permission error, a loop): not something the
            // instructions describe, and not something to move blind -- an
            // optional one is left where it is and said.
            Err(_) if item.optional => {
                not_ours.push(Warning::WillKeep {
                    path: shown(home, &item.path),
                    what: KeptWhat::NotOurs,
                });
                continue;
            }
            Err(_) => {
                return Err(
                    Refusal::new(&item.path, UninstallUnsafeReason::NotWhatInstructionsExpect)
                        .into_error(home),
                )
            }
        }
        match check_item(&look, &kept, &item.rel, item.expect, &item.path) {
            Ok(identity) => {
                warnings.push(Warning::WillTrash {
                    path: shown(home, &item.path),
                    what: item.what,
                });
                identities.push(identity);
                paths.push(item.path);
            }
            // An optional path that is there but not, as far as Canager can
            // tell, this install's (spec §6.3 check 4, §十三 #27): kept, and
            // said after the moves. Never the launcher, which is never
            // optional.
            Err(refusal) if item.optional && keeps_instead(refusal.reason) => {
                not_ours.push(Warning::WillKeep {
                    path: shown(home, &item.path),
                    what: KeptWhat::NotOurs,
                });
            }
            Err(refusal) => return Err(refusal.into_error(home)),
        }
    }
    if paths.is_empty() {
        return Err(AdapterError::Refused(format!(
            "{}: nothing on the uninstall list is there to move",
            job.recipe.id
        )));
    }
    warnings.extend(not_ours);
    warnings.extend(kept.iter().map(|kept| Warning::WillKeep {
        path: shown(home, &kept.path),
        what: kept.spec.what,
    }));
    warnings.extend(outside_home_keeps(&look));
    Ok(Removal {
        paths,
        identities,
        warnings,
    })
}
```

(i) In `take_turn`, replace C's lookup

```rust
    let home = job.detected.home.as_path();
    let Some(spec) = job
        .remove
        .iter()
        .find(|spec| route::expand(home, spec.path) == path)
    else {
        return Turn::Changed(path.to_path_buf());
    };
```

with

```rust
    // The item as the list stands now (`listed_items`: a listed path, or a
    // backup a pattern matches); one that is no longer listed -- a backup
    // that vanished by its turn -- is not what the preview saw.
    let Some(item) = listed_items(job).into_iter().find(|item| item.path == path) else {
        return Turn::Changed(path.to_path_buf());
    };
```

and its check call `check_item(&look, &kept, spec, path)` with `check_item(&look, &kept, &item.rel, item.expect, path)`. (`home` is no longer used in `take_turn` before `Look::new`; delete the `let home = …` line if nothing else in the function reads it — `-D warnings` says.)

In `crates/canager-core/src/adapters/standalone/mod.rs`, in `plan`'s `Uninstall::Paths { remove, keep } => { let removal = removal::plan_removal(&removal::Job { … }) }` literal and in `execute`'s `removal::execute_removal(&removal::Job { … }, …)` literal, add `globs: self.recipe.backup_globs,` after `keep,` in each.

(j) In `docs/what-we-run.md`, in the `## Claude Code` section's check paragraph (the one beginning `Before the preview is shown every listed path is checked`), two sentences become false with this commit, since `~/.claude/downloads` is an optional path. Replace

```
folder between the home folder and the path must be a real folder, not
a link — so a `~/.local/bin` kept as a link to a dotfiles folder
refuses the uninstall, and so does a `~/.claude` that is a link when
the download cache is inside it; the path must belong to the user
```

with

```
folder between the home folder and the path must be a real folder, not
a link — so a `~/.local/bin` kept as a link to a dotfiles folder
refuses the uninstall, while a `~/.claude` that is a link leaves the
download cache inside it where it is, and the preview says so; the path
must belong to the user
```

and replace

```
along (of `~/.claude`, only `downloads` lies inside it, as listed). If
any check fails, the whole uninstall is refused, in the user's language,
and nothing is moved. The preview also records what each path is — its
```

with

```
along (of `~/.claude`, only `downloads` lies inside it, as listed). If a
check fails on a path the list requires, the whole uninstall is refused,
in the user's language, and nothing is moved; an optional path that is
there but that Canager cannot confirm is the tool's — the wrong kind of
thing, a link elsewhere, a folder on the way that is a link — stays,
and the preview lists it among what is kept. Not yours, or would take a
kept path along, refuses whether the path is optional or not. The
preview also records what each path is — its
```

(Hard-wrapped prose: match by the words, keep the wrapping style. `test_what_we_run_states_the_trash_call_and_the_pause_after_each_move` and its neighbours fold whitespace before comparing, so the re-wrap is free.)

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p canager-core --lib adapters::standalone` and `cargo test -p canager-core --test what_we_run_test`
Expected: PASS — C's removal tests except the two this task changed (`…_refuses_an_optional_path_of_the_wrong_shape` replaced by a keep, `…_refuses_a_path_reached_through_a_linked_folder_inside_home` cut down to its launcher half; every other C test is unchanged in behaviour, since no recipe has globs or an outside keep yet), the seven new removal tests (including `test_points_into_…`), the three rewritten recipes tests, B's and E's route/recipes/adapter tests; A's document tests over the re-wrapped Claude Code paragraph. `cargo test -p canager-core --test standalone_uninstall_test`: PASS, unchanged (Claude Code's list has no optional path of the wrong shape in those layouts).

- [ ] **Step 5: Format, gates, commit**

Run `cargo fmt --all`, then the five gates. Expected: all clean.

```bash
git add crates/canager-core/src/adapters/standalone/recipe.rs crates/canager-core/src/adapters/standalone/removal.rs crates/canager-core/src/adapters/standalone/mod.rs crates/canager-core/src/adapters/standalone/recipes.rs docs/what-we-run.md
git commit -m "$(cat <<'EOF'
Teach the path-list uninstall backups, regular files and what is not its own

Check 5: a recipe's backup-file patterns are matched on the disk and each
match is moved before the launcher, as a file. Expect::File, for a
launcher that is the whole program and for completion files. An optional
path Canager cannot confirm is the tool's -- the wrong shape, a link
elsewhere, a folder on the way that is a link, a place Canager never
moves from -- is kept and said instead of refusing the uninstall a
stranger's file should not be able to block; not yours and would-take-
what-stays still refuse, and the Claude Code section of the trust file
now says which is which. A kept path outside the home folder is
reported, never protected, since it may be a link into what moves -- and
reported only when it is a link into the tool's root, so Homebrew's
/usr/local/bin/grok on an Intel Mac is never called a dead link. The
launcher-last invariant now also pins what the launcher is, a link or a
file. No shipped recipe uses any of this yet; the two that do land with
the next commit.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

### Task 5: The two recipes and the adapter — `HttpJsonField`, `Command`, `upgrade: Option`, `arch`, `AGY`, `GROK`, their meta, the host (one commit, stages 5a–5d)

One commit, built in stages, because every new shape here has its only producer in the two recipe constants and its only reader in the adapter, and the fixture-set test is untouched until Task 6 registers them (B's Task 3 and E's Task 6 are the precedent). Stage 5a: the shapes and the adapter; 5b: the meta files, the two constants and their invariants tests; 5c: the adapter tests on synthetic layouts; 5d: format, gates, commit.

**Files:**
- Modify: `crates/canager-core/src/adapters/standalone/recipe.rs` — `Latest::{HttpJsonField, Command}`, `Recipe.upgrade: Option<UpgradeCmd>`, module doc
- Modify: `crates/canager-core/src/adapters/standalone/latest.rs` — `parse_json_field`, `UpdateCheck`, `parse_update_check`, `MANIFEST_VERIFIED_ARCHES`, `manifest_arch_allowed`; tests
- Modify: `crates/canager-core/src/adapters/standalone/mod.rs` — `StandaloneAdapter.arch`, `new`, `with_arch`, `Published`, `published` (replacing `latest_version`), `check_updates`, `plan`'s Upgrade arm; `testing::{AgyLayout, agy_layout, GrokLayout, grok_layout}`; tests
- Modify: `crates/canager-core/src/adapters/standalone/recipes.rs` — `AGY`, `GROK`; `CLAUDE`/`RUSTUP` `upgrade: Some(…)`; `test_every_recipe_latest_url_is_an_allowed_https_host`'s arms; new tests
- Create: `adapters/meta/standalone-agy.toml`, `adapters/meta/standalone-grok.toml`
- Modify: `crates/canager-core/src/http/real.rs` — `ALLOWED_HTTPS_HOSTS`, its doc
- Modify: `docs/what-we-run.md` — one row of the network table (A's test wants the host named in the same commit as the constant)
- Test: `cargo test -p canager-core --lib adapters::standalone`, `cargo test -p canager-core --test what_we_run_test`

**Interfaces:**
- Consumes: B's `read_version`, `artifact_key`, `check_updates`, `latest_version` (with E's `HttpTomlVersion` arm), `uncheckable_candidate`, `compare_dotted`, `is_dotted_version`, `parse_version`; E's `seated_detected_for`, `locks`, `RouteKind::FlatFile`, `VersionParse::SecondToken`, `no_extra_locks`, `Detected`; C's `Uninstall::Paths`, `RemoveSpec`, `KeepSpec`, `Expect` (+ `File`, Task 4), `MockTrasher`, `with_trash_gap`, `reconcile_after_uninstall`; `Glob` and `Recipe.backup_globs` (Task 3); `UpdateBlocked::SelfUpdatesOnly` (Task 1); `RemovedWhat::Backups`, the five `KeptWhat`s (Task 2); `MockRunner`, `MockHttpClient`, `RecordingRunner`, `TempHome`.
- Produces (verbatim): `Latest::HttpJsonField { url: &'static str, field: &'static str }` and `Latest::Command { args: &'static [&'static str], timeout_secs: u64, latest_field: &'static str, available_field: &'static str }` (reader: `published`; producers `AGY`, `GROK`); `Recipe.upgrade: Option<UpgradeCmd>` (readers: `check_updates`'s `blocked`, `plan(Upgrade)`; producers: `AGY` with `None`, every other recipe with `Some`); `pub fn parse_json_field(body: &str, field: &str) -> Result<String, String>`, `pub struct UpdateCheck { pub latest: String, pub available: bool }`, `pub fn parse_update_check(stdout: &str, latest_field: &str, available_field: &str) -> Result<UpdateCheck, String>`, `pub const MANIFEST_VERIFIED_ARCHES: [&str; 1]`, `pub fn manifest_arch_allowed(arch: &str) -> Result<(), String>` (reader: `published`); `StandaloneAdapter.arch: &'static str` (writer: `new`, from `std::env::consts::ARCH`; reader: `published`), `pub fn with_arch(self, arch: &'static str) -> StandaloneAdapter` (readers: the tests); private `enum Published { Version(String), ToolSays(latest::UpdateCheck) }` and `async fn published(&self, launcher: &Path) -> Result<Published, String>` (reader: `check_updates`); `pub static AGY: Recipe`, `pub static GROK: Recipe` (readers: `RECIPES` in Task 6, the tests here and in Task 6); `adapters/meta/standalone-agy.toml`, `adapters/meta/standalone-grok.toml` (readers: the two `meta_toml`s; A's `what_we_run_test` once registered); `"antigravity-cli-auto-updater-974169037036.us-central1.run.app"` in `ALLOWED_HTTPS_HOSTS` (readers: A's `host_allowed` in `send`, the doc row, A's `test_what_we_run_names_every_allowed_https_host`, `test_every_recipe_latest_url_is_an_allowed_https_host` once `AGY` is in `RECIPES`); `testing::{AgyLayout { launcher, root }, agy_layout(home), GrokLayout { launcher, agent, root, real }, grok_layout(home, version)}` (readers: the tests here and Task 6's).

#### Stage 5a: the shapes and the adapter

- [ ] **Step 1: Write the failing tests**

In `crates/canager-core/src/adapters/standalone/latest.rs`, inside `mod tests`, append before the module's closing `}`:

```rust

    #[test]
    fn test_parse_json_field_reads_agys_manifest_version_and_nothing_else() {
        // agy.md §4, VERIFIED live 2026-09-24: the manifest is one JSON
        // object with `version`, `url`, `sha512`. Only `version` is read;
        // the other two are the installer's business.
        let manifest = r#"{"version":"1.2.9","url":"https://storage.googleapis.com/antigravity-public/antigravity-cli/1.2.9-5905287731871744/darwin-arm/cli_mac_arm64.tar.gz","sha512":"8a96"}"#;
        assert_eq!(
            parse_json_field(manifest, "version"),
            Ok("1.2.9".to_string())
        );
        assert_eq!(
            parse_json_field(r#"{ "version" : " 1.2.10 " }"#, "version"),
            Ok("1.2.10".to_string())
        );
        for (body, needle) in [
            ("", "not JSON"),
            ("<html>Sign in</html>", "not JSON"),
            (r#"{"url":"x"}"#, "no `version` string"),
            (r#"{"version":12}"#, "no `version` string"),
            (r#"{"version":"latest"}"#, "not a version"),
            (r#"{"version":"1.2.9-beta"}"#, "not a version"),
        ] {
            let err = parse_json_field(body, "version").expect_err(body);
            assert!(err.contains(needle), "{body:?}: {err}");
            assert!(err.len() < 120, "the reason stays short: {err}");
        }
    }

    #[test]
    fn test_parse_update_check_reads_groks_answer_and_nothing_else() {
        // grok.md §3, VERIFIED on this Mac: `grok update --check --json`
        // prints one JSON object. Only the three fields the recipe names are
        // read; `latest` is taken as printed, since it is never compared;
        // a non-null `error` makes the whole answer a failure, since
        // `updateAvailable: false` beside an error is "could not check",
        // never "up to date" (ruling 10).
        let answer = r#"{"currentVersion":"1.0.41","latestVersion":"1.0.41","updateAvailable":false,"installer":"internal","channel":"stable","autoUpdate":true,"error":null}"#;
        assert_eq!(
            parse_update_check(answer, "latestVersion", "updateAvailable", Some("error")),
            Ok(UpdateCheck {
                latest: "1.0.41".to_string(),
                available: false,
            })
        );
        assert_eq!(
            parse_update_check(
                r#"{"latestVersion":"1.0.42-alpha.1","updateAvailable":true}"#,
                "latestVersion",
                "updateAvailable",
                Some("error")
            ),
            Ok(UpdateCheck {
                latest: "1.0.42-alpha.1".to_string(),
                available: true,
            })
        );
        // No error field named: the key is not looked at.
        assert!(parse_update_check(
            r#"{"latestVersion":"1.0.41","updateAvailable":false,"error":"ignored"}"#,
            "latestVersion",
            "updateAvailable",
            None
        )
        .is_ok());
        for (body, needle) in [
            ("", "did not print JSON"),
            ("Checking for updates...\n", "did not print JSON"),
            (r#"{"updateAvailable":true}"#, "no `latestVersion` string"),
            (r#"{"latestVersion":"1.0.42"}"#, "no `updateAvailable` boolean"),
            (r#"{"latestVersion":"1.0.42","updateAvailable":"yes"}"#, "no `updateAvailable` boolean"),
            (r#"{"latestVersion":"","updateAvailable":true}"#, "is empty"),
            (
                r#"{"latestVersion":"1.0.41","updateAvailable":false,"error":"network unreachable"}"#,
                "reported: network unreachable",
            ),
            (
                r#"{"latestVersion":"1.0.41","updateAvailable":false,"error":{"code":7}}"#,
                r#"reported: {"code":7}"#,
            ),
        ] {
            let err = parse_update_check(body, "latestVersion", "updateAvailable", Some("error"))
                .expect_err(body);
            assert!(err.contains(needle), "{body:?}: {err}");
            assert!(err.len() < 140, "the reason stays short: {err}");
        }
    }

    #[test]
    fn test_manifest_arch_allowed_only_on_apple_silicon() {
        // Spec §3.1/§3.5: only the darwin_arm64 manifest was fetched; an
        // Intel Mac, or a universal build under Rosetta (which reports
        // x86_64), gets "could not check" with the reason, not a request
        // to an unverified URL. `std::env::consts::ARCH` spells it
        // `aarch64`, never `arm64`.
        assert_eq!(MANIFEST_VERIFIED_ARCHES, ["aarch64"]);
        assert_eq!(manifest_arch_allowed("aarch64"), Ok(()));
        for arch in ["x86_64", "arm64", ""] {
            let err = manifest_arch_allowed(arch).expect_err(arch);
            assert!(err.contains("Intel"), "{arch:?}: {err}");
            assert!(err.contains(arch), "{arch:?}: {err}");
        }
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p canager-core --lib adapters::standalone::latest`
Expected: FAIL to compile — `cannot find function \`parse_json_field\``, `\`parse_update_check\``, `\`manifest_arch_allowed\``; `cannot find struct, variant or union type \`UpdateCheck\``; `cannot find value \`MANIFEST_VERIFIED_ARCHES\``.

- [ ] **Step 3: Write the shapes and the adapter**

In `crates/canager-core/src/adapters/standalone/latest.rs`, after `parse_release_stable_toml`'s closing `}` (E's; before `#[cfg(test)]`), insert:

```rust

/// The version a JSON manifest names in its top-level `field` -- agy's
/// `manifests/darwin_arm64.json` answers `{"version":"1.2.9","url":…,
/// "sha512":…}` (agy.md §4, VERIFIED live) -- trimmed; `Err` with a short
/// reason for a body that is not JSON, has no such string, or names
/// something that is not a dotted version. The reason becomes an
/// uncheckable row's description, so it quotes at most a few characters
/// of the body, never a page of HTML.
pub fn parse_json_field(body: &str, field: &str) -> Result<String, String> {
    let shown = || -> String { body.trim().chars().take(40).collect() };
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|_| format!("the manifest is not JSON (got {:?})", shown()))?;
    let version = value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("the manifest has no `{field}` string (got {:?})", shown()))?
        .trim();
    if is_dotted_version(version) {
        Ok(version.to_string())
    } else {
        Err(format!(
            "the manifest's `{field}` is not a version (got {version:?})"
        ))
    }
}

/// What a tool's own read-only update check answered (grok's `update
/// --check --json`, grok.md §3, VERIFIED on this Mac): the newest version
/// it knows of, and whether it calls that an update. Read by
/// `StandaloneAdapter::check_updates`, which trusts `available` and never
/// compares (spec §4.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateCheck {
    pub latest: String,
    pub available: bool,
}

/// `UpdateCheck` out of the check command's stdout: a JSON object whose
/// `latest_field` is a non-empty string, whose `available_field` is a
/// boolean, and whose `error_field` (when the recipe names one) is absent
/// or `null` -- `{"currentVersion":"1.0.41","latestVersion":"1.0.41",
/// "updateAvailable":false,…,"error":null}`. `Err` with a short reason
/// otherwise; a non-null error is quoted, since `updateAvailable: false`
/// beside it means the tool could not find out, not that nothing is newer
/// (ruling 10). The tool's `latest` is taken as it is, suffix and all: it
/// is shown, not compared.
pub fn parse_update_check(
    stdout: &str,
    latest_field: &str,
    available_field: &str,
    error_field: Option<&str>,
) -> Result<UpdateCheck, String> {
    let shown = || -> String { stdout.trim().chars().take(40).collect() };
    let value: serde_json::Value = serde_json::from_str(stdout)
        .map_err(|_| format!("the update check did not print JSON (got {:?})", shown()))?;
    if let Some(error) = error_field.and_then(|field| value.get(field)) {
        if !error.is_null() {
            let text = match error.as_str() {
                Some(text) => text.trim().to_string(),
                None => error.to_string(),
            };
            let text: String = text.chars().take(80).collect();
            return Err(format!("the update check reported: {text}"));
        }
    }
    let latest = value
        .get(latest_field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            format!(
                "the update check has no `{latest_field}` string (got {:?})",
                shown()
            )
        })?
        .trim()
        .to_string();
    let available = value
        .get(available_field)
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| {
            format!(
                "the update check has no `{available_field}` boolean (got {:?})",
                shown()
            )
        })?;
    if latest.is_empty() {
        return Err(format!("the update check's `{latest_field}` is empty"));
    }
    Ok(UpdateCheck { latest, available })
}

/// The CPU architectures a `Latest::HttpJsonField` manifest URL has been
/// verified for: agy's `darwin_arm64.json` was fetched live (agy.md §4);
/// the `darwin_amd64.json` the install script's `${os}_${arch}` rule
/// implies was not (spec §3.5, §十一). Spelled as `std::env::consts::ARCH`
/// spells it -- `aarch64`, never `arm64`.
pub const MANIFEST_VERIFIED_ARCHES: [&str; 1] = ["aarch64"];

/// `Ok` when a manifest lookup may be made on `arch`; otherwise the reason
/// the row says "could not check" -- an Intel Mac, or a universal build
/// under Rosetta, which reports `x86_64`: the safe direction, no request
/// to an unverified URL. Read by `StandaloneAdapter::published`.
pub fn manifest_arch_allowed(arch: &str) -> Result<(), String> {
    if MANIFEST_VERIFIED_ARCHES.contains(&arch) {
        Ok(())
    } else {
        Err(format!(
            "not yet verified on Intel Macs: this Canager runs as {arch:?}, and the manifest URL is verified for Apple silicon only"
        ))
    }
}
```

In `crates/canager-core/src/adapters/standalone/recipe.rs`:

(a) In `pub enum Latest`, after E's `HttpTomlVersion { url: &'static str },` arm add:

```rust
    /// `GET url`, a JSON object whose top-level `field` is the newest
    /// version (agy's `manifests/darwin_arm64.json`, VERIFIED live, agy.md
    /// §4). Made only on the architectures `latest::MANIFEST_VERIFIED_ARCHES`
    /// names: on an Intel Mac, or under Rosetta, the row is "could not
    /// check" with the reason and no request is sent (spec §3.1; the amd64
    /// manifest is §十一's). Read by `StandaloneAdapter::published`
    /// (`latest::parse_json_field`).
    HttpJsonField {
        url: &'static str,
        field: &'static str,
    },
    /// `<launcher> args`, whose stdout is a JSON object: `latest_field` the
    /// newest version, `available_field` whether the tool calls it an
    /// update -- trusted as answered, never compared (spec §4.3) -- and
    /// `error_field`, when the tool has one, the key whose non-null value
    /// means the check itself failed (grok's `"error":null` on success,
    /// grok.md §3): then the row is "could not check" with that text, never
    /// "up to date". Only a subcommand whose own `--help` says it installs
    /// nothing may be named here (grok's `update --check --json`: "Check for
    /// updates without installing", grok.md §4;
    /// `recipes::tests::test_every_command_latest_source_only_checks`),
    /// since it runs on every refresh. Read by `StandaloneAdapter::published`
    /// (`latest::parse_update_check`).
    Command {
        args: &'static [&'static str],
        timeout_secs: u64,
        latest_field: &'static str,
        available_field: &'static str,
        error_field: Option<&'static str>,
    },
```

(b) Replace the `upgrade` field and its doc (`/// The tool's own documented update command. Read by \`plan(Upgrade)\`.` / `pub upgrade: UpgradeCmd,`) with:

```rust
    /// The tool's own documented update command, or `None` for a tool that
    /// installs its updates itself and offers nothing Canager may run
    /// (agy: `agy update` is undocumented, takes no options and has never
    /// been run, agy.md §4). `None` puts `UpdateBlocked::SelfUpdatesOnly`
    /// on every update candidate the recipe produces and makes
    /// `plan(Upgrade)` refuse with the same reason (spec §4.4, D5). Read by
    /// `check_updates` and `plan(Upgrade)`.
    pub upgrade: Option<UpgradeCmd>,
```

(c) In the module doc, change `the other \`Latest\` sources and an optional \`upgrade\` (agy updates itself only)` to `the two \`Latest\` sources a manifest and a tool's own check need, and an optional \`upgrade\` (agy updates itself only)`.

In `crates/canager-core/src/adapters/standalone/recipes.rs`, in `CLAUDE` and in `RUSTUP`, wrap the `upgrade` value: `upgrade: UpgradeCmd { … },` becomes `upgrade: Some(UpgradeCmd { … }),`. The same in every other `Recipe {` literal (C's `NO_UNINSTALL` in `mod.rs`'s tests; the compiler names them: `expected \`Option<UpgradeCmd>\`, found \`UpgradeCmd\``). In `test_claude_updates_with_its_own_updater`, the assertions on `CLAUDE.upgrade.args`/`timeout_secs`/`cancel` become `let upgrade = CLAUDE.upgrade.as_ref().expect("claude has an update command");` followed by the same three assertions on `upgrade.…`. E's rustup test that reads `RUSTUP.upgrade` gets the same `as_ref().expect(…)`.

In `crates/canager-core/src/adapters/standalone/mod.rs`:

(d) In `pub struct StandaloneAdapter`, after `detected: Mutex<Option<Detected>>,` add:

```rust
    /// The CPU architecture this Canager runs as (`std::env::consts::ARCH`;
    /// `with_arch` in tests): a `Latest::HttpJsonField` manifest is fetched
    /// only on the architectures it was verified for
    /// (`latest::manifest_arch_allowed`). Read by `published`.
    arch: &'static str,
```

In `new`, add `arch: std::env::consts::ARCH,` to the `StandaloneAdapter { … }` literal, and after `with_trash_gap`'s closing `}` add:

```rust

    /// Test seam: the architecture `published` believes it runs on, so
    /// both the Apple-silicon and the Intel branch of a manifest lookup
    /// are tested on whatever machine runs the tests.
    pub fn with_arch(mut self, arch: &'static str) -> StandaloneAdapter {
        self.arch = arch;
        self
    }
```

(e) At module level, directly above `impl StandaloneAdapter {` (after `pub struct StandaloneAdapter { … }`'s closing `}`), insert:

```rust
/// What the world knows about this tool's newest version, per the recipe's
/// `Latest` (`StandaloneAdapter::published`).
enum Published {
    /// A version to compare with the installed one (a channel pointer, a
    /// release file, a manifest).
    Version(String),
    /// The tool's own verdict (grok's `update --check --json`): shown as
    /// answered, never compared (spec §4.3).
    ToolSays(latest::UpdateCheck),
}
```

(f) Replace `check_updates`'s body from `let remote = match self.latest_version().await {` to the end of the function (B's `Ok(match latest::compare_dotted(&current, &remote) { … }.into())`) with:

```rust
        let decided: Result<Option<(String, UpdateChannel)>, String> =
            match self.published(&inst.exe_path).await {
                Err(reason) => Err(reason),
                // The tool's own verdict, as answered (spec §4.3).
                Ok(Published::ToolSays(check)) => {
                    Ok(check.available.then_some((check.latest, UpdateChannel::Native)))
                }
                Ok(Published::Version(remote)) => match latest::compare_dotted(&current, &remote) {
                    Some(Ordering::Less) => Ok(Some((remote, UpdateChannel::Registry))),
                    Some(Ordering::Equal | Ordering::Greater) => Ok(None),
                    None => Err(format!(
                        "cannot compare the installed version {current:?} with the published {remote:?}"
                    )),
                },
            };
        Ok(match decided {
            Err(reason) => vec![uncheckable_candidate(
                key,
                current,
                UpdateChannel::Registry,
                reason,
            )],
            Ok(None) => Vec::new(),
            Ok(Some((target, channel))) => vec![UpdateCandidate {
                key,
                current,
                target,
                channel,
                checkable: true,
                warnings: Vec::new(),
                // A tool with no update command Canager may run: the newer
                // version is real and has no button (spec §4.4, D5 item 4).
                blocked: self
                    .recipe
                    .upgrade
                    .is_none()
                    .then_some(UpdateBlocked::SelfUpdatesOnly),
            }],
        }
        .into())
```

and add `UpdateBlocked` to the `use crate::model::{…}` list. In `check_updates`'s doc comment, add after the sentence ending `never hidden behind a setting.`: `A recipe with no \`upgrade\` (agy) gets its candidate with \`UpdateBlocked::SelfUpdatesOnly\`: no button, a badge, and a sentence saying to open the tool. A \`Latest::Command\` recipe (grok) is asked itself and believed: \`updateAvailable\` decides, and \`latestVersion\` is shown as printed (\`Published::ToolSays\`).`

(g) Rename `latest_version` to `published`, change its signature to `async fn published(&self, launcher: &Path) -> Result<Published, String>`, its doc comment to:

```rust
    /// What the world knows about this tool's newest version, per the
    /// recipe's `Latest`: a version to compare with the installed one
    /// (`Published::Version`), or the tool's own verdict
    /// (`Published::ToolSays`, grok). `Err` is the one-line reason of an
    /// uncheckable row.
```

keep B's `Latest::ClaudeChannel { base } => { … }` arm and E's `Latest::HttpTomlVersion { url } => { … }` arm exactly as they are except that each arm's final expression -- `latest::parse_channel_body(&resp.body)` and `latest::parse_release_stable_toml(&resp.body)` -- gains `.map(Published::Version)`, and add two arms:

```rust
            Latest::HttpJsonField { url, field } => {
                // Only where the manifest URL was verified (Apple silicon);
                // elsewhere the row says so and nothing is sent.
                latest::manifest_arch_allowed(self.arch)?;
                let resp = self
                    .http
                    .send(HttpRequest {
                        method: "GET",
                        url: url.to_string(),
                        headers: Vec::new(),
                        timeout: Duration::from_secs(30),
                    })
                    .await
                    .map_err(|e| format!("request to {url} failed: {e}"))?;
                if resp.status != 200 {
                    return Err(format!("{url} returned status {}", resp.status));
                }
                latest::parse_json_field(&resp.body, field).map(Published::Version)
            }
            Latest::Command {
                args,
                timeout_secs,
                latest_field,
                available_field,
                error_field,
            } => {
                // The tool's own read-only check, against the launcher, with
                // no environment of Canager's (spec §3.4's variables are for
                // the version read).
                let shown = args.join(" ");
                let output = self
                    .runner
                    .run(
                        CommandSpec {
                            program: launcher.to_path_buf(),
                            args: args.iter().map(|a| a.to_string()).collect(),
                            env: Vec::new(),
                            cwd: None,
                            timeout: Duration::from_secs(timeout_secs),
                            output_use: OutputUse::Parsed,
                        },
                        None,
                        CancellationToken::new(),
                    )
                    .await
                    .map_err(|e| format!("could not run `{shown}`: {e}"))?;
                if output.timed_out || output.cancelled {
                    return Err(format!("`{shown}` did not finish within {timeout_secs} s"));
                }
                if output.exit_code != Some(0) {
                    return Err(format!(
                        "`{shown}` exited with {:?}: {}",
                        output.exit_code,
                        output.stderr.lines().next().unwrap_or("").trim()
                    ));
                }
                latest::parse_update_check(
                    &output.stdout,
                    latest_field,
                    available_field,
                    error_field,
                )
                .map(Published::ToolSays)
            }
```

(h) In `plan`'s `OpKind::Upgrade => { … }` arm (E's, beginning `let detected = self.seated_detected_for(inst)?;`), replace `let upgrade = &self.recipe.upgrade;` with:

```rust
                // A tool that installs its updates itself and offers nothing
                // Canager may run (agy): the gate refuses this first
                // (`blocked_upgrade`, from the candidate's `blocked`); this is
                // its late twin for a stale snapshot (spec §五).
                let Some(upgrade) = &self.recipe.upgrade else {
                    return Err(AdapterError::UpdateBlocked {
                        reason: UpdateBlocked::SelfUpdatesOnly,
                    });
                };
```

In `crates/canager-core/src/adapters/standalone/recipes.rs`, in `test_every_recipe_latest_url_is_an_allowed_https_host`, add two arms to the `match recipe.latest { … }` after E's `Latest::HttpTomlVersion { url } => vec![url.to_string()],`:

```rust
                Latest::HttpJsonField { url, .. } => vec![url.to_string()],
                // The tool's own command makes its own connection, under its
                // own configuration (docs/what-we-run.md, the network
                // section's last paragraph): no host of Canager's.
                Latest::Command { .. } => Vec::new(),
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p canager-core --lib adapters::standalone`
Expected: PASS — the three new `latest` tests; every B/C/E test as before (`check_updates`'s comparison path is unchanged in behaviour; the `Option` wrap is mechanical). `cargo build -p canager-core` clean: `with_arch`, `Published::ToolSays` and the two new arms are `pub` or matched, so no dead-code warning even before a recipe produces them (the gates run at 5d).

- [ ] **Step 5: Continue the task**

No commit: continue to stage 5b.

#### Stage 5b: the meta files, `AGY`, `GROK`, the host, their invariants

- [ ] **Step 1: Write the failing tests**

In `crates/canager-core/src/adapters/standalone/recipes.rs`, inside `mod tests`, append before the module's closing `}`:

```rust

    #[test]
    fn test_agy_is_a_flat_file_read_with_its_auto_update_off_that_updates_itself() {
        // Spec §3.4/§3.5's agy column, and agy.md §2/§4 (VERIFIED on this
        // Mac): a regular Mach-O file at `~/.local/bin/agy`, root
        // `~/.gemini/antigravity-cli`; `--version` prints one bare version
        // and is read with Google's documented updater switch.
        assert_eq!(AGY.id, "agy");
        assert_eq!(AGY.route.kind, RouteKind::FlatFile);
        assert_eq!(AGY.route.launcher, "~/.local/bin/agy");
        assert_eq!(AGY.route.root, "~/.gemini/antigravity-cli");
        assert_eq!(AGY.version.args, &["--version"]);
        assert_eq!(AGY.version.env, &[("AGY_CLI_DISABLE_AUTO_UPDATE", "true")]);
        assert_eq!(AGY.version.parse, VersionParse::FirstToken);
        assert!(AGY.self_updates);
        assert_eq!(
            AGY.latest,
            Latest::HttpJsonField {
                url: "https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/darwin_arm64.json",
                field: "version",
            }
        );
        let meta = AdapterMeta::from_toml(AGY.meta_toml).expect("meta");
        assert_eq!(meta.name, "Antigravity CLI");
        assert_eq!(meta.homepage, "https://antigravity.google/docs/cli/install/");
    }

    #[test]
    fn test_agy_has_no_update_command_and_a_one_path_uninstall_with_a_backup_pattern() {
        // Spec §4.4 D5 item 4: `agy update` is undocumented and unrun, so no
        // upgrade -- every candidate is SelfUpdatesOnly. Spec §6.3's agy row
        // as this plan rules it (rulings 1 and 2): the launcher is the whole
        // program and goes alone, the updater's `.old` copies go before it,
        // the root and the staging folder stay and are said.
        assert!(AGY.upgrade.is_none());
        let Some(Uninstall::Paths { remove, keep }) = &AGY.uninstall else {
            panic!("agy has a path list");
        };
        let remove: Vec<(&str, Expect, RemovedWhat, bool)> = remove
            .iter()
            .map(|spec| (spec.path, spec.expect, spec.what, spec.optional))
            .collect();
        assert_eq!(
            remove,
            vec![("~/.local/bin/agy", Expect::File, RemovedWhat::Launcher, false)]
        );
        let keep: Vec<(&str, KeptWhat)> = keep.iter().map(|spec| (spec.path, spec.what)).collect();
        assert_eq!(
            keep,
            vec![
                ("~/.gemini/antigravity-cli", KeptWhat::ToolState),
                ("~/.cache/antigravity", KeptWhat::InstallerCache),
                ("~/.zshrc", KeptWhat::ShellConfigLines),
                ("~/.zprofile", KeptWhat::ShellConfigLines),
            ]
        );
        assert_eq!(
            AGY.backup_globs,
            &[Glob {
                dir: "~/.local/bin",
                prefix: "agy.",
                suffix: ".old",
                what: RemovedWhat::Backups,
            }]
        );
    }

    #[test]
    fn test_grok_is_a_relative_link_route_read_with_the_second_token_that_asks_itself_for_updates() {
        // Spec §3.5's grok column, grok.md §1/§3/§4 (VERIFIED on this Mac):
        // `~/.grok/bin/grok -> ../downloads/grok-<v>-macos-aarch64`, a
        // relative link into `~/.grok`; `grok --version` prints
        // `grok 1.0.41 (4220f3b224a6)`; `update --check --json` is its own
        // read-only check ("without installing"); `grok update` is the
        // documented upgrade; whether it installs updates on its own is
        // UNVERIFIED, so it is not called self-updating (spec §4.4).
        assert_eq!(GROK.id, "grok");
        assert_eq!(GROK.route.kind, RouteKind::SymlinkIntoRoot);
        assert_eq!(GROK.route.launcher, "~/.grok/bin/grok");
        assert_eq!(GROK.route.root, "~/.grok");
        assert_eq!(GROK.version.args, &["--version"]);
        assert!(GROK.version.env.is_empty());
        assert_eq!(GROK.version.parse, VersionParse::SecondToken);
        assert!(!GROK.self_updates);
        assert_eq!(
            GROK.latest,
            Latest::Command {
                args: &["update", "--check", "--json"],
                timeout_secs: 60,
                latest_field: "latestVersion",
                available_field: "updateAvailable",
                error_field: Some("error"),
            }
        );
        let upgrade = GROK.upgrade.as_ref().expect("grok has an update command");
        assert_eq!(upgrade.args, &["update"]);
        assert_eq!(upgrade.timeout_secs, 1800);
        assert_eq!(upgrade.cancel, CancelPolicy::KillThenReconcile);
        assert!(GROK.backup_globs.is_empty());
        let meta = AdapterMeta::from_toml(GROK.meta_toml).expect("meta");
        assert_eq!(meta.name, "Grok Build");
        assert_eq!(meta.homepage, "https://x.ai/build");
    }

    #[test]
    fn test_grok_uninstall_moves_its_own_fallback_links_first_and_its_launcher_link_last() {
        // Spec §6.3's grok row, in this plan's order (rulings 3 and 4): the
        // two optional fallback links first (their link text is unverified,
        // so they go while every folder it could pass through is still
        // there), the program folders, the fish completion, then the two
        // links the installer put in `~/.grok/bin` -- `agent`, and `grok`
        // itself last -- never the folder, which may hold the user's own
        // scripts (it is on PATH). `~/.grok` itself stays with its
        // settings, login, sessions and memory; the shell file stays; a
        // fallback link in /usr/local/bin is reported, never touched.
        let Some(Uninstall::Paths { remove, keep }) = &GROK.uninstall else {
            panic!("grok has a path list");
        };
        let remove: Vec<(&str, Expect, RemovedWhat, bool)> = remove
            .iter()
            .map(|spec| (spec.path, spec.expect, spec.what, spec.optional))
            .collect();
        assert_eq!(
            remove,
            vec![
                ("~/.local/bin/grok", Expect::SymlinkIntoRoot, RemovedWhat::Launcher, true),
                ("~/.local/bin/agent", Expect::SymlinkIntoRoot, RemovedWhat::Launcher, true),
                ("~/.grok/downloads", Expect::Dir, RemovedWhat::Program, false),
                ("~/.grok/bundled", Expect::Dir, RemovedWhat::Program, true),
                ("~/.grok/completions", Expect::Dir, RemovedWhat::Program, true),
                ("~/.config/fish/completions/grok.fish", Expect::File, RemovedWhat::Program, true),
                ("~/.grok/bin/agent", Expect::SymlinkIntoRoot, RemovedWhat::Launcher, true),
                ("~/.grok/bin/grok", Expect::SymlinkIntoRoot, RemovedWhat::Launcher, false),
            ]
        );
        let keep: Vec<(&str, KeptWhat)> = keep.iter().map(|spec| (spec.path, spec.what)).collect();
        assert_eq!(
            keep,
            vec![
                ("~/.grok", KeptWhat::SettingsAndHistory),
                ("~/.zshrc", KeptWhat::ShellConfigLines),
                ("/usr/local/bin/grok", KeptWhat::OutsideHome),
                ("/usr/local/bin/agent", KeptWhat::OutsideHome),
            ]
        );
    }

    #[test]
    fn test_every_command_latest_source_only_checks() {
        // Spec §3.1: a `Latest::Command` may name only a subcommand whose own
        // --help says it installs nothing -- grok's `update --check --json`
        // ("Check for updates without installing", grok.md §4). It runs on
        // every refresh; `update` without `--check` would be an upgrade.
        for recipe in RECIPES.iter().chain([&&AGY, &&GROK]) {
            if let Latest::Command {
                args, timeout_secs, ..
            } = recipe.latest
            {
                assert!(
                    args.contains(&"--check"),
                    "{}: {args:?} must carry --check",
                    recipe.id
                );
                assert!(
                    args.contains(&"--json"),
                    "{}: {args:?} must ask for machine-readable output",
                    recipe.id
                );
                assert!(timeout_secs <= 120, "{}: a check is not an install", recipe.id);
            }
        }
    }
```

(The `.chain([&&AGY, &&GROK])` reaches the two constants before Task 6 puts them in `RECIPES`; Task 6 removes the chain. `Glob`, `KeptWhat`, `RemovedWhat`, `Expect`, `Uninstall`, `CancelPolicy`, `AdapterMeta`, `RouteKind`, `VersionParse`, `Latest` are already imported in that module by B, C, E and Task 3.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p canager-core --lib adapters::standalone::recipes`
Expected: FAIL to compile — `cannot find value \`AGY\` in this scope`, `cannot find value \`GROK\``.

- [ ] **Step 3: Write the meta files, the constants and the host**

Create `adapters/meta/standalone-agy.toml`:

```toml
schema_version = 1
id = "standalone-agy"
name = "Antigravity CLI"
kind = "standalone"
platforms = ["macos"]
homepage = "https://antigravity.google/docs/cli/install/"
verified_versions = ["1.2.10"]
```

Create `adapters/meta/standalone-grok.toml`:

```toml
schema_version = 1
id = "standalone-grok"
name = "Grok Build"
kind = "standalone"
platforms = ["macos"]
homepage = "https://x.ai/build"
verified_versions = ["1.0.41"]
```

(`kind = "standalone"` is documentary, as for claude. Each `verified_versions` is what Task 6 records; if the recording day's version differs — agy self-updates, and this Mac's `agy` binary changed again on 2026-09-25 — Task 6 changes the line to match.)

In `crates/canager-core/src/adapters/standalone/recipes.rs`, add `Glob` (Task 3 did) and make sure `Expect, KeepSpec, RemoveSpec, Uninstall, no_extra_locks` are in the `use super::recipe::{…}` list and `KeptWhat, RemovedWhat` in the `use crate::model::{…}` list, then after `CLAUDE` (before E's `RUSTUP`) add:

```rust

/// Antigravity CLI (`agy`), Google's terminal agent, installed by its own
/// script (`curl -fsSL https://antigravity.google/cli/install.sh | bash`,
/// run by the user; Canager never runs it).
///
/// Every value here is from `.superpowers/phase4/agy.md` (VERIFIED on this
/// Mac, in the install script read in full, or in Google's own
/// documentation, 2026-09-24, unless noted) and from the recording in
/// `adapters/fixtures/standalone-agy/<version>/`:
/// - the launcher `~/.local/bin/agy` is a regular Mach-O file (176 MB on
///   this Mac), the whole program; the installer copies it there
///   (`TARGET_DIR=$HOME/.local/bin`, `BINARY_PATH=$TARGET_DIR/agy`, §3a).
///   The root `~/.gemini/antigravity-cli` holds its conversations, logs,
///   cache, builtin skills and updater state together (§2); `~/.gemini`
///   itself is shared with Gemini CLI and is never touched. The Homebrew
///   cask's `agy` is a link into its Caskroom and is Homebrew's row (§3b);
/// - `agy --version` prints one bare version (`1.2.9`, §4). It is read with
///   `AGY_CLI_DISABLE_AUTO_UPDATE=true`, the switch Google documents for
///   its background updater (§4, doc text). On 1.2.10 `--version` did not
///   reach the updater at all -- no new log file, `update_status.json`
///   untouched, no updater process (spec §3.4, §十三 #10; the fixture
///   README records the same observation for the recorded version) -- so
///   the switch is a belt on top; a run with a prompt is what spawns the
///   updater (§4);
/// - the newest published version is the `version` of the JSON manifest
///   the installer and the updater both read,
///   `…/manifests/darwin_arm64.json` (§3a, §4; VERIFIED live). Only that
///   file was fetched, so the lookup is made on Apple silicon only
///   (`latest::manifest_arch_allowed`); the amd64 manifest is spec §十一's;
/// - it installs its updates itself, in the background, at most every 15
///   minutes (§4: the documented debounce and this Mac's own log), so
///   `self_updates` is true and, since `agy update` is undocumented, takes
///   no options and has never been run (§4), there is no `upgrade`: every
///   newer version is `UpdateBlocked::SelfUpdatesOnly` -- a badge, no
///   button, and a sentence saying to open it once (spec §4.4, D5, Q3);
/// - there is no vendor uninstall document and no `agy uninstall` (§5).
///   The list is the installer's own path plus the cask's `zap` (which
///   trashes only `~/.gemini/antigravity-cli`), and the fixture README says
///   so: the launcher goes -- alone, since it is the program -- after any
///   `agy.<time>.old` its updater left beside it (`backup_globs`, spec
///   §3.5). Kept, and said when present: the root (`ToolState`: no vendor
///   list says which of its folders could go alone, and the cask treats it
///   as one, spec §十三 #24), `~/.cache/antigravity` (the installer's
///   staging folder, directly in `~/.cache`, which check 1 never moves
///   from -- kept and said rather than excepted, phase 4 step D plan ruling
///   1), and the two shell files the installer added its PATH line to
///   (`# Added by Antigravity CLI installer`, §2).
pub static AGY: Recipe = Recipe {
    id: "agy",
    meta_toml: include_str!("../../../../../adapters/meta/standalone-agy.toml"),
    route: Route {
        kind: RouteKind::FlatFile,
        launcher: "~/.local/bin/agy",
        root: "~/.gemini/antigravity-cli",
    },
    version: VersionCmd {
        args: &["--version"],
        env: &[("AGY_CLI_DISABLE_AUTO_UPDATE", "true")],
        parse: VersionParse::FirstToken,
    },
    latest: Latest::HttpJsonField {
        url: "https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/darwin_arm64.json",
        field: "version",
    },
    self_updates: true,
    upgrade: None,
    uninstall: Some(Uninstall::Paths {
        remove: &[RemoveSpec {
            path: "~/.local/bin/agy",
            expect: Expect::File,
            what: RemovedWhat::Launcher,
            optional: false,
        }],
        keep: &[
            KeepSpec {
                path: "~/.gemini/antigravity-cli",
                what: KeptWhat::ToolState,
            },
            KeepSpec {
                path: "~/.cache/antigravity",
                what: KeptWhat::InstallerCache,
            },
            KeepSpec {
                path: "~/.zshrc",
                what: KeptWhat::ShellConfigLines,
            },
            KeepSpec {
                path: "~/.zprofile",
                what: KeptWhat::ShellConfigLines,
            },
        ],
    }),
    extra_locks: no_extra_locks,
    backup_globs: &[Glob {
        dir: "~/.local/bin",
        prefix: "agy.",
        suffix: ".old",
        what: RemovedWhat::Backups,
    }],
};

/// Grok Build (`grok`), xAI's terminal agent, installed by its own script
/// (`curl -fsSL https://x.ai/cli/install.sh | bash`, run by the user;
/// Canager never runs it).
///
/// Every value here is from `.superpowers/phase4/grok.md` (VERIFIED on
/// this Mac, in the install script read in full, or in the README the tool
/// ships, 2026-09-24, unless noted) and from the recording in
/// `adapters/fixtures/standalone-grok/<version>/`:
/// - the launcher `~/.grok/bin/grok` is a *relative* symbolic link,
///   `../downloads/grok-<version>-macos-aarch64`, into the root `~/.grok`;
///   `bin/agent` is a second link to the same file (§1, §2). Old downloads
///   stay in `downloads/` after an update (three on this Mac, ~400 MB).
///   The Homebrew cask `grok-build` puts its links in `/opt/homebrew/bin`
///   and is Homebrew's row; the Homebrew *formula* named `grok` is an
///   unrelated regex library (§7);
/// - `grok --version` prints `grok 1.0.41 (4220f3b224a6)` (§1): the second
///   token, no environment (none is documented; whether `--version` runs
///   grok's launch-time updater, and whether that updater installs or only
///   checks, are both UNVERIFIED, §5 -- the fixture README records what
///   `~/.grok/version.json`'s mtime, the `bin/` links and `downloads/` did
///   around the recorded read, and the recording stops if a link or
///   `downloads/` changed);
/// - the newest published version is asked of grok itself: `update --check
///   --json`, whose `--help` says "Check for updates without installing"
///   (§3, §4; run on this Mac) and which prints `{"currentVersion":…,
///   "latestVersion":…,"updateAvailable":…,"error":null}`.
///   `updateAvailable` is believed and `latestVersion` shown (spec §4.3);
///   a non-null `error` makes the row "could not check" with that text
///   (ruling 10); 60 s. grok's own check records its time in
///   `~/.grok/version.json` (`checked_at`), the one write on the Mac a
///   Canager refresh causes -- grok's, not Canager's; the trust file says
///   so;
/// - `auto_update = true` in its config means "check for updates on
///   launch" (§5); whether it *installs* one is UNVERIFIED, so the row is
///   not called self-updating (spec §4.4, §十三 #25);
/// - `grok update` is the documented upgrade (§4): a new file in
///   `downloads/` and a re-pointed link, the old binary left in place.
///   1800 s, `KillThenReconcile`. How it behaves with its input closed
///   has not been observed (spec §五): the author records it on a CI
///   runner before this step merges (the step D plan's "pre-merge
///   verification");
/// - there is no `grok uninstall` and no vendor uninstall document (§6);
///   the de-facto `rm -rf ~/.grok` would take the login, sessions and
///   memory, which spec Q4 keeps. The list is the README's "File
///   Locations" table plus the install script: the two optional fallback
///   links the installer makes when `~/.grok/bin` is not on PATH *first*
///   (their link text is UNVERIFIED -- one hop or two -- so they go while
///   every folder it could pass through is still on the disk; a
///   precaution, since C's check 4 would accept them dangling too; step D
///   plan ruling 3), then `downloads/` (the program), `bundled/` and
///   `completions/` (optional), the fish completion the installer also
///   writes (optional; spec §十三 #17), then the two links the installer
///   put in `~/.grok/bin`: `agent` (optional, a second name for the same
///   command) and `grok` -- the launcher -- last. The folder `~/.grok/bin`
///   itself is not moved (spec §6.3 listed it whole; step D plan ruling 4):
///   it is on the user's PATH, so a script of their own may sit in it, and
///   it stays, emptied, inside the kept `~/.grok`. Kept, and said when
///   present: `~/.grok` (`config.toml`, `auth.json`, `sessions/`,
///   `memory/`, `skills/`, `plugins/`), the shell file the installer's
///   marked block is in, and, reported only when it is a link into
///   `~/.grok` (never Homebrew's or another CLI's), a link the installer
///   may have put in `/usr/local/bin`, which becomes a dead link (spec
///   §6.3).
pub static GROK: Recipe = Recipe {
    id: "grok",
    meta_toml: include_str!("../../../../../adapters/meta/standalone-grok.toml"),
    route: Route {
        kind: RouteKind::SymlinkIntoRoot,
        launcher: "~/.grok/bin/grok",
        root: "~/.grok",
    },
    version: VersionCmd {
        args: &["--version"],
        env: &[],
        parse: VersionParse::SecondToken,
    },
    latest: Latest::Command {
        args: &["update", "--check", "--json"],
        timeout_secs: 60,
        latest_field: "latestVersion",
        available_field: "updateAvailable",
        error_field: Some("error"),
    },
    self_updates: false,
    upgrade: Some(UpgradeCmd {
        args: &["update"],
        timeout_secs: 1800,
        cancel: CancelPolicy::KillThenReconcile,
    }),
    uninstall: Some(Uninstall::Paths {
        remove: &[
            RemoveSpec {
                path: "~/.local/bin/grok",
                expect: Expect::SymlinkIntoRoot,
                what: RemovedWhat::Launcher,
                optional: true,
            },
            RemoveSpec {
                path: "~/.local/bin/agent",
                expect: Expect::SymlinkIntoRoot,
                what: RemovedWhat::Launcher,
                optional: true,
            },
            RemoveSpec {
                path: "~/.grok/downloads",
                expect: Expect::Dir,
                what: RemovedWhat::Program,
                optional: false,
            },
            RemoveSpec {
                path: "~/.grok/bundled",
                expect: Expect::Dir,
                what: RemovedWhat::Program,
                optional: true,
            },
            RemoveSpec {
                path: "~/.grok/completions",
                expect: Expect::Dir,
                what: RemovedWhat::Program,
                optional: true,
            },
            RemoveSpec {
                path: "~/.config/fish/completions/grok.fish",
                expect: Expect::File,
                what: RemovedWhat::Program,
                optional: true,
            },
            RemoveSpec {
                path: "~/.grok/bin/agent",
                expect: Expect::SymlinkIntoRoot,
                what: RemovedWhat::Launcher,
                optional: true,
            },
            RemoveSpec {
                path: "~/.grok/bin/grok",
                expect: Expect::SymlinkIntoRoot,
                what: RemovedWhat::Launcher,
                optional: false,
            },
        ],
        keep: &[
            KeepSpec {
                path: "~/.grok",
                what: KeptWhat::SettingsAndHistory,
            },
            KeepSpec {
                path: "~/.zshrc",
                what: KeptWhat::ShellConfigLines,
            },
            KeepSpec {
                path: "/usr/local/bin/grok",
                what: KeptWhat::OutsideHome,
            },
            KeepSpec {
                path: "/usr/local/bin/agent",
                what: KeptWhat::OutsideHome,
            },
        ],
    }),
    extra_locks: no_extra_locks,
    backup_globs: &[],
};
```

(`RECIPES` stays `&[&CLAUDE, &RUSTUP]` until Task 6: adding the two registers them through `all()`, and `fixtures_layout_test`/`what_we_run_test` then demand the recordings and the trust-file sections, which Task 6 brings in the same commit.)

In `crates/canager-core/src/http/real.rs`, change the constant (E's five entries) to:

```rust
pub const ALLOWED_HTTPS_HOSTS: &[&str] = &[
    "crates.io",
    "pypi.org",
    "registry.ollama.ai",
    "downloads.claude.ai",
    "static.rust-lang.org",
    "antigravity-cli-auto-updater-974169037036.us-central1.run.app",
];
```

and in its doc comment, change E's clause `and static.rust-lang.org (the same, rustup's release file).` to `static.rust-lang.org (the same, rustup's release file) and antigravity-cli-auto-updater-974169037036.us-central1.run.app (the same, Antigravity CLI's version manifest, a Google Cloud Run service; on Apple silicon only).`

In `docs/what-we-run.md`, in the table under `## Network: Canager only connects to these hosts`, after E's `static.rust-lang.org` row add:

```markdown
| `antigravity-cli-auto-updater-974169037036.us-central1.run.app` | `GET /manifests/darwin_arm64.json` — the newest published Antigravity CLI version for Apple silicon, as the JSON manifest its installer and its updater read (`version`, `url`, `sha512`; only `version` is used) | Antigravity CLI's `check_updates` (`StandaloneAdapter`), only when Canager itself runs on Apple silicon — on an Intel Mac no request is made and the row says the check is not yet verified there |
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p canager-core --lib adapters::standalone::recipes` and `cargo test -p canager-core --test what_we_run_test`
Expected: PASS — the five new recipes tests; B's/C's/E's invariants over `RECIPES` unchanged (the two constants are not in it yet); A's `test_what_we_run_names_every_allowed_https_host` with the new row.

- [ ] **Step 5: Continue the task**

No commit: continue to stage 5c.

#### Stage 5c: the adapter over the two recipes, on synthetic layouts

- [ ] **Step 1: Add the two layouts to `testing`**

In `crates/canager-core/src/adapters/standalone/mod.rs`, inside `#[cfg(test)] pub(super) mod testing { … }`, after `claude_layout`'s closing `}` (before C's `Unreadable`), insert:

```rust

    /// Antigravity's layout as its installer writes it (agy.md §2, §3a): a
    /// regular executable at `~/.local/bin/agy` -- the whole program -- and
    /// the tool's root beside the Gemini CLI's other folders.
    pub struct AgyLayout {
        pub launcher: PathBuf,
        pub root: PathBuf,
    }

    pub fn agy_layout(home: &TempHome) -> AgyLayout {
        let launcher = home.executable(".local/bin/agy");
        let root = home.dir(".gemini/antigravity-cli");
        home.file(".gemini/antigravity-cli/updater/update_status.json");
        home.file(".gemini/antigravity-cli/conversations/c1.jsonl");
        AgyLayout { launcher, root }
    }

    /// Grok Build's layout as its installer writes it (grok.md §1, §2): the
    /// download `~/.grok/downloads/grok-<version>-macos-aarch64`, two
    /// *relative* links to it in `~/.grok/bin` (`grok`, `agent`), the
    /// vendored `bundled/` and `completions/`, and the files `~/.grok`
    /// keeps that an uninstall leaves alone.
    pub struct GrokLayout {
        pub launcher: PathBuf,
        pub agent: PathBuf,
        pub root: PathBuf,
        pub real: PathBuf,
    }

    pub fn grok_layout(home: &TempHome, version: &str) -> GrokLayout {
        let real = home.executable(&format!(".grok/downloads/grok-{version}-macos-aarch64"));
        let target = PathBuf::from(format!("../downloads/grok-{version}-macos-aarch64"));
        let launcher = home.link(".grok/bin/grok", &target);
        let agent = home.link(".grok/bin/agent", &target);
        home.file(".grok/bundled/agents/default.md");
        home.file(".grok/completions/zsh/_grok");
        home.file(".grok/config.toml");
        home.file(".grok/auth.json");
        home.file(".grok/sessions/s1.jsonl");
        home.file(".grok/memory/notes.md");
        GrokLayout {
            launcher,
            agent,
            root: home.path().join(".grok"),
            real,
        }
    }
```

- [ ] **Step 2: Write the agy tests (failing until the recipes and the adapter agree)**

In `mod.rs`'s `mod tests`, add `use super::recipes::{AGY, GROK};` beside B's `use super::recipes::CLAUDE;`, add `agy_layout, grok_layout` to the `use super::testing::{…}` list, add `UpdateBlocked` to the `use crate::model::{…}` list, add `use crate::trash::MockTrasher;` if C's tests import it elsewhere (they do: keep one import), and append before the module's closing `}`:

```rust

    // ---- Antigravity CLI ----

    const AGY_MANIFEST_URL: &str =
        "https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/darwin_arm64.json";

    fn agy_adapter(runner: Arc<dyn CommandRunner>, http: Arc<MockHttpClient>) -> StandaloneAdapter {
        StandaloneAdapter::new(&AGY, runner, http, Arc::new(MockTrasher::new()))
            .with_trash_gap(Duration::ZERO)
            .with_arch("aarch64")
    }

    fn agy_request(kind: OpKind) -> OpRequest {
        OpRequest {
            kind,
            instance_id: "standalone-agy".to_string(),
            artifact_kind: ArtifactKind::Binary,
            name: "agy".to_string(),
        }
    }

    /// A detected agy over `home`, `--version` answering `version`.
    async fn detected_agy(
        home: &TempHome,
        version: &str,
        http: Arc<MockHttpClient>,
    ) -> (StandaloneAdapter, ManagerInstance, super::testing::AgyLayout) {
        let layout = agy_layout(home);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0(&format!("{version}\n")),
        );
        let adapter = agy_adapter(runner, http);
        let inst = adapter.detect(&env_as_owner(home)).await.remove(0);
        (adapter, inst, layout)
    }

    #[tokio::test]
    async fn test_detect_lists_agy_as_a_flat_file_read_with_its_auto_update_off() {
        // Spec §3.3 (FlatFile: a regular file, its real path itself), §3.4
        // (the documented switch on every version read), §2.2 (exe_path is
        // the file, prefix the root).
        let home = TempHome::new("agy-detect");
        let layout = agy_layout(&home);
        let runner = Arc::new(RecordingRunner {
            specs: StdMutex::new(Vec::new()),
            output: exited_0("1.2.10\n"),
        });
        let adapter = agy_adapter(runner.clone(), Arc::new(MockHttpClient::new()));

        let instances = adapter.detect(&env_as_owner(&home)).await;

        assert_eq!(instances.len(), 1);
        let inst = &instances[0];
        assert_eq!(inst.id, "standalone-agy");
        assert_eq!(inst.adapter_id, "standalone-agy");
        assert_eq!(inst.exe_path, layout.launcher);
        assert_eq!(inst.prefix, layout.root);
        assert_eq!(inst.version.as_deref(), Some("1.2.10"));
        assert_eq!(inst.status.unavailable, None);
        let specs = runner.specs.lock().unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].program, layout.launcher);
        assert_eq!(specs[0].args, vec!["--version".to_string()]);
        assert_eq!(
            specs[0].env,
            vec![(
                "AGY_CLI_DISABLE_AUTO_UPDATE".to_string(),
                "true".to_string()
            )]
        );
        assert_eq!(specs[0].timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_detect_lists_nothing_for_an_agy_that_is_a_link() {
        // The Homebrew cask's `agy` is a link into its Caskroom (agy.md §3b):
        // Homebrew's row, never this one; and a flat-file route has no
        // launcher-only state (E's ruling 8), so a dangling link is nothing.
        let home = TempHome::new("agy-link");
        let cask = home.executable("opt/homebrew/Caskroom/antigravity-cli/1.2.9/antigravity");
        home.link(".local/bin/agy", &cask);
        home.dir(".gemini/antigravity-cli");
        let adapter = agy_adapter(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        assert!(adapter.detect(&env_as_owner(&home)).await.is_empty());
        std::fs::remove_file(home.path().join(".local/bin/agy")).unwrap();
        home.link(".local/bin/agy", &home.path().join(".gemini/antigravity-cli/bin/agy"));
        assert!(adapter.detect(&env_as_owner(&home)).await.is_empty());
    }

    #[tokio::test]
    async fn test_check_updates_for_agy_lists_a_newer_manifest_version_with_no_button() {
        // Spec §4.4 D5 item 4: a real candidate (the manifest is newer),
        // `SelfUpdatesOnly` (no `upgrade`), `Registry` channel, one GET with
        // no header of Canager's.
        let home = TempHome::new("agy-check-newer");
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            AGY_MANIFEST_URL,
            answer(r#"{"version":"1.2.11","url":"https://storage.googleapis.com/x.tar.gz","sha512":"00"}"#),
        );
        let (adapter, inst, _) = detected_agy(&home, "1.2.10", http.clone()).await;

        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");

        assert_eq!(
            out.candidates,
            vec![UpdateCandidate {
                key: ArtifactKey {
                    instance_id: "standalone-agy".to_string(),
                    kind: ArtifactKind::Binary,
                    name: "agy".to_string(),
                },
                current: "1.2.10".to_string(),
                target: "1.2.11".to_string(),
                channel: UpdateChannel::Registry,
                checkable: true,
                warnings: Vec::new(),
                blocked: Some(UpdateBlocked::SelfUpdatesOnly),
            }]
        );
        assert_eq!(http.calls(), vec![AGY_MANIFEST_URL.to_string()]);
        let request = &http.requests()[0];
        assert_eq!(request.method, "GET");
        assert!(request.headers.is_empty());
        assert_eq!(request.timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_check_updates_for_agy_lists_nothing_when_the_manifest_is_not_newer() {
        for body in [r#"{"version":"1.2.10"}"#, r#"{"version":"1.2.9"}"#] {
            let home = TempHome::new("agy-check-current");
            let http = Arc::new(MockHttpClient::new());
            http.respond(AGY_MANIFEST_URL, answer(body));
            let (adapter, inst, _) = detected_agy(&home, "1.2.10", http).await;
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("check_updates");
            assert!(out.candidates.is_empty(), "{body}");
        }
    }

    #[tokio::test]
    async fn test_check_updates_for_agy_is_uncheckable_on_an_intel_mac_without_a_request() {
        // Spec §3.1/§3.5: the darwin_amd64 manifest is unverified, so an
        // Intel Mac (or Rosetta) gets "could not check" with the reason and
        // nothing leaves the machine.
        let home = TempHome::new("agy-check-intel");
        let http = Arc::new(MockHttpClient::new());
        http.respond(AGY_MANIFEST_URL, answer(r#"{"version":"1.2.11"}"#));
        let layout = agy_layout(&home);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("1.2.10\n"),
        );
        let adapter = agy_adapter(runner, http.clone()).with_arch("x86_64");
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);

        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");

        assert_eq!(out.candidates.len(), 1);
        let c = &out.candidates[0];
        assert!(!c.checkable);
        assert_eq!(c.current, "1.2.10");
        assert_eq!(c.target, "1.2.10");
        assert!(
            matches!(&c.warnings[..], [Warning::Message(m)] if m.contains("Intel") && m.contains("x86_64")),
            "{:?}",
            c.warnings
        );
        assert!(http.calls().is_empty(), "no request on an unverified architecture");
    }

    #[tokio::test]
    async fn test_check_updates_for_agy_marks_a_bad_manifest_uncheckable() {
        for (body, needle) in [
            ("<html>Sign in to the network</html>", "manifest is not JSON"),
            (r#"{"url":"x"}"#, "no `version` string"),
            (r#"{"version":"latest"}"#, "not a version"),
        ] {
            let home = TempHome::new("agy-check-bad");
            let http = Arc::new(MockHttpClient::new());
            http.respond(AGY_MANIFEST_URL, answer(body));
            let (adapter, inst, _) = detected_agy(&home, "1.2.10", http).await;
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("a failed lookup is not a source failure");
            assert_eq!(out.candidates.len(), 1, "{body}");
            assert!(!out.candidates[0].checkable);
            assert!(
                matches!(&out.candidates[0].warnings[..], [Warning::Message(m)] if m.contains(needle)),
                "{body}: {:?}",
                out.candidates[0].warnings
            );
        }
        let home = TempHome::new("agy-check-503");
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            AGY_MANIFEST_URL,
            HttpResponse {
                status: 503,
                body: String::new(),
            },
        );
        let (adapter, inst, _) = detected_agy(&home, "1.2.10", http).await;
        let out = adapter.check_updates(&inst, &CheckOptions::default()).await.unwrap();
        assert!(!out.candidates[0].checkable);
        assert!(
            matches!(&out.candidates[0].warnings[..], [Warning::Message(m)] if m.contains("503"))
        );
    }

    #[tokio::test]
    async fn test_plan_upgrade_for_agy_is_refused_as_self_updating() {
        // The gate refuses it first from the candidate's `blocked`; the
        // adapter refuses it again for a stale snapshot (spec §五).
        let home = TempHome::new("agy-plan-upgrade");
        let (adapter, inst, _) = detected_agy(&home, "1.2.10", Arc::new(MockHttpClient::new())).await;
        assert!(matches!(
            adapter.plan(&inst, &agy_request(OpKind::Upgrade)).await,
            Err(AdapterError::UpdateBlocked {
                reason: UpdateBlocked::SelfUpdatesOnly
            })
        ));
    }

    #[tokio::test]
    async fn test_plan_uninstall_for_agy_lists_the_backup_and_the_program_and_keeps_its_state() {
        // Spec §6.3's agy row as ruled: any `agy.<time>.old` first, the
        // program (the launcher) last; the root, the staging folder and the
        // two shell files kept and said when present -- and not said when
        // absent (C's ruling 6).
        let home = TempHome::new("agy-plan-uninstall-full");
        home.file(".local/bin/agy.1727000000.old");
        home.dir(".cache/antigravity/staging");
        home.file(".zshrc");
        home.file(".zprofile");
        let (adapter, inst, layout) =
            detected_agy(&home, "1.2.10", Arc::new(MockHttpClient::new())).await;

        let plan = adapter
            .plan(&inst, &agy_request(OpKind::Uninstall))
            .await
            .expect("a plan");

        let PlanAction::TrashPaths { paths, previewed } = &plan.action else {
            panic!("a path list: {:?}", plan.action);
        };
        assert_eq!(
            paths,
            &vec![
                home.path().join(".local/bin/agy.1727000000.old"),
                layout.launcher.clone(),
            ]
        );
        assert_eq!(previewed.len(), 2);
        assert_eq!(
            plan.warnings,
            vec![
                Warning::WillTrash {
                    path: "~/.local/bin/agy.1727000000.old".to_string(),
                    what: RemovedWhat::Backups,
                },
                Warning::WillTrash {
                    path: "~/.local/bin/agy".to_string(),
                    what: RemovedWhat::Launcher,
                },
                Warning::WillKeep {
                    path: "~/.gemini/antigravity-cli".to_string(),
                    what: KeptWhat::ToolState,
                },
                Warning::WillKeep {
                    path: "~/.cache/antigravity".to_string(),
                    what: KeptWhat::InstallerCache,
                },
                Warning::WillKeep {
                    path: "~/.zshrc".to_string(),
                    what: KeptWhat::ShellConfigLines,
                },
                Warning::WillKeep {
                    path: "~/.zprofile".to_string(),
                    what: KeptWhat::ShellConfigLines,
                },
            ]
        );
        assert_eq!(plan.locks, vec![ResourceLock("standalone-agy".to_string())]);
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);

        // The minimum: no backup, no staging folder, no zprofile.
        let home = TempHome::new("agy-plan-uninstall-min");
        home.file(".zshrc");
        let (adapter, inst, layout) =
            detected_agy(&home, "1.2.10", Arc::new(MockHttpClient::new())).await;
        let plan = adapter
            .plan(&inst, &agy_request(OpKind::Uninstall))
            .await
            .expect("a plan");
        let PlanAction::TrashPaths { paths, .. } = &plan.action else {
            panic!("a path list");
        };
        assert_eq!(paths, &vec![layout.launcher.clone()]);
        assert_eq!(
            plan.warnings,
            vec![
                Warning::WillTrash {
                    path: "~/.local/bin/agy".to_string(),
                    what: RemovedWhat::Launcher,
                },
                Warning::WillKeep {
                    path: "~/.gemini/antigravity-cli".to_string(),
                    what: KeptWhat::ToolState,
                },
                Warning::WillKeep {
                    path: "~/.zshrc".to_string(),
                    what: KeptWhat::ShellConfigLines,
                },
            ]
        );
    }

    #[tokio::test]
    async fn test_execute_for_agy_moves_both_items_leaves_its_state_and_reads_as_gone() {
        let home = TempHome::new("agy-execute");
        home.file(".local/bin/agy.1727000000.old");
        home.dir(".cache/antigravity/staging");
        let trasher = Arc::new(MockTrasher::new());
        let layout = agy_layout(&home);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("1.2.10\n"),
        );
        let adapter = StandaloneAdapter::new(
            &AGY,
            runner,
            Arc::new(MockHttpClient::new()),
            trasher.clone(),
        )
        .with_trash_gap(Duration::ZERO)
        .with_arch("aarch64");
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        let plan = adapter
            .plan(&inst, &agy_request(OpKind::Uninstall))
            .await
            .expect("a plan");

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(outcome, Outcome::Succeeded);
        let PlanAction::TrashPaths { paths, .. } = &plan.action else {
            panic!("a path list");
        };
        assert_eq!(&trasher.calls(), paths);
        assert_eq!(trasher.kinds(), vec![ItemKind::File, ItemKind::File]);
        assert!(layout.root.join("conversations/c1.jsonl").is_file(), "the root stays");
        assert!(home.path().join(".cache/antigravity/staging").is_dir(), "the staging folder stays");
        let key = adapter.artifact_key(&inst);
        assert!(!adapter
            .reconcile_after_uninstall(&inst, &key)
            .await
            .expect("a reading")
            .present);
        assert!(adapter.detect(&env_as_owner(&home)).await.is_empty());
    }

    #[tokio::test]
    async fn test_execute_for_agy_refuses_a_launcher_its_updater_replaced_after_the_preview() {
        // Review Focus 1: agy's updater writes a new file (a new inode) at
        // the launcher's path -- this Mac's did twice in two days. The
        // preview's identity no longer matches: nothing moves.
        let home = TempHome::new("agy-execute-replaced");
        let trasher = Arc::new(MockTrasher::new());
        let layout = agy_layout(&home);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("1.2.10\n"),
        );
        let adapter = StandaloneAdapter::new(
            &AGY,
            runner,
            Arc::new(MockHttpClient::new()),
            trasher.clone(),
        )
        .with_trash_gap(Duration::ZERO)
        .with_arch("aarch64");
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        let plan = adapter
            .plan(&inst, &agy_request(OpKind::Uninstall))
            .await
            .expect("a plan");
        // Made while the old one still exists, then renamed over it, so it
        // cannot get the old inode back (C's tests do the same).
        let replacement = home.executable(".local/bin/agy.new");
        std::fs::rename(&replacement, &layout.launcher).unwrap();

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(
            outcome,
            Outcome::CanagerFailed(Fault::PathChanged {
                path: "~/.local/bin/agy".to_string()
            })
        );
        assert!(trasher.calls().is_empty());
    }
```

(`env_as_owner` is C's helper in this module; `answer`, `exited_0`, `RecordingRunner`, `StdMutex` are B's; `ItemKind`, `Fault`, `KeptWhat`, `RemovedWhat`, `PlanAction`, `HttpResponse` are imported by C's tests. `MockHttpClient::requests()` returns every request with its headers.)

- [ ] **Step 3: Continue the stage**

No run yet: the grok tests follow, then one run for the stage.

- [ ] **Step 4: Write the grok tests**

In the same `mod tests`, append before the module's closing `}`:

```rust

    // ---- Grok Build ----

    fn grok_adapter(runner: Arc<dyn CommandRunner>) -> StandaloneAdapter {
        StandaloneAdapter::new(
            &GROK,
            runner,
            Arc::new(MockHttpClient::new()),
            Arc::new(MockTrasher::new()),
        )
        .with_trash_gap(Duration::ZERO)
    }

    fn grok_request(kind: OpKind) -> OpRequest {
        OpRequest {
            kind,
            instance_id: "standalone-grok".to_string(),
            artifact_kind: ArtifactKind::Binary,
            name: "grok".to_string(),
        }
    }

    const GROK_VERSION_LINE: &str = "grok 1.0.41 (4220f3b224a6)\n";
    const GROK_CHECK_CURRENT: &str = r#"{"currentVersion":"1.0.41","latestVersion":"1.0.41","updateAvailable":false,"installer":"internal","channel":"stable","autoUpdate":true,"error":null}"#;
    const GROK_CHECK_NEWER: &str = r#"{"currentVersion":"1.0.41","latestVersion":"1.0.42","updateAvailable":true,"installer":"internal","channel":"stable","autoUpdate":true,"error":null}"#;

    /// A runner answering grok's two read-only commands.
    fn grok_runner(layout: &super::testing::GrokLayout, check: CommandOutput) -> Arc<MockRunner> {
        let runner = Arc::new(MockRunner::new());
        let launcher = layout.launcher.to_str().unwrap();
        runner.respond(vec![launcher, "--version"], exited_0(GROK_VERSION_LINE));
        runner.respond(vec![launcher, "update", "--check", "--json"], check);
        runner
    }

    /// A detected grok over `home` (`trasher` its Trash), its check command
    /// answering `check`.
    async fn detected_grok(
        home: &TempHome,
        check: CommandOutput,
        trasher: Arc<MockTrasher>,
    ) -> (StandaloneAdapter, ManagerInstance, super::testing::GrokLayout) {
        let layout = grok_layout(home, "1.0.41");
        let runner = grok_runner(&layout, check);
        let adapter = StandaloneAdapter::new(&GROK, runner, Arc::new(MockHttpClient::new()), trasher)
            .with_trash_gap(Duration::ZERO);
        let inst = adapter.detect(&env_as_owner(home)).await.remove(0);
        (adapter, inst, layout)
    }

    #[tokio::test]
    async fn test_detect_lists_grok_through_its_relative_launcher_link_reading_the_second_token() {
        // grok.md §1: `~/.grok/bin/grok -> ../downloads/grok-1.0.41-macos-aarch64`,
        // relative; `grok --version` -> `grok 1.0.41 (4220f3b224a6)`. No
        // environment on the read (none is documented).
        let home = TempHome::new("grok-detect");
        let layout = grok_layout(&home, "1.0.41");
        let runner = Arc::new(RecordingRunner {
            specs: StdMutex::new(Vec::new()),
            output: exited_0(GROK_VERSION_LINE),
        });
        let adapter = grok_adapter(runner.clone());

        let instances = adapter
            .detect(&home.env(vec![home.path().join(".grok/bin")]))
            .await;

        // One instance, although `bin/agent` resolves to the same download:
        // the route looks at the one fixed launcher path.
        assert_eq!(instances.len(), 1);
        assert_eq!(std::fs::canonicalize(&layout.agent).unwrap(), layout.real);
        let inst = &instances[0];
        assert_eq!(inst.id, "standalone-grok");
        assert_eq!(inst.exe_path, layout.launcher);
        assert_eq!(inst.prefix, layout.root);
        assert_eq!(inst.version.as_deref(), Some("1.0.41"));
        assert!(inst.status.notes.is_empty(), "PATH finds this very copy");
        let specs = runner.specs.lock().unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].args, vec!["--version".to_string()]);
        assert!(specs[0].env.is_empty());
    }

    #[tokio::test]
    async fn test_check_updates_for_grok_asks_its_own_read_only_check_and_trusts_its_answer() {
        // Spec §4.3: grok's `updateAvailable` decides, `latestVersion` is
        // shown, the channel is Native (the tool's own answer), no button is
        // withheld (grok has `grok update`).
        let home = TempHome::new("grok-check-newer");
        let (adapter, inst, layout) =
            detected_grok(&home, exited_0(GROK_CHECK_NEWER), Arc::new(MockTrasher::new())).await;

        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");

        assert_eq!(
            out.candidates,
            vec![UpdateCandidate {
                key: ArtifactKey {
                    instance_id: "standalone-grok".to_string(),
                    kind: ArtifactKind::Binary,
                    name: "grok".to_string(),
                },
                current: "1.0.41".to_string(),
                target: "1.0.42".to_string(),
                channel: UpdateChannel::Native,
                checkable: true,
                warnings: Vec::new(),
                blocked: None,
            }]
        );
        // The check runs against the launcher, with the recipe's argv and
        // its own timeout, and no environment of Canager's.
        let runner = Arc::new(RecordingRunner {
            specs: StdMutex::new(Vec::new()),
            output: exited_0(GROK_VERSION_LINE),
        });
        let adapter = grok_adapter(runner.clone());
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        let _ = adapter.check_updates(&inst, &CheckOptions::default()).await;
        let specs = runner.specs.lock().unwrap();
        let check = specs
            .iter()
            .find(|spec| spec.args.first().map(String::as_str) == Some("update"))
            .expect("the check command ran");
        assert_eq!(check.program, layout.launcher);
        assert_eq!(
            check.args,
            vec!["update".to_string(), "--check".to_string(), "--json".to_string()]
        );
        assert!(check.env.is_empty());
        assert_eq!(check.timeout, Duration::from_secs(60));
        assert_eq!(check.output_use, OutputUse::Parsed);
    }

    #[tokio::test]
    async fn test_check_updates_for_grok_lists_nothing_when_it_says_no_update_and_is_uncheckable_when_it_fails() {
        // Review Focus 4: the tool's word is final when it answers; when it
        // fails, prints something else, or times out, the row is "could not
        // check" with a short reason -- never an `Err` for the source.
        let home = TempHome::new("grok-check-current");
        let (adapter, inst, _) =
            detected_grok(&home, exited_0(GROK_CHECK_CURRENT), Arc::new(MockTrasher::new())).await;
        let out = adapter.check_updates(&inst, &CheckOptions::default()).await.unwrap();
        assert!(out.candidates.is_empty());

        let failing = [
            (
                CommandOutput {
                    exit_code: Some(1),
                    stdout: String::new(),
                    stderr: "error: could not reach x.ai\n".to_string(),
                    timed_out: false,
                    cancelled: false,
                },
                "exited with Some(1)",
            ),
            (
                exited_0("<html><body>Sign in to the network</body></html>\n"),
                "did not print JSON",
            ),
            (
                exited_0(r#"{"currentVersion":"1.0.41","latest":"1.0.42","updateAvailable":true}"#),
                "no `latestVersion` string",
            ),
            // Exit 0, `updateAvailable: false`, and grok's own `error` set:
            // the offline shape. Not "up to date" -- "could not check", with
            // grok's words (ruling 10).
            (
                exited_0(r#"{"currentVersion":"1.0.41","latestVersion":"1.0.41","updateAvailable":false,"installer":"internal","channel":"stable","autoUpdate":true,"error":"failed to reach the update server"}"#),
                "reported: failed to reach the update server",
            ),
            (
                CommandOutput {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    timed_out: true,
                    cancelled: false,
                },
                "did not finish within 60 s",
            ),
        ];
        for (check, needle) in failing {
            let home = TempHome::new("grok-check-failing");
            let (adapter, inst, _) = detected_grok(&home, check, Arc::new(MockTrasher::new())).await;
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("a failed lookup is not a source failure");
            assert_eq!(out.candidates.len(), 1, "{needle}");
            let c = &out.candidates[0];
            assert!(!c.checkable);
            assert_eq!(c.current, "1.0.41");
            assert_eq!(c.target, "1.0.41");
            assert_eq!(c.channel, UpdateChannel::Registry);
            assert!(
                matches!(&c.warnings[..], [Warning::Message(m)] if m.contains(needle)),
                "{needle}: {:?}",
                c.warnings
            );
        }
    }

    #[tokio::test]
    async fn test_plan_upgrade_for_grok_is_its_own_update_command() {
        // Spec §五's grok row: `<grok> update`, 1800 s, KillThenReconcile,
        // its own lock and no other, no environment, no password.
        let home = TempHome::new("grok-plan-upgrade");
        let (adapter, inst, layout) =
            detected_grok(&home, exited_0(GROK_CHECK_CURRENT), Arc::new(MockTrasher::new())).await;
        let plan = adapter
            .plan(&inst, &grok_request(OpKind::Upgrade))
            .await
            .expect("a plan");
        assert_eq!(command_program(&plan), layout.launcher);
        assert_eq!(command_args(&plan), &["update".to_string()]);
        assert!(command_env(&plan).is_empty());
        assert!(!plan.needs_password);
        assert_eq!(plan.locks, vec![ResourceLock("standalone-grok".to_string())]);
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);
        assert_eq!(plan.timeout_secs, 1800);
        assert!(plan.warnings.is_empty());
    }

    #[tokio::test]
    async fn test_plan_uninstall_for_grok_moves_its_folders_and_its_launcher_link_last_and_keeps_its_home() {
        // Spec §6.3's grok row as ruled (rulings 3 and 4): with everything
        // present but no fallback links, three folders and the fish file,
        // then `bin/agent` and `bin/grok` last -- the folder `~/.grok/bin`
        // itself is not listed; `~/.grok` and `~/.zshrc` kept and said. A
        // `/usr/local/bin/grok` on the machine running this test, if there is
        // one, is not a link into this temp home, so it gets no sentence
        // (ruling 6): the expected list does not depend on the host.
        let home = TempHome::new("grok-plan-uninstall-full");
        home.file(".zshrc");
        home.file(".config/fish/completions/grok.fish");
        home.file(".grok/bin/my-own-script");
        let (adapter, inst, layout) =
            detected_grok(&home, exited_0(GROK_CHECK_CURRENT), Arc::new(MockTrasher::new())).await;

        let plan = adapter
            .plan(&inst, &grok_request(OpKind::Uninstall))
            .await
            .expect("a plan");

        let PlanAction::TrashPaths { paths, previewed } = &plan.action else {
            panic!("a path list: {:?}", plan.action);
        };
        assert_eq!(
            paths,
            &vec![
                home.path().join(".grok/downloads"),
                home.path().join(".grok/bundled"),
                home.path().join(".grok/completions"),
                home.path().join(".config/fish/completions/grok.fish"),
                layout.agent.clone(),
                layout.launcher.clone(),
            ]
        );
        assert_eq!(
            previewed.iter().map(|i| i.kind).collect::<Vec<_>>(),
            vec![
                ItemKind::Dir,
                ItemKind::Dir,
                ItemKind::Dir,
                ItemKind::File,
                ItemKind::Symlink,
                ItemKind::Symlink
            ]
        );
        assert_eq!(
            plan.warnings,
            vec![
                Warning::WillTrash { path: "~/.grok/downloads".to_string(), what: RemovedWhat::Program },
                Warning::WillTrash { path: "~/.grok/bundled".to_string(), what: RemovedWhat::Program },
                Warning::WillTrash { path: "~/.grok/completions".to_string(), what: RemovedWhat::Program },
                Warning::WillTrash { path: "~/.config/fish/completions/grok.fish".to_string(), what: RemovedWhat::Program },
                Warning::WillTrash { path: "~/.grok/bin/agent".to_string(), what: RemovedWhat::Launcher },
                Warning::WillTrash { path: "~/.grok/bin/grok".to_string(), what: RemovedWhat::Launcher },
                Warning::WillKeep { path: "~/.grok".to_string(), what: KeptWhat::SettingsAndHistory },
                Warning::WillKeep { path: "~/.zshrc".to_string(), what: KeptWhat::ShellConfigLines },
            ]
        );
        assert_eq!(plan.timeout_secs, removal::TIMEOUT_SECS);
        // The user's own script in the PATH folder is neither listed nor
        // moved (ruling 4).
        assert!(!paths.contains(&home.path().join(".grok/bin/my-own-script")));
        assert!(!paths.contains(&home.path().join(".grok/bin")));

        // The minimum: downloads and the two links only, nothing kept to
        // mention but `~/.grok` itself.
        let home = TempHome::new("grok-plan-uninstall-min");
        let layout_min = grok_layout(&home, "1.0.41");
        std::fs::remove_dir_all(layout_min.root.join("bundled")).unwrap();
        std::fs::remove_dir_all(layout_min.root.join("completions")).unwrap();
        let runner = grok_runner(&layout_min, exited_0(GROK_CHECK_CURRENT));
        let adapter = grok_adapter(runner);
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        let plan = adapter.plan(&inst, &grok_request(OpKind::Uninstall)).await.expect("a plan");
        let PlanAction::TrashPaths { paths, .. } = &plan.action else {
            panic!("a path list");
        };
        assert_eq!(
            paths,
            &vec![
                home.path().join(".grok/downloads"),
                layout_min.agent.clone(),
                layout_min.launcher.clone()
            ]
        );
        assert_eq!(plan.warnings.len(), 4, "{:?}", plan.warnings);
    }

    #[tokio::test]
    async fn test_plan_uninstall_for_grok_keeps_a_foreign_agent_link_and_moves_its_own_fallback_links_first() {
        // Review Focus 2 (spec §十三 #27): `~/.local/bin/agent` belongs to
        // another CLI -- kept and said, the uninstall goes on. grok's own
        // `~/.local/bin/grok` (a link into the root) is moved, and moved
        // first, while every folder its text could pass through still
        // exists (ruling 3).
        let home = TempHome::new("grok-plan-foreign-agent");
        let other = home.executable("other-cli/agent");
        home.link(".local/bin/agent", &other);
        let trasher = Arc::new(MockTrasher::new());
        let (adapter, inst, layout) =
            detected_grok(&home, exited_0(GROK_CHECK_CURRENT), trasher.clone()).await;
        let fallback = home.link(".local/bin/grok", &layout.launcher);

        let plan = adapter
            .plan(&inst, &grok_request(OpKind::Uninstall))
            .await
            .expect("a plan, not a refusal");

        let PlanAction::TrashPaths { paths, .. } = &plan.action else {
            panic!("a path list");
        };
        assert_eq!(paths[0], fallback);
        assert_eq!(paths.last(), Some(&layout.launcher));
        assert!(!paths.contains(&home.path().join(".local/bin/agent")));
        assert_eq!(
            plan.warnings[0],
            Warning::WillTrash { path: "~/.local/bin/grok".to_string(), what: RemovedWhat::Launcher }
        );
        let kept_position = plan
            .warnings
            .iter()
            .position(|w| *w == Warning::WillKeep { path: "~/.local/bin/agent".to_string(), what: KeptWhat::NotOurs })
            .expect("the foreign link is said to stay");
        assert!(
            plan.warnings[..kept_position].iter().all(|w| matches!(w, Warning::WillTrash { .. })),
            "after the moves, before the recipe's kept paths: {:?}",
            plan.warnings
        );

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(&trasher.calls(), paths);
        assert!(
            std::fs::symlink_metadata(home.path().join(".local/bin/agent")).unwrap().file_type().is_symlink(),
            "the foreign link is untouched"
        );
        assert!(layout.root.join("config.toml").is_file());
        assert!(layout.root.join("auth.json").is_file());
        assert!(layout.root.join("sessions/s1.jsonl").is_file());
        let key = adapter.artifact_key(&inst);
        assert!(!adapter.reconcile_after_uninstall(&inst, &key).await.unwrap().present);
    }

    #[tokio::test]
    async fn test_a_stopped_grok_uninstall_leaves_a_launcher_only_row_that_a_second_uninstall_finishes() {
        // Review Focus 6: macOS refuses `bundled` (the second item). The
        // download folder is in the Trash, `~/.grok/bin/grok` and
        // `~/.grok/bin/agent` dangle into `~/.grok` -- the launcher-only
        // state (spec §3.3 step 2, relative link text) -- the row stays, and
        // a second uninstall lists `downloads` as already gone and finishes
        // with the two links, `grok` last.
        let home = TempHome::new("grok-stopped");
        let trasher = Arc::new(MockTrasher::new());
        trasher.refuse_call(1, "Operation not permitted");
        let (adapter, inst, layout) =
            detected_grok(&home, exited_0(GROK_CHECK_CURRENT), trasher.clone()).await;
        let plan = adapter.plan(&inst, &grok_request(OpKind::Uninstall)).await.expect("a plan");

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(
            outcome,
            Outcome::Failed { exit_code: None, summary: "Operation not permitted".to_string() }
        );
        assert_eq!(trasher.calls().len(), 2);
        assert!(std::fs::symlink_metadata(&layout.launcher).unwrap().file_type().is_symlink());
        // Both links now dangle: there, but resolving to nothing.
        assert!(std::fs::symlink_metadata(&layout.agent).unwrap().file_type().is_symlink());
        assert!(!layout.agent.exists() && !layout.launcher.exists());
        assert_eq!(
            route::probe(RouteKind::SymlinkIntoRoot, &layout.launcher, &layout.root),
            Probe::LauncherOnly
        );
        let key = adapter.artifact_key(&inst);
        assert!(adapter.reconcile_after_uninstall(&inst, &key).await.unwrap().present);

        // The next refresh: a launcher-only row, no version read.
        let rows = adapter.detect(&env_as_owner(&home)).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status.notes, vec![InstanceNote::LauncherOnly]);
        assert_eq!(rows[0].version, None);

        // The second uninstall, with a Trash that accepts everything.
        let second = Arc::new(MockTrasher::new());
        let runner = grok_runner(&layout, exited_0(GROK_CHECK_CURRENT));
        let adapter = StandaloneAdapter::new(&GROK, runner, Arc::new(MockHttpClient::new()), second.clone())
            .with_trash_gap(Duration::ZERO);
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        let plan = adapter.plan(&inst, &grok_request(OpKind::Uninstall)).await.expect("a plan");
        let PlanAction::TrashPaths { paths, .. } = &plan.action else {
            panic!("a path list");
        };
        assert_eq!(
            paths,
            &vec![
                home.path().join(".grok/bundled"),
                home.path().join(".grok/completions"),
                layout.agent.clone(),
                layout.launcher.clone(),
            ]
        );
        assert_eq!(
            plan.warnings[0],
            Warning::AlreadyGone { path: "~/.grok/downloads".to_string() }
        );
        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 10, CancellationToken::new())
            .await
            .expect("execute");
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(
            route::probe(RouteKind::SymlinkIntoRoot, &layout.launcher, &layout.root),
            Probe::Absent
        );
        assert!(std::fs::symlink_metadata(&layout.agent).is_err(), "agent went too");
        assert!(layout.root.join("bin").is_dir(), "the emptied folder stays (ruling 4)");
        assert!(adapter.detect(&env_as_owner(&home)).await.is_empty());
    }
```

(`command_program`/`command_args`/`command_env` are C's `crate::testing` helpers — import them if B's tests do not already; `RouteKind`, `Probe`, `InstanceNote`, `OutputUse`, `CommandOutput` are B's imports. The exit-code row's reason also names the argv -- `` `update --check --json` exited with Some(1): error: could not reach x.ai `` -- which is what the sentence behind "Show technical details" shows.)

- [ ] **Step 5: Run to verify the stage passes**

Run: `cargo test -p canager-core --lib adapters::standalone`
Expected: PASS — the seventeen new adapter tests (ten agy, seven grok), the five recipes tests of 5b, the three `latest` tests of 5a, and every B/C/E test. `test_a_stopped_grok_uninstall_…`'s second `detect` finds the same instance id, so the seat (E's `seated_detected_for`) matches the fresh instance.

#### Stage 5d: format, gates, commit

- [ ] **Step 1: Format and gates**

Run `cargo fmt --all`, then the five gates from Global Constraints. Expected: all clean. (`pnpm test`/`pnpm typecheck` are unaffected by this stage and must still pass.)

- [ ] **Step 2: Commit**

```bash
git add crates/canager-core/src/adapters/standalone/recipe.rs crates/canager-core/src/adapters/standalone/latest.rs crates/canager-core/src/adapters/standalone/mod.rs crates/canager-core/src/adapters/standalone/recipes.rs adapters/meta/standalone-agy.toml adapters/meta/standalone-grok.toml crates/canager-core/src/http/real.rs docs/what-we-run.md
git commit -m "$(cat <<'EOF'
Add the Antigravity CLI and Grok Build recipes to the standalone adapter

Two data rows and the shapes they are the first to need: a JSON version
manifest fetched on Apple silicon only (an Intel Mac gets an honest
"could not check"), a tool's own read-only update check trusted as it
answers, and an optional upgrade whose None marks every candidate
SelfUpdatesOnly -- agy installs its updates itself and offers no command
Canager may run; grok's own error field makes a failed check "could not
check", never "up to date". Both uninstall with a path list: agy's
launcher is the whole program and goes after any backup its updater
left; grok's fallback links go first, its folders next and its two bin
links last (the folder, which is on PATH, stays), so a stopped run
leaves the launcher-only row a second run finishes. Not registered yet:
the recordings and the trust-file sections land with the registration.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

(Plus any file the `Recipe {` grep made you edit for `upgrade: Some(…)` that is not listed above.)

### Task 6: Recording, registration, `owned_roots`, the trust file, the end-to-end test

**Files:**
- Create: `adapters/fixtures/standalone-agy/<version>/{README.md, version.txt, manifest-darwin_arm64.json, update_status.json, layout.txt}` — recorded, never typed (the directory is named after the recorded version; Step 1)
- Create: `adapters/fixtures/standalone-grok/<version>/{README.md, version.txt, update-check.json, layout.txt}` — recorded (Step 2)
- Modify: `adapters/meta/standalone-agy.toml`, `adapters/meta/standalone-grok.toml` — only if the recorded versions differ from `1.2.10` / `1.0.41`
- Modify: `crates/canager-core/src/adapters/standalone/recipes.rs` — `RECIPES`; `test_recipes_lists_each_registered_tool_once_in_reading_order`; the `.chain([&&AGY, &&GROK])` of `test_every_command_latest_source_only_checks`
- Modify: `crates/canager-core/src/adapters/standalone/mod.rs` — four fixture-backed tests
- Modify: `crates/canager-core/src/session/mod.rs` — `test_new_registers_all_nine_adapters` → eleven
- Modify: `crates/canager-core/src/scan/mod.rs` — `owned_roots` rows, its doc, `test_owned_roots_table`
- Modify: `crates/canager-core/src/lib.rs` — the crate doc's list of sources
- Modify: `docs/what-we-run.md` — intro, "Where the program comes from", `## Antigravity CLI`, `## Grok Build`, files read, never-list, one clause of "Moving files to the Trash", the network section's last paragraph
- Modify: `crates/canager-core/tests/what_we_run_test.rs` — one test
- Create: `crates/canager-core/tests/standalone_agy_grok_test.rs` — through `Session`
- Test: `fixtures_layout_test.rs`, `what_we_run_test.rs` (both existing), the session test, the fixture tests, the new integration test.

**Interfaces:**
- Consumes: `AGY`, `GROK` (Task 5); `standalone::all` (B, C); `Session::new`'s `adapters.extend(standalone::all(…))`; F's `owned_roots`; A's document structure with B's `## Claude Code` and E's `## rustup`; `Session::{with_adapters, refresh, issue_plan, submit, operations}`; C's `outcome_of` shape.
- Produces: the registered ids `standalone-agy`, `standalone-grok` (readers: `Session::refresh`'s fan-out, `fixtures_layout_test`, `what_we_run_test`, `ADAPTER_LABEL_KEYS` in Task 7); `RECIPES = &[&CLAUDE, &AGY, &GROK, &RUSTUP]` (reader: `all()`); `owned_roots`'s two rows (reader: `Known::index`, rule 3); the two sections (reader: the person spec §12 wrote the file for; `what_we_run_test`).

Why one task: `tests/fixtures_layout_test.rs` asserts the fixture directory set equals the registered id set, and `tests/what_we_run_test.rs` asserts a `## <meta.name>` section per registered id — so the recordings, the registration and the sections cannot be green separately (B's ruling 6, E's Task 10).

- [ ] **Step 1: Record Antigravity CLI on this Mac (read-only commands only)**

In the repo root, on the author's Mac. **Never run `agy update`, bare `agy`, or `agy` with a prompt here.** The one `agy` invocation is `--version` under the documented switch, run **once** (its output is the fixture; the version number is read back from the file); everything else is `curl`, `cat`, `stat`, `ls`, `ps`. Spec §3.4 (§十三 #10) requires the observation that this read did not reach the updater, taken *around the very read that is recorded*: the count of files in the tool's log folder and the mtime of its updater's status file before and after, and that no new updater process appeared — as a before/after diff of the process list, not a name grep: the Antigravity desktop app (`antigravity`, `antigravity-ide` casks, agy.md §1) may be open and is called `antigravity` too, and would trip a grep falsely; the log count and the mtime are the primary evidence, the process diff the secondary.

```bash
S="${TMPDIR:-/tmp}/canager-agy-recording"; mkdir -p "$S"   # scratch, never committed

# 1. The observation, part one: what the updater's state looks like now,
#    and which processes exist (pid, parent, name; diffed in step 3).
LOGS_BEFORE=$(ls -1 ~/.gemini/antigravity-cli/log 2>/dev/null | wc -l | tr -d ' ')
STATUS_BEFORE=$(stat -f '%m' ~/.gemini/antigravity-cli/updater/update_status.json)
ps -axo pid,ppid,comm | sort > "$S/ps-before.txt"
echo "logs=$LOGS_BEFORE status_mtime=$STATUS_BEFORE"                                   # -> [LOGS_BEFORE], [STATUS_BEFORE]

# 2. The installed version, under the switch, run once into a scratch
#    file. Expected shape: one bare version (agy.md §4). The number is
#    read from the file (the fixture directory is named after it) and the
#    file is then moved into place, so the recorded bytes are this run's.
#    The same number goes in adapters/meta/standalone-agy.toml's
#    verified_versions and the "Verified against Antigravity CLI <VERSION>"
#    sentence of Step 6 -- the three must agree.
AGY_CLI_DISABLE_AUTO_UPDATE=true ~/.local/bin/agy --version > "$S/version.txt"
cat "$S/version.txt"                                                                   # -> e.g. 1.2.10
VERSION_AGY=$(tr -d '[:space:]' < "$S/version.txt")
D="adapters/fixtures/standalone-agy/$VERSION_AGY"; mkdir -p "$D"
mv "$S/version.txt" "$D/version.txt"

# 3. The observation, part two, after a pause long enough for a spawned
#    updater to have written anything.
sleep 10
LOGS_AFTER=$(ls -1 ~/.gemini/antigravity-cli/log 2>/dev/null | wc -l | tr -d ' ')
STATUS_AFTER=$(stat -f '%m' ~/.gemini/antigravity-cli/updater/update_status.json)
echo "logs=$LOGS_AFTER status_mtime=$STATUS_AFTER"                                     # -> must equal step 1's two numbers
ps -axo pid,ppid,comm | sort > "$S/ps-after.txt"
comm -13 "$S/ps-before.txt" "$S/ps-after.txt" | grep -i -E 'agy|antigravity' || echo none   # -> [PROCS]: NEW processes named agy/antigravity only; "none" expected

# 4. The manifest, byte for byte, no redirect followed (as RealHttpClient).
curl --fail --silent --show-error \
  https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/darwin_arm64.json \
  > "$D/manifest-darwin_arm64.json"

# 5. The updater's own status file, as evidence of what "did not touch"
#    means (its content has no personal data: agy.md §2 shows it).
cat ~/.gemini/antigravity-cli/updater/update_status.json > "$D/update_status.json"

# 6. The layout: the tool's own path only (spec §十三 #30), owners numeric,
#    the home folder abbreviated -- the README states both transformations.
ls -lan ~/.local/bin/agy | sed "s|$HOME|~|g" > "$D/layout.txt"

# 7. For the README's provenance lines only (not recorded as files).
date +%F                                                                              # -> [DATE]
sw_vers -productVersion; uname -m                                                     # -> [MACOS], [ARCH]
ls -1 ~/.local/bin | grep -c -E '^agy\..+\.old$' || true                               # -> [OLD_COUNT]: backups present right now
ls -A ~/.cache/antigravity/staging 2>/dev/null | wc -l | tr -d ' '                    # -> [STAGING_COUNT] (0 expected: empty)
grep -c 'Added by Antigravity CLI installer' ~/.zshrc ~/.zprofile 2>/dev/null         # -> [RC_MARKERS]: one per file expected
grep -o '"version": *"[^"]*"' "$D/manifest-darwin_arm64.json"                         # -> [MANIFEST_VERSION]
```

**If `LOGS_AFTER != LOGS_BEFORE`, `STATUS_AFTER != STATUS_BEFORE`, or `[PROCS]` is not `none`: stop.** (A `[PROCS]` line that is plainly the desktop app — a parent pid that is not the recording shell's, present in both lists — is not a stop; a *new* `agy` child is.) Spec §3.4 says what follows — the recipe's version read must change for this version (read the updater's own record instead, or `version: None` with a sentence) — and that is the author's decision, not this plan's. Report the three values in the handover and do not commit the recording.

Then write `adapters/fixtures/standalone-agy/$VERSION_AGY/README.md`, filling the bracketed values from the commands above and nothing else:

```markdown
# Antigravity CLI [VERSION_AGY] fixtures (native installer route, `standalone-agy`)

Recorded [DATE] on the author's MacBook (macOS [MACOS], [ARCH]) by running the
commands below. `version.txt`, `manifest-darwin_arm64.json` and
`update_status.json` are the commands' output byte for byte. `layout.txt` is
`ls -lan` (numeric owner and group) with the home folder's absolute path
replaced by `~` (`sed "s|$HOME|~|g"`) — the only two edits, so no user name or
home path enters the repository. The recording itself changed nothing.

Commands:
- `AGY_CLI_DISABLE_AUTO_UPDATE=true ~/.local/bin/agy --version` -> `version.txt`
  (one bare version; the switch is the one Google's troubleshooting page
  documents for the background updater)
- `curl --fail --silent --show-error https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/darwin_arm64.json`
  -> `manifest-darwin_arm64.json` (direct 200, no redirect; the manifest the
  installer and the updater read; only its `version` is used by Canager)
- `cat ~/.gemini/antigravity-cli/updater/update_status.json` -> `update_status.json`
- `ls -lan ~/.local/bin/agy | sed "s|$HOME|~|g"` -> `layout.txt` (a regular
  file, not a link: the installer copies the binary there)

The read did not reach the updater (spec §3.4, checked around the one
`--version` run of this recording, whose output is `version.txt`):
`~/.gemini/antigravity-cli/log` held [LOGS_BEFORE] files before and [LOGS_AFTER]
after; `updater/update_status.json`'s mtime was [STATUS_BEFORE] before and
[STATUS_AFTER] after; no new `agy` or `antigravity` process had appeared ten
seconds later (a before/after diff of `ps -axo pid,ppid,comm`; the Antigravity
desktop app, if open, is the same in both lists). Whether the switch itself
does anything cannot be shown by `--version`, which never gets that far on this
version; it stays on the read as Google's documented belt. A run with a prompt
is what writes a log and spawns the updater (agy.md §4).

The manifest answered `version` [MANIFEST_VERSION]. The recording ran no
`agy update` and no bare `agy`.

Other observations, for the reader (not recorded as files): [OLD_COUNT]
`agy.<time>.old` backup(s) in `~/.local/bin` at recording time (the updater's
transient leftover, spec §3.5); `~/.cache/antigravity/staging` held
[STAGING_COUNT] entries; `~/.zshrc` and `~/.zprofile` each carry the
`# Added by Antigravity CLI installer` marker ([RC_MARKERS]). No Homebrew
`antigravity-cli` cask is installed on this Mac (agy.md §2), so the cask route
is not recorded; the shared-exclusion and PATH cases use synthetic unit tests.

## Uninstall list

Nothing here was recorded for the uninstall: Canager runs no command for it.
The list in `crates/canager-core/src/adapters/standalone/recipes.rs`
(`AGY.uninstall`, `AGY.backup_globs`) is not a vendor document — Google
publishes none and there is no `agy uninstall` (agy.md §5). It is the install
script's own path (`TARGET_DIR=$HOME/.local/bin`, `BINARY_PATH=$TARGET_DIR/agy`,
read from the script) plus the Homebrew cask's `zap` stanza, which trashes only
`~/.gemini/antigravity-cli`. Canager moves `~/.local/bin/agy` (the whole
program) after any `agy.<time>.old` beside it; it keeps `~/.gemini/antigravity-cli`
(conversations, history and the program's own state together; no vendor list
separates them), `~/.cache/antigravity` (the installer's staging folder, directly
in `~/.cache`, which Canager never moves anything out of — spec §6.3 listed it for
removal; the step D plan's ruling 1 keeps it), and the two shell files.
```

- [ ] **Step 2: Record Grok Build on this Mac (read-only commands only)**

**Never run `grok update` without `--check`, or bare `grok`, here.** The two `grok` invocations are `--version` (run **once**, into the fixture) and `update --check --json`, whose `--help` says "Check for updates without installing" (grok.md §4; run on this Mac during the research). Two things are UNVERIFIED (grok.md §5, open question 2): whether `--version` runs grok's launch-time updater at all, and whether that updater *installs* or only checks — and Canager's refresh runs `grok --version` three to four times. So this recording has agy's stop rule, not a weaker one (ruling 16): a snapshot of `~/.grok/bin` and `~/.grok/downloads` (`ls -lan`), of `readlink ~/.grok/bin/grok`, and of `~/.grok/version.json`'s mtime (it carries `checked_at`) is taken *before and after each* grok invocation. A changed link target or a new entry in either folder is a stop. A moved `version.json` mtime alone is not a stop but is recorded as what it is: after `--version`, evidence that the launch-time path is reachable from a version read (an open question the trust file states, not a pre-decided "changes no recipe"); after the check, expected — grok's own check writing its time, the one write on the Mac a Canager refresh causes.

```bash
S="${TMPDIR:-/tmp}/canager-grok-recording"; mkdir -p "$S"   # scratch, never committed
snap() { ls -lan ~/.grok/bin ~/.grok/downloads | sed "s|$HOME|~|g"; readlink ~/.grok/bin/grok; }

# 1. Before: the layout snapshot and version.json's mtime.
snap > "$S/layout-0.txt"
VJSON_BEFORE=$(stat -f '%m' ~/.grok/version.json 2>/dev/null || echo absent)
echo "version.json mtime=$VJSON_BEFORE"                                               # -> [VJSON_BEFORE]

# 2. The installed version, run once into a scratch file: `grok <version>
#    (<hash>)`, the second token is VERSION_GROK (grok.md §1). Same rule as
#    agy for the three places that must agree.
~/.grok/bin/grok --version > "$S/version.txt"
cat "$S/version.txt"                                                                   # -> grok 1.0.41 (4220f3b224a6)
VERSION_GROK=$(awk '{ print $2 }' "$S/version.txt")
D="adapters/fixtures/standalone-grok/$VERSION_GROK"; mkdir -p "$D"
mv "$S/version.txt" "$D/version.txt"
sleep 10
snap > "$S/layout-1.txt"
VJSON_MID=$(stat -f '%m' ~/.grok/version.json 2>/dev/null || echo absent)             # -> [VJSON_AFTER_VERSION]
diff "$S/layout-0.txt" "$S/layout-1.txt" && echo unchanged                            # -> [LAYOUT_AFTER_VERSION]: "unchanged" expected

# 3. grok's own read-only check, byte for byte.
~/.grok/bin/grok update --check --json > "$D/update-check.json"
sleep 10
snap > "$S/layout-2.txt"
VJSON_AFTER=$(stat -f '%m' ~/.grok/version.json 2>/dev/null || echo absent)           # -> [VJSON_AFTER_CHECK]
diff "$S/layout-1.txt" "$S/layout-2.txt" && echo unchanged                            # -> [LAYOUT_AFTER_CHECK]: "unchanged" expected

# 4. The layout as recorded: the two link-holding folders, owners numeric,
#    home abbreviated. The link texts are relative (`../downloads/…`) and
#    carry nothing personal. (The same `ls` the snapshots took.)
ls -lan ~/.grok/bin ~/.grok/downloads | sed "s|$HOME|~|g" > "$D/layout.txt"

# 5. For the README only.
date +%F; sw_vers -productVersion; uname -m                                            # -> [DATE], [MACOS], [ARCH]
~/.grok/bin/grok update --help | grep -- '--check'                                     # -> [CHECK_HELP]: the line saying "without installing"
for p in ~/.grok/bundled ~/.grok/completions ~/.config/fish/completions/grok.fish ~/.local/bin/grok ~/.local/bin/agent /usr/local/bin/grok /usr/local/bin/agent; do if [ -e "$p" ] || [ -L "$p" ]; then echo "present ${p/#$HOME/~}"; else echo "absent  ${p/#$HOME/~}"; fi; done   # -> [OPTIONAL_PATHS]
grep -c 'grok installer' ~/.zshrc 2>/dev/null                                          # -> [RC_MARKERS] (2 expected: the >>> and <<< lines)
ls -1 ~/.grok/downloads | wc -l | tr -d ' '                                            # -> [DOWNLOADS_COUNT]
```

**If either `diff` printed anything — a changed link target, a new or renamed entry in `bin/` or `downloads/` — stop.** The version read reached grok's updater and the updater changed the install; the recipe's version read must change for this version (spec §3.4's shapes: read `~/.grok/version.json`'s `version` instead of running the launcher, or `version: None` with a sentence), which is the author's decision. Report both snapshots in the handover and do not commit the recording. `[VJSON_AFTER_VERSION] != [VJSON_BEFORE]` with an unchanged layout is not a stop: it is recorded in the README and in `## Grok Build` as the open question it is (the launch-time path was reached; whether it can install is what the layout diff watched for, and on this run it did not). `[VJSON_AFTER_CHECK] != [VJSON_AFTER_VERSION]` is expected.

Then write `adapters/fixtures/standalone-grok/$VERSION_GROK/README.md`:

```markdown
# Grok Build [VERSION_GROK] fixtures (native installer route, `standalone-grok`)

Recorded [DATE] on the author's MacBook (macOS [MACOS], [ARCH]) by running the
commands below. `version.txt` and `update-check.json` are the commands' output
byte for byte. `layout.txt` is `ls -lan` (numeric owner and group) with the home
folder's absolute path replaced by `~` (`sed "s|$HOME|~|g"`) — the only two
edits. The recording itself changed nothing.

Commands:
- `~/.grok/bin/grok --version` -> `version.txt` (`grok <version> (<hash>)`; the
  second token is the version; no environment variable — none is documented)
- `~/.grok/bin/grok update --check --json` -> `update-check.json` (grok's own
  read-only check: its `--help` line for `--check` reads `[CHECK_HELP]`; one
  JSON object whose `updateAvailable` Canager believes and whose
  `latestVersion` it shows)
- `ls -lan ~/.grok/bin ~/.grok/downloads | sed "s|$HOME|~|g"` -> `layout.txt`
  (`bin/grok` and `bin/agent` are relative links, `../downloads/grok-<v>-macos-aarch64`,
  into the root; [DOWNLOADS_COUNT] downloads were present, older versions kept
  by `grok update`)

The version read did not change the install (spec §3.4, checked around the one
`--version` run of this recording, whose output is `version.txt`): `ls -lan` of
`~/.grok/bin` and `~/.grok/downloads` and `readlink ~/.grok/bin/grok` were
[LAYOUT_AFTER_VERSION] ten seconds after `--version` and [LAYOUT_AFTER_CHECK]
ten seconds after the check. `~/.grok/version.json` (which records `checked_at`)
had mtime [VJSON_BEFORE] before, [VJSON_AFTER_VERSION] after `--version`, and
[VJSON_AFTER_CHECK] after the check. Whether `--version` alone runs grok's
launch-time updater is UNVERIFIED, and whether that updater installs or only
checks is UNVERIFIED too (grok.md §5, open question 2): a moved mtime after
`--version` means the launch-time path was reached by the read, and the
unchanged layout is the whole of what this recording can say about installing.
The check's own write to `version.json` is grok's, made on every Canager
refresh; `docs/what-we-run.md` says so.

The recording ran no `grok update` without `--check` and no bare `grok`. How
`grok update` behaves with its input closed has not been observed by this
project; the author records it on a CI runner before this step merges
(docs/superpowers/plans/2026-09-25-phase-4-step-d-grok-agy.md, "The author's
pre-merge verification").

Optional paths at recording time: [OPTIONAL_PATHS] (each line `present`/`absent`
followed by the path). The Homebrew cask `grok-build` is not installed on this
Mac (grok.md §2), so that route is not recorded. `~/.zshrc` carries the
installer's marked block ([RC_MARKERS] marker lines).

## Uninstall list

Nothing here was recorded for the uninstall: Canager runs no command for it.
The list in `crates/canager-core/src/adapters/standalone/recipes.rs`
(`GROK.uninstall`) is not a vendor document — xAI publishes none and there is
no `grok uninstall` (grok.md §6). It is the README grok ships ("File
Locations") plus its install script: Canager moves the two optional fallback
links the installer makes when `~/.grok/bin` is not on PATH (first: their link
text is unverified, so they go while every folder it could pass through is
still there — a precaution), `~/.grok/downloads`, `~/.grok/bundled` and
`~/.grok/completions` (optional), the fish completion the installer also
writes (optional), then the two links the installer put in `~/.grok/bin` —
`agent`, and `grok` last. The folder `~/.grok/bin` itself is not moved: the
installer put it on `PATH`, so it may hold the user's own scripts, and it stays,
emptied, inside `~/.grok`. It keeps `~/.grok` (`config.toml`, `auth.json`,
`sessions/`, `memory/`, `skills/`, `plugins/`: the de-facto `rm -rf ~/.grok`
would take the login, sessions and memory, which spec Q4 keeps) and `~/.zshrc`,
and reports — never touches — a `/usr/local/bin/grok` or `/usr/local/bin/agent`
when it is a link into `~/.grok` (never Homebrew's link or another program's
file), since that one becomes a dead link.
```

If either recorded version differs from the meta file's, change `verified_versions` in `adapters/meta/standalone-agy.toml` / `standalone-grok.toml` to the recorded number (the three places must agree: directory name, meta, trust file).

- [ ] **Step 3: Continue the task**

No run yet: the registration follows.

- [ ] **Step 4: Write the failing tests (registration, roots, fixtures, the trust file)**

In `crates/canager-core/src/adapters/standalone/recipes.rs`, replace E's `test_recipes_lists_each_registered_tool_once_in_reading_order`'s `assert_eq!(RECIPES.len(), 2);` with `assert_eq!(RECIPES.len(), 4);` (its uniqueness assertion stays), and in `test_every_command_latest_source_only_checks` change `for recipe in RECIPES.iter().chain([&&AGY, &&GROK]) {` to `for recipe in RECIPES {`.

In `crates/canager-core/src/session/mod.rs`, replace E's `test_new_registers_all_nine_adapters` with:

```rust
    #[test]
    fn test_new_registers_all_eleven_adapters() {
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
                "standalone-agy".to_string(),
                "standalone-claude".to_string(),
                "standalone-grok".to_string(),
                "standalone-rustup".to_string(),
                "uv".to_string(),
            ]
        );
    }
```

In `crates/canager-core/src/scan/mod.rs`, in `test_owned_roots_table`, after the `claude` block (its `assert_eq!(owned_roots(&claude), vec![PathBuf::from("/Users/someone/.local/share/claude")]);`) add:

```rust
        // The two other path-list tools own their roots the same way
        // (phase 4 step D): agy's `~/.gemini/antigravity-cli`, grok's
        // `~/.grok`, each the instance's `prefix`.
        for (adapter, prefix) in [
            ("standalone-agy", "/Users/someone/.gemini/antigravity-cli"),
            ("standalone-grok", "/Users/someone/.grok"),
        ] {
            let inst = ManagerInstance {
                prefix: PathBuf::from(prefix),
                ..crate::testing::manager_instance(adapter, adapter)
            };
            assert_eq!(owned_roots(&inst), vec![PathBuf::from(prefix)], "{adapter}");
        }
```

In `crates/canager-core/src/adapters/standalone/mod.rs`'s `mod tests`, after B's `fixture(name)` helper add:

```rust

    /// A file of a recipe's recorded fixture directory: the one version its
    /// meta names (`verified_versions[0]`), under
    /// `adapters/fixtures/standalone-<id>/`. B's `fixture` is claude's.
    fn recorded(recipe: &'static Recipe, name: &str) -> String {
        let meta = AdapterMeta::from_toml(recipe.meta_toml).expect("meta");
        let version = meta
            .verified_versions
            .first()
            .expect("meta lists the recorded version");
        let path = format!(
            "../../adapters/fixtures/standalone-{}/{version}/{name}",
            recipe.id
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
    }

    #[test]
    fn test_agys_recorded_version_line_is_one_bare_version_matching_the_meta() {
        let meta = AdapterMeta::from_toml(AGY.meta_toml).expect("meta");
        let line = recorded(&AGY, "version.txt");
        let version = latest::parse_version(&line, AGY.version.parse).expect("a version");
        assert_eq!(Some(&version), meta.verified_versions.first());
        assert!(latest::is_dotted_version(&version), "{version:?}");
        assert_eq!(
            line.trim(),
            version,
            "agy prints the bare version and nothing else (agy.md §4)"
        );
    }

    #[test]
    fn test_agys_recorded_manifest_names_a_version_the_check_can_compare() {
        // The manifest may be newer than the recorded launcher (agy updates
        // itself between the two reads): what is pinned is that the check
        // can read and compare it, not which way the comparison goes.
        let manifest = recorded(&AGY, "manifest-darwin_arm64.json");
        let Latest::HttpJsonField { field, .. } = AGY.latest else {
            panic!("agy reads a manifest");
        };
        let remote = latest::parse_json_field(&manifest, field).expect("a version");
        let local = latest::parse_version(&recorded(&AGY, "version.txt"), AGY.version.parse).unwrap();
        assert!(latest::compare_dotted(&local, &remote).is_some(), "{local} vs {remote}");
    }

    #[test]
    fn test_groks_recorded_version_line_yields_the_second_token_matching_the_meta() {
        let meta = AdapterMeta::from_toml(GROK.meta_toml).expect("meta");
        let line = recorded(&GROK, "version.txt");
        assert!(line.starts_with("grok "), "{line:?}");
        let version = latest::parse_version(&line, GROK.version.parse).expect("a version");
        assert_eq!(Some(&version), meta.verified_versions.first());
        assert!(latest::is_dotted_version(&version), "{version:?}");
    }

    #[test]
    fn test_groks_recorded_update_check_parses_and_names_the_installed_version_when_nothing_is_newer() {
        let body = recorded(&GROK, "update-check.json");
        let Latest::Command {
            latest_field,
            available_field,
            error_field,
            ..
        } = GROK.latest
        else {
            panic!("grok asks itself");
        };
        let check = latest::parse_update_check(&body, latest_field, available_field, error_field)
            .expect("grok's JSON: both fields present, `error` null");
        // `latest` is shown, never compared, and the recipe takes it with
        // any suffix (a prerelease day is a truthful recording too), so it
        // is not held to a dotted shape here. When grok said nothing was
        // available, its latest is the installed version it also printed.
        if !check.available {
            let local = latest::parse_version(&recorded(&GROK, "version.txt"), GROK.version.parse).unwrap();
            assert_eq!(check.latest, local);
        }
    }
```

In `crates/canager-core/tests/what_we_run_test.rs`, replace C's `test_what_we_run_names_every_path_claude_codes_uninstall_moves_or_keeps` (from its `#[test]` through its closing `}`; landed at `what_we_run_test.rs:191-218`, iterating `CLAUDE.uninstall` only) with the test below, which pins every `Paths` recipe's section — so the two new sections cannot drift from `AGY`/`GROK` any more than Claude Code's could — and pins the never-list to the state each list keeps. Change the file's `use canager_core::adapters::standalone::recipes::CLAUDE;` (line 18 at `db42e79`) to `use canager_core::adapters::standalone::recipes::RECIPES;` (if another landed test still reads `CLAUDE`, import both), and add `use canager_core::model::KeptWhat;` (or `KeptWhat` to an existing `use canager_core::model::{…}` line, if one exists); `AdapterMeta`, `Uninstall` and `TIMEOUT_SECS` are already imported for the landed tests (lines 17-20).

```rust
#[test]
fn test_what_we_run_names_every_path_a_path_list_uninstall_moves_or_keeps() {
    // The lists are the recipes', and a reader deciding whether to press
    // Uninstall reads them here: a path added to or dropped from a
    // recipe's `uninstall` or `backup_globs` without its section changing
    // is a trust file that no longer says what Canager moves. Every
    // `Paths` recipe (Claude Code, Antigravity CLI, Grok Build since step
    // D), by the name its meta gives its section; each section states the
    // uninstall's budget; and every settings-and-state path a list keeps
    // (`Settings`, `SettingsAndHistory`, `ToolState`) is also named in the
    // never-list, whose promise is the one the reader relies on.
    let doc = read_doc();
    let never = section_body(&doc, "What Canager never does")
        .unwrap_or_else(|| panic!("docs/what-we-run.md has no `## What Canager never does` section"));
    let budget = format!("{TIMEOUT_SECS} s");
    let mut paths_recipes = 0;
    for recipe in RECIPES {
        let Some(Uninstall::Paths { remove, keep }) = &recipe.uninstall else {
            continue;
        };
        paths_recipes += 1;
        let meta = AdapterMeta::from_toml(recipe.meta_toml).expect("meta");
        let body = section_body(&doc, &meta.name)
            .unwrap_or_else(|| panic!("docs/what-we-run.md has no `## {}` section", meta.name));
        let listed = remove
            .iter()
            .map(|spec| spec.path)
            .chain(keep.iter().map(|spec| spec.path))
            .chain(recipe.backup_globs.iter().map(|glob| glob.dir));
        for path in listed {
            assert!(
                body.contains(&format!("`{path}`")),
                "the `## {}` section of docs/what-we-run.md does not name `{path}`, which {}'s uninstall lists",
                meta.name,
                recipe.id
            );
        }
        assert!(
            body.contains(&budget),
            "the `## {}` section of docs/what-we-run.md does not state the uninstall's budget, {budget:?} (removal::TIMEOUT_SECS)",
            meta.name
        );
        for spec in keep.iter().filter(|spec| {
            matches!(
                spec.what,
                KeptWhat::Settings | KeptWhat::SettingsAndHistory | KeptWhat::ToolState
            )
        }) {
            assert!(
                never.contains(&format!("`{}`", spec.path)),
                "the never-list of docs/what-we-run.md does not promise `{}` stays, which {}'s uninstall keeps",
                spec.path,
                recipe.id
            );
        }
    }
    assert_eq!(paths_recipes, 3, "Claude Code, Antigravity CLI and Grok Build uninstall by path list");
}
```

and after `test_what_we_run_has_the_unknown_scan_section_stating_both_of_its_limits` add:

```rust

#[test]
fn test_what_we_run_states_the_read_only_check_command_of_every_tool_that_asks_itself() {
    // A `Latest::Command` recipe runs the tool's own subcommand on every
    // refresh (grok's `update --check --json`, which its --help calls a
    // check "without installing"). The section for that tool has to show
    // the argv and say it installs nothing -- a reader who sees `grok
    // update` in a refresh table and nothing more would think Canager
    // upgrades grok behind their back.
    use canager_core::adapters::standalone::recipe::Latest;
    use canager_core::adapters::standalone::recipes::RECIPES;
    let doc = read_doc();
    for recipe in RECIPES {
        let Latest::Command { args, .. } = recipe.latest else {
            continue;
        };
        let meta = AdapterMeta::from_toml(recipe.meta_toml).expect("meta");
        let body = section_body(&doc, &meta.name)
            .unwrap_or_else(|| panic!("docs/what-we-run.md has no `## {}` section", meta.name));
        let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            folded.contains(&args.join(" ")),
            "the `## {}` section does not show `{}`",
            meta.name,
            args.join(" ")
        );
        assert!(
            folded.contains("without installing"),
            "the `## {}` section does not say the check installs nothing",
            meta.name
        );
    }
}
```

- [ ] **Step 5: Run to verify they fail**

Run: `cargo test -p canager-core`
Expected: FAIL — `test_recipes_lists_each_registered_tool_once_in_reading_order` (`RECIPES.len()` is 2); `test_new_registers_all_eleven_adapters` (no `standalone-agy`/`standalone-grok`); `fixtures_layout_test::test_every_registered_adapter_has_a_documented_fixture_directory` (two fixture directories no registered adapter matches); `test_owned_roots_table` (the `_` arm answers `vec![]` for the two); the four fixture tests PASS already (they read the recording through the constants). `what_we_run_test`'s rewritten path test FAILS on `paths_recipes == 3` (one `Paths` recipe registered) and its new `--check` test PASSES vacuously (no registered `Command` recipe yet); both fail after registration until Step 6's sections exist — the order of edits in Step 6 keeps every gate red until the last edit.

- [ ] **Step 6: Register, own the roots, write the sections**

In `crates/canager-core/src/adapters/standalone/recipes.rs`, change E's `pub static RECIPES: &[&Recipe] = &[&CLAUDE, &RUSTUP];` to:

```rust
pub static RECIPES: &[&Recipe] = &[&CLAUDE, &AGY, &GROK, &RUSTUP];
```

In `crates/canager-core/src/scan/mod.rs`, in `owned_roots`, replace the arm `"standalone-claude" => vec![inst.prefix.clone()],` (with the comment above it, from `// A tool installed by its own installer owns its root` through the arm) with:

```rust
        // A tool installed by its own installer owns its root: Claude Code's
        // `~/.local/share/claude` (the `versions/<v>` store its launcher
        // links into), Antigravity's `~/.gemini/antigravity-cli`, Grok's
        // `~/.grok` (whose `downloads/` its two links resolve into). The
        // launcher itself is the instance's `exe_path` and rules 0/1 have
        // it; this row is for anything else that resolves under the root.
        // `standalone-rustup` never joins: its root is the Cargo home, whose
        // `bin/` is scanned; rule 1 has its launcher and proxies, rule 2 the
        // `cargo install`ed programs.
        "standalone-claude" | "standalone-agy" | "standalone-grok" => vec![inst.prefix.clone()],
```

and in `owned_roots`'s doc comment change the sentence `The standalone adapters add their tool roots as their recipes land -- … -- in the same change that first produces an instance with one of those ids.` (F's wording; E may have re-wrapped it) to `The standalone adapters own their tool roots -- \`standalone-claude\` → \`~/.local/share/claude\`, \`standalone-agy\` → \`~/.gemini/antigravity-cli\`, \`standalone-grok\` → \`~/.grok\`, each the instance's \`prefix\`; \`standalone-rustup\` nothing (its root is the Cargo home, whose \`bin/\` is scanned; rule 1 has the launcher and its proxies, rule 2 the \`cargo install\`ed programs).` Keep the paragraph's remaining sentence about a row without a producer.

In `crates/canager-core/src/lib.rs`, in the crate doc, change E's `(Claude Code, rustup). This crate is the part that does the work: the` to `(Claude Code, Antigravity CLI, Grok Build, rustup). This crate is the part that does the work: the`.

In `docs/what-we-run.md` (hard-wrapped; match by words, keep the wrapping style):

(a) In the opening paragraph, change E's `for the nine sources it manages today: Homebrew, npm, pipx, uv, pip (read-only), Cargo, Ollama, and two tools with their own installer, Claude Code and rustup.` to `for the eleven sources it manages today: Homebrew, npm, pipx, uv, pip (read-only), Cargo, Ollama, and four tools with their own installer: Claude Code, Antigravity CLI, Grok Build and rustup.`

(b) Under `## How Canager runs anything`, in `**Where the program comes from.**`, change E's `Claude Code at the one path its installer writes, rustup at \`$CARGO_HOME/bin/rustup\` (their sections).` to `Claude Code at \`~/.local/bin/claude\`, Antigravity CLI at \`~/.local/bin/agy\`, Grok Build at \`~/.grok/bin/grok\`, rustup at \`$CARGO_HOME/bin/rustup\` (their sections).`

(c) After the `## Claude Code` section (before `## rustup`) insert the two sections below. Every bracketed value is filled from Steps 1–2 before the commit: `[VERSION_AGY]`/`[VERSION_GROK]` the recorded versions, `[DATE]` the recording date, `[VJSON_VERSION_OBSERVATION: …]` and `[VJSON_CHECK_OBSERVATION: …]` one of the alternatives each bracket offers, chosen by `[VJSON_AFTER_VERSION]`/`[VJSON_AFTER_CHECK]` against their predecessors (the bracket and its alternatives are removed, the chosen words stay). `grep -n '\[' docs/what-we-run.md` after filling must show no bracket that is not a Markdown link:

```markdown

## Antigravity CLI

Adapter: `StandaloneAdapter` over the `AGY` recipe in
`crates/canager-core/src/adapters/standalone/` (`recipes.rs` is the data,
`mod.rs` the behaviour, `route.rs` the recognition, `removal.rs` the
uninstall). Verified against Antigravity CLI [VERSION_AGY] (the version in
`adapters/meta/standalone-agy.toml` and the name of the recorded fixture
directory). The row is one tool, installed by Google's own installer
(`curl -fsSL https://antigravity.google/cli/install.sh | bash`, run by the
user — Canager never runs it), and the one item under it is the tool
itself.

**Detect.** Canager looks at the fixed path the installer writes,
`~/.local/bin/agy` — never an `agy` found through `PATH` — and checks with
`lstat` and `realpath` that it is a regular file: the installer copies the
binary there, and the Homebrew cask's `agy` is a link into its Caskroom
and is Homebrew's row. There is no launcher-only state: the file *is* the
program. Canager then runs `<agy> --version` (30 s) with
`AGY_CLI_DISABLE_AUTO_UPDATE=true` in its environment, the switch Google
documents for its background updater. On the recorded version, `--version`
alone did not reach the updater at all — no new log file under
`~/.gemini/antigravity-cli/log`, `updater/update_status.json` untouched, no
updater process, checked around the very read the fixture records — so the
switch is a belt on top of that; a run with a prompt is what writes a log
and spawns the updater (agy.md §4). The version is the first token of the
first non-empty line (`[VERSION_AGY]`).

Canager also asks where `agy` would run from if typed in Terminal, as it
does for Claude Code, and says so under the source. That is a notice, not
a command.

**Environment Canager adds to version reads** (`AGY.version.env`):

    AGY_CLI_DISABLE_AUTO_UPDATE=true

**Read-only commands and requests** (background checks; never need a
password):

| Purpose | Argv or request | Timeout |
|---|---|---|
| Detect, inventory, the fresh update check, and the reading after an uninstall | `<agy> --version`, with `AGY_CLI_DISABLE_AUTO_UPDATE=true` | 30 s |
| Newest published version (`check_updates`), Apple silicon only | `GET https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/darwin_arm64.json` — the manifest the installer and the updater read; its top-level `version` | 30 s |

On an Intel Mac, or under Rosetta (a universal build reports `x86_64`), no
request is made and the row says the check is not yet verified there: only
the Apple-silicon manifest has been fetched. An update is listed only when
the manifest's version is greater than the installed one, comparing
dot-separated integers; a failed request, a non-200 answer or a body that
is not such a manifest is "could not check", never an error for the source.

**Write commands**: none. Antigravity CLI installs its updates itself in
the background (a 15-minute debounce, Google's documentation and this
Mac's own log), and its `agy update` subcommand is undocumented, has no
options and has never been run — so Canager offers no Update button: a
newer version is listed with the badge "Updates itself" and a sentence
that says to open the tool once and quit it. `Session::issue_plan` refuses
the upgrade as well, and so does the adapter.

**Uninstall** (only after the user reviews and confirms a preview; no
command runs): Canager moves to the Trash, in this order, any backup copy
`agy.<time>.old` the updater left in `~/.local/bin` (a regular file with
that name shape, each listed in the preview), then `~/.local/bin/agy`
itself — through the same call and the same checks as Claude Code's
uninstall ("Moving files to the Trash"). It keeps, and the preview says so
when they exist: `~/.gemini/antigravity-cli` (the tool's own root, where
its conversations, history, builtin skills, cache and updater state live
together — no vendor list says which of them could go alone, and the
Homebrew cask's `zap` treats it as one folder; `~/.gemini` itself is shared
with Gemini CLI and is never touched), `~/.cache/antigravity` (the
installer's download staging folder, usually empty: it sits directly in
`~/.cache`, one of the folders Canager never moves anything out of), and
`~/.zshrc` and `~/.zprofile`, to which the installer added its `PATH` line
(Canager never edits a startup file, and does not read these to find the
line). The whole uninstall has 120 s, as Claude Code's does. There is no
vendor uninstall document; the list is the installer script's own path
plus the cask's `zap`, and the fixture README says so.

## Grok Build

Adapter: `StandaloneAdapter` over the `GROK` recipe in
`crates/canager-core/src/adapters/standalone/`. Verified against Grok
Build [VERSION_GROK] (the version in `adapters/meta/standalone-grok.toml`
and the name of the recorded fixture directory). The row is one tool,
installed by xAI's own installer (`curl -fsSL https://x.ai/cli/install.sh
| bash`, run by the user — Canager never runs it), and the one item under
it is the tool itself.

**Detect.** Canager looks at the fixed path the installer writes,
`~/.grok/bin/grok`, and checks with `lstat`, `readlink` and `realpath` that
it is a symbolic link — a relative one, `../downloads/grok-<version>-macos-aarch64`
— whose own text names a place inside `~/.grok` and which resolves there
(the installer's layout; `bin/agent` is a second link to the same file).
A `grok` there that resolves into a `Cellar`, `Caskroom`, `node_modules`
or `corepack` directory is a package manager's copy and is not listed
here; the Homebrew cask `grok-build` puts its links in `/opt/homebrew/bin`
and is Homebrew's row; the Homebrew *formula* named `grok` is an unrelated
library. A dangling link whose own text points into `~/.grok` (the
downloads folder was removed — by an uninstall that stopped partway, or by
hand) is listed with no version and a notice saying so, and Uninstall
removes what is left. For a link that resolves, Canager runs `<grok>
--version` (30 s) with no added environment (none is documented). Whether
`--version` runs grok's launch-time updater, and whether that updater
installs or only checks, are both unverified; on the recorded version
(`[VERSION_GROK]`, [DATE]) `--version` left `~/.grok/bin` and
`~/.grok/downloads` unchanged and `~/.grok/version.json`'s timestamp
[VJSON_VERSION_OBSERVATION: "unchanged" | "moved, so the launch-time
path was reached by the read; the layout did not change"], as the
fixture README records around the very read it holds. The version is the
second token of the first non-empty line (`grok 1.0.41 (4220f3b224a6)`).

Canager also asks where `grok` would run from if typed in Terminal and
says so under the source. That is a notice, not a command.

**Read-only commands** (background checks; never need a password):

| Purpose | Argv | Timeout |
|---|---|---|
| Detect, inventory, and the reading before and after an operation | `<grok> --version` | 30 s |
| Newest published version (`check_updates`) | `<grok> update --check --json` — grok's own check; its `--help` describes `--check` as "Check for updates without installing" | 60 s |

Grok's own check prints one JSON object; Canager believes its
`updateAvailable` and shows its `latestVersion`, comparing nothing itself
(the channel is the tool's own, "Native"). A check that exits non-zero,
prints something that is not that JSON, does not finish in 60 seconds, or
answers with a non-null `error` field (grok could not find out — say,
offline) is "could not check" with grok's own words, never "up to date"
and never an error for the source. Canager makes no network request of
its own for grok; the check's connection is grok's, under grok's
configuration (`~/.grok/config.toml`, which Canager does not read). The
check records its time in `~/.grok/version.json` (`checked_at`): that
file's timestamp [VJSON_CHECK_OBSERVATION: "moved" | "did not move"] on
[DATE] after the recorded check — grok's own write, made on every Canager
refresh, and the one change on the Mac a refresh causes ("Files Canager
writes"). Whether grok installs updates on its own (`auto_update = true`
means "check for updates on launch") is unverified, so the row is not
described as self-updating.

**Write commands** (only run after the user reviews and confirms a plan
preview):

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Upgrade | `<grok> update` | 1800 s | No |

`grok update` downloads the new version into `~/.grok/downloads` and
re-points the `bin/` links, leaving the old download in place (the
installer's layout; the update's own steps were not read). Cancel: allowed
(`KillThenReconcile`) — the runner stops the process group, Canager reads
`<grok> --version` again, and the operation is reported as unconfirmed
regardless of that reading. An update that exits 0 with the version
unchanged is reported as needing attention, as for every source. **How
`grok update` behaves when nothing can answer a prompt (Canager gives it
no terminal and a closed stdin) has not been observed by this project**;
the author records it on a CI runner before this step merges, and this
paragraph then says what was seen.

**Uninstall** (only after the user reviews and confirms a preview; no
command runs): Canager moves to the Trash, in this order, `~/.local/bin/grok`
and `~/.local/bin/agent` when the installer made them (it does so only when
`~/.grok/bin` was not on `PATH`; they go first, while every folder their
link text could pass through is still there — what that text says has not
been checked on a Mac that has them), `~/.grok/downloads` (the program:
every downloaded version), `~/.grok/bundled` and `~/.grok/completions`
(the vendored agents and shell completions, when present),
`~/.config/fish/completions/grok.fish` (when present), then the two links
the installer put in `~/.grok/bin`: `~/.grok/bin/agent` (when present) and
last `~/.grok/bin/grok`, the command itself. The folder `~/.grok/bin` is
not moved: the installer put it on your `PATH`, so a script of your own
may be in it, and it stays, empty, inside `~/.grok`. Each path passes the
checks Claude Code's section describes; an optional one Canager cannot
confirm is grok's own — a `~/.local/bin/agent` that belongs to another
program, say — stays and the preview says so. The whole uninstall has
120 s, as Claude Code's does. It keeps `~/.grok` itself (`config.toml`,
`auth.json` — the login —, `sessions/`, `memory/`, `skills/`, `plugins/`)
and `~/.zshrc`, to which the installer added its marked block; a
`/usr/local/bin/grok` or `/usr/local/bin/agent` is outside your home
folder, so Canager never touches it — and when it is a link into
`~/.grok` (the installer's fallback), the preview says it becomes a dead
link; when it is something else (Homebrew's `grok-build` link on an Intel
Mac, another program's `agent`), the preview says nothing about it. There
is no vendor uninstall document and no `grok uninstall`; the list is
grok's own README ("File Locations") plus its install script, and the
fixture README says so.
```

(d) Under `## Files Canager reads`, after the `Claude Code:` bullet (and E's rustup bullet, if it sits there), add:

```markdown
- Antigravity CLI: whether `~/.local/bin/agy` exists and what it is (`lstat`,
  `realpath`); for the notice under the source, each `PATH` directory's
  `agy` as for Claude Code; during the uninstall preview, the names in
  `~/.local/bin` (for `agy.<time>.old` backups) and whether
  `~/.gemini/antigravity-cli`, `~/.cache/antigravity`, `~/.zshrc` and
  `~/.zprofile` exist (`lstat`; their contents are not read). The Unknown
  page's scan reads the same names for its rule 4.
- Grok Build: whether `~/.grok/bin/grok` exists and where it links to
  (`lstat`, `readlink`, `realpath`, also for `~/.grok`); for the notice,
  each `PATH` directory's `grok`; during the uninstall preview, whether
  `~/.grok/downloads`, `~/.grok/bundled`, `~/.grok/completions`,
  `~/.config/fish/completions/grok.fish`, `~/.local/bin/grok`,
  `~/.local/bin/agent`, `~/.grok`, `~/.zshrc`, `/usr/local/bin/grok` and
  `/usr/local/bin/agent` exist and what they are (`lstat`, `realpath`).
  Nothing in `~/.grok/config.toml` or `~/.grok/auth.json` is read.
```

(e) Under `## What Canager never does`, three edits. First, in C's bullet beginning `- Never deletes a file and never empties the Trash. Never writes a file`, change `Never writes a file on the Mac itself other than its own \`settings.json\`, and moves files` to `Never writes a file on the Mac itself other than its own \`settings.json\` (the one write a refresh causes is grok's own: its update check records its time in \`~/.grok/version.json\`, Grok Build's section), and moves files`. Second, in C's bullet beginning `- Never moves anything outside the home folder`, replace its last clause

```
  describe; never moves the settings, login and history Claude Code keeps
  in `~/.claude` (of that folder only its download cache,
  `~/.claude/downloads`) or `~/.claude.json`, nor anything they lead to.
```

with

```
  describe; never moves the settings, login and history Claude Code keeps
  in `~/.claude` (of that folder only its download cache,
  `~/.claude/downloads`) or `~/.claude.json`, the login, sessions, memory
  and settings Grok Build keeps in `~/.grok` (of that folder only
  `downloads/`, `bundled/`, `completions/` and the two links in `bin/`),
  or anything in Antigravity CLI's `~/.gemini/antigravity-cli` — nor
  `~/.gemini` itself, which Gemini CLI shares — nor anything they lead to.
```

(`test_what_we_run_names_every_path_a_path_list_uninstall_moves_or_keeps` holds this bullet to every `Settings`/`SettingsAndHistory`/`ToolState` path the three lists keep.) Third, after the bullet about moving files only to the Trash (C's), add:

```markdown
- Never runs `agy update` (undocumented, never observed), and never runs
  `grok update` from a refresh: the refresh runs `grok update --check
  --json`, which grok's own help describes as checking without
  installing; `grok update` runs only after a confirmed preview.
- Never opens a tool to make it update itself: a self-updating tool's row
  tells the user how, and Canager runs nothing.
```

(h) Under `## Files Canager writes`, after the sentence ending `moves the paths its preview listed to the Trash (next section).` (C's; E may have changed `Claude Code, today` — match by words), and before `Every other change to what is installed`, insert:

```
One write on the Mac is caused by a refresh without being Canager's: Grok
Build's own update check (`grok update --check --json`, Grok Build's
section) records the time of the check in `~/.grok/version.json`.
```

(f) In the `## Moving files to the Trash` section, change C's `and it is called only by a confirmed path-list uninstall (\`removal::execute_removal\`, Claude Code's section)` to `and it is called only by a confirmed path-list uninstall (\`removal::execute_removal\`; the Claude Code, Antigravity CLI and Grok Build sections)`.

(g) In the network section's last paragraph, change E's list of tools that make their own connections (`\`ollama pull\`, \`claude update\` and \`rustup self update\`` or however E worded it) to include `\`grok update --check --json\`` and `\`grok update\``, keeping the sentence's shape.

- [ ] **Step 7: Run to verify they pass**

Run: `cargo test -p canager-core`
Expected: PASS — `test_new_registers_all_eleven_adapters`; `fixtures_layout_test` (eleven directories, eleven ids, one README each); `what_we_run_test` (the two sections, every host, the `--check` test now over grok, the path test over three recipes and the never-list); `test_recipes_lists_each_registered_tool_once_in_reading_order`; `test_every_recipe_latest_url_is_an_allowed_https_host` (agy's host), `test_every_recipe_path_is_under_home_or_the_cargo_home`, `test_a_paths_recipe_names_only_home_paths`, `test_every_uninstall_path_is_under_home_and_not_in_a_shared_folder`, `test_every_paths_recipe_moves_its_launcher_last_and_lists_no_path_inside_another`, `test_every_backup_glob_is_under_home_and_names_a_pattern`, `test_backup_globs_lists_every_recipe_under_its_adapter_id` (now over four recipes), B's `test_every_recipe_launcher_is_named_after_its_id` and `test_every_recipe_meta_parses_and_names_the_standalone_id`; `test_owned_roots_table`; the four fixture tests; everything before.

- [ ] **Step 8: The end-to-end test through `Session`**

Create `crates/canager-core/tests/standalone_agy_grok_test.rs`:

```rust
//! The two step D recipes through `Session`: the gate refusing agy's
//! upgrade from its candidate, and grok's uninstall from the refresh
//! through the gate, the operation manager and the reading after -- so the
//! seams the unit tests exercise one at a time are proven joined. Every
//! layout is synthetic, in a temp home; every name is invented; the Trash
//! is a `MockTrasher` (a temp directory).

use canager_core::adapters::standalone::recipes::{AGY, GROK};
use canager_core::adapters::standalone::StandaloneAdapter;
use canager_core::adapters::{Adapter, AdapterError, CheckOptions};
use canager_core::events::{OpId, VecSink};
use canager_core::http::{HttpResponse, MockHttpClient};
use canager_core::model::{
    ArtifactKind, OpKind, OpRequest, OpStatus, Outcome, PlanAction, UpdateBlocked,
};
use canager_core::runner::{CommandOutput, HostEnv, MockRunner};
use canager_core::session::Session;
use canager_core::trash::MockTrasher;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A fresh, canonical home directory for one test, removed when the test
/// ends (macOS's `/var/folders` is `/private/var/…`).
struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Home {
        let raw = std::env::temp_dir().join(format!(
            "canager-agy-grok-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&raw).expect("create temp home");
        Home(std::fs::canonicalize(&raw).expect("canonical temp home"))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn dir(&self, rel: &str) -> PathBuf {
        let dir = self.0.join(rel);
        std::fs::create_dir_all(&dir).expect("create dir");
        dir
    }

    fn file(&self, rel: &str) -> PathBuf {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create parent");
        std::fs::write(&path, b"x").expect("write file");
        path
    }

    fn executable(&self, rel: &str) -> PathBuf {
        let path = self.file(rel);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    fn link(&self, rel: &str, target: &Path) -> PathBuf {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).expect("create parent");
        std::os::unix::fs::symlink(target, &path).expect("symlink");
        path
    }

    /// `HostEnv` for this home, as the user who owns it, with nothing on
    /// `PATH`.
    fn env(&self) -> HostEnv {
        HostEnv {
            path_dirs: Vec::new(),
            home: self.0.clone(),
            euid: std::fs::metadata(&self.0).expect("stat home").uid(),
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        }
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn exited_0(stdout: &str) -> CommandOutput {
    CommandOutput {
        exit_code: Some(0),
        stdout: stdout.to_string(),
        stderr: String::new(),
        timed_out: false,
        cancelled: false,
    }
}

const AGY_MANIFEST_URL: &str =
    "https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/darwin_arm64.json";

/// Both tools installed in `home`; agy at 1.2.10 with a 1.2.11 manifest,
/// grok at 1.0.41 with its own check saying nothing is newer; the Trash is
/// `trasher`. Returns the session and grok's launcher.
fn session_with(home: &Home, trasher: Arc<MockTrasher>) -> (Arc<Session>, PathBuf) {
    let agy = home.executable(".local/bin/agy");
    home.file(".gemini/antigravity-cli/conversations/c1.jsonl");
    let real = home.executable(".grok/downloads/grok-1.0.41-macos-aarch64");
    let target = Path::new("../downloads/grok-1.0.41-macos-aarch64");
    let grok = home.link(".grok/bin/grok", target);
    home.link(".grok/bin/agent", target);
    home.file(".grok/bundled/agents/default.md");
    home.file(".grok/config.toml");
    home.file(".grok/auth.json");
    home.file(".grok/sessions/s1.jsonl");
    home.file(".zshrc");
    let _ = real;

    let runner = Arc::new(MockRunner::new());
    runner.respond(vec![agy.to_str().unwrap(), "--version"], exited_0("1.2.10\n"));
    runner.respond(
        vec![grok.to_str().unwrap(), "--version"],
        exited_0("grok 1.0.41 (4220f3b224a6)\n"),
    );
    runner.respond(
        vec![grok.to_str().unwrap(), "update", "--check", "--json"],
        exited_0(r#"{"currentVersion":"1.0.41","latestVersion":"1.0.41","updateAvailable":false}"#),
    );
    let http = Arc::new(MockHttpClient::new());
    http.respond(
        AGY_MANIFEST_URL,
        HttpResponse {
            status: 200,
            body: r#"{"version":"1.2.11","url":"x","sha512":"y"}"#.to_string(),
        },
    );
    let agy_adapter = StandaloneAdapter::new(&AGY, runner.clone(), http.clone(), trasher.clone())
        .with_trash_gap(Duration::ZERO)
        .with_arch("aarch64");
    let grok_adapter = StandaloneAdapter::new(&GROK, runner, http, trasher)
        .with_trash_gap(Duration::ZERO);
    let session = Session::with_adapters(
        Arc::new(VecSink::new()),
        vec![
            Arc::new(agy_adapter) as Arc<dyn Adapter>,
            Arc::new(grok_adapter) as Arc<dyn Adapter>,
        ],
        None,
    );
    (session, grok)
}

fn request(instance_id: &str, kind: OpKind, name: &str) -> OpRequest {
    OpRequest {
        kind,
        instance_id: instance_id.to_string(),
        artifact_kind: ArtifactKind::Binary,
        name: name.to_string(),
    }
}

/// Waits for `op_id` to finish and returns its outcome.
async fn outcome_of(session: &Arc<Session>, op_id: OpId) -> Outcome {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(op) = session.operations().into_iter().find(|op| op.id == op_id) {
            if op.status == OpStatus::Done {
                return op.outcome.expect("a finished operation has an outcome");
            }
        }
        assert!(Instant::now() < deadline, "the operation never finished");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn test_the_gate_refuses_an_upgrade_of_agy_as_self_updating_and_the_row_carries_the_reason() {
    // Spec §4.4 D5 item 4 through the whole path: the refresh lists agy's
    // newer version with `SelfUpdatesOnly`, the Installed row says nothing
    // about being up to date, and `issue_plan` refuses the upgrade from the
    // candidate before the adapter is asked.
    let home = Home::new("agy-gate");
    let (session, _) = session_with(&home, Arc::new(MockTrasher::new()));

    let snapshot = session.refresh(&home.env(), &CheckOptions::default()).await;

    let candidate = snapshot
        .updates
        .iter()
        .find(|u| u.key.instance_id == "standalone-agy")
        .expect("agy's newer version is listed");
    assert_eq!(candidate.current, "1.2.10");
    assert_eq!(candidate.target, "1.2.11");
    assert!(candidate.checkable);
    assert_eq!(candidate.blocked, Some(UpdateBlocked::SelfUpdatesOnly));
    let row = snapshot
        .artifacts
        .iter()
        .find(|a| a.key.instance_id == "standalone-agy")
        .expect("agy is listed");
    assert!(row.auto_updates);
    assert_eq!(row.uninstall_blocked, None, "the uninstall is offered");

    let refused = session
        .issue_plan(&request("standalone-agy", OpKind::Upgrade, "agy"))
        .await;
    assert!(
        matches!(
            refused,
            Err(AdapterError::UpdateBlocked {
                reason: UpdateBlocked::SelfUpdatesOnly
            })
        ),
        "{refused:?}"
    );
    assert!(snapshot
        .updates
        .iter()
        .all(|u| u.key.instance_id != "standalone-grok"), "grok said nothing is newer");
}

#[tokio::test]
async fn test_uninstalling_grok_through_the_session_moves_its_folders_and_keeps_its_home() {
    // The whole path for grok: refresh, the gate, the preview (two folders
    // present, the fish file and the fallback links absent, the two bin
    // links last), submit, `run_operation`'s reading after, the next
    // refresh with no grok row -- and `~/.grok`'s settings, login and
    // sessions untouched. A `/usr/local/bin/grok` on the machine running
    // this test is not a link into the temp home, so no sentence names it.
    let home = Home::new("grok-uninstall");
    let trasher = Arc::new(MockTrasher::new());
    let (session, launcher) = session_with(&home, trasher.clone());
    session.refresh(&home.env(), &CheckOptions::default()).await;

    let issued = session
        .issue_plan(&request("standalone-grok", OpKind::Uninstall, "grok"))
        .await
        .expect("the gate lets it through and the preview is built");
    let PlanAction::TrashPaths { paths, .. } = &issued.plan.action else {
        panic!("a path list, not a command: {:?}", issued.plan.action);
    };
    assert_eq!(
        paths,
        &vec![
            home.path().join(".grok/downloads"),
            home.path().join(".grok/bundled"),
            home.path().join(".grok/bin/agent"),
            launcher.clone(),
        ]
    );

    let op_id = session.submit(issued.id).expect("submit");
    assert_eq!(outcome_of(&session, op_id).await, Outcome::Succeeded);
    assert_eq!(&trasher.calls(), paths);
    assert!(!launcher.exists() && std::fs::symlink_metadata(&launcher).is_err());
    assert!(home.path().join(".grok/bin").is_dir(), "the emptied PATH folder stays");
    assert!(home.path().join(".grok/config.toml").is_file());
    assert!(home.path().join(".grok/auth.json").is_file());
    assert!(home.path().join(".grok/sessions/s1.jsonl").is_file());
    assert!(home.path().join(".zshrc").is_file());

    let after = session.refresh(&home.env(), &CheckOptions::default()).await;
    assert!(after.instances.iter().all(|i| i.id != "standalone-grok"));
    assert!(after.instances.iter().any(|i| i.id == "standalone-agy"), "agy is untouched");
}
```

- [ ] **Step 9: Run to verify it passes**

Run: `cargo test -p canager-core --test standalone_agy_grok_test`
Expected: PASS, both tests. (If `Session::refresh`'s return or `submit`'s signature differ from C's Task 7 usage, copy the calls from `tests/standalone_uninstall_test.rs` as landed.)

- [ ] **Step 10: Format, gates, commit**

Run `cargo fmt --all`, then the five gates. Expected: all clean.

```bash
git add adapters/fixtures/standalone-agy adapters/fixtures/standalone-grok adapters/meta/standalone-agy.toml adapters/meta/standalone-grok.toml crates/canager-core/src/adapters/standalone/recipes.rs crates/canager-core/src/adapters/standalone/mod.rs crates/canager-core/src/session/mod.rs crates/canager-core/src/scan/mod.rs crates/canager-core/src/lib.rs docs/what-we-run.md crates/canager-core/tests/what_we_run_test.rs crates/canager-core/tests/standalone_agy_grok_test.rs
git commit -m "$(cat <<'EOF'
Register Antigravity CLI and Grok Build, with their recordings and sections

Eleven sources. The recordings are read-only (--version under the
documented switch, grok's own check, one curl, ls with the home folder
abbreviated) and agy's README records what spec §3.4 demands: the version
read reached no updater. The trust file says what each tool's row runs,
reads and moves, and that how grok update behaves unattended is still to
be recorded on CI. Both roots are theirs on the Unknown page.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

### Task 7: Front end — labels, summaries, empty states

**Files:**
- Modify: `src/lib/sources.ts` — `ADAPTER_LABEL_KEYS`, `StandaloneAdapterId`, `STANDALONE_SUMMARY_KEYS`; `src/lib/sources.test.ts` — the label assertions, `describe("STANDALONE_SUMMARY_KEYS", …)`
- Modify: `src/i18n/en.json`, `src/i18n/zh-CN.json` — `adapters.*`, `standalone.summary.*`, `emptyStates.*`
- Modify: `src/components/SnapshotStatus.test.tsx` — the two sentences
- Test: the files above.

**Interfaces:**
- Consumes: B's `ADAPTER_LABEL_KEYS`, `StandaloneAdapterId`, `STANDALONE_SUMMARY_KEYS`, `standaloneSummaryKey`; E's `"standalone-rustup"` rows and empty-state wording; the ids Task 6 registered.
- Produces: `ADAPTER_LABEL_KEYS["standalone-agy"] = "adapters.standalone-agy"`, `["standalone-grok"] = "adapters.standalone-grok"` (readers: `InstalledPage`, `UpdatesPage`, `UninstallDialog` through `sourceLabelFor`/the label lookups); `StandaloneAdapterId = "standalone-claude" | "standalone-rustup" | "standalone-agy" | "standalone-grok"`; `STANDALONE_SUMMARY_KEYS["standalone-agy"]`, `["standalone-grok"]` (reader: `InstalledPage`'s `installedDescription`); the keys `adapters.standalone-agy`, `adapters.standalone-grok`, `standalone.summary.standalone-agy`, `standalone.summary.standalone-grok`; the two `emptyStates` sentences naming four tools (ruling 20).

- [ ] **Step 1: Write the failing tests**

In `src/lib/sources.test.ts`, in the test that asserts `expect(en.adapters["standalone-claude"]).toBe("Claude Code");` (and E's rustup line beside it), add after E's rustup assertions:

```ts
    // Spec §9.2: the label carries the command name in parentheses, since
    // "Antigravity CLI" and "Grok Build" are not what the user types.
    expect(en.adapters["standalone-agy"]).toBe("Antigravity CLI (agy)");
    expect(zhCN.adapters["standalone-agy"]).toBe("Antigravity CLI（agy）");
    expect(en.adapters["standalone-grok"]).toBe("Grok Build (grok)");
    expect(zhCN.adapters["standalone-grok"]).toBe("Grok Build（grok）");
    expect(ADAPTER_LABEL_KEYS["standalone-agy"]).toBe("adapters.standalone-agy");
    expect(ADAPTER_LABEL_KEYS["standalone-grok"]).toBe("adapters.standalone-grok");
```

In `describe("STANDALONE_SUMMARY_KEYS", …)`, in `it("gives each standalone tool a sentence and every other source none", …)` add after the claude (and E's rustup) `expect`:

```ts
    expect(standaloneSummaryKey("standalone-agy")).toBe("standalone.summary.standalone-agy");
    expect(standaloneSummaryKey("standalone-grok")).toBe("standalone.summary.standalone-grok");
```

and append a test to that `describe`:

```ts
  it("has the two AI CLIs' sentences in both locales, naming the publisher and the installer route", () => {
    expect(en.standalone.summary["standalone-agy"]).toBe(
      "Google's Antigravity coding assistant for the terminal. Installed with its own installer.",
    );
    expect(zhCN.standalone.summary["standalone-agy"]).toBe(
      "Google 的 Antigravity 终端编程助手。用它自己的安装器装的。",
    );
    expect(en.standalone.summary["standalone-grok"]).toBe(
      "xAI's Grok coding assistant for the terminal. Installed with its own installer.",
    );
    expect(zhCN.standalone.summary["standalone-grok"]).toBe(
      "xAI 的 Grok 终端编程助手。用它自己的安装器装的。",
    );
  });
```

In `src/components/SnapshotStatus.test.tsx`, replace E's two expected sentences — `"Canager works with Homebrew, npm, pipx, uv, pip, Cargo and Ollama, and with Claude Code and rustup at their own installers' default locations. None of them are set up on this Mac yet — Homebrew is the easiest place to start."` and `"Items installed with Homebrew, npm, pipx, uv, pip, Cargo or Ollama appear here, along with Claude Code and rustup installed at their own installers' default locations."` — with:

```ts
        "Canager works with Homebrew, npm, pipx, uv, pip, Cargo and Ollama, and with Claude Code, Antigravity CLI, Grok Build and rustup at their own installers' default locations. None of them are set up on this Mac yet — Homebrew is the easiest place to start.",
```

and

```ts
        "Items installed with Homebrew, npm, pipx, uv, pip, Cargo or Ollama appear here, along with Claude Code, Antigravity CLI, Grok Build and rustup installed at their own installers' default locations.",
```

- [ ] **Step 2: Run to verify they fail**

Run: `pnpm typecheck`
Expected: FAIL — `Property 'standalone-agy' does not exist on type '{ "standalone-claude": string; "standalone-rustup": string; }'` at `en.standalone.summary["standalone-agy"]` and `en.adapters["standalone-agy"]` (and grok's). `pnpm exec vitest run src/components/SnapshotStatus.test.tsx`: FAIL on both sentences.

- [ ] **Step 3: Write the rows and the copy**

In `src/lib/sources.ts`:

```ts
// in ADAPTER_LABEL_KEYS, after E's `"standalone-rustup": "adapters.standalone-rustup",`
  "standalone-agy": "adapters.standalone-agy",
  "standalone-grok": "adapters.standalone-grok",
```

```ts
export type StandaloneAdapterId =
  | "standalone-claude"
  | "standalone-rustup"
  | "standalone-agy"
  | "standalone-grok";
```

```ts
// in STANDALONE_SUMMARY_KEYS, after E's rustup row
  "standalone-agy": "standalone.summary.standalone-agy",
  "standalone-grok": "standalone.summary.standalone-grok",
```

In `src/i18n/en.json`: in `"adapters"`, after `"standalone-rustup": "rustup"` add `,` and

```json
    "standalone-agy": "Antigravity CLI (agy)",
    "standalone-grok": "Grok Build (grok)"
```

in `"standalone"` → `"summary"`, after E's rustup line add `,` and

```json
      "standalone-agy": "Google's Antigravity coding assistant for the terminal. Installed with its own installer.",
      "standalone-grok": "xAI's Grok coding assistant for the terminal. Installed with its own installer."
```

and in `"emptyStates"`, replace E's `"noSources"` → `"description"` with

```json
      "description": "Canager works with Homebrew, npm, pipx, uv, pip, Cargo and Ollama, and with Claude Code, Antigravity CLI, Grok Build and rustup at their own installers' default locations. None of them are set up on this Mac yet — Homebrew is the easiest place to start."
```

and E's `"nothingInstalled"` → `"description"` with

```json
      "description": "Items installed with Homebrew, npm, pipx, uv, pip, Cargo or Ollama appear here, along with Claude Code, Antigravity CLI, Grok Build and rustup installed at their own installers' default locations."
```

In `src/i18n/zh-CN.json`, the same four places:

```json
    "standalone-agy": "Antigravity CLI（agy）",
    "standalone-grok": "Grok Build（grok）"
```

```json
      "standalone-agy": "Google 的 Antigravity 终端编程助手。用它自己的安装器装的。",
      "standalone-grok": "xAI 的 Grok 终端编程助手。用它自己的安装器装的。"
```

```json
      "description": "Canager 支持 Homebrew、npm、pipx、uv、pip、Cargo、Ollama，以及用各自的原生安装器装在默认位置的 Claude Code、Antigravity CLI、Grok Build 和 rustup。这台 Mac 上一个都还没装，建议先从 Homebrew 开始。"
```

```json
      "description": "用 Homebrew、npm、pipx、uv、pip、Cargo、Ollama 装的东西，以及用各自的原生安装器装在默认位置的 Claude Code、Antigravity CLI、Grok Build 和 rustup，会出现在这里。"
```

- [ ] **Step 4: Run to verify they pass**

Run: `pnpm exec vitest run src/lib/sources.test.ts src/components/SnapshotStatus.test.tsx src/i18n` and `pnpm typecheck`
Expected: PASS — the label and summary tests, the two empty states, `completeness.test.ts` (every new key is referenced through the two `Record`s' literals; the empty-state keys' call sites are unchanged), `no-literal-strings.test.ts`; `tsc` clean.

- [ ] **Step 5: Gates, commit**

Run the five gates (the Rust ones are unaffected and must still pass). Expected: all clean.

```bash
git add src/lib/sources.ts src/lib/sources.test.ts src/i18n/en.json src/i18n/zh-CN.json src/components/SnapshotStatus.test.tsx
git commit -m "$(cat <<'EOF'
Name Antigravity CLI and Grok Build on the pages

Their group labels carry the command the user types, their rows a
sentence saying whose tool it is and that its own installer put it
there, and the two empty states now name all four tools that come with
their own installer.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

### Task 8: README, backlog, delivery note

**Files:**
- Modify: `README.md` — the "What it manages" table (two rows after E's rustup row) and the two test-count sentences (the `> **Status: pre-release.**` block and its Chinese counterpart under `## 中文`)
- Modify: `docs/superpowers/backlog.md` — the agy-cache entry C opened, closed; two new entries
- Test: none new (prose); the five gates.

**Interfaces:** none — prose. The rows' claims are Task 5 (reads, the blocked upgrade, both uninstalls), Task 6 (what the previews say, the trust file), Task 1 (the badge).

- [ ] **Step 1: Add the rows**

Directly after E's table row that begins `| rustup — the Rust toolchain manager, via its own installer |`, insert:

```markdown
| Antigravity CLI (`agy`) — Google's terminal agent, via its own installer | yes | updates **no** — it installs updates itself in the background, so the row shows the newer version with an "Updates itself" badge and says to open the tool once; install no (the installer is Google's, and Canager never runs it); uninstall yes — the `agy` program (and any `agy.<time>.old` backup its updater left beside it) goes to the Trash; its conversations, history and settings in `~/.gemini/antigravity-cli` stay, and so does its staging folder in `~/.cache` |
| Grok Build (`grok`) — xAI's terminal agent, via its own installer | yes | updates yes (`grok update`, offered when grok's own `update --check --json` says a newer version exists; how the update behaves unattended is still being recorded on CI); install no (the installer is xAI's); uninstall yes — its downloads, bundled files, completions and the two links in its `bin` folder go to the Trash (the folder itself, which is on your `PATH`, stays); `~/.grok`'s settings, login, sessions and memory stay |
```

- [ ] **Step 2: Update the two test counts**

Get the numbers from the suites, never by hand:

```bash
cargo test --workspace 2>&1 | grep -E '^test result' | awk '{ passed += $4 } END { print passed }'
pnpm test 2>&1 | grep -E '^\s*Tests\s'
```

Put the Rust total where the status block says `covered by <N> Rust tests` and the Chinese block says `有 <N> 个 Rust 测试`, and the front-end total where both say `<M> front-end tests` / `<M> 个前端测试` (E's Task 12 and C's Task 8 changed the same four numbers; whatever they read now, replace them with today's).

- [ ] **Step 3: The backlog**

In `docs/superpowers/backlog.md`, under `## 阶段 4（独立安装工具）进行中的遗留（2026-09-25 立，分支 feat/phase-4-standalone）`, find C's entry beginning `- **检查 1 的「永不」清单挡住了 agy 的 \`~/.cache/antigravity\`**` and append to it (after its last sentence, `**步骤 D 要做的决定**：…不要悄悄放宽整条规则。`):

```markdown
  **步骤 D 定案（2026-09-25）：改清单，不改规则。** `~/.cache/antigravity` 不移，列为保留项（新变体
  `KeptWhat::InstallerCache`，文案说它是安装器的下载暂存文件夹、通常是空的、Canager 不会移动直接放在 `~/.cache`
  里的东西、可以自己删）。本机它是空的（`staging/` 0 项）；中断的更新最多留一个 ~180 MB 的包。作者可见的后果：
  卸载 Antigravity 后 `~/.cache/antigravity` 留在原地，对话框会说；来源不明页不会列它（不是 bin 目录）。
```

and append two new entries at the end of that section (before `## 阶段 5（发现页）之前必须处理`):

```markdown

- **grok 回退链接的链接文本未核实**（2026-09-25，步骤 D）。`~/.local/bin/grok`、`~/.local/bin/agent` 只在 `~/.grok/bin`
  不在 PATH 上时由安装器创建（grok.md §2），本机没有，链接文本指向 `~/.grok/bin/grok` 还是直接指向 `downloads/` 里的
  文件不知道。配方把这两条列为 optional 且**排在最前**（步骤 D 计划裁定 3）——这是预防，不是纠错：C 的检查 4
  对悬空链接按其文本判定（`probe_strict` 的 NotFound 分支只折叠**已存在**的前缀），两种文本在 `downloads/` 进废纸篓
  之后都会答 `LauncherOnly` 并被接受；排在最前只是让它们在自己文本可能经过的每个文件夹都还在时就走掉，检查 4
  据此按「解析成功」而非「按文本」放行，中途停下也不会留下一条看起来像别人的悬空 `~/.local/bin/grok`。若它不是
  grok 的（另一个 CLI 的 `agent`），按 `NotOurs` 保留。`/usr/local/bin` 里的同名路径只在**链接进 `~/.grok`** 时才报
  「会变成失效链接」（Intel Mac 上它可能是 Homebrew `grok-build` 的活链接，步骤 D 计划裁定 6）。CI runner 上用
  `GROK_BIN_DIR` 之外的 PATH 装一次即可核实文本。
- **grok 的 `~/.grok/bin` 不整目录移动**（2026-09-25，步骤 D 计划裁定 4，与 spec §6.3 的 `~/.grok/bin · Dir` 不同）。
  安装器把它加进了 PATH，用户自己的脚本可能放在里面；清单列的是安装器放进去的两条链接（`agent`、最后 `grok`），
  空文件夹留在被保留的 `~/.grok` 里。若日后要连文件夹一起移，形状是「文件夹里只剩清单上的条目才移」的检查，不是
  放宽启动器最后的不变量。
- **`grok update` / `claude update` 无交互时的行为待 CI 录制**（2026-09-25，步骤 D，spec §五；CI 额度 2026-10-01 恢复）。
  步骤 D 计划「The author's pre-merge verification」一节给了工作流、观察项与每种结果对应的配方改法；在录制到之前，
  `docs/what-we-run.md` 的 Grok Build 一节照实说「尚未观察」。**grok 的结果挡合并**（配方是本步新加的）；**claude 的
  结果不挡本步合并**（B 已经交付了按钮，若会提示/挂起，是发布前要修的回归）——这与 spec §五「两个都在 D 之前录」
  不同，步骤 D 计划 deviation 13 记了。若任一命令会提示或挂起，形状是新的 `UpdateBlocked::NeedsTerminal`（自己的
  copy record），不是 `SelfUpdatesOnly`。
- **`grok --version` 是否触发启动时更新器、更新器是否会静默安装**（2026-09-25，步骤 D；grok.md §5 开放问题 2）。
  录制在 `--version` 前后各拍一次 `~/.grok/bin`、`~/.grok/downloads`、`readlink` 与 `version.json` mtime 的快照：
  布局变了就停（配方的版本读取要改成读 `version.json`）；只有 mtime 动了则照实写进 fixture README 与
  `## Grok Build`——启动时路径被版本读取碰到了，能不能装还是未知。刷新每次跑 3–4 次 `grok --version`，这条没关
  之前 grok 的 `self_updates` 不能改成 true。
```

- [ ] **Step 4: Run the gates**

Run all five from Global Constraints. Expected: all clean.

- [ ] **Step 5: Commit**

```bash
git add README.md docs/superpowers/backlog.md
git commit -m "$(cat <<'EOF'
List Antigravity CLI and Grok Build among the sources, with the new test counts

Reads for both, a blocked update for the one that updates itself, a real
one for the other, and a path-list uninstall for each that says what
stays -- which the rows say rather than imply. The backlog records the
decision on agy's staging folder and the two facts still owed to CI.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 6: Delivery note (goes in the branch's PR description / handover; not a file)**

> **Step D: Antigravity CLI and Grok Build.** A native `agy` (`~/.local/bin/agy`, a flat file) is an *Antigravity CLI (agy)* group with one row; its badge compares the launcher's live version with Google's Apple-silicon manifest (an Intel Mac sees "could not check" and sends nothing) and a newer version shows as *Updates itself* with no button and a sentence saying to open the tool once — `agy update` is undocumented and unrun. *Uninstall* moves any `agy.<time>.old` its updater left and then `agy` itself to the Trash, and keeps `~/.gemini/antigravity-cli`, `~/.cache/antigravity` (directly in `~/.cache`, which Canager never moves from — the list changed, not check 1) and the two shell files, saying so. A native `grok` (`~/.grok/bin/grok`, a relative link into `~/.grok`) is a *Grok Build (grok)* group; its badge is grok's own `update --check --json` (its help calls it a check "without installing"), believed as answered; *Update* runs `grok update`; a check that answers with grok's own `error` set is "could not check", never "up to date"; *Uninstall* moves its fallback links first, then `downloads/`, `bundled/`, `completions/`, the fish completion, then the two links in `~/.grok/bin` (`grok` last; the folder, which is on `PATH`, stays), keeps `~/.grok` (login, sessions, memory) and `~/.zshrc`, keeps-and-says an optional path it cannot confirm is grok's, and reports a `/usr/local/bin/grok` as a dead link only when it is a link into `~/.grok` (never Homebrew's). The Unknown page's rule 4 claims a fresh `agy.<time>.old` for agy while agy is installed. Eleven sources.
>
> **Open before merge (the author's, on CI after 2026-10-01):** record how `grok update` behaves with stdin closed — the workflow, what to look for and how each result changes the recipes are in this plan's "The author's pre-merge verification"; the Grok section of `docs/what-we-run.md` says "not yet observed" until then. **grok's result gates the merge.** `claude update` is recorded in the same run; a bad result there is a pre-release fix (B's button already shipped), not a gate on this step.
>
> **Rulings taken** (see "Rulings this plan makes"): agy's staging folder kept and said, not moved (1); agy's launcher is `Launcher` (2); grok's fallback links first, as a precaution (3); grok's two `bin/` links are the items, never the folder (4); the `NotOurs` skip covers three refusal reasons for optional paths (5); outside-home keeps are report-only and only for a link into the root (6); backups before the last item, in name order, optional (7); `Glob` in `scan/` (8); rule 4 by name, for a file, while installed (9); grok's verdict trusted, its `error` field believed too (10); the manifest on Apple silicon only, with a test seam (11); `upgrade: Option` (12); versions in the `SelfUpdatesOnly` sentence (13); the launcher, bare, as its command (14); "any lines" for a shell file (15); the version-read observation per recording (16); `~` and numeric owners in fixtures (17); the recorded version rules (18); eleven adapters (19); four tools in the empty states (20); both roots owned (21); the CI recording is the author's (22).
>
> **Recorded on this Mac**: `adapters/fixtures/standalone-agy/<version>/` and `standalone-grok/<version>/` — the version lines, agy's manifest and updater status, grok's own check, the two layouts with `~` and numeric owners; `--version` once per tool (under agy's switch), `update --check --json`, `curl`, `cat`, `stat`, `ls`, `ps` only. agy's README records that the recorded `--version` reached no updater (log count, status mtime, no new process); grok's records that it changed no link and no download, and what `version.json`'s mtime did. Never run: `grok update`, `agy update`, bare `agy`, bare `grok`.
>
> **UNVERIFIED, left as such**: grok's fallback link text (backlog); whether `grok --version` runs grok's launch-time updater and whether that updater installs (the README records the layout and `version.json`'s mtime around the read; backlog); whether grok installs updates on its own (not called self-updating); the amd64 manifest (Intel rows say so); `grok update`/`claude update` unattended (CI).

---

## Self-review against the spec

**1. Spec coverage.** Every requirement spec §十 row D lists, and every D-relevant line of §3, §4, §6, §8.3, §9 and 附录 A, has a task:

| Spec | Task |
|---|---|
| §十 row D: grok + agy recipes, meta, fixtures (agy README with the `--version` observation) | Task 5 (5b), Task 6 (Steps 1–2) |
| §3.1 `Latest::Command`, `Latest::HttpJsonField` (x86_64 → uncheckable with a reason) | Task 5 (5a); Review Focus 3 |
| §3.1 `Recipe.backup_globs` + `Glob` in `scan/` | Task 3 |
| §3.1 `upgrade: Option`, `uninstall: Option` (C), `Expect::File` | Task 5 (5a), Task 4 |
| §3.3 detection: agy `FlatFile` (E's kind), grok `SymlinkIntoRoot` with a relative link | Task 5 (5c: `test_detect_lists_agy_…`, `test_detect_lists_grok_…`) |
| §3.4 version reads: `AGY_CLI_DISABLE_AUTO_UPDATE=true`; grok none; the per-recording observation | Task 5 (5b, 5c), Task 6 (Step 1, the stop rule; ruling 16) |
| §3.5 the two data rows | Task 5 (5b) — with rulings 1–3 where the plan departs |
| §4.1 endpoints VERIFIED only; failure → uncheckable, never `Err` | Task 5 (5a, 5c: the bad-manifest and failing-check tests) |
| §4.2 the agy host on `ALLOWED_HTTPS_HOSTS`, its doc row, the recipe test | Task 5 (5b) |
| §4.3 remote > local for the manifest; grok's `updateAvailable` believed | Task 5 (5a `decided`, 5c) |
| §4.4 D5: `SelfUpdatesOnly` — badge, no button, gate, copy with the 15 minutes, `selfUpdatingDescription: null` | Task 1; Task 5 (5c `test_check_updates_for_agy_lists_a_newer_manifest_version_with_no_button`, `test_plan_upgrade_for_agy_is_refused_as_self_updating`); Task 6 (Step 8 through `Session`) |
| §4.4 grok not self-updating until verified | Task 5 (5b `self_updates: false`) |
| §五 grok's `grok update` plan | Task 5 (5c `test_plan_upgrade_for_grok_is_its_own_update_command`) |
| §五 the CI recording of both upgrade commands | "The author's pre-merge verification"; Task 8 (backlog, delivery note) |
| §6.1–§6.2 `Paths` uninstall for both, launcher last, `TrashPaths` | Task 4 (`expect` per `RouteKind` on the launcher-last test), Task 5 (5b, 5c); ruling 4 for grok's `bin/` links |
| §6.3 check 5 (globs), the agy and grok rows, `optional` fingerprint mismatch → `NotOurs`, `OutsideHome` keeps (only a link into the root) | Task 4; Task 5 (5b, 5c); rulings 1, 4, 6 where the rows depart |
| §6.5 `RemovedWhat::Backups`, `KeptWhat::{ToolState, ShellConfigLines, OutsideHome, NotOurs}` + copy | Task 2 (and `InstallerCache`, ruling 1) |
| §8.3 rule 4, the `globs` parameter, `Session::scan_unknown` filling from `RECIPES` | Task 3 |
| §9.1 mirrors: `UpdateBlocked`, `RemovedWhat`, `KeptWhat` | Tasks 1, 2 |
| §9.2 copy: `adapters.standalone-{agy,grok}`, `standalone.summary.*`, `updates.blocked.SelfUpdatesOnly.*`, `warnings.willTrash.Backups`, `warnings.willKeep.*`, empty states | Tasks 1, 2, 7 |
| §9.3 fixtures: agy (`version.txt`, manifest, `update_status.json`, `layout.txt` of the tool's own path only), grok (`version.txt`, `update-check.json`, `layout.txt`); READMEs with provenance; no personal paths | Task 6 (Steps 1–2; ruling 17) |
| §9.4 tests: recipe invariants, parsing tables, route detection, `check_updates` cases (agy → `SelfUpdatesOnly`, grok `updateAvailable: false` → none, x86_64), `plan(Uninstall)` (`optional` mismatch → `NotOurs`, glob only regular files, order), scan rule 4 and the no-instance `.old` | Tasks 3–6 |
| §9.5 trust file: the two sections, files read, the host, the never-list | Tasks 3, 5, 6 |
| 附录 A: `UpdateBlocked::SelfUpdatesOnly` readers; `Recipe.backup_globs` readers; `KeptWhat::NotOurs` reader; `ALLOWED_HTTPS_HOSTS` readers; `InstalledArtifact.path`/`prefix` for the two (`owned_roots`) | Tasks 1, 3, 4, 5, 6 |
| 附录 B: no shell (`Latest::Command` is a fixed argv on the launcher), nothing outside `$HOME` moved (`OutsideHome` report-only), no version read triggers a self-update (the switch + the observation), no unreviewed host | Tasks 4–6; Global Constraints |

Not in this step, by the spec's own list or this plan's rulings (each with its owner): the amd64 manifest (§十一; Intel rows say "not yet verified"); grok's `self_updates` from `config.toml` (§十一, UNVERIFIED); a `NeedsTerminal` reason if the CI probe finds a prompt (the author's, after the probe); F's follow-ups in `followups-after-c.md` (B's upgrade route revalidation, refresh-phase coherence, external kills, B's fixture paths) — not D's files.

**2. Placeholder scan.** Searched for "TBD", "TODO", "implement later", "fill in", "similar to Task", "appropriate error handling", "handle edge cases": none. Every code step carries its code; every test its body. Values the executor measures, each with its command: the two recorded versions (Task 6 Steps 1–2), the README's bracketed provenance values (the same steps), the test counts (Task 8 Step 2). Values the author records later: the CI probe's results (the pre-merge section).

**3. Type consistency.** Checked across tasks against Core Interfaces: `UpdateBlocked::SelfUpdatesOnly` (Tasks 1, 5, 6); `RemovedWhat::Backups`, `KeptWhat::{ToolState, ShellConfigLines, OutsideHome, NotOurs, InstallerCache}` (Tasks 2, 4, 5, 6); `Glob { dir, prefix, suffix, what }`, `Glob::dir_under(home)`, `Glob::matches_name(name)` (Tasks 3, 4, 5); `scan_dirs(dirs, env, instances, artifacts, globs, budget)`, `scan_unknown(env, instances, artifacts, globs, budget)`, `Known::index(instances, artifacts, globs, home)`, `Known::claimant(raw, dir, resolved, kind)` (Task 3); `Recipe.backup_globs`, `recipes::backup_globs()` (Tasks 3, 4, 5, 6); `Job { recipe, detected, remove, keep, globs }` (Tasks 4, 5); `Item { rel, path, expect, what, optional }`, `listed_items(job)`, `check_item(look, kept, rel, expect, path)`, `keeps_instead(reason)`, `outside_home_keeps(look)`, `points_into(link, root)` (Task 4); `Expect::File` (Tasks 4, 5); `Latest::HttpJsonField { url, field }`, `Latest::Command { args, timeout_secs, latest_field, available_field, error_field }` (Tasks 5, 6); `Recipe.upgrade: Option<UpgradeCmd>` (Task 5); `parse_json_field(body, field)`, `UpdateCheck { latest, available }`, `parse_update_check(stdout, latest_field, available_field, error_field)`, `MANIFEST_VERIFIED_ARCHES`, `manifest_arch_allowed(arch)` (Tasks 5, 6); `StandaloneAdapter.arch`, `with_arch`, `Published::{Version, ToolSays}`, `published(launcher)` (Task 5, 6); `AGY`, `GROK`, `RECIPES` of four (Tasks 5, 6); `testing::{AgyLayout, agy_layout, GrokLayout, grok_layout}` (Task 5); `ADAPTER_LABEL_KEYS`/`STANDALONE_SUMMARY_KEYS` rows and `StandaloneAdapterId` (Task 7); `UPDATE_BLOCKED_KEYS.SelfUpdatesOnly` with `launcherCommand` (Task 1). Locale keys: Tasks 1, 2 and 7 add the ones Core Interfaces lists, in both files.

**4. Review Focus.** The eight inputs are listed at the top with the tests that pin each, all inside tasks: agy's self-update after the preview (Task 5), a foreign `agent` link (Tasks 4, 5), an Intel Mac (Task 5), grok's check failing — including exit 0 with its `error` set (Task 5), a backup appearing after the preview (Task 4), a stopped grok uninstall (Task 5), the optional paths absent (Task 5), Homebrew's `/usr/local/bin/grok` (Task 4). Checked and deliberately not added: a `~/.local/bin` that is a dotfiles link (C's ancestry rule refuses agy's uninstall there as it refuses Claude Code's; C's backlog entry covers it, and agy's launcher is not optional, so the skip does not apply); two grok launchers (`grok` and `agent`) resolving to one download — rule 1 claims both on the Unknown page (F's test for rustup's proxies is the same shape); `~/.gemini` itself as a kept path (never listed: it is Gemini CLI's too, ruling 24).

## Deviations from the spec, and facts found while writing this plan

1. **`~/.cache/antigravity` is kept, not moved** (ruling 1): spec §6.3 lists it for removal; C's check 1 never-list (which the spec's own words gave C) refuses a path directly in `~/.cache`. A new `KeptWhat::InstallerCache` says what it is and that the user may delete it.
2. **agy's launcher is `RemovedWhat::Launcher`** (ruling 2), where spec §6.3 wrote `Program`.
3. **grok's fallback links are moved first** (ruling 3), where spec §6.3 orders them after `downloads/`. A precaution, not a correctness fix: C's check 4 would accept them dangling too (`probe_strict`'s dangling branch answers `LauncherOnly` for either link text), so the reordering only makes check 4 answer from a resolving link and keeps a stopped run from leaving a dangling `~/.local/bin/grok`.
4. **The `NotOurs` skip covers `OutsideHome` and `SharedFolder` as well as the fingerprint** (ruling 5); spec §6.3 names the fingerprint only. And its sentence says Canager "couldn't confirm it's part of this install", not the spec's "it isn't part of this install", since a linked folder on the way is one of the cases. C's `## Claude Code` check paragraph is reworded in the same commit (Task 4), since `~/.claude/downloads` is optional and its "refuses" sentences would otherwise be false.
5. **`OutsideHome` keeps are report-only, absolute, and reported only for a link into the root** (ruling 6); the spec lists them beside the `~/` keeps "若存在" without saying how they are checked, C's `disturbed` would refuse the uninstall for a link into the program folder, and a mere existence test would call Homebrew's `/usr/local/bin/grok` (Intel Macs) a dead link the user should delete.
6. **`ShellConfigLines` says "any lines its installer added"** (ruling 15); spec §9.2's "the lines its installer added" would be false for a `~/.zshrc` without them, since Canager does not read the file.
7. **`Recipe.upgrade` is `Option<UpgradeCmd>` only now** (ruling 12), as B's ruling 1 said it would become with agy.
8. **`Glob::matches_name` needs a character between prefix and suffix** (ruling 7); the spec's "`prefix` + 任意串 + `suffix`" could read as allowing the empty string.
9. **The `SelfUpdatesOnly` sentence's `{{current}}`/`{{target}}` need one more line in `rowDescription`'s `t()` call** (ruling 13); the spec's copy assumed they were interpolated.
10. **The version-read observation is a stop rule in the recording, with the change left to the author** (ruling 16); spec §3.4 says which version reads change and how, which this plan does not pre-decide.
11. **Fixtures carry `~` and numeric owners** (ruling 17); spec §9.3 says "逐字节" for command output, and the README says exactly what was transformed — the task's rule that no personal path enters a committed file wins.
12. **The spec's `Paths.source`/provenance is the README and the constant's doc comment** (C's ruling, kept).
13. **The CI recording of the two upgrade commands is written as the author's own section**, since CI minutes are gone until 2026-10-01 and neither command may run on the author's Mac — and only grok's result gates this merge; claude's is recorded in the same run but a bad result is a pre-release fix, since B already shipped the button. Spec §五 asked for both before step D.
14. **grok's `~/.grok/bin` is not moved as a folder** (ruling 4); spec §6.3 lists `~/.grok/bin · Dir · Launcher` last. The two links the installer put there are the items (`agent`, then `grok`), and the emptied folder — which the installer put on `PATH`, so it may hold the user's own scripts — stays inside the kept `~/.grok`. C's launcher-last invariant stays exactly as C wrote it.
15. **`Latest::Command` carries `error_field`** (ruling 10); spec §3.1 gave it `latest_field` and `available_field` only. grok's `"error"` is believed like its `updateAvailable`: set, the row is "could not check" with grok's words, never "up to date".
16. **grok's recording has the same stop rule as agy's** (ruling 16); spec §3.4 wrote the observation for agy. A changed link or download after `grok --version` stops the recording; a moved `version.json` mtime is recorded as the open question it is, and the check's own write to that file is named in the trust file as the one write a refresh causes.
17. **Facts found on 2026-09-25**: `~/.local/bin/agy` changed again (186,406,752 bytes, 12:25) — agy self-updates faster than a plan is written, which is why the recorded version is whatever the day prints (ruling 18) and why Review Focus 1 is first; `~/.cache/antigravity/staging` exists and is empty; `~/.grok/bin/{grok,agent}` are relative links to `grok-1.0.41-macos-aarch64` and `downloads/` holds three versions; `~/.grok/bundled`, `~/.grok/completions` and `~/.config/fish/completions/grok.fish` (148 KB) exist; no fallback links in `~/.local/bin` or `/usr/local/bin`; `~/.zshrc` carries both installers' markers and `~/.zprofile` agy's.

## Review log

Adversarial review of this plan, 2026-09-25, 23 points. Each was re-verified against the landed tree at `db42e79` (`~/dev/Canager-phase4`, read only) — `removal.rs`, `route.rs`, `recipes.rs`, `scan/mod.rs`, `tests/unknown_scan_test.rs`, `tests/what_we_run_test.rs`, `docs/what-we-run.md`, the spec's §6.3 grok row — and, for the two compile claims, against scratch `rustc` builds in a temp directory. Every point held; the plan was changed in place as listed. Numbers are the review's.

| # | Verdict | What was verified, and what changed |
|---|---|---|
| 1 | **Accepted** | Scratch build: `2 positional arguments in format string, but there is 1 argument`. Task 3's `test_every_backup_glob_is_under_home_and_names_a_pattern` now passes `glob.dir` as the second argument. |
| 2 | **Accepted** | Scratch build of the tuple-of-closures array: E0308, "no two closures … have the same type"; the typed `[(&str, fn(&TempHome)); 2]` form compiles and runs. Task 4's `test_plan_removal_keeps_an_optional_path_it_cannot_confirm_is_the_tools_and_says_so` uses the typed array. |
| 3 | **Accepted** | `recipes.rs:95-98`: `~/.claude/downloads` is `optional: true`; `removal.rs:859-879` (first half of C's linked-folder test) expects `NotWhatInstructionsExpect` for it, which `keeps_instead` turns into a `WillKeep { NotOurs }` and `Ok`. Task 4 now cuts that test down to its launcher half (renamed `…_refuses_a_launcher_reached_through_a_linked_folder_inside_home`), folds the `~/.claude -> ~/Documents` case into `…_keeps_an_optional_path_whose_folder_leads_elsewhere` with the keep expectation, and Step 4 names both changed tests. Checklist row 6 lists the landed test. |
| 4 | **Accepted** | `docs/what-we-run.md:585-599`: "and so does a `~/.claude` that is a link when the download cache is inside it" and "If any check fails, the whole uninstall is refused" both describe the now-optional-and-kept case. Task 4 gains edit (j) rewording both sentences (required path refuses; optional one Canager cannot confirm stays and is said; not-yours and overlaps-kept refuse either way) and `docs/what-we-run.md` in its `git add`; checklist row 23 and the File Structure name it. |
| 5 | **Accepted** | `scan/mod.rs:386-393`: `exe_canonical` is checked before any rule 4 could be; `link(&bin, "agy.2.old", &agy)` resolves to the instance's `exe_path`, so rule 1 claims it and the listed set would be `[agy.old]` with `attributed == 3`. The link now points at an executable in a sibling `elsewhere/` folder that no rule claims, so rule 4's `kind == File` guard is what keeps it listed; the two assertions hold as written. |
| 6 | **Accepted** | `outside_home_keeps` did `symlink_metadata(spec.path)` on the recipe's absolute `/usr/local/bin/{grok,agent}`, so the grok tests' exact warning lists depended on the host. Fixed together with 12: the sentence is produced only for a symbolic link whose target lies under the recipe's root for *this* home (`points_into`), which a link in the real `/usr/local/bin` can never satisfy for a temp home. Every grok test comment says so; Review Focus 7 and 8 record it. No seam needed. |
| 7 | **Accepted** | `route.rs:164-175` (dangling branch: `one_hop` + `canonicalize_existing_prefix` on the hop's parent, which exists while `~/.grok/bin` does) and `removal.rs:327-334` (`check_item` accepts `Present \| LauncherOnly`): a two-hop fallback link dangling after `downloads/` moved answers `LauncherOnly`, not `Absent`. Ruling 3 rewritten as a precaution with the correct mechanism stated and the earlier claim retracted in place; the GROK doc comment, the recipes test comment, the 5c test comments, the fixture README, the `## Grok Build` uninstall paragraph, the backlog entry, deviation 3 and the delivery note all carry the new reason. The order itself is kept. |
| 8 | **Accepted** | HEAD moved twice since the baseline: `db42e79` (C's Task 7, `tests/standalone_uninstall_test.rs` exists; `session/mod.rs:514` still `eight`, so E has not landed; C's Task 8 not landed). Baseline updated to `db42e79`; the confirm-grep gains `ls tests/standalone_uninstall_test.rs` + `grep async fn outcome_of` and an explicit stop rule on `eight` / a missing test file; line-number hints re-anchored. |
| 9 | **Accepted** | `removal.rs:1312-1316`: C creates the replacement while the old entry exists ("so it cannot get its inode"). Task 5c's `test_execute_for_agy_refuses_a_launcher_its_updater_replaced_after_the_preview` now writes `.local/bin/agy.new` and renames it over the launcher. |
| 10 | **Accepted** | `parse_update_check` and the `Latest::Command` doc take `latestVersion` "shown, not compared", suffix and all; pinning `is_dotted_version` on the recording would fail on a truthful prerelease day. The fixture test (renamed `…_parses_and_names_the_installed_version_when_nothing_is_newer`) drops that assertion and keeps only the parse and the `!available → latest == installed` check. |
| 11 | **Accepted** | `tests/what_we_run_test.rs:191-218` iterates `CLAUDE.uninstall` only. Task 6 now replaces it with `test_what_we_run_names_every_path_a_path_list_uninstall_moves_or_keeps` over every `Uninstall::Paths` recipe (section by `meta.name`; `remove` + `keep` + `backup_globs` dirs; the budget sentence per section — both new sections gained "The whole uninstall has 120 s"), asserting three such recipes; import change spelled out against the landed `use` lines. |
| 12 | **Accepted** | Copy `warnings.willKeep.OutsideHome` says "a dead link you can delete yourself"; with existence as the only test, Homebrew's live `/usr/local/bin/grok` on an Intel Mac (`grok-build` cask; grok.md §2) or another CLI's `agent` would get that sentence — a false, safety-relevant instruction. Task 4 adds `points_into(link, root)` (resolved target under the canonical root, or, dangling, the link's own text folded from its folder with `route::lexical_join` under the root as spelled or canonical); `outside_home_keeps(look)` reports only those. Tests: the outside-keep test gains a Homebrew-shaped link and a regular file (no sentence, no refusal), and a new `test_points_into_…` covers resolving, dangling absolute, dangling relative, elsewhere, file, folder, missing. Ruling 6, the `KeptWhat::OutsideHome` doc, `KeepSpec`'s doc, the recipe doc, the trust file and the delivery note say the condition; the copy is unchanged and now true. |
| 13 | **Accepted** | The grok recording had only `version.json`'s mtime around `--version`, no layout snapshot before, and a pre-written "changes no recipe" conclusion, while both whether `--version` reaches the launch-time updater and whether that updater installs are UNVERIFIED (grok.md §5). Task 6 Step 2 now snapshots `ls -lan ~/.grok/bin ~/.grok/downloads` + `readlink ~/.grok/bin/grok` before and after each grok invocation, with agy's stop rule (a changed link or download stops the recording; the author decides the version read), records a moved mtime after `--version` as the open question it is, and the README, ruling 16, the GROK doc comment, `## Grok Build` (a bracketed observation to fill), the backlog and deviation 16 say what was observed rather than a pre-decided conclusion. |
| 14 | **Accepted** | grok.md §3: the answer carries `"error":null`; the parser read only two fields, so exit 0 + `updateAvailable:false` + a non-null `error` would show "up to date" for a failed check. `Latest::Command` gains `error_field: Option<&'static str>` (GROK: `Some("error")`), `parse_update_check` a fourth parameter returning `Err("the update check reported: <text>")` for a present non-null value (string trimmed, other JSON as printed, 80 chars); table rows added to the parser test (string error, object error, `None` ignores the key), a `check_updates` case added to the failing list, the recipe test and the fixture test destructure the new field; ruling 10, Core Interfaces, `## Grok Build` and deviation 15 record it. |
| 15 | **Accepted** | Spec §6.3 (line 489) lists `~/.grok/bin · Dir · Launcher` last; grok.md §2's rc block puts `~/.grok/bin` on `PATH`, so a user's own script there would go to the Trash under "(the command itself)". The review's first option is taken: `~/.grok/bin/agent` (`SymlinkIntoRoot · Launcher · optional`) and `~/.grok/bin/grok` (`SymlinkIntoRoot · Launcher`, last) are the items, the emptied folder stays inside the kept `~/.grok`, and C's launcher-last invariant (`last.path == route.launcher`) stays as C wrote it — Task 4 adds only the `expect`-per-`RouteKind` assertion agy's `File` needs. Ruling 4 rewritten with the author-facing consequence; the recipe, its test, every 5c/Session path list (with `layout.agent`), the `RemovedWhat::Launcher` doc, `listed_items`' doc, the trust file, the fixture README, the README row, a new backlog entry and deviation 14 follow. A `my-own-script` in `~/.grok/bin` is asserted untouched. |
| 16 | **Accepted** | Landed never-list bullet (`docs/what-we-run.md:861-867`) names only `~/.claude`/`~/.claude.json`; spec §9.5 promises `~/.grok` and `~/.gemini` too. Task 6 (e) amends that bullet to name `~/.grok` (what of it moves), `~/.gemini/antigravity-cli` and `~/.gemini` itself; the generalized path test (point 11) also asserts every `Settings`/`SettingsAndHistory`/`ToolState` keep path appears in the never-list section. |
| 17 | **Accepted** | grok's check rewrites `~/.grok/version.json` (`checked_at`) on every Canager refresh — a write the "后台刷新不写机器" promise must name. Task 6 adds a bracketed observation sentence to `## Grok Build`'s check paragraph, a sentence under `## Files Canager writes` (new edit (h)), and the exception to the "Never writes a file" bullet; the GROK doc comment and ruling 16 say it. |
| 18 | **Accepted** | `ps -axo comm \| grep -i -E 'agy\|antigravity'` matches the Antigravity desktop app/IDE (agy.md §1) and anything containing "agy". Task 6 Step 1 now takes `ps -axo pid,ppid,comm` before and after and diffs (`comm -13`) for *new* processes, names the false positive, and makes the log count and mtime the primary evidence; ruling 16 and the README wording follow. |
| 19 | **Accepted** | `--version` ran twice inside one window while the README said "the very `--version` that is recorded". Both recordings now run `--version` once into a scratch file, read the version back from it and move the file into place; the READMEs say "the one `--version` run of this recording, whose output is `version.txt`". Global Constraints say "(once)". |
| 20 | **Accepted** (same finding as 7) | Verified as under 7; ruling 3 now states the defensible core (UNVERIFIED link text → go while every folder it could pass through exists; a dangling fallback would be accepted as `LauncherOnly`, so the order is a precaution). |
| 21 | **Accepted** | macOS runners have no `timeout` (coreutils is g-prefixed); `exit=127` had no row. The workflow resolves `gtimeout`/`timeout` first and writes `no timeout command` to the `.exit` file otherwise; the table gains an `exit=127`/`no timeout command` row. The section header no longer says "blocking" for both; a "What blocks the merge, stated once" paragraph makes grok's result the gate and claude's a pre-release follow-up (B shipped the button), Step 3, Step 4, the delivery note, the backlog and deviation 13 say the same thing, and the departure from spec §五 is named. |
| 22 | **Accepted** | `## Antigravity CLI` hard-coded "(`1.2.10`)" beside `[VERSION_AGY]`; now "(`[VERSION_AGY]`)". |
| 23 | **Accepted** | `GrokLayout.agent` had no reader. It is now read in the detect test (resolves to `layout.real`; still one instance), in every grok path list (it is an item since ruling 4), in the stopped-uninstall test (dangles after `downloads/` moved; gone after the second run, the folder still there) and in the Session test's expected list. |

**Counts:** 23 points; 23 accepted (20 is a duplicate of 7 and is recorded as such); 0 rejected.

**Remaining risks after this pass** (each with its owner):

1. **grok's fallback link text is still UNVERIFIED** and no Mac in reach has the links; the recipe's `SymlinkIntoRoot` expectation for `~/.local/bin/{grok,agent}` is an inference from the installer's shape. If the text is two hops through `~/.grok/bin/grok`, `probe_strict`'s `one_hop` rule (the launcher must be *one* link into the root) may call a *resolving* two-hop fallback `Absent` → `NotWhatInstructionsExpect` → kept as `NotOurs` (optional), which is safe but leaves the link behind with a "couldn't confirm" sentence. Backlog entry; the CI probe can settle it with `GROK_BIN_DIR` off `PATH`.
2. **The trust file now carries bracketed observations** (`[VJSON_VERSION_OBSERVATION]`, `[VJSON_CHECK_OBSERVATION]`, `[DATE]`) that the executor fills from the recording; a forgotten bracket would ship as literal text. `what_we_run_test` does not scan for `[` — the executor's Step 7 read of the two sections is the check. A `test_what_we_run_has_no_unfilled_brackets` would be a one-liner if the author wants it pinned.
3. **`points_into`'s dangling branch is lexical**, as C's `probe_strict` is; a fallback link whose text climbs through a *symlinked* folder outside the home (`/usr/local/bin` → `/opt/homebrew/bin` on some setups) is compared by text, not by the disk. It errs toward *no sentence* (a false negative), never toward the false "dead link" sentence.
4. **Ruling 4's emptied `~/.grok/bin`** remains on the user's `PATH` via the installer's rc line; harmless, and a future rc-line cleanup (backlog, spec §十一) is where it would be addressed. The spec's folder shape is departed from; the author should confirm the deviation when merging.
5. **The CI probe's claude half is no longer a merge gate**; if `claude update` prompts, B's shipped button fails honestly (`Failed` with the prompt text) until the follow-up lands. Named in the header, the backlog and deviation 13 so it cannot be mistaken for an oversight.
6. **Deviation numbering**: deviations 14–16 were inserted before the "Facts found" entry, now 17; rulings and the pre-merge header cite 13 and 14 by number, checked after the renumber.
