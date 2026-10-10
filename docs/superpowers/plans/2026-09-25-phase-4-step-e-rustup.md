# Phase 4 Step E: rustup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Banager a *rustup* group: the native `$CARGO_HOME/bin/rustup` install as one row with an honest update badge against `static.rust-lang.org/rustup/release-stable.toml`, an *Update* button that runs `rustup self update` (the first `NoCancel` plan any adapter produces, holding the cargo instance's lock as well as its own), and an *Uninstall* that runs rustup's official `rustup self uninstall -y` — offered only when Rust lives in its standard folders, `~/.cargo` and `~/.rustup`, computed exactly as rustup computes them — after a preview that names both folders by path and says, from rustup 1.29.1's own source, that they are deleted permanently and not moved to the Trash: every toolchain by name, the whole Cargo folder with its settings and saved login, every program in its `bin` folder by name where known, whether a Homebrew rustup loses its toolchains too, and which shell startup file will be left loading Cargo's env file once it is gone. The preview runs no command at all. A refresh that lands while rustup's update or uninstall is running leaves rustup and cargo alone. Plus the cargo adapter learning where each `cargo install`ed program lives, so the Unknown page stops listing `hexyl`.

**Architecture:** One more data row for the `StandaloneAdapter` step B built — the `RUSTUP` recipe — with the three shapes rustup is the first to need: a `FlatFile` route under `$CARGO_HOME`, a `SecondToken` version read, a `HttpTomlVersion` endpoint. Its uninstall is the new `Uninstall::Command` arm: a gate (`blocked`) that refuses any layout but the standard one, and a warnings function that reads `~/.rustup/toolchains/`, `~/.cargo/bin`, `.crates2.json`, Homebrew's Cellar and eight shell startup files — read-only, no command — and answers six `Warning` variants the uninstall dialog lists; the command itself runs through `run_plan` unchanged. Both of rustup's plans carry a second lock, built by the one function that spells the cargo instance's id (`cargo::instance_id_for`), because `self update` unlinks and re-copies the binary all thirteen `~/.cargo/bin` proxies exec and `self uninstall` deletes the file cargo's inventory reads. Every version read of rustup — its own and the cargo proxy's — carries `RUSTUP_AUTO_INSTALL=0`, and `refresh_round` skips the detection of any adapter whose instance an operation is holding, so no read of Banager's ever runs the rustup binary while an operation replaces or removes it. `HostEnv` learns `RUSTUP_HOME` and `ZDOTDIR`, the two variables rustup's own uninstall reads that Banager did not. `NoCancel` gets its first producer, and `operations.noCancelHint` its two readers.

**Tech Stack:** Rust (banager-core: `std::fs`, `toml` (already a dependency) for the release file, the existing `CommandRunner`/`HttpClient` seams; no new crate), TypeScript 5 `strict`, React 19, i18next, vitest.

**Spec:** `docs/superpowers/2026-09-24-phase-4-standalone-spec.md` (authoritative; Chinese). This plan implements §十 row E and argues from §0.1 (the four "No adapter produces `NoCancel` yet" sites), §一 D6/D9, §2.2–2.4, §3.1–3.5 (rustup's column), §4.1–4.3, §五 (rustup's row), §6.1, §6.4–6.6, §9.1–9.5 and 附录 A/B. Raw research: `~/dev/Banager/.superpowers/phase4/rustup.md` (tool facts) and `unknown-scan.md` (the thirteen proxies are relative symlinks). rustup's source was read at tag `1.29.1` and the `home` crate's at `home-0.5.12` (the version rustup 1.29.1 and cargo pin), both with `curl -sS https://raw.githubusercontent.com/…`, read-only. This plan will live at `docs/superpowers/plans/2026-09-25-phase-4-step-e-rustup.md`.

## Baseline, and how to read the anchors below

Branch `feat/phase-4-standalone`, worktree `~/dev/Banager-phase4`. First written at HEAD `f7e2917`, re-baselined at `3b5117a`, and **revised at HEAD `ea30cfb`** (2026-09-25: A and F landed; **all of B landed** — `71eacd0` the adapter, `f61cd94`, `dcf0e7c` and the follow-up wording fixes through `306dd57`; C's plan committed as `ea30cfb`, C's code not yet landed; working tree clean). Every `file:line` below is `ea30cfb`'s unless a table row says otherwise; C will move lines in the files it touches, which is why edits in those files are anchored by symbol or quoted text, never by number alone. **At execution time**, before Task 1, re-run `git log --oneline | head` and the confirm-grep in the table below, and treat a line number as a hint next to its textual anchor, not as the anchor. The chosen order is **A → F → B → C → E → D**, so when this plan executes:

- **A** (`docs/superpowers/plans/2026-09-24-phase-4-step-a-trust-and-guards.md`, landed): `ALLOWED_HTTPS_HOSTS` and `host_allowed` in `crates/banager-core/src/http/real.rs`; `warningKey`/`warningArgs` in `src/lib/warnings.ts` exhaustive over `Warning` with `never` defaults; `docs/what-we-run.md` rewritten with one `## <meta.name>` section per registered source, `## Files Banager reads`, `## Network: Banager only connects to these hosts` (a table `| Host | What is fetched | By |`), `## What Banager never does`; `crates/banager-core/tests/what_we_run_test.rs` holding it to the code.
- **F** (`…-step-f-unknown-scan.md`, landed): `crates/banager-core/src/scan/mod.rs` with `owned_roots` and `Known::index` (rules 0–3; rule 2 reads `InstalledArtifact.path`, "starts with"); `crates/banager-core/tests/unknown_scan_test.rs` whose `test_rule_1_claims_everything_that_resolves_to_an_instances_launcher` says in its comment that `hexyl` is listed "until step E fills `InstalledArtifact.path` for cargo binaries" (`:409`); the Unknown page.
- **B** (`docs/superpowers/plans/2026-09-24-phase-4-step-b-skeleton-claude.md`, landed in full): everything under "What already exists" below, **as it stands in the tree at `ea30cfb`** — `crates/banager-core/src/adapters/standalone/{mod,recipe,recipes,route,latest}.rs`, `adapters/meta/standalone-claude.toml`, `adapters/fixtures/standalone-claude/`, the `## Claude Code` section of `docs/what-we-run.md`, the README row. Where this plan modifies a B file it quotes the tree's text (rustfmt'd) and says what it becomes.
- **C** (path-list uninstall; plan `~/dev/Banager/.superpowers/phase4/plan-step-c-trash-uninstall.md`, committed as `ea30cfb` at `docs/superpowers/plans/…step-c…`; **being revised by another agent as this plan is revised**): lands before this step. This plan takes C's shapes from C's plan's Core Interfaces (which spell them in Rust) and, where those disagree with the spec, from C: `PlanAction::{Command { program, args, env }, TrashPaths { paths, previewed }}` and `Plan.action`; `Recipe.uninstall: Option<Uninstall>` with `Uninstall::Paths { remove, keep }`; `Detected { home, euid }` with `#[derive(Clone, Debug)]`; `Warning::{WillTrash, WillKeep, AlreadyGone}`; `Adapter::reconcile_after_uninstall` with `StandaloneAdapter`'s override over `route::probe_strict`; `route::probe_strict` with `probe = probe_strict(..).unwrap_or(Absent)`; `StandaloneAdapter::new(recipe, runner, http, trasher: Arc<dyn Trasher>)` with `standalone::all(runner, http, trasher)` and `banager_core::trash::MockTrasher`; `scan::display_path` made `pub(crate)`; `crate::testing::{command_program, command_args, command_env}`; `removal.rs` calling B's two-argument `route::expand`; a test-only `Recipe` literal with `uninstall: None` (C's ruling 2); `CommandPreview` taking `action={plan.action}`. **Every place this plan touches one of those is listed in "C dependency checklist" below, which the executor re-verifies against the landed C before Task 1 and again before Task 4.** The confirm-grep: `git log --oneline | head`, then `grep -n "enum PlanAction\|pub action:\|pub uninstall:\|pub euid\|WillTrash\|pub fn new(\|trasher\|fn probe_strict\|fn reconcile_after_uninstall\|fn display_path" crates/banager-core/src/model.rs crates/banager-core/src/adapters/standalone/recipe.rs crates/banager-core/src/adapters/standalone/mod.rs crates/banager-core/src/adapters/standalone/route.rs crates/banager-core/src/adapters/mod.rs crates/banager-core/src/scan/mod.rs` and `grep -rn "Recipe {$\|route::expand(\|Detected {" crates/banager-core/src/adapters/standalone/`.

In files A, F, B or C touch, **every edit below is anchored by a symbol, function, type or quoted line, never by a line number alone**. In files none of them touch (`session/refresh.rs`, `ops/mod.rs`'s lock set, `runner/path_env.rs`, `adapters/cargo.rs`, `tests/ops_cancel_test.rs`, `tests/ops_upgrade_version_test.rs`), `file:line` is cited at `ea30cfb`.

## C dependency checklist

Every point where this plan meets C. The executor re-verifies each row when C has landed, before Task 1 and again before Task 4 (the first task that edits a C file), and writes what it found beside the row in the branch's handover. A row whose C shape differs from what is written here is resolved by taking C's spelling wherever this plan uses it; the row says how far that substitution reaches.

| # | C shape | Where E touches it | What E does if C spelled it differently |
|---|---|---|---|
| 1 | `Plan { action: PlanAction::Command { program, args, env }, … }` (C Task 1) | Task 4 (Upgrade plan), Task 6 (Uninstall plan), Task 8 (ops tests read `plan.locks`, `plan.action`), Task 9 (`plan.cancel_policy` only) | Use C's field names; nothing else changes |
| 2 | `Detected { home, euid }`, `#[derive(Clone, Debug)]` (C stage 6c) — written in `detect`; C's test helper `fn detected(home: &Path) -> Detected` in `removal.rs`'s tests; C's `Detected { euid: …, ..detected(home.path()) }` | Task 4 adds `cargo_home: Option<PathBuf>`, `rustup_home: Option<PathBuf>`, `zdotdir: Option<PathBuf>`; `detect`'s literal gains the three; **C's `removal.rs` test helper `detected(home)` must gain `cargo_home: Some(home.join(".cargo")), rustup_home: Some(home.join(".rustup")), zdotdir: None`** (the struct-update literal needs nothing) | If C did not add `euid`, drop `euid: 501,` from `testing::detected`; if C's helper has another name, edit that one — `grep -rn "Detected {" crates/banager-core/src/adapters/standalone/` lists every literal, and `missing field` stops the build at any the grep missed |
| 3 | `route::expand(home, spec)` (B's two-argument function, unchanged by C) called from `removal.rs` at `Look::new` (`launcher`/`root`), `kept_places`, `plan_removal`'s remove loop and `take_turn` — five sites in C's plan | **Untouched.** E does not change `expand`'s signature; Task 4 adds `route::expand_route(home, cargo_home, spec)` for `detect`, and `recipes::tests::test_a_paths_recipe_names_only_home_paths` pins that every `Uninstall::Paths` recipe's route and spec paths start with `~/`, so `removal.rs` can never meet `$CARGO_HOME` | If C changed `expand`'s signature after all, make `expand_route` call C's `expand` for the `~/` case and re-run the grep |
| 4 | `StandaloneAdapter::new(recipe, runner, http, trasher)`; `standalone::all(runner, http, trasher)`; `banager_core::trash::MockTrasher` (behind `test-support`/`cfg(test)`) | Task 6's `rustup_adapter`, Task 7's and Task 8's adapters, Task 10's fixture tests: every direct constructor call passes `Arc::new(MockTrasher::new())` as the fourth argument | If `new` kept three arguments, drop the argument and the import |
| 5 | `Recipe.uninstall: Option<Uninstall>`, `Uninstall::Paths { remove, keep }`; C's `inventory` rule for `uninstall_blocked` (C ruling 2: `None` is the only producer of `NoSafeMethod`); C's `execute` dispatch; C's `plan` `Uninstall` arm; C's RECIPES-wide tests destructuring `Paths`; C's test-only `Recipe { …, uninstall: None }` | Task 4 adds `extra_locks: no_extra_locks` to every `Recipe {` literal; Task 6 adds `Uninstall::Command(CommandUninstall)` and settles every `match`/`if let` over `Uninstall` (the four-case list in Task 6 Step 3); Task 10 gives each RECIPES-wide `Paths` test its skip | `grep -rn "Uninstall::Paths\|recipe.uninstall\|\.uninstall\b" crates/banager-core/src/adapters/standalone/` before Task 6; the compiler names any non-exhaustive match |
| 6 | `Adapter::reconcile_after_uninstall` (C Task 2) and `StandaloneAdapter::reconcile_after_uninstall` answering presence through `route::probe_strict` (C stage 6e) | Task 8's end-to-end uninstall tests depend on it: after `rustup self uninstall -y`, presence of `$CARGO_HOME/bin/rustup` decides `Succeeded` / `StillInstalledAfterUninstall` / `Unconfirmed` | If C's override reads something other than `probe_strict`, Task 8's expectations still hold as long as it answers presence of the launcher; re-read it and say so in the handover |
| 7 | `route::probe_strict(kind, launcher, root) -> io::Result<Probe>` with `probe` as its `unwrap_or(Absent)` wrapper (C stage 6c) | Task 4's `FlatFile` arms go into **`probe_strict`** (the function that holds the `match kind`), not into the wrapper | If C did not split `probe`, the arms go into `probe` as B wrote it |
| 8 | `scan::display_path(path, home) -> PathBuf` made `pub(crate)` (C stage 6c) | Task 5's `rustup.rs` spells `~/.cargo` and `~/.rustup` with it | If still private, E makes it `pub(crate)` in Task 5 (F's file; one keyword) |
| 9 | `Warning::{WillTrash { path, what }, WillKeep { path, what }, AlreadyGone { path }}` in `model.rs`, `types.ts`, `warnings.ts` | Task 2 adds six variants beside them; the `never` defaults in `warnings.ts` are the anchors | Order in the enum is irrelevant |
| 10 | `<CommandPreview action={plan.action} />` in `UninstallDialog.tsx` (`:252` at `ea30cfb`, C may move it) and `UpdatesPage.tsx` (`:1086`) | Task 9 inserts a sibling element after it | Whatever its props are |
| 11 | `issuedPlanFor` in `UpdatesPage.test.tsx` / `UninstallDialog.test.tsx` | Task 9 changes only its `cancel_policy` line / overrides only `cancel_policy` | — |
| 12 | `UninstallBlocked::NoSafeMethod` produced by `inventory` for a recipe with no uninstall (C ruling 2) | Task 6 makes `Uninstall::Command`'s `blocked` a second producer of the same variant (the non-standard-layout gate), and Task 11 gives rustup's row its own sentence for it | If C's rule is an exhaustive `match`, widen it as Task 6 Step 3 says |
| 13 | `crate::testing::{command_program, command_args, command_env}` (C Task 1) | Task 8's tests may read a plan's program through them | Or match `plan.action` directly |
| 14 | `Session::new` extending `standalone::all(runner, http, trasher)` (C stage 6e) | Task 10's nine-adapter test | — |
| 15 | C's what-we-run.md rewording of the never-list bullet about moving files to the Trash, and its `## Claude Code` additions | Task 10's edits are anchored by words, not lines | Match by the quoted words |
| 16 | C's accessor for the seat in its `Paths` arm (C may hold one of its own) | Task 4 adds `seated_detected_for(inst)`, which binds the seat to the instance (ruling 9); the executor routes C's `Paths` arm through it too, so one binding rule holds for both arms | If C already binds the seat to the instance, keep C's and have `Command` use it |
| 17 | C's `test_new_registers_all_eight_adapters` untouched by C | Task 10 → nine | — |

## Global Constraints

Copied verbatim from the spec's binding rules (spec lines 20–23):

> 产品规则一条不让（spec §1、§6）：每一步说人话；后台工作绝不问密码；执行前先看到确切命令；
> 结果诚实——版本没动是 `NeedsAttention(UnchangedAfterUpgrade)`，中途停止是 `Unconfirmed`，
> 没有证据绝不说成功；fixture 只收真机录制；Banager 不跑 shell、不把下载管进 `sh`；
> 界面绝不提供 Rust 会拒绝的操作；所有文案 en + zh-CN。

And from spec §十 ("每一步只带**该步有生产者**的变体与字段——「先定义、后面某步再用」正是本项目最常见的缺陷") and §2.2/§2.3 ("每个新字段点名生产读取方"), applied to this step:

- **Every new field, variant, constant or function names its production reader in the same task** (as a doc comment, and in the task's Interfaces block), and that reader lands **within this step** — the spec's rule is per step (§十: 每一步只带该步有生产者的变体与字段), and the tasks below are one step's commits. Where the reader lands in a later task of this step, the Interfaces block says so by task number rather than implying it; the declared deferrals: `HostEnv.rustup_home`/`HostEnv.zdotdir` (Task 1; read by `StandaloneAdapter::detect` from Task 4 and by `rustup.rs` from Task 5), `Detected.{cargo_home, rustup_home, zdotdir}` (written by `detect` in Task 4, read in production by `rustup::{extra_locks, uninstall_blocked, uninstall_warnings}` from Task 6 on), and the whole of `rustup.rs` (Task 5, whose production caller is the `RUSTUP` recipe in Task 6). Everything this step defines has its producer here: `RouteKind::FlatFile`, `VersionParse::SecondToken`, `Latest::HttpTomlVersion` (the `RUSTUP` recipe); `Uninstall::Command`, `CommandUninstall` (the same recipe); the six `Warning` variants (`rustup::uninstall_warnings`); `Recipe.extra_locks` (`rustup::extra_locks`; claude's is the empty `no_extra_locks`); `cargo::{parse_crates2_bins, instance_id_for, cargo_home_of, RUSTUP_AUTO_INSTALL_OFF}` and `path_env::tool_home` (cargo's own `detect`/`inventory` and the rustup recipe); `rustup::RUSTUP_PROXIES` (`rustup::bin_programs_rustup_removes`); `OperationManager::locks_held` (`refresh_round`); `operations.noCancelHint` (two readers). What this step does **not** define: `Recipe.backup_globs`/`Glob`, `UpdateBlocked::SelfUpdatesOnly`, `Latest::{HttpJsonField, Command}` — step D's, with their producers.
- **Honest outcomes.** Nothing here touches `run_operation`'s outcome arms. `rustup self update` that exits 0 with the version unchanged is `NeedsAttention(UnchangedAfterUpgrade)`; one stopped by the timeout is `Unconfirmed` whatever the two version readings say (`ops/mod.rs:787-799`: the `Ok(Outcome::Unconfirmed)` arm keeps an `Upgrade` `Unconfirmed` unconditionally); `rustup self uninstall -y` is judged by presence of the launcher after the run — exit 0 with the launcher gone is `Succeeded`, exit 0 with it still there is `NeedsAttention(StillInstalledAfterUninstall)`, a timeout is `Succeeded` only when the launcher is gone and `Unconfirmed` while it is there (Task 8 has one test per outcome). Both of rustup's commands are `NoCancel`, so the only stop is the timeout. The uninstall's preview lists what rustup 1.29.1 actually deletes (Ruling 1), not what its newer source does.
- **Fixtures come from real machines only.** The one fixture directory this step adds, `adapters/fixtures/standalone-rustup/<version>/`, is recorded on the author's Mac by the read-only commands in Task 10 and nothing else, every rustup invocation among them carrying `RUSTUP_AUTO_INSTALL=0` (Ruling 20). **Never run** `rustup update`, `rustup self update`, `rustup self uninstall`, `rustup toolchain install/uninstall`, or `rustup check`. Synthetic layouts live in temp directories the tests build; inline strings in tests are not fixtures.
- **No shell.** `PlanAction::Command.program` is only ever the instance's `exe_path` (the launcher); the recipe has no field that could name another program; the uninstall preview runs nothing at all.
- **No read of Banager's runs rustup while an operation is replacing or removing it.** `RUSTUP_AUTO_INSTALL=0` on every version read of the rustup binary (rustup's own `--version` and the cargo proxy's, Ruling 20); `refresh_round` skips the detection of an adapter whose instance an operation is holding and carries it forward (Ruling 19, Task 7). Residual windows are stated in Task 7, not hidden.
- **The UI never offers what Rust refuses.** rustup's upgrade is refused nowhere; its uninstall is offered only when both roots are the standard ones (Ruling 18; the artifact carries `NoSafeMethod` otherwise, with a sentence saying why); a Running `NoCancel` op is refused a cancel by `OperationManager::cancel` (`ops/mod.rs:317-353`) and offered no Cancel button by `OperationBar.tsx` — both already at HEAD — and now told to the user in the preview by `operations.noCancelHint`.
- **en + zh-CN for all copy.** Every new key in both `src/i18n/en.json` and `src/i18n/zh-CN.json`; `src/i18n/completeness.test.ts` requires each key to be looked up by a *literal* in non-test source (lookups go through `Record`s of literal keys or literal `t("…")` calls, never assembled strings); `src/i18n/no-literal-strings.test.ts` forbids English literals in JSX; zh-CN prose uses full-width `，：（）` between CJK characters.
- **No author-machine details in tests or source** beyond public tool names the spec itself uses (`rustup`, `hexyl`, `ripgrep`/`rg`, the thirteen proxy names, `stable-aarch64-apple-darwin`); recorded fixtures carry what the commands printed.
- **The five gates**, from README.md "Tests — all five must pass before anything is committed" (the TypeScript gate is `pnpm typecheck`, two `tsc` programs):

  ```bash
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  pnpm test
  pnpm typecheck
  ```

  Run `cargo fmt --all` before the `--check` gate: the Rust below is written *for* rustfmt, and rustfmt decides line breaks.
- **Commits:** `git add <exact paths>` (never `-A`), an imperative subject in sentence case, a body that says why, a blank line, then the `Co-Authored-By:` attribution line **the executing session is instructed to use**. Every commit block below ends with the placeholder `Co-Authored-By: <the executing session's attribution line>`; substitute it, never paste it.

## Rulings this plan makes

Facts were checked against rustup's source at the tag the installed binary was built from: `rustup --version` on this Mac prints `rustup 1.29.1 (d95a37b6a 2026-08-13)`, GitHub's tag `1.29.1` is an annotated tag whose commit is `d95a37b6ab92cc1e455d1576039333c97ca3e2c5`, and that tag's `Cargo.toml` says `version = "1.29.1"`. Every rustup line number below is that tag's, read on 2026-09-25 with `curl -sS https://raw.githubusercontent.com/rust-lang/rustup/1.29.1/<path>`; every `home` line number is `home-0.5.12`'s `crates/home/src/env.rs`.

1. **rustup 1.29.1's `self uninstall` deletes `cargo install`ed programs too; the spec's `LeavesUnmanaged` is replaced by `RemovesCargoInstalled`.** rustup.md §8 quoted `clean_cargo_home` with an `is_same_file` check that keeps non-proxy binaries — that function exists on `master` and **not in 1.29.1**. The tag's `uninstall()` (`src/cli/self_update.rs:924-1032`): removes every toolchain (`:955-958`), `$RUSTUP_HOME` (`:960-966`), the shell lines (`:971-973`, before anything in `$CARGO_HOME`), everything in `$CARGO_HOME` except `bin/` (`:977-993`), then **everything in `bin/` that is not one of `TOOLS`/`DUP_TOOLS`/`rustup`** (`:996-1022`: `if file_is_tool == Some(false) { … remove_file … }`), and finally `delete_rustup_and_cargo_home` (`:1029`), which is `utils::remove_dir("cargo_home", &cargo_home)` — the whole directory, every entry in it, whatever its name (`src/cli/self_update/unix.rs:50-53`). Its own doc comment (`:915-922`, "Try to remove $CARGO_HOME/bin directory if it's empty") describes the newer behaviour, not the body. So on this Mac `hexyl` goes with `~/.cargo`, and "it stays on your Mac" would be a data-loss lie in the one sentence that matters. The variant is `Warning::RemovesCargoInstalled { names }` with copy that says the programs are deleted and can be reinstalled with `cargo install` afterwards; the unconditional Cargo sentence is `DeletesCargoHome { path }` (ruling 16). A newer rustup that keeps them makes this warning over-cautious, never false in the dangerous direction; `verified_versions` is `["1.29.1"]`, the trust file says which version the sentence was read from, and re-verifying it is part of bumping that list.
2. **The startup files rustup 1.29.1 edits are VERIFIED, and the model is rustup's own sequence of visits, not a count per file name.** `do_remove_from_path` (`unix.rs:55-77`) iterates `get_available_shells` in `enumerate_shells` order (`shell.rs:63-74`: Posix, Bash, Zsh, Fish, Nu, Tcsh, Pwsh, Xonsh) and, for each shell's `rcfiles()` that `is_file()`, removes the first exact `<source_string>\n` (`find_exact_line`, `unix.rs:164-172`: the line together with its newline, at a line start, byte for byte, first match only) from the file as it stands at that visit; then `remove_legacy_paths` (`unix.rs:174-194`) does the same for `export PATH="<S>/bin:$PATH"` and then for `source "<S>/env"` over `legacy_paths` (`shell.rs:564-574`: `~/.bash_profile`, `~/.profile`, `$ZDOTDIR/.zprofile` when `Zsh::zdotdir` answers, `~/.zprofile`). The shells and their `rcfiles` (`shell.rs`): Posix, always (`:154-168`) → `~/.profile`; Bash, when any of its files exists (`:179-195`) → `~/.bash_profile`, `~/.bash_login`, `~/.bashrc`; Zsh, when `$SHELL` contains `zsh` or `zsh` is on `PATH` (`:229-245`) → `$ZDOTDIR/.zshenv` and `~/.zshenv`, **with no deduplication, so `ZDOTDIR=$HOME` visits `~/.zshenv` twice and removes two copies**; Fish (`:265-290`) → `…/fish/conf.d/rustup.fish`; Nu, Tcsh, Pwsh, Xonsh → files Banager does not read. `<S>` is `$HOME/.cargo` when the Cargo home is `<home>/.cargo` (compared lexically, `cargo_home_str_with_home`, `:43-58`) and the absolute path otherwise; the line each of sh/bash/zsh writes is `. "<S>/env"` (`source_string`, `:138-140`). `Zsh::zdotdir` (`:207-225`) reads `ZDOTDIR` from the environment when `SHELL` contains `zsh`, else asks `zsh -c 'echo -n $ZDOTDIR'`. So `rustup::rustup_rc_visits(home, zdotdir, S)` lists the visits in that order, `rustup::shell_config_leftovers` applies them to in-memory copies of the eight files Banager reads and looks at what is left; Banager runs no `zsh` and reads `ZDOTDIR` from `HostEnv.zdotdir` (ruling 20) — a `ZDOTDIR` rustup would learn only by asking zsh is not modelled, and the trust file says so. What is left is classified in two tiers (ruling 22).
3. **`extra_locks` lives on `Recipe`, not on `Uninstall::Command`.** Spec §3.1 puts `extra_locks: fn(&Detected) -> Vec<ResourceLock>` inside `Uninstall::Command`, but §2.4 and §五 require the cargo lock on the *Upgrade* plan too, and `UpgradeCmd` has no slot. One field on the recipe, read by both plans through `StandaloneAdapter::locks`, is one declaration for one fact; claude's (and, in D, agy's and grok's) is `recipe::no_extra_locks`.
4. **`Uninstall::Command` is a tuple variant over `CommandUninstall { args, timeout_secs, cancel, blocked, warnings }`, and there is no probe.** Spec §3.1's `Probe { args, timeout_secs }` (`rustup toolchain list`) is dropped: the preview must not run rustup at all (Astra finding 3 and the controller's ruling — `rustup_mode::main` and `proxy_mode::main` both begin with `cleanup_self_updater` (`rustup_mode.rs:669`, `proxy_mode.rs:15`), which deletes `$CARGO_HOME/bin/rustup-init` (`self_update.rs:1314-1323`), the updater a running `self update` has just downloaded there (`prepare_update`, `:1165-1225`) and is about to run (`run_update`, `unix.rs:120-131`)). The toolchain names come from a read-only listing of `<rustup_home>/toolchains/` (`rustup::toolchain_names`), which is also what `uninstall()`'s `cfg.list_toolchains()` removes. `blocked: fn(&Detected) -> Option<UninstallBlocked>` is the gate (ruling 18); `warnings: fn(&Detected) -> Vec<Warning>` the preview.
5. **`route::expand` keeps B's two-argument signature; `route::expand_route(home, cargo_home: Option<&Path>, spec) -> Option<PathBuf>` is the function that knows `$CARGO_HOME`.** B's `expand(home, spec)` joins `~/` only, and C's `removal.rs` calls it at five sites for `Uninstall::Paths` recipes, whose paths are all `~/`. Changing its signature would break C for nothing (Astra finding 5); a `$CARGO_HOME` recipe with a `Paths` uninstall is a programming error that `recipes::tests::test_a_paths_recipe_names_only_home_paths` turns into a red build. `expand_route` answers `None` for a `$CARGO_HOME` path when the Cargo home is unsupported (ruling 6), and `detect` then lists nothing; `~/` paths always expand.
6. **One rule for the Cargo home — the `home` crate's — one producer for the cargo instance id, and `RUSTUP_AUTO_INSTALL=0` on the cargo proxy's version read.** `path_env::tool_home(setting, home, default_dir)` is `home-0.5.12`'s `cargo_home_with_cwd_env`/`rustup_home_with_cwd_env` (`env.rs:67-79`, `:101-113`): an empty variable is ignored (`filter(|h| !h.is_empty())`), an absolute one is taken as is, a relative one is joined onto the *tool's* current directory — which Banager neither knows nor shares, so for Banager a relative value is **unsupported** (`None`): `cargo::cargo_home_of(&HostEnv) -> Option<PathBuf>` is `tool_home(env.cargo_home, env.home, ".cargo")`, `CargoAdapter::detect` lists nothing for `None` (today it would name an instance whose `prefix` is a relative path joined onto Banager's own cwd, and read `.crates2.json` from there — a wrong answer, not a missing one), and `StandaloneAdapter::detect` seats it. `cargo::instance_id_for(&Path)` is `model::instance_id("cargo", Some(<path>))`, used by `CargoAdapter::detect` and by `rustup::extra_locks` (spec §2.4, §十三 #42). And `CargoAdapter::detect`'s `cargo --version` carries `RUSTUP_AUTO_INSTALL=0` (`cargo::RUSTUP_AUTO_INSTALL_OFF`): on a rustup Mac `cargo` is the rustup binary in proxy mode, whose `Cfg::from_env(…, allow_auto_install = true, …)` and `local_toolchain(None)` → `maybe_ensure_active_toolchain` (`proxy_mode.rs:48-56`, `config.rs:555-578`, `:771-790`) install a toolchain when none is active unless that variable is `0` (`should_auto_install`, `config.rs:435-441`). A Homebrew or distro cargo ignores the variable. All `pub(crate)`.
7. **`parse_crates2_bins` reads the `bins` arrays; the artifact's `path` is one binary per crate.** `InstalledArtifact.path` is a single `Option<PathBuf>`; a crate with several binaries gets the one named after the crate when there is one, else the first the record lists (`cargo-binstall` → `cargo-binstall`; `ripgrep` → `rg`). The remaining binaries of a multi-binary crate stay on the Unknown page until `path` can hold several — a backlog note in Task 1's doc comment, in the delivery note and in the README row, not a lie in the code.
8. **This step introduces `RouteKind::FlatFile` and `VersionParse::SecondToken`** (B's ruling 1 expected D to, "with agy" / "with grok"); in the chosen order E precedes D, and rustup is a flat file read with the second token. D finds them present. **A flat-file route has no launcher-only state.** B's `probe` answers a dangling launcher from its link text alone — `LauncherOnly` whenever the text lands under `root`, whatever the route kind — and rustup's root is the whole Cargo home, so a dangling `$CARGO_HOME/bin/rustup -> rustup.old` would be listed as a rustup with no version, the launcher-only notice, and plans whose program is a dangling link. `probe_strict`'s dangling branch therefore decides by kind first, with an exhaustive `match` so a future kind must decide too: `SymlinkIntoRoot` keeps B's rule, `FlatFile` is `Absent` (Task 4).
9. **`plan` and `inventory` read the seat through `seated_detected_for(inst)`, which binds the seat to the instance.** The seat is one mutable slot that the latest `detect` overwrites; `plan` receives an instance and would otherwise run *that* instance's launcher with the *latest* seat's locks and warnings — detect home A, then home B, then plan for A's instance runs A's binary under B's cargo lock and B's startup files (Astra finding 6). The accessor expands the recipe's launcher and root against the seat and refuses (`AdapterError::Refused`) unless both equal `inst.exe_path` and `inst.prefix`; it also refuses before any detect, which `Session` never does (spec §3.2). `inventory` uses it for the gate (a mismatched or missing seat reads as blocked). C's `Paths` arm goes through the same accessor (C checklist row 16). B's refusal tests (wrong instance, wrong name, `Install`) still pass without a detect; B's two Upgrade-success tests gain a detect (Task 4 quotes them).
10. **Recording is read-only and the fixture files are the commands' bytes**: `RUSTUP_AUTO_INSTALL=0 ~/.cargo/bin/rustup --version` (stdout → `version.txt`, stderr → `version-stderr.txt`), `curl -sS …/release-stable.toml`, `ls -1 ~/.rustup/toolchains` (→ `toolchains.txt`, the names the preview lists), `ls -la ~/.cargo/bin` (→ `layout.txt`). No `rustup toolchain list`: no rustup subcommand but `--version` is run, and that one under the switch. The README states what was on the Mac as observations of those commands, not as copied research.
11. **`rustup self update` is not an atomic swap, which is why it stays `NoCancel` and holds the cargo lock** (spec left the atomicity UNVERIFIED). `update()` (`self_update.rs:1088-1138`) downloads a new `rustup-init` into `$CARGO_HOME/bin` (`prepare_update`, `:1165-1225`) and runs it with `--self-replace` (`run_update`, `unix.rs:120-131`); `self_replace` (`unix.rs:141-145`) calls `install_bins` (`self_update.rs:771-785`), which **`remove_file`s the running `rustup` and then `copy_file_symlink_to_source`s the new one in** (`:779-782`, with the comment "we must unlink it first"). Between those two calls `$CARGO_HOME/bin/rustup` does not exist and all thirteen proxies — `cargo` among them — are dangling links. A kill in that window leaves no Rust at all; a `cargo --version` from a cargo refresh in that window fails, reads a half-written file, or — earlier in the run — deletes the downloaded `rustup-init` (ruling 4). `NoCancel`, 600 s, the cargo instance's lock on the plan, and the refresh skip (ruling 19).
12. **The end-to-end "unchanged" test scripts an output whose wording is not recorded.** `rustup self update` was never run (rule above); the ops test in Task 8 scripts a plausible log and, as the file's own doc says, the outcome depends only on the exit code and the two `--version` readings, never on the text.
13. **`Session::new` registers nine adapters after this step** (`standalone-rustup` sorts after `standalone-claude`, before `uv`); the test is renamed `test_new_registers_all_nine_adapters`. D makes it eleven.
14. **The two empty-state sentences name Claude Code *and* rustup, in B's wording, not the spec's.** B's landed sentences (`src/i18n/en.json:310`, `:321` at `ea30cfb`) both name Claude Code "at its native installer's default location"; this step edits both to name Claude Code and rustup (Task 11 quotes them), and D adds Antigravity and Grok with their recipes.
15. **`RemovesToolchains { path, names }` is built from the entry names of `<rustup_home>/toolchains/`**, sorted, hidden names skipped; an empty `names` (no directory, an unreadable one, or none installed) makes the front end pick `warnings.removesToolchainsUnlisted`, as the spec says for a failed probe. That directory is what `uninstall()` removes toolchain by toolchain (`cfg.list_toolchains()`, `:955-958`) before deleting `$RUSTUP_HOME` whole, so its names are the toolchains that go.
16. **The unconditional Cargo sentence names the folder by path and says it is deleted permanently; the programs sentence follows rustup's own `bin/` rule, read-only.** 1.29.1's `uninstall()` removes everything in `$CARGO_HOME` except `bin/` (`self_update.rs:977-993`: `registry/`, `git/`, `.crates.toml`, `.crates2.json`, and also `config.toml`, `credentials.toml` — the crates.io login token — and `env`), then every entry of `bin/` whose *name* is not `rustup` or one of the thirteen proxies (`:996-1022`, `file_is_tool` compares names only, so a program copied into `bin/` by hand goes too, recorded or not), then the directory (`:1029`). The spec's `DeletesCargoCaches` ("Cargo's downloaded packages and its list of programs") omitted the settings, the login, the folder itself and the fact that nothing goes to the Trash. The variant is renamed **`DeletesCargoHome { path }`** — the spec's name no longer described the sentence, and a variant whose name contradicts its copy is the kind of drift this project refuses — and the copy names the path, says "permanently — not to the Trash", and lists what the folder holds (Task 2). The names in `RemovesCargoInstalled` come from `rustup::bin_programs_rustup_removes`: the union of `.crates2.json`'s `bins` and a read-only listing of `<cargo_home>/bin` minus `rustup` and `RUSTUP_PROXIES` (the same thirteen names, `TOOLS` + `DUP_TOOLS` at `src/lib.rs:16-32` of the tag), sorted, deduplicated, hidden entries (`.DS_Store`: deleted with the folder, but no program to name) skipped, and names that are not UTF-8 skipped — **not because rustup keeps them**: `file_is_tool` is `None` for such a name so the by-name loop (`:1009-1019`) skips it, but `remove_dir` on the whole folder (`:1029`) deletes it with everything else; a name that cannot be spelled in a sentence is simply not named, and `DeletesCargoHome` says the whole folder goes. The listing is what rustup acts on; the record is the spec's named source (§6.4) and still names what cargo installed when the directory cannot be listed. A record entry whose file is already gone is named although nothing is left to delete — an over-statement in the safe direction, never an omission.
17. **The uninstall acts on the Rust that Banager's own process environment sees, and the trust file says so.** `fix_path_env::fix()` (`src-tauri/src/lib.rs:18`) is `fix_vars(&["PATH"])` at the pinned rev `c4c45d5` (its `src/lib.rs:91-92`): it restores `PATH` from the login shell and nothing else. `HostEnv::discover` reads `CARGO_HOME`, and from this step `RUSTUP_HOME` and `ZDOTDIR`, from the process environment (`runner/path_env.rs`), `RealRunner` hands the child the inherited environment plus `spec.env` (`runner/real.rs:666`), and rustup's `uninstall()` reads `RUSTUP_HOME`/`CARGO_HOME` from its own environment (`home::rustup_home()`, `self_update.rs:963`; `process.cargo_home()`, `:932`). A variable exported only in a shell startup file is therefore invisible to Banager and to the rustup it runs — the two agree, which is what the gate (ruling 18) relies on. For `RUSTUP_HOME` that means both look at the default `~/.rustup`; toolchains kept in a directory only the shell names are left behind, not deleted. For `CARGO_HOME` it means Banager looks for rustup under `~/.cargo` and lists no rustup installed elsewhere, so nothing is offered for it. Both are the safe direction (nothing extra goes), but without the disclosure the preview's "every toolchain" would overstate. This step adds one sentence to the `RUSTUP` recipe doc (Task 6) and the `## rustup` section (Task 10).
18. **The uninstall is offered only for the standard layout** (Astra findings 2 and 4; the controller's ruling). `rustup::standard_roots(&Detected) -> Option<StandardRoots>` answers `Some { cargo_home: <home>/.cargo, rustup_home: <home>/.rustup }` only when: `Detected.cargo_home` and `Detected.rustup_home` are `Some` (a relative `CARGO_HOME`/`RUSTUP_HOME` is unsupported, ruling 6); each equals `<home>/<default>` lexically — the same comparison rustup itself makes when it decides how to spell the Cargo home in the shell line (`cargo_home_str_with_home`, `shell.rs:43-58`), over the same `HOME` (`HostEnv.home` is the process's `HOME`, `home::home_dir` is `std::env::home_dir`, which reads `HOME` first); `<home>/.cargo` is a directory and not a symbolic link (`symlink_metadata`); `<home>/.rustup` is a directory and not a symbolic link, or does not exist. Anything else — a custom absolute home, an empty-but-set variable that resolves elsewhere (it does not: empty means default, ruling 6), a root that is a link to somewhere else — and `rustup::uninstall_blocked` answers `Some(UninstallBlocked::NoSafeMethod)`: `inventory` puts it on the artifact (the gate in `session/plans.rs` then refuses, the Installed page hides the button), `plan(Uninstall)` refuses with the same reason, and the row's sentence says Banager only removes Rust from its standard folders (Task 11, both locales; the variant is C's/B's, the sentence is rustup's row's). With `RUSTUP_HOME=~/Documents`, `uninstall()` would `remove_dir` the user's documents (`:960-966`); this gate is why it cannot be asked to. The upgrade is not gated: `self update` touches only `$CARGO_HOME/bin`.
19. **A refresh does not run an adapter's detect while an operation holds one of its instances' locks, and carries the locked instances forward unchanged** (Astra finding 3; the controller's option (a), chosen over the rustup-only (b) because the hazard is not rustup's alone). `refresh_round` (`session/refresh.rs:156-167` at `ea30cfb`) spawns every adapter's `detect` before any lock is taken (`:274-277`); `CargoAdapter::detect` runs `cargo --version` (`cargo.rs:129-150`), and on a rustup Mac `cargo` *is* the rustup binary, whose `proxy_mode::main` begins with `cleanup_self_updater` (ruling 4) — so during `rustup self update`, which holds `standalone-rustup` and `cargo:<home>`, a refresh's cargo detect could delete the updater the operation is about to run, and its rustup detect could read the unlinked binary (ruling 11). Moving rustup's version read under its lock (option b) would leave cargo's. So: `OperationManager::locks_held()` snapshots the `held` set; `refresh_round`, before the detection fan-out, skips every adapter one of whose previous-round instance ids is a held lock and reuses that adapter's previous instances **unchanged** (no `Unavailable`, no note, not `stale`), and the per-instance fan-out carries forward the artifacts and updates of every instance whose lock is held instead of waiting for the lock; instances of a skipped adapter whose own lock is free are still inventoried under it. This replaces the previous behaviour for every adapter — a refresh used to *wait* on the instance's lock until the operation ended (`test_refresh_is_mutually_exclusive_with_an_operation_on_the_same_instance_but_not_others`, `refresh.rs:1395-1465`), which for a `brew install` was minutes — and Task 7 renames and re-asserts that test. **Residual, stated honestly:** the check is a snapshot at the start of the round, so an operation submitted after it can start while a detect's command is still running (the window is one `--version`); an operation still Queued behind another holds nothing and skips nothing; an operation holding only the cargo lock (a `cargo install`) does not skip rustup's detect, whose `rustup --version` is then harmless (`RUSTUP_AUTO_INSTALL=0`; `rustup-init` exists only during a self update, whose operation holds rustup's lock too); the operation's own `reconcile` readings run under its locks, as today; the front end refreshes when an operation finishes, as today, so the carried-forward rows are replaced then.
20. **`HostEnv` gains `rustup_home: Option<PathBuf>` (`RUSTUP_HOME`) and `zdotdir: Option<PathBuf>` (`ZDOTDIR`), both read by `HostEnv::discover` exactly as `cargo_home` is; every `HostEnv {` literal in the tree gains the two lines.** Spec §3.2 refused `HostEnv.rustup_home` because nothing read it (§十三 #16/#44); ruling 18's gate and ruling 2's visit model are its readers, so the spec's own rule admits it. `grep -rn --include='*.rs' "HostEnv {" crates src-tauri` finds 40 literals at `ea30cfb` (one the definition), in `runner/path_env.rs` (5), `adapters/{brew/mod,cargo,npm,ollama/mod,pip,pipx}.rs` (4, 4, 4, 6, 2, 2), `adapters/standalone/mod.rs` (2), `scan/mod.rs` (2), `session/scan.rs` (1), `session/test_support.rs` (4), `tests/unknown_scan_test.rs` (2), `src-tauri/src/ipc.rs` (1); none uses struct-update syntax, so each gets `rustup_home: None,` and `zdotdir: None,` after its `cargo_home: …,` line, and `missing field` names any the grep missed. And **`RUSTUP_AUTO_INSTALL=0` goes on every version read of the rustup binary** (Astra finding 1): `display_version` (`rustup_mode.rs:1819-1837`) calls `maybe_ensure_active_toolchain(None)`, which with no active toolchain and auto-install on (the default: `should_auto_install`, `config.rs:435-441`, is `true` unless the variable is `0` or `rustup set auto-install disable` was run) *installs* the default toolchain — a download and a write during a refresh, cut off by the 30 s timeout. With the variable set, `active_toolchain()` alone runs and rustup prints `info: no \`rustc\` is currently active` on stderr and exits 0; the version line on stdout is unchanged. The variable goes on `RUSTUP.version.env` (Task 6), on `CargoAdapter::detect`'s `cargo --version` (ruling 6, Task 1) and on the recording commands (Task 10). Two side effects of *any* rustup invocation remain and are disclosed in the trust file, not hidden: `Cfg::from_env` creates `$RUSTUP_HOME` when it is missing (`config.rs:321-323`, `ensure_dir_exists`) and `cleanup_self_updater` deletes a leftover `$CARGO_HOME/bin/rustup-init` from an earlier self update (ruling 4); the refresh skip (ruling 19) keeps the second away from a running self update.
21. **A Homebrew rustup is detected by a read-only look at Homebrew's Cellar, and the toolchain sentence always says other rustups lose theirs too.** rustup's homes depend only on `RUSTUP_HOME`/`CARGO_HOME`/`HOME` (`home::rustup_home_with_cwd_env`, `env.rs:101-113`), never on where the binary sits, so Homebrew's keg-only `rustup` formula shares `~/.rustup` with the native install — VERIFIED from source, where the spec (§十一) had it UNVERIFIED by test. `plan()` sees no snapshot, but `<prefix>/Cellar/rustup` existing under Homebrew's two default prefixes (`rustup::HOMEBREW_PREFIXES = ["/opt/homebrew", "/usr/local"]`; a custom prefix is unsupported by Homebrew itself on Apple Silicon) is a local, read-only signal (`rustup::homebrew_rustup_present`): when it is there the preview adds `Warning::HomebrewRustupLosesToolchains`; the `removesToolchains` sentence ends "Any other rustup that uses this folder loses its toolchains too" in every case. `uninstall_warnings` reads the real prefixes; `warnings_with(d, prefixes)` is the same function over a caller-given list, so its tests are hermetic and the adapter-level test filters the conditional line out (Task 6).
22. **What a startup file will do after rustup's cleanup is said in two tiers, never as a bare substring match** (Astra finding 7; the controller's ruling). After `rustup_rc_visits` has been applied to the in-memory copies (ruling 2), each remaining line of each of the eight files is looked at: a comment (trimmed, starting with `#`) counts for nothing; a line whose trimmed text is exactly one of the sourcing forms rustup itself writes — `. "<X>/env"`, `source "<X>/env"` (and fish's `source "<X>/env.fish"`) — with `<X>` a spelling whose target is the real Cargo home (`$HOME/.cargo` or the absolute `<home>/.cargo` when the Cargo home is the default; the absolute custom path otherwise) **will** print an error in every new terminal: `LeavesShellConfigLine { path, certain: true }` → `warnings.leavesShellConfigLine`; any other non-comment line that mentions the env file (`.cargo/env` for the default home, `<custom>/env` for a custom one, and `$CARGO_HOME/env`/`${CARGO_HOME}/env` always — a guard `[ -f … ] && . …`, an `echo`, `source ~/.cargo/env`, `. "$CARGO_HOME/env"`) **may**: `{ certain: false }` → `warnings.leavesShellConfigLineMaybe` ("…mentions Cargo's env file on a line rustup won't remove; if that line loads the file, every new Terminal window will print an error…"). One warning per file, the certain tier winning. With a custom Cargo home a line naming `$HOME/.cargo/env` is neither: that file survives this uninstall (Astra's counterexample), and the gate (ruling 18) means the custom case never reaches a preview anyway — the classifier is still tested for it, because it is a pure function that must be right on its own.

## What already exists (do not rebuild)

- From HEAD: `CancelPolicy::NoCancel` with its two readers, `OperationManager::cancel` refusing a Running one (`ops/mod.rs:344-346`) and `OperationBar.tsx:38` (`cancellable = … !== "NoCancel" || status === "Queued"`); `OpSummary.cancel_policy`; IPC `{"kind":"no_cancel"}`; the policy matrix in `tests/ops_cancel_test.rs:735-866`; the two IPC tests `test_cancel_operation_impl_refuses_a_running_no_cancel_op` (`ipc.rs:1733`) and `test_cancel_operation_impl_cancels_a_queued_no_cancel_op` (`:1778`), named in the fake adapter's field doc (`:629-630`); the two `OperationBar.test.tsx` cases (`:170-198`, `:200-234`). The "no adapter produces `NoCancel` yet" sentences, as they read at `ea30cfb`: `model.rs`, the `NoCancel` doc inside `pub enum CancelPolicy` (`No adapter produces this yet: a standalone self-updating installer (\`rustup self update\`) is the expected first.`), `src/lib/types.ts:174-175` (`No adapter produces \`NoCancel\` yet.`), `OperationBar.tsx:37` (`// No adapter produces \`NoCancel\` yet.`), `tests/ops_cancel_test.rs:742-743` (`none produced yet; a standalone self-updating installer is the expected first`) — and `OperationBar.test.tsx:173-174`.
- `CargoAdapter` (`adapters/cargo.rs`): `parse_install_key` (`:29-37`), `Crates2Root`/`parse_crates2_entries` reading only the keys (`:39-55`), `parse_crates2` (`:57-79`, `path: None` at `:74`), `detect` computing `cargo_home` at `:133-136`, running `cargo --version` with `env: Vec::new()` at `:138-150` and the id at `:161`, `read_crates2` (`:186-198`), `inventory` (`:200-206`); its tests `test_parse_crates2_from_the_recorded_fixture`, `temp_cargo_home` (`:540`), `test_instance` (`:552`). The recorded fixture `adapters/fixtures/cargo/1.98.1/crates2.json` (one crate, `"bins":["hexyl"]`).
- `run_plan` (`adapters/mod.rs:470-514`), `second_token`, `uncheckable_candidate` (`:270`), `ensure_instance_match` (`:415`), `validate_package_name`, `reconcile_from` (`:386`), `AdapterError::{Refused, Unsupported, InvalidName, UninstallBlocked { reason }, Parse}`, `Adapter` (`:426-`). `HostEnv { path_dirs, home, euid, cargo_home, ollama_host }` and `HostEnv::discover` (`runner/path_env.rs:5-24`, `:70-93`), `resolve_exe` (`:95-103`); `MockRunner::{respond, delay, calls}` (`runner/mock.rs`); `MockHttpClient::{respond, fail, calls, requests}` (`http/mock.rs`); `crate::testing::manager_instance`; `OperationManager::{new, register_adapter, register_instance, submit, cancel, wait, summaries, acquire_resource_lock}` and its private `held: Arc<Mutex<HashSet<ResourceLock>>>` (`ops/mod.rs:174`, shared with `ResourceLockGuard`).
- `Warning` (`pub enum Warning`, `model.rs:314`; C adds three variants) and its shape test `test_warning_wire_shapes_match_the_hand_written_ts_mirror`; `Warning` in `types.ts` and its shape test in `types.test.ts` (`it("spells Warning's bare-string variants as bare strings and WouldBreak/Message as externally tagged", …)`); `warningKey`/`warningArgs` exhaustive (A); `warningTexts` read by `UninstallDialog.tsx` (`planWarnings`) and by `UpdatesPage.tsx` (`itemWarnings`).
- From A: `ALLOWED_HTTPS_HOSTS`, `host_allowed`, `test_what_we_run_names_every_allowed_https_host`, `test_what_we_run_has_a_section_for_every_registered_source` (a `## <meta.name>` heading per registered id: for rustup, `## rustup`). From F: `owned_roots` with B's `standalone-claude` row and the `("standalone-rustup", "standalone-rustup", "/Users/someone/.cargo")` tuple in `test_owned_roots_table`'s "never a root" list (`scan/mod.rs:810-815`, comment "No adapter with this id exists yet (rustup is step E)"); `display_path` (`:209`, private until C).
- From B (in the tree): `Recipe { id, meta_toml, route, version, latest, self_updates, upgrade }`, `Route { kind, launcher, root }`, `RouteKind::SymlinkIntoRoot`, `VersionCmd { args, env, parse }`, `VersionParse::FirstToken`, `Latest::ClaudeChannel { base }`, `UpgradeCmd { args, timeout_secs, cancel }`; `latest::{is_dotted_version, parse_version, compare_dotted, parse_channel_body, claude_channel, CHANNEL_LATEST, CHANNEL_STABLE}`; `recipes::{CLAUDE, RECIPES}` and its tests (`test_every_recipe_path_is_under_home`, `test_every_recipe_launcher_is_named_after_its_id`, `test_every_recipe_meta_parses_and_names_the_standalone_id`, `test_recipes_lists_claude_once`, `test_every_recipe_latest_url_is_an_allowed_https_host`); `route::{expand, Probe, probe, lexical_join, shadow_note}`; `mod.rs`: `Detected { home }`, `StandaloneAdapter { recipe, meta, runner, http, detected }` (C adds `trasher`, `trash_gap`), `new`, `detect`, `read_version` (with `VERSION_TIMEOUT`), `artifact_key`, `inventory`, `search`, `check_updates`, `latest_version`, `plan`, `execute`, `reconcile`, `all`; `testing::{TempHome { new, path, dir, file, executable, link, env }, ClaudeLayout, claude_layout}`; the test helpers `exited_0`, `adapter`, `RecordingRunner { specs, output }`, `instance_for`, `request`, `answer`, `detected_adapter`, `fixture`; `standalone::all` registered in `Session::new`; `test_new_registers_all_eight_adapters` (`session/mod.rs:508`); `ADAPTER_LABEL_KEYS["standalone-claude"]`, `StandaloneAdapterId`, `STANDALONE_SUMMARY_KEYS`, `standaloneSummaryKey` (`sources.ts:20-60`), `UNINSTALL_BLOCKED_KEYS` (`:554-591`); `updates.selfUpdatingHint`; the `## Claude Code` section of `docs/what-we-run.md` (`:460`) and its "eight sources" intro (`:4`); `README.md`'s Claude Code row (`:29`) and its two test counts (`:12-13`, `:176-177`).
- From C (see the checklist): `PlanAction`, `Plan.action`, `Recipe.uninstall`, `Uninstall::Paths`, `Detected.euid`, the three `Warning` variants, `Trasher`, `removal.rs`, `probe_strict`, `reconcile_after_uninstall`, `display_path` as `pub(crate)`.

## File Structure

```
adapters/meta/standalone-rustup.toml                              NEW   AdapterMeta, seven fields (Task 6)
adapters/fixtures/standalone-rustup/1.29.1/                       NEW   README.md, version.txt, version-stderr.txt, release-stable.toml, toolchains.txt, layout.txt — recorded (Task 10)
crates/banager-core/src/runner/path_env.rs                         MOD   HostEnv.rustup_home, HostEnv.zdotdir, discover, tool_home (1)
crates/banager-core/src/adapters/cargo.rs                          MOD   cargo_home_of (Option), instance_id_for, RUSTUP_AUTO_INSTALL_OFF on detect, parse_crates2_bins, inventory fills path (1)
crates/banager-core/src/adapters/{brew/mod,npm,ollama/mod,pip,pipx}.rs, scan/mod.rs, session/scan.rs, session/test_support.rs, tests/unknown_scan_test.rs, src-tauri/src/ipc.rs   MOD   HostEnv literals gain two lines (1)
crates/banager-core/src/model.rs                                   MOD   six Warning variants + shape test (2); CancelPolicy doc (9)
crates/banager-core/src/adapters/standalone/recipe.rs              MOD   FlatFile (4), SecondToken + HttpTomlVersion (3), extra_locks + no_extra_locks (4), CommandUninstall + Uninstall::Command (6)
crates/banager-core/src/adapters/standalone/latest.rs              MOD   SecondToken arm, parse_release_stable_toml (3)
crates/banager-core/src/adapters/standalone/route.rs               MOD   expand_route, FlatFile arms in probe_strict's two branches (4)
crates/banager-core/src/adapters/standalone/rustup.rs              NEW   roots gate, toolchains, RUSTUP_PROXIES, bin/ programs, Homebrew signal, the rc visit model, uninstall_blocked, uninstall_warnings, extra_locks (5)
crates/banager-core/src/adapters/standalone/recipes.rs             NEW ROW RUSTUP (6), RECIPES gains it (10); invariants tests updated (4, 6, 10)
crates/banager-core/src/adapters/standalone/mod.rs                 MOD   Detected +3 fields, seated_detected_for, locks, Upgrade plan (4); HttpTomlVersion arm (3); Uninstall Command arm, inventory's gate, C's other Uninstall branches if needed (6); pub mod rustup, RUSTUP_PROXIES leaves testing (5); tests
crates/banager-core/src/adapters/standalone/removal.rs             MOD   C's test helper `detected(home)` gains three fields (4)                 [C's file]
crates/banager-core/src/ops/mod.rs                                 MOD   locks_held (7)
crates/banager-core/src/session/refresh.rs                         MOD   the detection skip and the carry-forward; one test renamed and re-asserted; two new tests (7)
crates/banager-core/src/http/real.rs                               MOD   ALLOWED_HTTPS_HOSTS += "static.rust-lang.org"; doc; test (6)   [A's file]
crates/banager-core/src/scan/mod.rs                                MOD   two comments (1, 10); display_path pub(crate) if C did not (5)    [F's file]
crates/banager-core/src/session/mod.rs                             MOD   the nine-adapter test (10)                                         [B's file]
crates/banager-core/src/lib.rs                                     MOD   crate doc clause (10)                                               [F's/B's file]
crates/banager-core/tests/unknown_scan_test.rs                     MOD   rule-2 companion test for cargo (1)                                [F's file]
crates/banager-core/tests/ops_cancel_test.rs                       MOD   the policy-matrix comment (9)
crates/banager-core/tests/ops_upgrade_version_test.rs              MOD   rustup self update unchanged → UnchangedAfterUpgrade; moved version (8)   [B's file]
crates/banager-core/tests/ops_rustup_uninstall_test.rs             NEW   the uninstall outcomes and the lock cases through OperationManager (8)
src-tauri/src/ipc.rs                                               MOD   two NoCancel tests renamed; the field doc naming them (9); one HostEnv literal (1)
src/lib/types.ts, types.test.ts                                    MOD   six Warning variants (2); CancelPolicy doc (9)                     [A's/C's file]
src/lib/warnings.ts, warnings.test.ts                              MOD   six branches (2)                                                   [A's/C's file]
src/lib/sources.ts, sources.test.ts                                MOD   label, StandaloneAdapterId, summary key, uninstallBlockedCopy (11)   [B's file]
src/pages/InstalledPage.tsx, src/components/UninstallDialog.tsx    MOD   uninstallBlockedCopy in place of UNINSTALL_BLOCKED_KEYS[…] (11)    [B's/C's files]
src/components/OperationBar.tsx, OperationBar.test.tsx             MOD   comment; the two NoCancel cases renamed to rustup (9)
src/components/UninstallDialog.tsx, UninstallDialog.test.tsx       MOD   noCancelHint after the preview; test (9)                           [C's file]
src/pages/UpdatesPage.tsx, UpdatesPage.test.tsx                    MOD   noCancelHint after the preview; harness knob; test (9)             [B's/C's file]
src/components/SnapshotStatus.test.tsx                             MOD   the two empty-state sentences (11)                                 [B's file]
src/i18n/en.json, zh-CN.json                                       MOD   warnings.* (2), operations.noCancelHint (9), adapters/standalone.summary/emptyStates/installed.blocked.NoSafeMethod.standalone-rustup (11)   [B's/C's files]
docs/what-we-run.md                                                MOD   Cargo section (1); host row (6); intro, program-source paragraph, `## rustup`, files read, never-list (10)   [A's/B's file]
README.md                                                          MOD   the source row; the two test counts (12)                          [B's file]
```

Single responsibility: `rustup.rs` owns *what rustup's uninstall does, when Banager may offer it, and what to say about it* (data and pure functions over files); `recipe.rs` the shapes; `recipes.rs` the data; `latest.rs` versions; `route.rs` recognition; `mod.rs` the `Adapter` contract; `cargo.rs` everything about `.crates2.json` and the cargo instance's identity; `path_env.rs` what the host environment says and the `home` crate's rule for reading it; `refresh.rs` which adapters a round asks.

## Core Interfaces (authoritative — every task uses these names verbatim)

```rust
// crates/banager-core/src/runner/path_env.rs
pub struct HostEnv { pub path_dirs: Vec<PathBuf>, pub home: PathBuf, pub euid: u32, pub cargo_home: Option<PathBuf>, pub rustup_home: Option<PathBuf>, pub zdotdir: Option<PathBuf>, pub ollama_host: Option<String> }
pub(crate) fn tool_home(setting: Option<&Path>, home: &Path, default_dir: &str) -> Option<PathBuf>;   // home 0.5.12's rule; None = relative, unsupported

// crates/banager-core/src/adapters/cargo.rs
pub(crate) const RUSTUP_AUTO_INSTALL_OFF: (&str, &str) = ("RUSTUP_AUTO_INSTALL", "0");
pub(crate) fn cargo_home_of(env: &HostEnv) -> Option<PathBuf>;
pub(crate) fn instance_id_for(cargo_home: &Path) -> String;
pub(crate) fn parse_crates2_bins(json: &str) -> Result<Vec<(String, Vec<String>)>, AdapterError>;

// crates/banager-core/src/model.rs
pub enum Warning { …existing…, RemovesToolchains { path: String, names: Vec<String> }, DeletesCargoHome { path: String }, RemovesCargoInstalled { names: Vec<String> }, HomebrewRustupLosesToolchains, EditsShellConfig, LeavesShellConfigLine { path: String, certain: bool } }

// crates/banager-core/src/adapters/standalone/recipe.rs
pub enum RouteKind { SymlinkIntoRoot, FlatFile }
pub enum VersionParse { FirstToken, SecondToken }
pub enum Latest { ClaudeChannel { base: &'static str }, HttpTomlVersion { url: &'static str } }
pub struct CommandUninstall { pub args: &'static [&'static str], pub timeout_secs: u64, pub cancel: CancelPolicy, pub blocked: fn(&Detected) -> Option<UninstallBlocked>, pub warnings: fn(&Detected) -> Vec<Warning> }
pub enum Uninstall { Paths { … } /* C */, Command(CommandUninstall) }
pub struct Recipe { …B's fields…, pub uninstall: Option<Uninstall> /* C */, pub extra_locks: fn(&Detected) -> Vec<ResourceLock> }
pub fn no_extra_locks(_: &Detected) -> Vec<ResourceLock>;

// crates/banager-core/src/adapters/standalone/latest.rs
pub fn parse_release_stable_toml(body: &str) -> Result<String, String>;

// crates/banager-core/src/adapters/standalone/route.rs
pub fn expand(home: &Path, spec: &str) -> PathBuf;                                           // B's, unchanged
pub fn expand_route(home: &Path, cargo_home: Option<&Path>, spec: &str) -> Option<PathBuf>;   // `~/` always Some; `$CARGO_HOME…` Some iff cargo_home is Some

// crates/banager-core/src/adapters/standalone/rustup.rs
pub const SHELL_RC_CANDIDATES: [&str; 8];
pub const RUSTUP_PROXIES: [&str; 13];   // defined in `testing` by Task 4, moved here by Task 5
pub const HOMEBREW_PREFIXES: [&str; 2];
pub struct StandardRoots { pub cargo_home: PathBuf, pub rustup_home: PathBuf }
pub fn standard_roots(d: &Detected) -> Option<StandardRoots>;
pub fn uninstall_blocked(d: &Detected) -> Option<UninstallBlocked>;
pub fn toolchain_names(rustup_home: &Path) -> Vec<String>;
pub fn bin_programs_rustup_removes(cargo_home: &Path) -> Vec<String>;
pub fn homebrew_rustup_present(prefixes: &[PathBuf]) -> bool;
pub fn cargo_home_str(home: &Path, cargo_home: &Path) -> String;
pub struct RcVisit { pub file: PathBuf, pub line: String }
pub fn rustup_rc_visits(home: &Path, zdotdir: Option<&Path>, cargo_home_str: &str) -> Vec<RcVisit>;
pub fn remove_first_exact_line(contents: &mut String, line: &str) -> bool;
pub struct LeftoverPatterns { pub sourcing: Vec<String>, pub needles: Vec<String> }
pub fn leftover_patterns(home: &Path, cargo_home: &Path) -> LeftoverPatterns;
pub enum Leftover { Sources, Mentions }
pub fn classify_leftover(contents: &str, patterns: &LeftoverPatterns) -> Option<Leftover>;
pub fn shell_config_leftovers(home: &Path, zdotdir: Option<&Path>, cargo_home: &Path) -> Vec<Warning>;
pub fn warnings_with(d: &Detected, homebrew_prefixes: &[PathBuf]) -> Vec<Warning>;
pub fn uninstall_warnings(d: &Detected) -> Vec<Warning>;
pub fn extra_locks(d: &Detected) -> Vec<ResourceLock>;

// crates/banager-core/src/adapters/standalone/recipes.rs
pub static RUSTUP: Recipe;
pub static RECIPES: &[&Recipe] = &[&CLAUDE, &RUSTUP];

// crates/banager-core/src/adapters/standalone/mod.rs
#[derive(Clone, Debug)] pub struct Detected { pub home: PathBuf, pub euid: u32 /* C */, pub cargo_home: Option<PathBuf>, pub rustup_home: Option<PathBuf>, pub zdotdir: Option<PathBuf> }
impl StandaloneAdapter {
    fn seated_detected_for(&self, inst: &ManagerInstance) -> Result<Detected, AdapterError>;
    fn locks(&self, inst: &ManagerInstance, detected: &Detected) -> Vec<ResourceLock>;
    fn command_uninstall_plan(&self, inst: &ManagerInstance, req: &OpRequest, detected: &Detected, cmd: &CommandUninstall) -> Result<Plan, AdapterError>;
}
pub(super) mod testing { pub struct RustupLayout { pub cargo_home: PathBuf, pub launcher: PathBuf } pub fn rustup_layout(cargo_home: &Path) -> RustupLayout; pub fn detected(home: &Path, cargo_home: &Path) -> Detected; }

// crates/banager-core/src/ops/mod.rs
impl OperationManager { pub fn locks_held(&self) -> HashSet<ResourceLock>; }

// crates/banager-core/src/http/real.rs
pub const ALLOWED_HTTPS_HOSTS: &[&str] = &["crates.io", "pypi.org", "registry.ollama.ai", "downloads.claude.ai", "static.rust-lang.org"];
```

```ts
// src/lib/types.ts
export type Warning = …existing… | { RemovesToolchains: { path: string; names: string[] } } | { DeletesCargoHome: { path: string } } | { RemovesCargoInstalled: { names: string[] } } | "HomebrewRustupLosesToolchains" | "EditsShellConfig" | { LeavesShellConfigLine: { path: string; certain: boolean } };
// src/lib/sources.ts
export type StandaloneAdapterId = "standalone-claude" | "standalone-rustup";
export function uninstallBlockedCopy(reason: UninstallBlocked, adapterId: string | undefined): UninstallBlockedCopy;
// ADAPTER_LABEL_KEYS gains "standalone-rustup": "adapters.standalone-rustup"
// STANDALONE_SUMMARY_KEYS gains "standalone-rustup": "standalone.summary.standalone-rustup"
// i18n: warnings.{removesToolchains, removesToolchainsUnlisted, deletesCargoHome, removesCargoInstalled_one, removesCargoInstalled_other, homebrewRustupLosesToolchains, editsShellConfig, leavesShellConfigLine, leavesShellConfigLineMaybe}, operations.noCancelHint, adapters.standalone-rustup, standalone.summary.standalone-rustup, installed.blocked.NoSafeMethod.standalone-rustup.{description, refused}
```

## Task List

| # | Task | Deliverable |
|---|---|---|
| 1 | The host environment rustup reads: `HostEnv.{rustup_home, zdotdir}`, `tool_home`, cargo's home rule and id producer, `RUSTUP_AUTO_INSTALL=0` on cargo's read, `.crates2.json`'s binaries, `path` on every cargo artifact | `hexyl` leaves the Unknown page; the two names the rustup lock and gate will use exist once; no read of Banager's installs a toolchain |
| 2 | Six `Warning` variants, their mirror, keys and copy | the wire contract for what rustup's uninstall does, both locales |
| 3 | `SecondToken`, `HttpTomlVersion`, `parse_release_stable_toml` | rustup's version line and release file parse |
| 4 | `FlatFile`, `expand_route`, the seat bound to its instance, `Recipe.extra_locks`, the Upgrade plan's locks | the shapes rustup's route, gate and locks need, with claude unchanged in behaviour |
| 5 | `rustup.rs`: the roots gate, toolchains, cargo-installed programs, the Homebrew signal, the startup-file visit model, the warnings, the lock | every sentence of the uninstall preview and the gate that decides whether there is one, tested on synthetic files |
| 6 | `Uninstall::Command`, the `RUSTUP` recipe and meta, the Uninstall plan and the gate in `inventory`, the host on the allowlist | rustup as a recipe: detect, badge, both plans, the cross-lock equality test, the seat test |
| 7 | A refresh leaves an adapter alone while an operation holds its instance | no read of Banager's runs rustup during `rustup self update`; the concurrency tests |
| 8 | End to end through `OperationManager`: the upgrade's two outcomes, the uninstall's four, the lock cases | honest outcomes proven against the engine, not described |
| 9 | `NoCancel`'s first producer and `operations.noCancelHint`'s two readers | the four sentences point at rustup; the preview warns before the click |
| 10 | Recording, registration, the trust file | rustup is a source: fixtures, nine adapters, `## rustup` in what-we-run.md |
| 11 | Front end: label, summary, empty states, the standard-folders sentence | the pages name rustup and say why a non-standard layout gets no Uninstall |
| 12 | README | the source row and the test counts |

Order: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10 → 11 → 12. Tasks 1 and 2 are independent of each other; 3–6 are strictly sequential (6 needs all of 1–5); 7 needs 6 (its concurrency test runs the `RUSTUP` recipe); 8 needs 6 and 7; 9 needs 6; 10 needs 6–9; 11 needs 10's id; 12 is last because it counts tests.

## Review Focus

Seven inputs the spec implies but no test would otherwise exercise, most likely to bite first. Each has its test in the task named.

1. **`~/.rustup/toolchains/` is missing, empty or unreadable** (a rustup with every toolchain removed) → empty `names`, the "every toolchain" sentence, never a refused preview (Task 5, `test_toolchain_names_is_empty_for_no_directory_and_lists_entries_sorted`).
2. **`~/.zshenv` holds rustup's line *and* a hand-written `source ~/.cargo/env`, or rustup's line twice, or its line last with no newline; `ZDOTDIR` is `$HOME`** → rustup removes one exact line per visit and visits `.zshenv` twice under `ZDOTDIR=$HOME`; the certain tier is claimed only for a line rustup's own form spells, the hand-written spelling is the qualified tier (Task 5, `test_rustup_rc_visits_visit_zshenv_twice_when_zdotdir_is_home`, `test_shell_config_leftovers_removes_two_copies_when_zdotdir_is_home_and_one_otherwise`, `test_classify_leftover_puts_rustups_own_forms_in_the_certain_tier_and_the_rest_in_the_qualified_one`).
3. **`CARGO_HOME` or `RUSTUP_HOME` is set to another directory, is empty, is relative, or a root is a symbolic link** → the launcher and the cargo lock follow `CARGO_HOME` (empty means default; relative means no instance), and the uninstall is offered for none of the non-standard layouts (Task 1 `test_tool_home_follows_the_home_crates_rule`, Task 5 `test_standard_roots_accepts_only_the_default_layout_of_real_directories`, Task 6 `test_detect_follows_cargo_home_for_rustup`, `test_inventory_and_plan_refuse_the_uninstall_for_a_non_standard_layout`, `test_rustup_locks_name_the_cargo_instance_detect_produces_with_and_without_cargo_home`).
4. **`.crates2.json` is missing, unreadable or not JSON at preview time, or `bin/` holds a program no record lists** → the names come from the listing of `bin/` alone, and a program copied there by hand is named, since rustup deletes it by name; with nothing in either, no `RemovesCargoInstalled` line; never a refused or crashed preview (Task 5, `test_bin_programs_rustup_removes_names_a_program_no_record_lists`, `test_bin_programs_rustup_removes_is_empty_for_no_directory_and_no_record`; Task 6, `test_plan_uninstall_for_rustup_survives_an_empty_cargo_home_and_no_toolchains`).
5. **The release file answers with a `version` that is not a version, or with HTML** (a captive portal answering 200) → an uncheckable row with a short reason, never a candidate built from it (Task 3, `test_parse_release_stable_toml_refuses_anything_that_is_not_a_versioned_release_file`; Task 6, `test_check_updates_marks_a_bad_release_file_uncheckable`).
6. **A refresh arrives while `rustup self update` is running** → neither rustup's nor cargo's `--version` runs; both rows are carried forward unchanged; the next refresh after the operation reads again (Task 7, `test_a_refresh_during_rustups_self_update_runs_neither_rustup_nor_cargo`).
7. **The seat belongs to another home** (detect A, detect B, plan A) → `Refused`, and a fresh detect of A makes the plan go through with A's cargo lock (Task 6, `test_plan_refuses_an_instance_the_seat_no_longer_describes`; Task 8, `test_a_plan_for_an_instance_from_another_home_is_refused_and_a_redetect_restores_it`).

---

### Task 1: The host environment rustup reads — `HostEnv.{rustup_home, zdotdir}`, `tool_home`, cargo's home rule and id producer, `RUSTUP_AUTO_INSTALL=0` on cargo's read, `.crates2.json`'s binaries, `path` on every cargo artifact

**Files:**
- Modify: `crates/banager-core/src/runner/path_env.rs:5-24` (`HostEnv`), `:70-93` (`discover`), new `tool_home`, `mod tests` (its five `HostEnv {` literals and two new tests)
- Modify: every other `HostEnv {` literal in the tree — at `ea30cfb`: `crates/banager-core/src/adapters/brew/mod.rs` (4), `adapters/cargo.rs` (4), `adapters/npm.rs` (4), `adapters/ollama/mod.rs` (6), `adapters/pip.rs` (2), `adapters/pipx.rs` (2), `adapters/standalone/mod.rs` (2: `TempHome::env` and one test), `scan/mod.rs` (2), `session/scan.rs` (1), `session/test_support.rs` (4), `tests/unknown_scan_test.rs` (2), `src-tauri/src/ipc.rs` (1) — a mechanical two-line addition each
- Modify: `crates/banager-core/src/adapters/cargo.rs:39-79` (`Crates2Root`, `parse_crates2_entries`, `parse_crates2`), `:129-176` (`detect`: the home, the `--version` environment, the id), `:200-206` (`inventory`), `mod tests`  [untouched by A, F, B, C]
- Modify: `crates/banager-core/tests/unknown_scan_test.rs` — the comment of `test_rule_1_claims_everything_that_resolves_to_an_instances_launcher` (`:405-410`) and one companion test after it  [F's file]
- Modify: `crates/banager-core/src/scan/mod.rs` — two comments: `owned_roots`'s `_` arm, which reads `rule 1 places the proxies and, from step E, rule 2` (`:295`), and the rule 2 paragraph of `struct Known`'s doc comment, hard-wrapped as `gives Homebrew, in a directory every scan reads. cargo fills \`path\`` / `from step E, the standalone adapters from step B; for those, rules` (`:330-331`)  [F's file]
- Modify: `docs/what-we-run.md` — the `## Cargo` section's "Read-only reads" paragraph and its version-read sentence  [A's file]
- Test: `path_env.rs`'s and `cargo.rs`'s `mod tests`; `tests/unknown_scan_test.rs`.

**Interfaces:**
- Consumes: `parse_install_key` (`cargo.rs:29-37`), `HostEnv` (`runner/path_env.rs:5-24`), `crate::model::instance_id` (`model.rs:25-34`), `InstalledArtifact.path` (`pub path: Option<PathBuf>`), F's rule 2 (`scan/mod.rs`, `Known::index`: `canonicalize(entry)` starts with `canonicalize(artifact.path)`); rustup 1.29.1's `should_auto_install` (`config.rs:435-441`) and `proxy_mode::main` (`proxy_mode.rs:14-59`); `home-0.5.12`'s `cargo_home_with_cwd_env` (`env.rs:67-79`).
- Produces (verbatim): `HostEnv.rustup_home: Option<PathBuf>` and `HostEnv.zdotdir: Option<PathBuf>`, written by `HostEnv::discover` (readers: `StandaloneAdapter::detect`, Task 4, which seats them; `rustup.rs`, Task 5 — **declared deferral**: in this commit only `discover` writes them and the tests read them); `pub(crate) fn tool_home(setting: Option<&Path>, home: &Path, default_dir: &str) -> Option<PathBuf>` (readers: `cargo::cargo_home_of` here; `StandaloneAdapter::detect`, Task 4); `pub(crate) const RUSTUP_AUTO_INSTALL_OFF: (&str, &str)` (readers: `CargoAdapter::detect` here; `RUSTUP.version.env`, Task 6); `pub(crate) fn cargo_home_of(env: &HostEnv) -> Option<PathBuf>` (readers: `CargoAdapter::detect` here; `StandaloneAdapter::detect`, Task 4); `pub(crate) fn instance_id_for(cargo_home: &Path) -> String` (readers: `CargoAdapter::detect` here; `rustup::extra_locks`, Task 5); `pub(crate) fn parse_crates2_bins(json: &str) -> Result<Vec<(String, Vec<String>)>, AdapterError>` (readers: `parse_crates2` here; `rustup::bin_programs_rustup_removes`, Task 5); `InstalledArtifact.path = Some(<cargo_home>/bin/<binary>)` on every cargo artifact (reader: F's rule 2 — the first cargo input it gets).

Which binary: the one named after the crate when the record lists one, else the first listed (Ruling 7). `cargo install --list` and `.crates2.json` agree on the names; the record's `bins` array is what cargo itself wrote (`cargo/1.98.1/README.md`). Which home: the `home` crate's rule (Ruling 6) — empty is default, relative is unsupported. Which environment: on a rustup Mac `cargo` is the rustup binary, so its `--version` gets rustup's switch (Ruling 6, 20).

- [ ] **Step 1: Write the failing tests**

In `crates/banager-core/src/runner/path_env.rs`, inside `mod tests`, append after `test_resolve_exe_returns_none_for_a_missing_binary`:

```rust

    #[test]
    fn test_tool_home_follows_the_home_crates_rule() {
        // `home` 0.5.12 (the crate rustup 1.29.1 and cargo read their homes
        // through), `cargo_home_with_cwd_env` / `rustup_home_with_cwd_env`
        // (crates/home/src/env.rs:67-79, :101-113): an unset or *empty*
        // variable means `<home>/<default>`; an absolute one is taken as
        // is; a relative one is joined onto the tool's own current
        // directory, which Banager neither knows nor shares -- so for
        // Banager it is unsupported, and nothing pretends to know where
        // the tool will look.
        let home = Path::new("/Users/someone");
        assert_eq!(
            tool_home(None, home, ".cargo"),
            Some(PathBuf::from("/Users/someone/.cargo"))
        );
        assert_eq!(
            tool_home(Some(Path::new("")), home, ".cargo"),
            Some(PathBuf::from("/Users/someone/.cargo")),
            "an empty CARGO_HOME is filtered out before the crate looks at it"
        );
        assert_eq!(
            tool_home(Some(Path::new("/Volumes/Data/cargo")), home, ".cargo"),
            Some(PathBuf::from("/Volumes/Data/cargo"))
        );
        assert_eq!(
            tool_home(Some(Path::new("cargo-home")), home, ".rustup"),
            None,
            "relative: the crate joins it onto the tool's cwd, not Banager's"
        );
        assert_eq!(
            tool_home(None, home, ".rustup"),
            Some(PathBuf::from("/Users/someone/.rustup"))
        );
    }

    #[test]
    fn test_discover_reads_rustup_home_and_zdotdir_like_cargo_home() {
        // Both are read the way `cargo_home` is: raw, from the process
        // environment `fix_path_env` left (only PATH is restored from the
        // login shell), `None` when unset. Interpreting them -- empty means
        // default, relative means unsupported -- is `tool_home`'s and the
        // rustup recipe's job, not this reader's. The test does not set
        // the variables (a test must not change the process environment
        // other tests read); it pins the shape against the variables as
        // they are.
        let env = HostEnv::discover();
        assert_eq!(
            env.rustup_home,
            std::env::var_os("RUSTUP_HOME").map(PathBuf::from)
        );
        assert_eq!(env.zdotdir, std::env::var_os("ZDOTDIR").map(PathBuf::from));
    }
```

and add `use std::path::Path;` to that test module's `use` lines (after `use super::*;`; the file itself imports only `PathBuf`).

In `crates/banager-core/src/adapters/cargo.rs`, inside `mod tests`, replace `test_parse_crates2_from_the_recorded_fixture` with:

```rust
    #[test]
    fn test_parse_crates2_from_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/cargo/1.98.1/crates2.json")
            .expect("read cargo crates2.json fixture");
        let artifacts = parse_crates2(
            &json,
            "cargo:/Users/someone/.cargo",
            Path::new("/Users/someone/.cargo"),
        )
        .expect("parse crates2.json");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].key.kind, ArtifactKind::Binary);
        assert_eq!(artifacts[0].key.name, "hexyl");
        assert_eq!(artifacts[0].version, "0.17.0");
        // The program the crate installed, for the Unknown page's rule 2:
        // `hexyl`'s one binary is `hexyl` (`"bins":["hexyl"]` in the
        // recording), under the Cargo home's `bin/`.
        assert_eq!(
            artifacts[0].path,
            Some(PathBuf::from("/Users/someone/.cargo/bin/hexyl"))
        );
    }

    #[test]
    fn test_parse_crates2_bins_reads_the_recorded_fixture() {
        let json = std::fs::read_to_string("../../adapters/fixtures/cargo/1.98.1/crates2.json")
            .expect("read cargo crates2.json fixture");
        assert_eq!(
            parse_crates2_bins(&json).expect("parse"),
            vec![("hexyl".to_string(), vec!["hexyl".to_string()])]
        );
    }

    #[test]
    fn test_parse_crates2_bins_names_the_binaries_not_the_crate() {
        // Edge cases the recorded fixture (one crate, one binary named
        // after it) cannot show: `ripgrep` installs `rg`; a crate can
        // install several programs; a record with no `bins` key is a
        // crate that installed none. Sorted by crate name.
        let json = r#"{"installs":{
            "ripgrep 15.1.0 (registry+https://github.com/rust-lang/crates.io-index)":{"version_req":null,"bins":["rg"],"features":[],"all_features":false,"no_default_features":false,"profile":"release","target":"aarch64-apple-darwin","rustc":"rustc 1.98.1\n"},
            "cargo-binstall 1.16.0 (registry+https://github.com/rust-lang/crates.io-index)":{"version_req":null,"bins":["cargo-binstall","detect-targets"],"features":[],"all_features":false,"no_default_features":false,"profile":"release","target":"aarch64-apple-darwin","rustc":"rustc 1.98.1\n"},
            "libonly 0.1.0 (registry+https://github.com/rust-lang/crates.io-index)":{"version_req":null,"features":[],"all_features":false,"no_default_features":false,"profile":"release","target":"aarch64-apple-darwin","rustc":"rustc 1.98.1\n"}
        }}"#;
        assert_eq!(
            parse_crates2_bins(json).expect("parse"),
            vec![
                (
                    "cargo-binstall".to_string(),
                    vec!["cargo-binstall".to_string(), "detect-targets".to_string()]
                ),
                ("libonly".to_string(), Vec::new()),
                ("ripgrep".to_string(), vec!["rg".to_string()]),
            ]
        );
        let artifacts = parse_crates2(json, "cargo:/Users/someone/.cargo", Path::new("/Users/someone/.cargo"))
            .expect("parse");
        let path_of = |name: &str| {
            artifacts
                .iter()
                .find(|a| a.key.name == name)
                .expect(name)
                .path
                .clone()
        };
        // The binary named after the crate when there is one, else the
        // first listed, else none (ruling 7). `detect-targets`, the
        // second binary of `cargo-binstall`, carries no artifact path and
        // stays on the Unknown page until `path` can hold several.
        assert_eq!(path_of("ripgrep"), Some(PathBuf::from("/Users/someone/.cargo/bin/rg")));
        assert_eq!(
            path_of("cargo-binstall"),
            Some(PathBuf::from("/Users/someone/.cargo/bin/cargo-binstall"))
        );
        assert_eq!(path_of("libonly"), None);
    }

    #[test]
    fn test_parse_crates2_bins_is_a_parse_error_for_anything_that_is_not_the_record() {
        assert!(matches!(parse_crates2_bins("not json"), Err(AdapterError::Parse(_))));
        assert!(matches!(
            parse_crates2_bins(r#"{"installs":{"hexyl 0.17.0 (registry+x)":{"bins":"hexyl"}}}"#),
            Err(AdapterError::Parse(_))
        ));
    }

    #[test]
    fn test_cargo_home_of_is_the_home_crates_rule_over_the_host_environment() {
        // `tool_home` (runner/path_env.rs) over `HostEnv.cargo_home`:
        // unset and empty are `<home>/.cargo`, absolute is itself, relative
        // is unsupported -- the same answer rustup and cargo compute, so
        // the lock name built from it names the directory they use.
        let env = HostEnv {
            path_dirs: Vec::new(),
            home: PathBuf::from("/Users/someone"),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        assert_eq!(cargo_home_of(&env), Some(PathBuf::from("/Users/someone/.cargo")));
        let env = HostEnv {
            cargo_home: Some(PathBuf::from("")),
            ..env
        };
        assert_eq!(cargo_home_of(&env), Some(PathBuf::from("/Users/someone/.cargo")));
        let env = HostEnv {
            cargo_home: Some(PathBuf::from("/Volumes/Data/cargo")),
            ..env
        };
        assert_eq!(cargo_home_of(&env), Some(PathBuf::from("/Volumes/Data/cargo")));
        let env = HostEnv {
            cargo_home: Some(PathBuf::from("cargo")),
            ..env
        };
        assert_eq!(cargo_home_of(&env), None);
    }

    #[test]
    fn test_instance_id_for_is_the_persisted_cargo_shape() {
        // `cargo:<cargo_home>` (`model::instance_id`'s
        // `test_instance_id_reproduces_every_shape_already_persisted`).
        // The rustup recipe builds its cargo lock from this same function
        // (adapters/standalone/rustup.rs), so the id and the lock cannot
        // drift into two spellings that `acquire_resource_lock` would
        // treat as unrelated.
        assert_eq!(
            instance_id_for(Path::new("/Users/someone/.cargo")),
            "cargo:/Users/someone/.cargo"
        );
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        assert_eq!(adapter.meta.id, "cargo", "the literal instance_id_for spells");
    }

    /// `MockRunner` keys and records argv only; this records the whole
    /// `CommandSpec`, so a test can see the environment `detect` gave
    /// `cargo --version`.
    struct EnvRecordingRunner {
        specs: Mutex<Vec<CommandSpec>>,
    }

    #[async_trait::async_trait]
    impl CommandRunner for EnvRecordingRunner {
        async fn run(
            &self,
            spec: CommandSpec,
            _on_line: Option<crate::runner::LineCallback>,
            _cancel: CancellationToken,
        ) -> Result<CommandOutput, crate::runner::RunnerError> {
            self.specs.lock().unwrap().push(spec);
            Ok(CommandOutput {
                exit_code: Some(0),
                stdout: "cargo 1.98.1 (797e8a9bc 2026-08-05)\n".to_string(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
            })
        }
    }

    #[tokio::test]
    async fn test_detect_reads_cargos_version_with_rustups_auto_install_off() {
        // On a rustup Mac `cargo` is the rustup binary in proxy mode
        // (rustup 1.29.1 src/cli/proxy_mode.rs:14-59): before it runs the
        // real cargo it resolves the active toolchain, and with none active
        // it *installs* one unless `RUSTUP_AUTO_INSTALL=0`
        // (`should_auto_install`, config.rs:435-441). A refresh is
        // read-only, so the switch goes on this read; a cargo that is not
        // rustup's ignores the variable.
        let home = temp_cargo_home("auto-install-off");
        let bin = home.join("bin");
        std::fs::create_dir_all(&bin).expect("bin");
        std::fs::write(bin.join("cargo"), b"#!/bin/sh\n").expect("cargo");
        let runner = Arc::new(EnvRecordingRunner {
            specs: Mutex::new(Vec::new()),
        });
        let adapter = CargoAdapter::new(runner.clone(), Arc::new(MockHttpClient::new()));
        let env = HostEnv {
            path_dirs: vec![bin.clone()],
            home: home.parent().unwrap().to_path_buf(),
            euid: 501,
            cargo_home: Some(home.clone()),
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        let instances = adapter.detect(&env).await;
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].version, Some("1.98.1".to_string()));
        let specs = runner.specs.lock().unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].program, bin.join("cargo"));
        assert_eq!(specs[0].args, vec!["--version".to_string()]);
        assert_eq!(
            specs[0].env,
            vec![("RUSTUP_AUTO_INSTALL".to_string(), "0".to_string())]
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_detect_lists_no_cargo_for_a_relative_cargo_home() {
        // cargo itself would join a relative CARGO_HOME onto *its* current
        // directory (`home` 0.5.12); Banager's is not that, so an instance
        // whose prefix were that relative path would read `.crates2.json`
        // from the wrong place and lock a name nothing else uses. No
        // instance is the honest answer (ruling 6).
        let home = temp_cargo_home("relative");
        let bin = home.join("bin");
        std::fs::create_dir_all(&bin).expect("bin");
        std::fs::write(bin.join("cargo"), b"#!/bin/sh\n").expect("cargo");
        let runner = Arc::new(MockRunner::new());
        let adapter = CargoAdapter::new(runner.clone(), Arc::new(MockHttpClient::new()));
        let env = HostEnv {
            path_dirs: vec![bin],
            home: home.parent().unwrap().to_path_buf(),
            euid: 501,
            cargo_home: Some(PathBuf::from("cargo")),
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        assert!(adapter.detect(&env).await.is_empty());
        assert!(runner.calls().is_empty(), "nothing is run for a home Banager cannot name");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn test_inventory_gives_every_cargo_artifact_the_path_of_its_program() {
        let json = std::fs::read_to_string("../../adapters/fixtures/cargo/1.98.1/crates2.json")
            .expect("read cargo crates2.json fixture");
        let home = temp_cargo_home("paths");
        std::fs::create_dir_all(&home).expect("create cargo home");
        std::fs::write(home.join(".crates2.json"), &json).expect("write crates2.json");
        let adapter =
            CargoAdapter::new(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let inst = test_instance(home.clone());
        let artifacts = adapter.inventory(&inst).await.expect("inventory");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].path, Some(home.join("bin").join("hexyl")));
        let _ = std::fs::remove_dir_all(&home);
    }
```

and add to the test module's `use` lines (it has `use super::*;` at `:441` and a second block at `:536-538` importing `VecSink`, `HttpResponse, MockHttpClient`, `CommandOutput, MockRunner`): `use std::path::Path;`, `use std::sync::Mutex;` and `use crate::runner::{CommandRunner, CommandSpec};` — skipping any the module already has (`Path` is not imported by the file, which uses only `PathBuf`; `CommandSpec` and `CommandRunner` reach the module through `use super::*` if the file imports them by name — it does, `use crate::runner::{…, CommandRunner, CommandSpec, …}`, so leave those two out and keep `Path` and `Mutex`).

In `crates/banager-core/tests/unknown_scan_test.rs`, replace the comment lines (`:405-410`) of `test_rule_1_claims_everything_that_resolves_to_an_instances_launcher`

```rust
    // `~/.cargo/bin`: rustup itself, thirteen proxies that are relative
    // symlinks to it, and one crate installed with `cargo install`. The
    // cargo instance's own executable is one of the proxies, so
    // everything that resolves to `rustup` is cargo's. `hexyl` is not --
    // until step E fills `InstalledArtifact.path` for cargo binaries it
    // is listed here, honestly (spec §8.3; Task 10's delivery note).
```

with

```rust
    // `~/.cargo/bin`: rustup itself, thirteen proxies that are relative
    // symlinks to it, and one crate installed with `cargo install`. The
    // cargo instance's own executable is one of the proxies, so
    // everything that resolves to `rustup` is cargo's. `hexyl` is not
    // by this rule: with no artifact carrying its path it is listed,
    // honestly; the test after this one gives cargo's inventory its say.
```

and insert, after that test's closing `}` and before `#[test]\nfn test_rule_2_claims_a_shim_that_resolves_under_an_artifacts_path()`:

```rust

#[test]
fn test_rule_2_claims_a_cargo_installed_program_through_its_artifacts_path() {
    // The cargo adapter's inventory fills `InstalledArtifact.path` with
    // `<cargo_home>/bin/<binary>` for every crate (`parse_crates2` in
    // adapters/cargo.rs, phase 4 step E), so a `cargo install`ed program
    // is claimed by rule 2 -- the same rule uv's shims use -- and no
    // longer listed here.
    let home = Home::new("rule-2-cargo");
    let bin = home.dir(".cargo/bin");
    exe(&bin, "rustup", b"x");
    link(&bin, "cargo", Path::new("rustup"));
    let hexyl = exe(&bin, "hexyl", b"x");
    let cargo = ManagerInstance {
        exe_path: bin.join("cargo"),
        prefix: home.path().join(".cargo"),
        ..manager_instance(
            "cargo",
            &format!("cargo:{}", home.path().join(".cargo").display()),
        )
    };
    let hexyl_artifact = InstalledArtifact {
        key: ArtifactKey {
            instance_id: cargo.id.clone(),
            kind: ArtifactKind::Binary,
            name: "hexyl".to_string(),
        },
        ..artifact(&cargo.id, "hexyl", &hexyl)
    };

    let scan = scan_dirs(
        &[bin],
        &home.env(vec![]),
        &[cargo],
        &[hexyl_artifact],
        ScanBudget::default(),
    );

    assert!(scan.entries.is_empty(), "{:?}", scan.entries);
    assert_eq!(scan.attributed, 3);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib runner::path_env` and `cargo test -p banager-core --lib adapters::cargo`
Expected: FAIL to compile — `cannot find function \`tool_home\``; `struct \`HostEnv\` has no field named \`rustup_home\`` (and `zdotdir`); `this function takes 2 arguments but 3 arguments were supplied` at the `parse_crates2` calls; `cannot find function \`parse_crates2_bins\``, `\`cargo_home_of\``, `\`instance_id_for\``. (`cargo test -p banager-core --test unknown_scan_test test_rule_2_claims_a_cargo_installed_program` compiles and PASSES already: the scan reads whatever `path` an artifact carries — it is here because the inventory that will carry it is.)

- [ ] **Step 3: Write the fields, the rule, the functions, and fill `path`**

In `crates/banager-core/src/runner/path_env.rs`, change the import line `use std::path::PathBuf;` to `use std::path::{Path, PathBuf};`. In `pub struct HostEnv`, after the `cargo_home` field (its doc ends `cannot be tested.`), add:

```rust
    /// `RUSTUP_HOME` when the host environment sets it, raw; `None` means
    /// "use the default", `home/.rustup`. Same reasoning as `cargo_home`.
    /// Read by `StandaloneAdapter::detect` (adapters/standalone/mod.rs),
    /// which seats it for the rustup recipe: its uninstall is offered only
    /// when this resolves to the default (`rustup::standard_roots`), and
    /// its toolchain names are read under it. Interpreted by `tool_home`,
    /// never taken as a path directly.
    pub rustup_home: Option<PathBuf>,
    /// `ZDOTDIR` when the host environment sets it, raw; `None` when
    /// unset. rustup's own uninstall (1.29.1 `shell.rs:207-225`) edits
    /// `$ZDOTDIR/.zshenv` and `$ZDOTDIR/.zprofile` as well as the ones
    /// under `HOME`, so the rustup recipe's preview follows the same
    /// visits (`rustup::rustup_rc_visits`). Read by
    /// `StandaloneAdapter::detect`, which seats it.
    pub zdotdir: Option<PathBuf>,
```

In `discover`, after the line `let cargo_home = std::env::var_os("CARGO_HOME").map(PathBuf::from);` add:

```rust
        let rustup_home = std::env::var_os("RUSTUP_HOME").map(PathBuf::from);
        let zdotdir = std::env::var_os("ZDOTDIR").map(PathBuf::from);
```

and in the `HostEnv { … }` it returns, add `rustup_home,` and `zdotdir,` after `cargo_home,`. After `impl HostEnv { … }` (before `pub fn resolve_exe`), insert:

```rust

/// Where a tool that reads its home through the `home` crate will look --
/// `home` 0.5.12, the version rustup 1.29.1 and cargo pin,
/// `cargo_home_with_cwd_env` and `rustup_home_with_cwd_env`
/// (crates/home/src/env.rs:67-79, :101-113): `setting` when it is set,
/// not empty and absolute; `<home>/<default_dir>` when it is unset or
/// empty (the crate filters an empty value out before it looks at it);
/// `None` when it is relative. The crate joins a relative value onto the
/// *tool's* current directory, which Banager neither knows nor shares --
/// a Finder-launched app's is `/` -- so nothing Banager could read or
/// lock would be the directory the tool uses, and "unsupported" is the
/// only honest answer. Readers: `cargo::cargo_home_of` (cargo's instance,
/// and the rustup recipe's `$CARGO_HOME` paths and cargo lock) and
/// `StandaloneAdapter::detect`'s `rustup_home` (the rustup recipe's
/// uninstall gate and toolchain listing).
pub(crate) fn tool_home(setting: Option<&Path>, home: &Path, default_dir: &str) -> Option<PathBuf> {
    match setting {
        Some(p) if p.as_os_str().is_empty() => Some(home.join(default_dir)),
        Some(p) if p.is_absolute() => Some(p.to_path_buf()),
        Some(_) => None,
        None => Some(home.join(default_dir)),
    }
}
```

**Every other `HostEnv {` literal in the tree gains two lines**, `rustup_home: None,` and `zdotdir: None,`, directly after its `cargo_home: …,` line. Run `grep -rn --include='*.rs' "HostEnv {" crates src-tauri | grep -v "pub struct HostEnv"` and edit each hit; at `ea30cfb` they are: `crates/banager-core/src/runner/path_env.rs` (the `discover` literal, done above, and the test module's four); `crates/banager-core/src/adapters/brew/mod.rs` (4), `adapters/cargo.rs` (4, in `mod tests`), `adapters/npm.rs` (4), `adapters/ollama/mod.rs` (6), `adapters/pip.rs` (2), `adapters/pipx.rs` (2), `adapters/standalone/mod.rs` (2: `testing::TempHome::env` and the test that builds an env with `cargo_home: Some(…)`, if any — B's has none; Task 4 adds one), `scan/mod.rs` (2), `session/scan.rs` (1, `HostEnv::discover` is not a literal — check the hit is a literal before editing), `session/test_support.rs` (4: `non_root_env`, `root_env` and two more), `crates/banager-core/tests/unknown_scan_test.rs` (2, F's `Home::env`), `src-tauri/src/ipc.rs` (1). None of them uses struct-update syntax. The build stops with `missing field \`rustup_home\`` at any literal the grep missed, so the compiler is the checklist.

In `crates/banager-core/src/adapters/cargo.rs`, change the import line `use std::path::PathBuf;` to `use std::path::{Path, PathBuf};`. Replace `Crates2Root` through `parse_crates2` (`:39-79`) with:

```rust
#[derive(Debug, Deserialize)]
struct Crates2Root {
    #[serde(default)]
    installs: HashMap<String, serde_json::Value>,
}

fn parse_crates2_entries(json: &str) -> Result<Vec<(String, String, String)>, AdapterError> {
    let root: Crates2Root =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    let mut entries: Vec<(String, String, String)> = root
        .installs
        .keys()
        .filter_map(|k| parse_install_key(k))
        .collect();
    entries.sort();
    Ok(entries)
}

/// The one field of an `installs` *value* Banager reads: the programs the
/// crate put in `<cargo_home>/bin`.
#[derive(Debug, Deserialize)]
struct Crates2Install {
    #[serde(default)]
    bins: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Crates2Bins {
    #[serde(default)]
    installs: HashMap<String, Crates2Install>,
}

/// `(crate name, the programs it installed)` for every crate in
/// `.crates2.json`, sorted by name: the `bins` array in each `installs`
/// value. `parse_crates2_entries` reads only the keys, which carry the
/// name, version and source and nothing about the binaries -- so it
/// cannot say which *file* a crate left in `bin/`: `ripgrep` installs
/// `rg`, and a sentence that named the crate would name a program that
/// is not there (phase 4 spec §6.4, §十三 #4). Readers: `parse_crates2`
/// (the artifact's `path`) and the rustup recipe's uninstall warnings
/// (`adapters/standalone/rustup.rs`, which names what `rustup self
/// uninstall` deletes).
pub(crate) fn parse_crates2_bins(json: &str) -> Result<Vec<(String, Vec<String>)>, AdapterError> {
    let root: Crates2Bins =
        serde_json::from_str(json).map_err(|e| AdapterError::Parse(e.to_string()))?;
    let mut out: Vec<(String, Vec<String>)> = root
        .installs
        .into_iter()
        .filter_map(|(key, install)| parse_install_key(&key).map(|(name, _, _)| (name, install.bins)))
        .collect();
    out.sort();
    Ok(out)
}

/// One artifact per crate. `path` is the program the crate installed
/// under `<cargo_home>/bin`: the binary named after the crate when the
/// record lists one, else the first it lists, else `None` for a crate
/// that installed no program. `InstalledArtifact.path` holds one path,
/// so the other binaries of a multi-binary crate (`cargo-binstall`'s
/// `detect-targets`) are not attributed and stay on the Unknown page
/// until it can hold several (backlog). The reader is the Unknown page's
/// rule 2 (`scan/mod.rs`, `Known::index`): a `~/.cargo/bin/hexyl` that
/// canonicalises to this path is cargo's.
fn parse_crates2(
    json: &str,
    instance_id: &str,
    cargo_home: &Path,
) -> Result<Vec<InstalledArtifact>, AdapterError> {
    let entries = parse_crates2_entries(json)?;
    let bins = parse_crates2_bins(json)?;
    let bin_dir = cargo_home.join("bin");
    Ok(entries
        .into_iter()
        .map(|(name, version, _source_kind)| {
            let path = bins
                .iter()
                .find(|(crate_name, _)| *crate_name == name)
                .and_then(|(_, bins)| bins.iter().find(|b| **b == name).or_else(|| bins.first()))
                .map(|bin| bin_dir.join(bin));
            InstalledArtifact {
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
                path,
                auto_updates: false,
                uninstall_blocked: None,
            }
        })
        .collect())
}

/// rustup's switch against installing a toolchain as a side effect
/// (`RUSTUP_AUTO_INSTALL=0`; rustup 1.29.1 `should_auto_install`,
/// config.rs:435-441). On a rustup Mac `cargo` *is* the rustup binary,
/// running in proxy mode (src/cli/proxy_mode.rs:14-59): before it runs
/// the real cargo it resolves the active toolchain and, with none active
/// and this switch off, installs one -- a download and a write, which a
/// refresh must never cause. So every version read of that binary
/// carries it: `CargoAdapter::detect`'s `cargo --version` here, and the
/// rustup recipe's own `--version` (`recipes::RUSTUP`, its
/// `version.env`). A cargo that is not rustup's ignores the variable.
pub(crate) const RUSTUP_AUTO_INSTALL_OFF: (&str, &str) = ("RUSTUP_AUTO_INSTALL", "0");

/// Where cargo's home is, by the `home` crate's rule (`path_env::tool_home`
/// over `CARGO_HOME`): the one rule for the directory, shared with the
/// rustup recipe's `$CARGO_HOME/…` paths (`adapters/standalone/route.rs`,
/// through `StandaloneAdapter::detect`), so the two adapters can never
/// disagree about it, and so the lock name `instance_id_for` builds is
/// built from the same path `detect` names cargo's instance by. `None`
/// for a relative `CARGO_HOME`, which cargo resolves against a current
/// directory Banager does not share: `detect` then lists no cargo
/// instance rather than one whose prefix is somewhere cargo never looks.
pub(crate) fn cargo_home_of(env: &HostEnv) -> Option<PathBuf> {
    crate::runner::path_env::tool_home(env.cargo_home.as_deref(), &env.home, ".cargo")
}

/// The id of the cargo instance whose home is `cargo_home`:
/// `cargo:<cargo_home>`, the persisted shape (`model::instance_id`). The
/// single producer of that string (phase 4 spec §2.4): `detect` names its
/// instance with it, and the rustup recipe's `extra_locks`
/// (`adapters/standalone/rustup.rs`) builds the `ResourceLock` its `self
/// update` and `self uninstall` plans hold with it. `acquire_resource_lock`
/// compares lock names byte for byte and reports nothing for two that
/// merely look alike, so there is one function and not two spellings.
pub(crate) fn instance_id_for(cargo_home: &Path) -> String {
    crate::model::instance_id("cargo", Some(&cargo_home.display().to_string()))
}
```

In `detect`, replace

```rust
        let cargo_home = env
            .cargo_home
            .clone()
            .unwrap_or_else(|| env.home.join(".cargo"));
```

with

```rust
        // The `home` crate's rule; `None` is a relative CARGO_HOME, which
        // names a directory relative to cargo's own cwd, not Banager's:
        // no instance, rather than one that reads the wrong place.
        let Some(cargo_home) = cargo_home_of(env) else {
            return Vec::new();
        };
```

and in the `CommandSpec { … }` of its `cargo --version` run, replace `env: Vec::new(),` with:

```rust
                    // The cargo proxy is the rustup binary: never let a
                    // version read install a toolchain (RUSTUP_AUTO_INSTALL_OFF).
                    env: vec![(
                        RUSTUP_AUTO_INSTALL_OFF.0.to_string(),
                        RUSTUP_AUTO_INSTALL_OFF.1.to_string(),
                    )],
```

and replace

```rust
            id: crate::model::instance_id(&self.meta.id, Some(&cargo_home.display().to_string())),
```

with

```rust
            // Through `instance_id_for`, which the rustup recipe's cargo
            // lock is built with too: one spelling of this id.
            id: instance_id_for(&cargo_home),
```

In `inventory`, replace `parse_crates2(&json, &inst.id)` with `parse_crates2(&json, &inst.id, &inst.prefix)` (a cargo instance's `prefix` is its Cargo home, set in `detect`).

In `crates/banager-core/src/scan/mod.rs`, in `owned_roots`'s `_` arm comment, change `rule 1 places the proxies and, from step E, rule 2` to `rule 1 places the proxies and rule 2` (the sentence continues `places \`cargo install\`ed binaries`); in `struct Known`'s doc comment (the rules list), the rule 2 paragraph is hard-wrapped across two lines that read (at `ea30cfb`, `:330-331`) `gives Homebrew, in a directory every scan reads. cargo fills \`path\`` and `from step E, the standalone adapters from step B; for those, rules` — change the sentence `cargo fills \`path\` from step E, the standalone adapters from step B; for those, rules 1 and 2 compare the same file and rule 2 decides nothing new.` (which spans three lines) to `cargo fills \`path\` with the program each crate installed, which only this rule places (\`hexyl\` resolves to no instance's \`exe_path\`); the standalone adapters fill it from step B, and for those rules 1 and 2 compare the same file and rule 2 decides nothing new.` — "for those" must not take cargo in, since for cargo rule 2 is the only rule that places the program — and re-wrap the paragraph at the file's width. Match by those words, not by line number; change only the "from step E" sentence wherever it stands.

In `docs/what-we-run.md`, under `## Cargo`, in the paragraph beginning `**Read-only reads.** \`inventory\` runs no command: it reads`, after the sentence ending `(a missing file means nothing is installed).` insert the sentence:

```markdown
For each crate it also records the program the crate installed,
`<CARGO_HOME>/bin/<binary>` (the binary named after the crate when there is
one, else the first the record lists), which the Unknown page uses to place
that program under Cargo rather than list it.
```

and in the same section's description of `cargo --version` (the detect row or sentence — match by the argv `cargo --version`), add after it: `, with \`RUSTUP_AUTO_INSTALL=0\` in its environment: on a Mac with rustup, \`cargo\` is rustup's own binary standing in for cargo, and without that switch a version read with no Rust toolchain active would install one. A cargo that is not rustup's ignores it. \`CARGO_HOME\` is read as cargo itself reads it: an empty value means the default \`~/.cargo\`; a relative value names a folder relative to cargo's own working directory, which Banager cannot know, so Banager then lists no Cargo source rather than guess.`

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib runner::path_env`, `cargo test -p banager-core --lib adapters::cargo` and `cargo test -p banager-core --test unknown_scan_test`
Expected: PASS — the 2 new `path_env` tests, the 9 new/changed cargo tests and every earlier one; both rule tests. `cargo build --workspace --all-targets` compiles every `HostEnv {` literal (the two new fields on each).

- [ ] **Step 5: Run the gates**

Run `cargo fmt --all`, then all five from Global Constraints. Expected: all clean (the Tauri crate's one literal in `src-tauri/src/ipc.rs` is covered by `cargo clippy --workspace --all-targets`).

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/runner/path_env.rs crates/banager-core/src/adapters/cargo.rs crates/banager-core/src/adapters/brew/mod.rs crates/banager-core/src/adapters/npm.rs crates/banager-core/src/adapters/ollama/mod.rs crates/banager-core/src/adapters/pip.rs crates/banager-core/src/adapters/pipx.rs crates/banager-core/src/adapters/standalone/mod.rs crates/banager-core/src/scan/mod.rs crates/banager-core/src/session/scan.rs crates/banager-core/src/session/test_support.rs crates/banager-core/tests/unknown_scan_test.rs src-tauri/src/ipc.rs docs/what-we-run.md
git commit -m "$(cat <<'EOF'
Read the Rust host the way rustup does, and record which program each cargo crate installed

The host environment now carries RUSTUP_HOME and ZDOTDIR beside
CARGO_HOME, and all three are interpreted by the rule the home crate
gives rustup and cargo: empty means the default, relative means a place
Banager cannot name. cargo's version read runs with rustup's auto-install
switch off, because on a rustup Mac cargo is the rustup binary and a
read with no toolchain active would install one. .crates2.json keeps a
crate's binaries in the value of each record, which the inventory used to
throw away; now every cargo artifact carries the path of the program it
installed, so the Unknown page can place a cargo-installed program under
Cargo instead of listing it. The Cargo home rule and the instance id's
spelling move into two functions, so the rustup recipe can hold cargo's
lock and expand $CARGO_HOME paths through the same code cargo's own
detect uses.

Co-Authored-By: <the executing session's attribution line>
EOF
)"
```

---

### Task 2: Six `Warning` variants, their mirror, keys and copy

**Files:**
- Modify: `crates/banager-core/src/model.rs` — `enum Warning` (`pub enum Warning` at `:314` at `ea30cfb`; C adds three variants) and `test_warning_wire_shapes_match_the_hand_written_ts_mirror`
- Modify: `src/lib/types.ts` — `export type Warning` (C adds three arms)  [A's/C's file]
- Modify: `src/lib/types.test.ts` — `it("spells Warning's bare-string variants as bare strings and WouldBreak/Message as externally tagged", …)`  [A's/C's file]
- Modify: `src/lib/warnings.ts` — `warningKey`, `warningArgs`  [A's/C's file]
- Modify: `src/lib/warnings.test.ts` — the `describe`s for `warningKey` and `warningArgs`  [A's/C's file]
- Modify: `src/i18n/en.json`, `src/i18n/zh-CN.json` — the `warnings` object
- Test: the Rust shape test, `types.test.ts`, `warnings.test.ts`, `completeness.test.ts`.

**Interfaces:**
- Consumes: `Warning`'s wire rule (unit variants are bare strings, data variants externally tagged; the doc comment above `export type Warning`); `warningKey`'s two `never` defaults (A); `warningTexts` (`warnings.ts`), read by `UninstallDialog.tsx` (`planWarnings`) and `UpdatesPage.tsx` (`itemWarnings`).
- Produces (verbatim): `Warning::{RemovesToolchains { path: String, names: Vec<String> }, DeletesCargoHome { path: String }, RemovesCargoInstalled { names: Vec<String> }, HomebrewRustupLosesToolchains, EditsShellConfig, LeavesShellConfigLine { path: String, certain: bool }}` (producer: `rustup::uninstall_warnings`, Task 5; readers: `warningKey`/`warningArgs` → the uninstall dialog's list); the TS arms; the keys `warnings.removesToolchains`, `warnings.removesToolchainsUnlisted`, `warnings.deletesCargoHome`, `warnings.removesCargoInstalled` (`_one`/`_other`), `warnings.homebrewRustupLosesToolchains`, `warnings.editsShellConfig`, `warnings.leavesShellConfigLine`, `warnings.leavesShellConfigLineMaybe`.

Copy follows rulings 1, 2, 16, 21 and 22: the two folder sentences name the folder by path and say "permanently — not to the Trash" (Astra finding 2; the controller's ruling), the Cargo one lists the settings and the saved login; the toolchain sentence always ends with the other-rustups clause and has a second wording for when no names could be read; the Homebrew line is conditional (ruling 21); the leftover sentence has a certain and a qualified form (ruling 22). `{{path}}` arrives spelled with `~` from Rust (spec §6.5); `{{names}}` is the names joined with `, `; `count` drives the plural.

- [ ] **Step 1: Write the failing tests**

In `crates/banager-core/src/model.rs`, inside `test_warning_wire_shapes_match_the_hand_written_ts_mirror`, before the line `let round_tripped: Warning =`, insert:

```rust
        // Phase 4 step E: what rustup's own uninstall does (adapters/
        // standalone/rustup.rs). Two payload-free, four with a payload;
        // the same two spellings as above.
        assert_eq!(
            serde_json::to_string(&Warning::RemovesToolchains {
                path: "~/.rustup".to_string(),
                names: vec!["stable-aarch64-apple-darwin".to_string()]
            })
            .unwrap(),
            r#"{"RemovesToolchains":{"path":"~/.rustup","names":["stable-aarch64-apple-darwin"]}}"#
        );
        assert_eq!(
            serde_json::to_string(&Warning::DeletesCargoHome {
                path: "~/.cargo".to_string()
            })
            .unwrap(),
            r#"{"DeletesCargoHome":{"path":"~/.cargo"}}"#
        );
        assert_eq!(
            serde_json::to_string(&Warning::RemovesCargoInstalled {
                names: vec!["hexyl".to_string(), "rg".to_string()]
            })
            .unwrap(),
            r#"{"RemovesCargoInstalled":{"names":["hexyl","rg"]}}"#
        );
        assert_eq!(
            serde_json::to_string(&Warning::HomebrewRustupLosesToolchains).unwrap(),
            r#""HomebrewRustupLosesToolchains""#
        );
        assert_eq!(
            serde_json::to_string(&Warning::EditsShellConfig).unwrap(),
            r#""EditsShellConfig""#
        );
        assert_eq!(
            serde_json::to_string(&Warning::LeavesShellConfigLine {
                path: "~/.zshrc".to_string(),
                certain: true
            })
            .unwrap(),
            r#"{"LeavesShellConfigLine":{"path":"~/.zshrc","certain":true}}"#
        );
```

In `src/lib/types.test.ts`, inside the `it("spells Warning's bare-string variants as bare strings and WouldBreak/Message as externally tagged", …)` body, after the line `expect(roundTrip(message)).toEqual({ Message: "boom" });`, insert:

```ts

    // Phase 4 step E: what rustup's own uninstall does. Pinned against
    // `test_warning_wire_shapes_match_the_hand_written_ts_mirror` in
    // crates/banager-core/src/model.rs.
    const removesToolchains: Warning = {
      RemovesToolchains: { path: "~/.rustup", names: ["stable-aarch64-apple-darwin"] },
    };
    const deletesCargoHome: Warning = { DeletesCargoHome: { path: "~/.cargo" } };
    const removesCargoInstalled: Warning = { RemovesCargoInstalled: { names: ["hexyl", "rg"] } };
    const homebrew: Warning = "HomebrewRustupLosesToolchains";
    const editsShellConfig: Warning = "EditsShellConfig";
    const leavesShellConfigLine: Warning = {
      LeavesShellConfigLine: { path: "~/.zshrc", certain: true },
    };
    expect(JSON.stringify(removesToolchains)).toBe(
      '{"RemovesToolchains":{"path":"~/.rustup","names":["stable-aarch64-apple-darwin"]}}',
    );
    expect(JSON.stringify(deletesCargoHome)).toBe('{"DeletesCargoHome":{"path":"~/.cargo"}}');
    expect(JSON.stringify(removesCargoInstalled)).toBe(
      '{"RemovesCargoInstalled":{"names":["hexyl","rg"]}}',
    );
    expect(roundTrip(homebrew)).toBe("HomebrewRustupLosesToolchains");
    expect(roundTrip(editsShellConfig)).toBe("EditsShellConfig");
    expect(JSON.stringify(leavesShellConfigLine)).toBe(
      '{"LeavesShellConfigLine":{"path":"~/.zshrc","certain":true}}',
    );
    expect(roundTrip(leavesShellConfigLine)).toEqual(leavesShellConfigLine);
```

In `src/lib/warnings.test.ts`, inside `describe("warningKey", …)`, after the `it("gives each fixed warning its own key", …)` block's closing `});`, insert:

```ts

  it("gives each of rustup's uninstall warnings its key, and two of them a second key by payload", () => {
    // No toolchain names (the toolchains directory is missing or empty):
    // the sentence must not read "every toolchain ()" -- it drops the
    // parenthesis instead (spec §6.5). A startup-file line rustup will
    // not remove is "will print an error" only when it is one of the
    // sourcing forms rustup itself writes; any other mention is "may".
    expect(
      warningKey({ RemovesToolchains: { path: "~/.rustup", names: ["stable-aarch64-apple-darwin"] } }),
    ).toBe("warnings.removesToolchains");
    expect(warningKey({ RemovesToolchains: { path: "~/.rustup", names: [] } })).toBe(
      "warnings.removesToolchainsUnlisted",
    );
    expect(warningKey({ DeletesCargoHome: { path: "~/.cargo" } })).toBe("warnings.deletesCargoHome");
    expect(warningKey({ RemovesCargoInstalled: { names: ["hexyl"] } })).toBe(
      "warnings.removesCargoInstalled",
    );
    expect(warningKey("HomebrewRustupLosesToolchains")).toBe("warnings.homebrewRustupLosesToolchains");
    expect(warningKey("EditsShellConfig")).toBe("warnings.editsShellConfig");
    expect(warningKey({ LeavesShellConfigLine: { path: "~/.zshrc", certain: true } })).toBe(
      "warnings.leavesShellConfigLine",
    );
    expect(warningKey({ LeavesShellConfigLine: { path: "~/.zshrc", certain: false } })).toBe(
      "warnings.leavesShellConfigLineMaybe",
    );
  });
```

and in the `it("is null for Message and for nothing else", …)` test, append to the `all: Warning[]` array (after C's three entries, before `{ Message: "boom" }` or after it — order is irrelevant to the assertion):

```ts
      { RemovesToolchains: { path: "~/.rustup", names: ["stable-aarch64-apple-darwin"] } },
      { DeletesCargoHome: { path: "~/.cargo" } },
      { RemovesCargoInstalled: { names: ["hexyl"] } },
      "HomebrewRustupLosesToolchains",
      "EditsShellConfig",
      { LeavesShellConfigLine: { path: "~/.zshrc", certain: true } },
```

Inside `describe("warningArgs", …)`, after the `it("interpolates the registry host …", …)` block, insert:

```ts

  it("interpolates rustup's two folders, the toolchain names, the cargo-installed programs with a count, and the startup file", () => {
    expect(
      warningArgs({
        RemovesToolchains: {
          path: "~/.rustup",
          names: ["stable-aarch64-apple-darwin", "nightly-aarch64-apple-darwin"],
        },
      }),
    ).toEqual({ path: "~/.rustup", names: "stable-aarch64-apple-darwin, nightly-aarch64-apple-darwin" });
    // No names: the unlisted key has no names slot, only the path.
    expect(warningArgs({ RemovesToolchains: { path: "~/.rustup", names: [] } })).toEqual({
      path: "~/.rustup",
    });
    expect(warningArgs({ DeletesCargoHome: { path: "~/.cargo" } })).toEqual({ path: "~/.cargo" });
    expect(warningArgs({ RemovesCargoInstalled: { names: ["hexyl"] } })).toEqual({
      count: 1,
      names: "hexyl",
    });
    expect(warningArgs({ RemovesCargoInstalled: { names: ["hexyl", "rg"] } })).toEqual({
      count: 2,
      names: "hexyl, rg",
    });
    expect(warningArgs({ LeavesShellConfigLine: { path: "~/.zshrc", certain: false } })).toEqual({
      path: "~/.zshrc",
    });
    expect(warningArgs("HomebrewRustupLosesToolchains")).toEqual({});
    expect(warningArgs("EditsShellConfig")).toEqual({});
  });
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib model::tests::test_warning_wire_shapes_match_the_hand_written_ts_mirror` and `pnpm typecheck`
Expected: FAIL — Rust: `no variant named \`RemovesToolchains\` found for enum \`Warning\`` (and the other five); TS: `Type '{ RemovesToolchains: { path: string; names: string[]; }; }' is not assignable to type 'Warning'` in both test files.

- [ ] **Step 3: Add the variants, the mirror, the branches, the copy**

In `crates/banager-core/src/model.rs`, inside `enum Warning`, before `/// Not yet localised -- see this type's doc comment.` (the `Message(String)` variant's doc), insert:

```rust
    /// rustup's `self uninstall` deletes `path` (`$RUSTUP_HOME`, spelled
    /// `~/.rustup`; the standard layout is the only one Banager offers
    /// the uninstall for, `rustup::standard_roots`) permanently -- not
    /// to the Trash -- with every toolchain in it: `names` are the entry
    /// names of its `toolchains/` directory when the preview was built,
    /// empty when that directory is missing, empty or unreadable (the
    /// front end then says "every toolchain" without naming them).
    /// Produced by the rustup recipe's uninstall warnings
    /// (`adapters/standalone/rustup.rs`).
    RemovesToolchains { path: String, names: Vec<String> },
    /// rustup 1.29.1's `self uninstall` deletes the whole Cargo home,
    /// `path` (`$CARGO_HOME`, spelled `~/.cargo`), permanently -- not to
    /// the Trash: the registry and git caches, `.crates2.json` (its
    /// record of what `cargo install` installed), Cargo's own
    /// `config.toml` and `credentials.toml` (the crates.io login), `env`,
    /// and anything else kept there (self_update.rs:977-993, :1029;
    /// unix.rs:50-53). Always produced.
    DeletesCargoHome { path: String },
    /// rustup 1.29.1's `self uninstall` deletes everything in the Cargo
    /// home's `bin/` whose name is not `rustup` or one of its thirteen
    /// proxies -- by name, so a program copied there by hand goes too:
    /// `names` are the binaries `.crates2.json` lists (`rg`, not
    /// `ripgrep`) united with a read-only listing of `bin/` minus those
    /// fourteen names (`rustup::bin_programs_rustup_removes`) -- the
    /// programs named where known. Only produced when there are any.
    /// (The research read a newer rustup that keeps them; the tag this
    /// recipe is verified against does not -- see the recipe's doc.)
    RemovesCargoInstalled { names: Vec<String> },
    /// Homebrew's `rustup` formula is installed too (`Cellar/rustup`
    /// under one of Homebrew's default prefixes,
    /// `rustup::homebrew_rustup_present`), and rustup's homes depend
    /// only on `RUSTUP_HOME`/`CARGO_HOME`/`HOME`, never on where the
    /// binary sits (`home` 0.5.12), so it shares the folders this
    /// uninstall deletes and loses its toolchains with them. Produced
    /// only when the Cellar directory is there.
    HomebrewRustupLosesToolchains,
    /// rustup's `self uninstall` edits the shell startup files it added
    /// its `. "$HOME/.cargo/env"` line to. Banager itself never edits one.
    EditsShellConfig,
    /// After rustup's own cleanup, `path` (`$HOME` spelled `~`) will still
    /// hold a line about Cargo's env file, which is then gone. `certain`
    /// is true when that line is one of the sourcing forms rustup itself
    /// writes and its target is this Cargo home, so it *will* print an
    /// error in every new terminal until the user removes it (a file
    /// rustup does not edit, such as `~/.zshrc`, or a second copy of the
    /// line); false for any other mention rustup will not remove (a
    /// guarded `[ -f … ] && . …`, an `echo`, another spelling), which
    /// *may*. One per file (`rustup::shell_config_leftovers`).
    LeavesShellConfigLine { path: String, certain: bool },
```

In `src/lib/types.ts`, in `export type Warning = …`, add before the final `;` (after C's three arms):

```ts
  | { RemovesToolchains: { path: string; names: string[] } }
  | { DeletesCargoHome: { path: string } }
  | { RemovesCargoInstalled: { names: string[] } }
  | "HomebrewRustupLosesToolchains"
  | "EditsShellConfig"
  | { LeavesShellConfigLine: { path: string; certain: boolean } }
```

and extend the doc comment above the type: after the sentence ending `whose \`host\` interpolates it)`, add `, the phase 4 uninstall warnings (step C's \`WillTrash\`/\`WillKeep\`/\`AlreadyGone\`, step E's six for rustup's own uninstall)` — one clause, so the comment still names where each family comes from.

In `src/lib/warnings.ts`, in `warningKey`'s string `switch`, before `default: {`, add:

```ts
      case "HomebrewRustupLosesToolchains":
        return "warnings.homebrewRustupLosesToolchains";
      case "EditsShellConfig":
        return "warnings.editsShellConfig";
```

and in its object half, before the last `const unhandled: never = warning;` (the one after `if ("Message" in warning) return null;` and C's three `if`s), add:

```ts
  if ("RemovesToolchains" in warning) {
    // Without names the sentence has no parenthesis to fill.
    return warning.RemovesToolchains.names.length > 0
      ? "warnings.removesToolchains"
      : "warnings.removesToolchainsUnlisted";
  }
  if ("DeletesCargoHome" in warning) return "warnings.deletesCargoHome";
  if ("RemovesCargoInstalled" in warning) return "warnings.removesCargoInstalled";
  if ("LeavesShellConfigLine" in warning) {
    // "will print an error" only for a line rustup's own sourcing form
    // spells; anything else that mentions the env file "may".
    return warning.LeavesShellConfigLine.certain
      ? "warnings.leavesShellConfigLine"
      : "warnings.leavesShellConfigLineMaybe";
  }
```

In `warningArgs`, before its last `const unhandled: never = warning;`, add:

```ts
  if ("RemovesToolchains" in warning) {
    const { path, names } = warning.RemovesToolchains;
    return names.length > 0 ? { path, names: names.join(", ") } : { path };
  }
  if ("DeletesCargoHome" in warning) return { path: warning.DeletesCargoHome.path };
  if ("RemovesCargoInstalled" in warning) {
    const names = warning.RemovesCargoInstalled.names;
    return { count: names.length, names: names.join(", ") };
  }
  if ("LeavesShellConfigLine" in warning) return { path: warning.LeavesShellConfigLine.path };
```

In `src/i18n/en.json`, in the `"warnings"` object, after its last entry (C's last `willKeep`/`alreadyGone` key, or `"thirdPartyRegistry"` if C put its keys elsewhere) add `,` and:

```json
    "removesToolchains": "This deletes {{path}} permanently — not to the Trash — with every toolchain in it ({{names}}) and everything rustup downloaded. Projects that need Rust will stop building until you install it again. Any other rustup that uses this folder loses its toolchains too.",
    "removesToolchainsUnlisted": "This deletes {{path}} permanently — not to the Trash — with every toolchain in it and everything rustup downloaded. Projects that need Rust will stop building until you install it again. Any other rustup that uses this folder loses its toolchains too.",
    "deletesCargoHome": "This deletes {{path}} permanently — not to the Trash: Cargo's downloaded packages, its record of what cargo install installed, Cargo's own settings and saved login (config.toml, credentials.toml), and anything else kept there.",
    "removesCargoInstalled_one": "{{names}}, in the Cargo folder's bin folder, is deleted too. If cargo install put it there, cargo install can put it back after you reinstall Rust.",
    "removesCargoInstalled_other": "{{count}} programs in the Cargo folder's bin folder are deleted too: {{names}}. The ones cargo install put there, cargo install can put back after you reinstall Rust.",
    "homebrewRustupLosesToolchains": "Homebrew's rustup is installed too (brew install rustup). It uses the same folders, so it loses its toolchains as well.",
    "editsShellConfig": "rustup will edit your shell settings files to remove the line it added.",
    "leavesShellConfigLine": "After this, {{path}} still has a line that loads Cargo's env file, which will be gone, so every new Terminal window will print an error until you remove that line yourself.",
    "leavesShellConfigLineMaybe": "After this, {{path}} still mentions Cargo's env file on a line rustup won't remove. If that line loads the file, every new Terminal window will print an error until you change it yourself."
```

In `src/i18n/zh-CN.json`, the same position:

```json
    "removesToolchains": "这会把 {{path}} 永久删除（不是移到废纸篓），里面的所有工具链（{{names}}）以及 rustup 下载的全部内容一起消失。需要 Rust 的项目要等重新安装后才能再编译。其它用这个文件夹的 rustup 也会失去它们的工具链。",
    "removesToolchainsUnlisted": "这会把 {{path}} 永久删除（不是移到废纸篓），里面的所有工具链以及 rustup 下载的全部内容一起消失。需要 Rust 的项目要等重新安装后才能再编译。其它用这个文件夹的 rustup 也会失去它们的工具链。",
    "deletesCargoHome": "这会把 {{path}} 永久删除（不是移到废纸篓）：Cargo 下载的包、cargo install 的安装记录、Cargo 自己的设置和保存的登录信息（config.toml、credentials.toml），以及放在里面的其它所有东西。",
    "removesCargoInstalled_other": "Cargo 文件夹的 bin 文件夹里的 {{count}} 个程序也会被删掉：{{names}}。其中用 cargo install 装的，重新安装 Rust 之后可以再用 cargo install 装回来。",
    "homebrewRustupLosesToolchains": "这台 Mac 上还装了 Homebrew 的 rustup（brew install rustup）。它用的是同一组文件夹，所以它的工具链也会一起没了。",
    "editsShellConfig": "rustup 会修改你的 shell 设置文件，删掉它当初加的那一行。",
    "leavesShellConfigLine": "卸载后 {{path}} 里还有一行会去加载 Cargo 的 env 文件，而那个文件已经被删掉了，所以每开一个终端窗口都会报一句错，直到你自己把那一行删掉。",
    "leavesShellConfigLineMaybe": "卸载后 {{path}} 里还有一行提到 Cargo 的 env 文件，rustup 不会删它。如果那一行会去加载这个文件，每开一个终端窗口都会报一句错，直到你自己改掉它。"
```

(`zh-CN.json` carries only the `_other` form of a pluralised key -- `wouldBreak_other` and no `wouldBreak_one` at HEAD, since Chinese has no plural, and `completeness.test.ts` compares the two files with plural suffixes stripped -- so zh-CN gets `removesCargoInstalled_other` only, as written above, and no `removesCargoInstalled_one`.)

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib model::tests` and `pnpm typecheck && pnpm exec vitest run src/lib/types.test.ts src/lib/warnings.test.ts src/i18n/completeness.test.ts`
Expected: PASS — `completeness.test.ts` finds every new key as a literal in `warnings.ts` (`warningKey` returns them as literal strings) and the same key set in both locales.

- [ ] **Step 5: Run the gates**

Run `cargo fmt --all`, then all five from Global Constraints. Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/model.rs src/lib/types.ts src/lib/types.test.ts src/lib/warnings.ts src/lib/warnings.test.ts src/i18n/en.json src/i18n/zh-CN.json
git commit -m "$(cat <<'EOF'
Add the six warnings rustup's own uninstall needs, in both locales

What rustup self uninstall does is more than its argv says: ~/.rustup and
~/.cargo are deleted permanently, not moved to the Trash -- every
toolchain, Cargo's caches, its settings and saved login, and every
program in its bin folder, which the dialog names where it can -- a
Homebrew rustup sharing those folders loses its toolchains too, rustup
edits the shell startup files it once wrote to, and a file it does not
edit can be left loading Cargo's env file after that file is gone. Each
is a variant the uninstall dialog lists in the user's language; the
toolchain sentence has a second wording for when no names could be read,
and the startup-file sentence says "will" only for a line rustup's own
form spells and "may" for any other mention.

Co-Authored-By: <the executing session's attribution line>
EOF
)"
```

---

### Task 3: `SecondToken`, `HttpTomlVersion`, `parse_release_stable_toml`

**Files:**
- Modify: `crates/banager-core/src/adapters/standalone/recipe.rs` — `enum VersionParse`, `enum Latest`
- Modify: `crates/banager-core/src/adapters/standalone/latest.rs` — `parse_version`, new `parse_release_stable_toml`, tests
- Modify: `crates/banager-core/src/adapters/standalone/mod.rs` — `latest_version`'s `match self.recipe.latest`
- Modify: `crates/banager-core/src/adapters/standalone/recipes.rs` — `test_every_recipe_latest_url_is_an_allowed_https_host`'s `match recipe.latest`
- Test: `latest.rs`'s `mod tests`.

**Interfaces:**
- Consumes: B's `VersionParse::FirstToken`, `Latest::ClaudeChannel`, `parse_version`, `is_dotted_version`, `latest_version`; `toml` (a dependency already, `AdapterMeta::from_toml`).
- Produces (verbatim): `VersionParse::SecondToken` (reader: `parse_version`; producer: the `RUSTUP` recipe, Task 6); `Latest::HttpTomlVersion { url: &'static str }` (reader: `latest_version`; producer: `RUSTUP`); `pub fn parse_release_stable_toml(body: &str) -> Result<String, String>` (reader: `latest_version`'s new arm).

`rustup --version` prints `rustup 1.29.1 (d95a37b6a 2026-08-13)` on stdout and two `info:` lines on **stderr** (recorded in Task 10 as `version-stderr.txt`); the version is the second token of stdout's first non-empty line. `release-stable.toml` is `schema-version = '1'\nversion = '1.29.1'\n` (rustup.md §6, VERIFIED; the file `rustup self update` itself reads, `DEFAULT_UPDATE_ROOT`).

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` in `crates/banager-core/src/adapters/standalone/latest.rs` (before its closing `}`):

```rust

    #[test]
    fn test_parse_version_reads_the_second_token_of_rustups_recorded_line() {
        // `rustup --version` stdout on this Mac, 2026-09-25 (recorded in
        // Task 10 as version.txt): `rustup 1.29.1 (d95a37b6a 2026-08-13)`.
        assert_eq!(
            parse_version("rustup 1.29.1 (d95a37b6a 2026-08-13)\n", VersionParse::SecondToken),
            Some("1.29.1".to_string())
        );
        // The two `info:` lines rustup writes go to stderr and are never
        // handed to this function; had they been, "This" is no version.
        assert_eq!(
            parse_version(
                "info: This is the version for the rustup toolchain manager, not the rustc compiler.\n",
                VersionParse::SecondToken
            ),
            None
        );
        // A line with one token has no second.
        assert_eq!(parse_version("1.29.1\n", VersionParse::SecondToken), None);
    }

    #[test]
    fn test_parse_release_stable_toml_reads_the_version_string() {
        // The release file byte for byte (rustup.md §6, VERIFIED by curl;
        // recorded in Task 10 as release-stable.toml), and TOML's other
        // string quote.
        assert_eq!(
            parse_release_stable_toml("schema-version = '1'\nversion = '1.29.1'\n"),
            Ok("1.29.1".to_string())
        );
        assert_eq!(
            parse_release_stable_toml("version = \"1.30.0\"\nschema-version = \"1\"\n"),
            Ok("1.30.0".to_string())
        );
    }

    #[test]
    fn test_parse_release_stable_toml_refuses_anything_that_is_not_a_versioned_release_file() {
        // HTML answered with status 200 (a captive portal), a file with
        // no version, a version that is not one, a version of the wrong
        // type: an uncheckable row with a short reason, never a candidate.
        for body in [
            "<html><body>Sign in to the network</body></html>",
            "",
            "schema-version = '1'\n",
            "version = 'latest'\n",
            "version = 1\n",
            "version = ['1.29.1']\n",
        ] {
            let err = parse_release_stable_toml(body).expect_err(body);
            assert!(err.contains("release file"), "{body:?}: {err}");
            assert!(err.len() < 140, "the reason stays short: {err}");
        }
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib adapters::standalone::latest`
Expected: FAIL to compile — `no variant or associated item named \`SecondToken\` found for enum \`VersionParse\``; `cannot find function \`parse_release_stable_toml\``.

- [ ] **Step 3: Add the variants and the parser**

In `crates/banager-core/src/adapters/standalone/recipe.rs`, replace

```rust
pub enum VersionParse {
    /// The first whitespace-separated token of the first non-empty line:
    /// `2.1.281 (Claude Code)` → `2.1.281`.
    FirstToken,
}
```

with

```rust
pub enum VersionParse {
    /// The first whitespace-separated token of the first non-empty line:
    /// `2.1.281 (Claude Code)` → `2.1.281`.
    FirstToken,
    /// The second token: `rustup 1.29.1 (d95a37b6a 2026-08-13)` → `1.29.1`
    /// (rustup; grok's `grok 1.0.41 (…)` in step D). The two `info:` lines
    /// rustup prints after that go to stderr, which the version read
    /// never looks at (`adapters/fixtures/standalone-rustup/<v>/
    /// version-stderr.txt` records them).
    SecondToken,
}
```

and replace the `Latest` enum's closing (after the `ClaudeChannel { base: &'static str },` arm, before `}`) by adding the arm:

```rust
    /// `GET url`, whose body is TOML with a top-level `version = '…'`:
    /// rustup's `release-stable.toml`, the file `rustup self update`
    /// itself reads (`DEFAULT_UPDATE_ROOT` in rustup's
    /// `src/cli/self_update.rs`; rustup.md §6, VERIFIED). Parsed by
    /// `latest::parse_release_stable_toml`.
    HttpTomlVersion { url: &'static str },
```

In `crates/banager-core/src/adapters/standalone/latest.rs`, in `parse_version`, replace

```rust
    let token = match parse {
        VersionParse::FirstToken => line.split_whitespace().next()?,
    };
```

with

```rust
    let mut tokens = line.split_whitespace();
    let token = match parse {
        VersionParse::FirstToken => tokens.next()?,
        VersionParse::SecondToken => tokens.nth(1)?,
    };
```

and append after `parse_channel_body` (before `#[cfg(test)]`):

```rust

/// The `version` of a release file such as rustup's `release-stable.toml`
/// (`schema-version = '1'` / `version = '1.29.1'`), trimmed; `Err` with a
/// short reason for a body that is not TOML, has no top-level `version`
/// string, or whose version is not a dotted version. The reason becomes
/// an uncheckable row's description, so it quotes at most a few
/// characters of the body, never a page of HTML.
pub fn parse_release_stable_toml(body: &str) -> Result<String, String> {
    let shown = || -> String { body.trim().chars().take(40).collect() };
    let table: toml::Value = toml::from_str(body)
        .map_err(|_| format!("the release file is not TOML (got {:?})", shown()))?;
    let version = table
        .get("version")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("the release file has no `version` string (got {:?})", shown()))?;
    let version = version.trim();
    if is_dotted_version(version) {
        Ok(version.to_string())
    } else {
        Err(format!(
            "the release file's version is not a version (got {version:?})"
        ))
    }
}
```

In `crates/banager-core/src/adapters/standalone/mod.rs`, in `latest_version`'s `match self.recipe.latest { … }`, after the `Latest::ClaudeChannel { base } => { … }` arm, add:

```rust
            Latest::HttpTomlVersion { url } => {
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
                latest::parse_release_stable_toml(&resp.body)
            }
```

In `crates/banager-core/src/adapters/standalone/recipes.rs`, in `test_every_recipe_latest_url_is_an_allowed_https_host`, replace

```rust
            let urls: Vec<String> = match recipe.latest {
                Latest::ClaudeChannel { base } => vec![
                    format!("{base}/{}", crate::adapters::standalone::latest::CHANNEL_LATEST),
                    format!("{base}/{}", crate::adapters::standalone::latest::CHANNEL_STABLE),
                ],
            };
```

with

```rust
            let urls: Vec<String> = match recipe.latest {
                Latest::ClaudeChannel { base } => vec![
                    format!("{base}/{}", crate::adapters::standalone::latest::CHANNEL_LATEST),
                    format!("{base}/{}", crate::adapters::standalone::latest::CHANNEL_STABLE),
                ],
                Latest::HttpTomlVersion { url } => vec![url.to_string()],
            };
```

(If step D landed first and added `HttpJsonField`/`Command` arms, these two matches already have four arms and this adds the fifth.)

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib adapters::standalone`
Expected: PASS — 3 new `latest` tests; every earlier one. The `HttpTomlVersion` arm of `latest_version` is exercised end to end in Task 6, once a recipe produces it.

- [ ] **Step 5: Run the gates**

Run `cargo fmt --all`, then all five from Global Constraints. Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/adapters/standalone/recipe.rs crates/banager-core/src/adapters/standalone/latest.rs crates/banager-core/src/adapters/standalone/mod.rs crates/banager-core/src/adapters/standalone/recipes.rs
git commit -m "$(cat <<'EOF'
Read a version from the second token, and from a TOML release file

rustup prints its version as the second token of its first line (the
two info lines after it go to stderr and are never read), and publishes
the newest one as `version = '…'` in the release-stable.toml its own
self update reads. Both shapes join the recipe types with their parsers;
a release file that is not TOML, has no version, or has one that is not
a version becomes a could-not-check row with a short reason.

Co-Authored-By: <the executing session's attribution line>
EOF
)"
```

---

### Task 4: `FlatFile`, `expand_route`, the seat bound to its instance, `Recipe.extra_locks`, the Upgrade plan's locks

**Files:**
- Modify: `crates/banager-core/src/adapters/standalone/recipe.rs` — `enum RouteKind`, `struct Recipe` (+ `no_extra_locks`)
- Modify: `crates/banager-core/src/adapters/standalone/route.rs` — new `expand_route` beside B's `expand`; `probe_strict`'s two branches (C's; `probe` if C did not split it); tests
- Modify: `crates/banager-core/src/adapters/standalone/recipes.rs` — `CLAUDE` (one field), `test_every_recipe_path_is_under_home`, two new tests
- Modify: `crates/banager-core/src/adapters/standalone/mod.rs` — `Detected`, `detect`, new `seated_detected_for`/`locks`, the `OpKind::Upgrade` arm of `plan`; `testing` (two helpers, one constant); B's two Upgrade tests rewritten, three new tests
- Modify: `crates/banager-core/src/adapters/standalone/removal.rs` — C's test helper `fn detected(home: &Path) -> Detected` gains three fields (C checklist row 2)  [C's file]
- Test: `route.rs`, `recipes.rs`, `mod.rs` test modules; C's `removal.rs` tests keep passing.

**Interfaces:**
- Consumes: `cargo::cargo_home_of`, `path_env::tool_home`, `HostEnv.{rustup_home, zdotdir}` (Task 1); B's `expand`, `Probe`, `probe`/C's `probe_strict`, `Detected { home }` (+ C's `euid`), `plan`'s Upgrade arm, `testing::TempHome`; C's `PlanAction::Command` and `Plan.action`; `ResourceLock` (`model.rs:424`).
- Produces (verbatim): `RouteKind::FlatFile` (reader: `probe_strict`, both branches; producer: `RUSTUP`, Task 6); `pub fn expand_route(home: &Path, cargo_home: Option<&Path>, spec: &str) -> Option<PathBuf>` (readers: `detect`, `seated_detected_for`); `Detected.cargo_home: Option<PathBuf>`, `Detected.rustup_home: Option<PathBuf>`, `Detected.zdotdir: Option<PathBuf>` (writer: `detect`, here. **Declared deferral:** their production readers are `rustup::{extra_locks, uninstall_blocked, uninstall_warnings}`, written in Task 5 and called from `plan`/`inventory` once Task 6 puts them in the `RUSTUP` recipe; `seated_detected_for` reads `cargo_home` here for the binding — the fields are `pub` on a `pub` struct in a `pub mod`, so `-D warnings` raises nothing in between); `Recipe.extra_locks: fn(&Detected) -> Vec<ResourceLock>` and `pub fn no_extra_locks(_: &Detected) -> Vec<ResourceLock>` (reader: `StandaloneAdapter::locks`; producers: `CLAUDE` here with `no_extra_locks`, `RUSTUP` in Task 6 with `rustup::extra_locks`); `fn seated_detected_for(&self, inst: &ManagerInstance) -> Result<Detected, AdapterError>` and `fn locks(&self, inst: &ManagerInstance, detected: &Detected) -> Vec<ResourceLock>` (readers: `plan`'s Upgrade arm here, its Uninstall `Command` arm and `inventory` in Task 6; C's `Paths` arm, C checklist row 16); `testing::{RustupLayout, rustup_layout, RUSTUP_PROXIES, detected}` (readers: Tasks 5, 6 and 10's tests; `RUSTUP_PROXIES` moves to `rustup.rs` in Task 5, where `bin_programs_rustup_removes` reads it in production).

Rules: `FlatFile` (spec §3.3 step 3) — the launcher must be a regular file (`symlink_metadata().file_type().is_file()`: not a link); its `real` is its canonical path; the shared exclusion runs first as for every kind. **It has no launcher-only state** (Ruling 8): B's dangling branch (`Err(NotFound)` from `canonicalize`) answers from the link's text alone and never looks at `kind` — its `is_symlink` check returns `Absent` only for a launcher that is *not* a link, and a dangling link is one — so for rustup, whose root is the whole Cargo home, `$CARGO_HOME/bin/rustup -> rustup.old` would be `LauncherOnly`. That branch now matches `kind` first: `FlatFile` is `Absent`, `SymlinkIntoRoot` keeps B's rule. `expand_route` (spec §3.1, ruling 5): `~/…` under `home`, always; `$CARGO_HOME` alone, or `$CARGO_HOME/…`, under the Cargo home when there is one, `None` when the Cargo home is unsupported (a relative `CARGO_HOME`, ruling 6) — and `detect` then lists nothing. B's `expand` is untouched. The seat (ruling 9): `seated_detected_for(inst)` refuses before any detect and refuses an instance the seat does not describe. The Upgrade plan (spec §五, §2.4): `locks` = the instance's own lock followed by the recipe's extra locks, `action: PlanAction::Command { program: launcher, args, env: [] }`; everything else as B built it.

- [ ] **Step 1: Write the failing tests**

In `crates/banager-core/src/adapters/standalone/route.rs`, inside `mod tests`, after B's `test_expand_refuses_a_path_that_is_not_under_home` (B's three `expand` tests stay exactly as they are), append:

```rust

    use super::super::testing::rustup_layout;

    #[test]
    fn test_expand_route_joins_home_paths_always_and_cargo_home_paths_when_there_is_one() {
        // rustup's launcher and root (spec §3.5): `$CARGO_HOME/bin/rustup`
        // and the bare `$CARGO_HOME`. The Cargo home is whatever
        // `cargo::cargo_home_of` answered -- `CARGO_HOME` when set -- so
        // a Mac with it set finds rustup where rustup actually is; and
        // `None` (a relative CARGO_HOME, unsupported) means a recipe under
        // it has no path at all, while a `~/` path never depends on it.
        let home = Path::new("/Users/someone");
        let default = Path::new("/Users/someone/.cargo");
        assert_eq!(
            expand_route(home, Some(default), "~/.local/bin/claude"),
            Some(PathBuf::from("/Users/someone/.local/bin/claude"))
        );
        assert_eq!(
            expand_route(home, None, "~/.local/bin/claude"),
            Some(PathBuf::from("/Users/someone/.local/bin/claude"))
        );
        assert_eq!(
            expand_route(home, Some(default), "$CARGO_HOME/bin/rustup"),
            Some(PathBuf::from("/Users/someone/.cargo/bin/rustup"))
        );
        assert_eq!(
            expand_route(home, Some(Path::new("/Volumes/Data/cargo")), "$CARGO_HOME/bin/rustup"),
            Some(PathBuf::from("/Volumes/Data/cargo/bin/rustup"))
        );
        assert_eq!(
            expand_route(home, Some(Path::new("/Volumes/Data/cargo")), "$CARGO_HOME"),
            Some(PathBuf::from("/Volumes/Data/cargo"))
        );
        assert_eq!(expand_route(home, None, "$CARGO_HOME/bin/rustup"), None);
        assert_eq!(expand_route(home, None, "$CARGO_HOME"), None);
    }

    #[test]
    #[should_panic(expected = "must start with ~/ or $CARGO_HOME")]
    fn test_expand_route_refuses_any_other_shape() {
        let home = Path::new("/Users/someone");
        let _ = expand_route(home, Some(&home.join(".cargo")), "/usr/local/bin/rustup");
    }

    #[test]
    fn test_probe_finds_a_flat_file_launcher_and_its_real_path_is_itself() {
        // rustup: `$CARGO_HOME/bin/rustup` is an 11 MB Mach-O regular file
        // (spec §3.5, VERIFIED); its thirteen proxies are links *to* it,
        // but the launcher itself is no link.
        let home = TempHome::new("probe-flat-present");
        let layout = rustup_layout(&home.path().join(".cargo"));
        assert_eq!(
            probe(RouteKind::FlatFile, &layout.launcher, &layout.cargo_home),
            Probe::Present {
                real: layout.launcher.clone()
            }
        );
    }

    #[test]
    fn test_probe_rejects_a_link_or_a_directory_where_a_flat_file_is_expected() {
        // A `rustup` that is itself a symlink (Homebrew's keg-only formula
        // linked by hand, or anything else) is not the installer's copy;
        // nor is a directory of that name.
        let home = TempHome::new("probe-flat-link");
        let elsewhere = home.file("opt/homebrew/Cellar/rustup/1.29.1/bin/rustup");
        let launcher = home.link(".cargo/bin/rustup", &elsewhere);
        assert_eq!(
            probe(RouteKind::FlatFile, &launcher, &home.path().join(".cargo")),
            Probe::Absent
        );

        let home = TempHome::new("probe-flat-dir");
        let launcher = home.dir(".cargo/bin/rustup");
        assert_eq!(
            probe(RouteKind::FlatFile, &launcher, &home.path().join(".cargo")),
            Probe::Absent
        );
    }

    #[test]
    fn test_probe_is_absent_for_a_flat_file_launcher_that_is_not_there() {
        let home = TempHome::new("probe-flat-missing");
        assert_eq!(
            probe(
                RouteKind::FlatFile,
                &home.path().join(".cargo/bin/rustup"),
                &home.path().join(".cargo")
            ),
            Probe::Absent
        );
    }

    #[test]
    fn test_probe_is_absent_for_a_dangling_link_where_a_flat_file_is_expected() {
        // A flat-file launcher is the program itself, so a dangling link
        // at its path is not this install half-removed. rustup's root is
        // the whole Cargo home: without the flat-file rule this link's
        // text lands under it and the row would be a version-less rustup
        // whose plans run a dangling link (Ruling 8).
        let home = TempHome::new("probe-flat-dangling");
        let cargo_home = home.dir(".cargo");
        let launcher = home.link(".cargo/bin/rustup", Path::new("rustup.old"));
        assert_eq!(probe(RouteKind::FlatFile, &launcher, &cargo_home), Probe::Absent);
        // The same link under the link-shaped route is still B's
        // `LauncherOnly`: the rule belongs to the route, not to the link.
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &cargo_home),
            Probe::LauncherOnly
        );
    }
```

In `crates/banager-core/src/adapters/standalone/recipes.rs`, inside `mod tests`, replace `test_every_recipe_path_is_under_home` (B's, asserting `path.starts_with("~/")`) with:

```rust
    #[test]
    fn test_every_recipe_path_is_under_home_or_the_cargo_home() {
        // `route::expand_route` joins `~/` onto `HostEnv.home` and
        // `$CARGO_HOME` (bare, or with a `/`) onto the Cargo home, and
        // nothing else: a recipe path shaped any other way is a
        // programming error this test turns into a red build, not a
        // runtime surprise.
        for recipe in RECIPES {
            for path in [recipe.route.launcher, recipe.route.root] {
                assert!(
                    path.starts_with("~/") || path == "$CARGO_HOME" || path.starts_with("$CARGO_HOME/"),
                    "{}: recipe path {path:?} must start with ~/ or $CARGO_HOME",
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
    fn test_a_paths_recipe_names_only_home_paths() {
        // The path-list uninstall (`removal.rs`, step C) expands its
        // recipe's route and every remove/keep spec with B's two-argument
        // `route::expand`, which knows `~/` and nothing else. A tool whose
        // paths live under `$CARGO_HOME` (rustup) uninstalls with its own
        // command, so this holds today by construction; this test keeps
        // it true when the next `Paths` recipe lands, rather than letting
        // `expand` panic at plan time.
        for recipe in RECIPES {
            let Some(Uninstall::Paths { remove, keep }) = &recipe.uninstall else {
                continue;
            };
            let mut paths = vec![recipe.route.launcher, recipe.route.root];
            paths.extend(remove.iter().map(|spec| spec.path));
            paths.extend(keep.iter().map(|spec| spec.path));
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
    fn test_claude_holds_no_lock_but_its_own() {
        // `extra_locks` exists for rustup (step E), whose self update and
        // self uninstall touch what the cargo adapter reads; a tool with
        // its own directory holds only its own instance lock.
        let detected = crate::adapters::standalone::testing::detected(
            Path::new("/Users/someone"),
            Path::new("/Users/someone/.cargo"),
        );
        assert!((CLAUDE.extra_locks)(&detected).is_empty());
    }
```

(`Uninstall` reaches the test module through `use super::*;` once the file imports it for `CLAUDE.uninstall` — C's; if C's `RemoveSpec`/`KeepSpec` field for the path is not `path`, use C's name.)

In `crates/banager-core/src/adapters/standalone/mod.rs`, inside `mod tests`, replace B's `test_plan_upgrade_is_the_tools_own_update_command_without_the_version_env` and `test_execute_runs_the_plan_and_streams_its_output` (both build an adapter and call `plan` with no detect; the Upgrade arm now reads the seat) with:

```rust
    #[tokio::test]
    async fn test_plan_upgrade_is_the_tools_own_update_command_without_the_version_env() {
        // Spec §五: `<launcher> update`, 1800 s, KillThenReconcile, the
        // instance's own lock and no other (claude's `extra_locks` is
        // empty), no password. And *no* `DISABLE_AUTOUPDATER` (spec
        // §3.4): that variable is for the read-only version read; the
        // updater must be allowed to update.
        let home = TempHome::new("plan-upgrade");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = adapter(runner);
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);

        let plan = adapter
            .plan(&inst, &request(OpKind::Upgrade, ArtifactKind::Binary, "claude"))
            .await
            .expect("plan");

        assert_eq!(
            plan.action,
            PlanAction::Command {
                program: layout.launcher.clone(),
                args: vec!["update".to_string()],
                env: Vec::new(),
            },
            "no DISABLE_AUTOUPDATER on the upgrade"
        );
        assert!(!plan.needs_password);
        assert_eq!(plan.locks, vec![ResourceLock("standalone-claude".to_string())]);
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);
        assert!(plan.warnings.is_empty());
        assert!(plan.affected.is_empty());
        assert_eq!(plan.timeout_secs, 1800);
        assert_eq!(plan.request.name, "claude");
    }

    #[tokio::test]
    async fn test_plan_upgrade_is_refused_before_any_detect() {
        // Unreachable through `Session`, which detects before it plans;
        // the adapter's own answer for a plan asked of it cold (spec §3.2).
        let home = TempHome::new("plan-cold");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        assert!(matches!(
            adapter
                .plan(
                    &instance_for(&layout, Some("2.1.281")),
                    &request(OpKind::Upgrade, ArtifactKind::Binary, "claude")
                )
                .await,
            Err(AdapterError::Refused(_))
        ));
    }

    #[tokio::test]
    async fn test_plan_refuses_an_instance_the_seat_no_longer_describes() {
        // The seat is one slot the latest detect overwrites. Detect home
        // A, then home B, then plan for A's instance: the program would be
        // A's launcher and the locks and warnings B's (ruling 9). Refused,
        // until a detect of A seats A again.
        let home_a = TempHome::new("seat-a");
        let layout_a = claude_layout(&home_a, "2.1.281");
        let home_b = TempHome::new("seat-b");
        let layout_b = claude_layout(&home_b, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        for layout in [&layout_a, &layout_b] {
            runner.respond(
                vec![layout.launcher.to_str().unwrap(), "--version"],
                exited_0("2.1.281 (Claude Code)\n"),
            );
        }
        let adapter = adapter(runner);
        let inst_a = adapter.detect(&home_a.env(vec![])).await.remove(0);
        let inst_b = adapter.detect(&home_b.env(vec![])).await.remove(0);
        assert_ne!(inst_a.exe_path, inst_b.exe_path);

        let req = request(OpKind::Upgrade, ArtifactKind::Binary, "claude");
        assert!(
            matches!(adapter.plan(&inst_a, &req).await, Err(AdapterError::Refused(_))),
            "A's instance against B's seat"
        );
        assert_eq!(
            adapter.plan(&inst_b, &req).await.expect("B's instance against B's seat").action,
            PlanAction::Command {
                program: layout_b.launcher.clone(),
                args: vec!["update".to_string()],
                env: Vec::new(),
            }
        );
        adapter.detect(&home_a.env(vec![])).await;
        assert_eq!(
            adapter.plan(&inst_a, &req).await.expect("A's instance against A's seat").action,
            PlanAction::Command {
                program: layout_a.launcher.clone(),
                args: vec!["update".to_string()],
                env: Vec::new(),
            }
        );
    }

    #[tokio::test]
    async fn test_execute_runs_the_plan_and_streams_its_output() {
        let home = TempHome::new("execute");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "update"],
            exited_0("Successfully updated from 2.1.281 to version 2.1.290\n"),
        );
        let adapter = adapter(runner);
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
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
    async fn test_detect_seats_the_two_homes_and_zdotdir_for_the_plans_that_need_them() {
        // `Detected.cargo_home` follows `CARGO_HOME` (through
        // `cargo::cargo_home_of`), `rustup_home` follows `RUSTUP_HOME`
        // (through `path_env::tool_home`), `zdotdir` is carried raw: the
        // rustup recipe's lock, gate and warnings read them in `plan` and
        // `inventory`, which have no HostEnv of their own.
        let home = TempHome::new("detect-seat");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = adapter(runner);
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        let seat = adapter.seated_detected_for(&inst).expect("seated");
        assert_eq!(seat.cargo_home, Some(home.path().join(".cargo")));
        assert_eq!(seat.rustup_home, Some(home.path().join(".rustup")));
        assert_eq!(seat.zdotdir, None);

        let custom_cargo = home.path().join("elsewhere/cargo");
        let custom_rustup = home.path().join("elsewhere/rustup");
        let inst = adapter
            .detect(&HostEnv {
                cargo_home: Some(custom_cargo.clone()),
                rustup_home: Some(custom_rustup.clone()),
                zdotdir: Some(home.path().to_path_buf()),
                ..home.env(vec![])
            })
            .await
            .remove(0);
        let seat = adapter.seated_detected_for(&inst).expect("seated");
        assert_eq!(seat.cargo_home, Some(custom_cargo));
        assert_eq!(seat.rustup_home, Some(custom_rustup));
        assert_eq!(seat.zdotdir, Some(home.path().to_path_buf()));

        // Relative values: unsupported, seated as `None`; an empty one is
        // the default (the `home` crate's rule, Task 1).
        let inst = adapter
            .detect(&HostEnv {
                cargo_home: Some(PathBuf::from("cargo")),
                rustup_home: Some(PathBuf::from("")),
                ..home.env(vec![])
            })
            .await
            .remove(0);
        let seat = adapter.seated_detected_for(&inst).expect("seated");
        assert_eq!(seat.cargo_home, None);
        assert_eq!(seat.rustup_home, Some(home.path().join(".rustup")));
    }
```

and add `use crate::model::PlanAction;` to that test module's `use` lines unless C's tests already import it there (C's `PlanAction`, `model.rs`; a second identical `use` in one module is error E0252).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib adapters::standalone`
Expected: FAIL to compile — `cannot find function \`expand_route\``; `no variant or associated item named \`FlatFile\``; `cannot find function \`rustup_layout\``/`\`detected\`` in `testing`; `no field \`extra_locks\` on type \`Recipe\``; `no method named \`seated_detected_for\``; `no field \`cargo_home\` on type \`Detected\`` (and `rustup_home`, `zdotdir`).

- [ ] **Step 3: Write the shapes, the helpers, the plan's locks**

In `crates/banager-core/src/adapters/standalone/recipe.rs`, change `use crate::model::CancelPolicy;` to

```rust
use super::Detected;
use crate::model::{CancelPolicy, ResourceLock};
```

(C's `Uninstall` family may have brought `Warning` in as well; keep whatever C imports.) In `enum RouteKind`, after `SymlinkIntoRoot,` add:

```rust
    /// The launcher is a regular file, not a link (rustup:
    /// `$CARGO_HOME/bin/rustup`, an 11 MB Mach-O executable, VERIFIED on
    /// this Mac; agy in step D). Its real path is itself; `root` is the
    /// instance's `prefix` and plays no part in the fingerprint. A link
    /// at that path is not this route's install, and a dangling link at
    /// it is not this install half-removed (no launcher-only state).
    FlatFile,
```

In `struct Recipe`, after the last field (C's `uninstall: Option<Uninstall>`), add:

```rust
    /// Locks every plan of this tool holds besides its own instance lock,
    /// from what `detect` seated. rustup's is the cargo instance's
    /// (`rustup::extra_locks`): `rustup self update` unlinks and re-copies
    /// the binary all thirteen `$CARGO_HOME/bin` proxies run, `cargo`
    /// among them, and `rustup self uninstall` deletes the `.crates2.json`
    /// cargo's inventory reads (spec §2.4). A tool with its own directory
    /// holds nothing else: `no_extra_locks`. Read by
    /// `StandaloneAdapter::locks`, for the Upgrade and Uninstall plans.
    pub extra_locks: fn(&Detected) -> Vec<ResourceLock>,
```

and, after the `Recipe` struct definition (before `Route`):

```rust

/// `Recipe.extra_locks` for a tool that touches nothing another source
/// reads: only its own instance lock, which every plan holds anyway.
pub fn no_extra_locks(_: &Detected) -> Vec<ResourceLock> {
    Vec::new()
}
```

In `crates/banager-core/src/adapters/standalone/recipes.rs`, add `no_extra_locks` to the `use super::recipe::{…}` list, and in `pub static CLAUDE: Recipe = Recipe { … }` add, as its last field (after C's `uninstall: …,`):

```rust
    extra_locks: no_extra_locks,
```

Add the same line to every other `Recipe { … }` literal the baseline's `grep -rn "Recipe {$" crates/banager-core/src/adapters/standalone/` found — C's ruling 2 promises a test-only recipe with `uninstall: None` to keep the `NoSafeMethod` arm exercised — importing `no_extra_locks` (`super::recipe::no_extra_locks`, or the path that module uses for `Recipe`) where it sits. A literal without the field stops the build with `missing field \`extra_locks\``, so the compiler lists any the grep missed.

In `crates/banager-core/src/adapters/standalone/route.rs`, leave B's `expand` exactly as it is and insert after it:

```rust

/// A recipe route path under `home` (`~/.local/bin/claude`) or under the
/// Cargo home (`$CARGO_HOME/bin/rustup`, or the bare `$CARGO_HOME`,
/// rustup's root). `HostEnv.home` is the `HOME` the login shell exported
/// and `cargo_home` is `cargo::cargo_home_of`'s answer (`CARGO_HOME`
/// when set, by the `home` crate's rule), neither canonicalised: the
/// Unknown page (scan/mod.rs) compares an instance's raw `exe_path` with
/// the raw directory entries it reads, so both must come from the same
/// spelling. `None` only for a `$CARGO_HOME` path when there is no
/// usable Cargo home (a relative `CARGO_HOME`, which names a directory
/// relative to cargo's own cwd): `detect` then lists nothing. `expand`
/// (above) is the `~/`-only function the path-list uninstall uses; a
/// `Paths` recipe may name only `~/` paths
/// (`recipes::tests::test_a_paths_recipe_names_only_home_paths`). Any
/// other shape is a programming error in a recipe constant;
/// `recipes::tests::test_every_recipe_path_is_under_home_or_the_cargo_home`
/// catches it before this can. Read by `StandaloneAdapter::detect` and
/// `seated_detected_for`.
pub fn expand_route(home: &Path, cargo_home: Option<&Path>, spec: &str) -> Option<PathBuf> {
    if let Some(rest) = spec.strip_prefix("~/") {
        return Some(home.join(rest));
    }
    if spec == "$CARGO_HOME" {
        return cargo_home.map(Path::to_path_buf);
    }
    if let Some(rest) = spec.strip_prefix("$CARGO_HOME/") {
        return cargo_home.map(|cargo_home| cargo_home.join(rest));
    }
    panic!("recipe path {spec:?} must start with ~/ or $CARGO_HOME")
}
```

and in **`probe_strict`** (C's; the function that holds `match kind` — in B's tree it is `probe`, and if C did not split it the edits below go into `probe`), in the `Ok(real) => { … match kind { … } }` branch, after the `RouteKind::SymlinkIntoRoot => { … }` arm add:

```rust
                RouteKind::FlatFile => {
                    // The installer's own copy is a regular file; a link
                    // of that name points at somebody else's.
                    if meta.file_type().is_file() {
                        Probe::Present { real }
                    } else {
                        Probe::Absent
                    }
                }
```

(C's `probe_strict` wraps each `Probe` in `Ok(…)`; match that: `Ok(Probe::Present { real })` / `Ok(Probe::Absent)`.) And in its dangling branch, replace B's

```rust
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if !meta.file_type().is_symlink() {
                return Probe::Absent;
            }
```

with

```rust
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Only a link-shaped route has a launcher-only state (program
            // files gone, link left). A flat-file launcher *is* the
            // program, so a dangling link at its path is not this install
            // half-removed -- and rustup's root is the whole Cargo home,
            // under which any link text would land. Exhaustive, so a
            // future kind has to decide.
            match kind {
                RouteKind::SymlinkIntoRoot => {}
                RouteKind::FlatFile => return Probe::Absent,
            }
            if !meta.file_type().is_symlink() {
                return Probe::Absent;
            }
```

(again `Ok(Probe::Absent)` in C's `probe_strict`; the rest of the branch — `read_link`, the canonical parent, `canonicalize_existing_prefix`, C's `one_hop`, the marker check — is B's/C's, unchanged).

In `crates/banager-core/src/adapters/standalone/mod.rs`:

(a) Replace `Detected` (B's one field, C's `euid` and doc) with:

```rust
/// What `detect` learned that the `Adapter` methods without a `HostEnv`
/// need later -- the same seat `CargoAdapter.binstall` is (detect writes,
/// later calls read; `Session` always detects before it asks anything
/// else of an instance). `home`: `check_updates` finds
/// `~/.claude/settings.json` with it, the removal expands its paths
/// against it, and the rustup recipe's uninstall warnings read the shell
/// startup files under it. `euid`: the removal's ownership check (step
/// C). `cargo_home`: `CARGO_HOME` by the `home` crate's rule
/// (`cargo::cargo_home_of`, the rule `CargoAdapter::detect` names its
/// instance by; `None` for a relative value, unsupported), which the
/// rustup recipe's `extra_locks` spells the cargo lock from and its gate
/// and warnings read `.crates2.json` and `bin/` under. `rustup_home`:
/// `RUSTUP_HOME` by the same rule (`path_env::tool_home`), which the
/// rustup recipe's gate compares with `~/.rustup` and its warnings list
/// `toolchains/` under. `zdotdir`: `ZDOTDIR` raw, which the rustup
/// recipe's startup-file model visits `.zshenv`/`.zprofile` under, as
/// rustup's own cleanup does. The seat is bound to an instance by
/// `seated_detected_for`. `Clone`, so `plan` can take a copy out of the
/// mutex before it awaits anything.
#[derive(Clone, Debug)]
pub struct Detected {
    pub home: PathBuf,
    pub euid: u32,
    pub cargo_home: Option<PathBuf>,
    pub rustup_home: Option<PathBuf>,
    pub zdotdir: Option<PathBuf>,
}
```

(drop `pub euid: u32,` if C did not add it — C checklist row 2).

(b) In `detect`, replace

```rust
        let launcher = route::expand(&env.home, self.recipe.route.launcher);
        let root = route::expand(&env.home, self.recipe.route.root);
```

with

```rust
        // The Cargo home by the `home` crate's rule (`None`: a relative
        // CARGO_HOME, which no path of Banager's can stand for). A recipe
        // under `$CARGO_HOME` then has no launcher to look for; a `~/`
        // recipe is unaffected and seats `None`.
        let cargo_home = crate::adapters::cargo::cargo_home_of(env);
        let (Some(launcher), Some(root)) = (
            route::expand_route(&env.home, cargo_home.as_deref(), self.recipe.route.launcher),
            route::expand_route(&env.home, cargo_home.as_deref(), self.recipe.route.root),
        ) else {
            return Vec::new();
        };
```

and replace the `Detected { … }` literal it writes (`*self.detected.lock().unwrap() = Some(Detected { home: env.home.clone(), euid: env.euid, })` after C) with:

```rust
        *self.detected.lock().unwrap() = Some(Detected {
            home: env.home.clone(),
            euid: env.euid,
            cargo_home,
            rustup_home: crate::runner::path_env::tool_home(
                env.rustup_home.as_deref(),
                &env.home,
                ".rustup",
            ),
            zdotdir: env.zdotdir.clone(),
        });
```

(c) After `artifact_key` (before `inventory`), insert:

```rust

    /// What `detect` seated, bound to `inst` -- for the plan arms and the
    /// inventory gate that need it: rustup's cargo lock, its
    /// standard-layout gate and its uninstall warnings. The seat is one
    /// slot that the latest `detect` overwrites, and `plan` is handed an
    /// instance: a plan for an instance detected under home A after a
    /// detect under home B would run A's launcher with B's locks and
    /// warnings. So the launcher and root the seat expands to must be the
    /// instance's own `exe_path` and `prefix`; anything else is
    /// `Refused`, and so is a plan asked before any detect, which
    /// `Session` never does (spec §3.2). A copy, so no mutex guard is
    /// held across anything `plan` awaits.
    fn seated_detected_for(&self, inst: &ManagerInstance) -> Result<Detected, AdapterError> {
        let seat = self.detected.lock().unwrap().clone().ok_or_else(|| {
            AdapterError::Refused(format!(
                "{} has not been detected yet, so nothing can be planned for it",
                self.meta.name
            ))
        })?;
        let route = &self.recipe.route;
        let launcher = route::expand_route(&seat.home, seat.cargo_home.as_deref(), route.launcher);
        let root = route::expand_route(&seat.home, seat.cargo_home.as_deref(), route.root);
        if launcher.as_deref() != Some(inst.exe_path.as_path())
            || root.as_deref() != Some(inst.prefix.as_path())
        {
            return Err(AdapterError::Refused(format!(
                "{} was last detected under a different home than this instance's; refresh and try again",
                self.meta.name
            )));
        }
        Ok(seat)
    }

    /// Every lock a plan for this instance holds: its own, then the
    /// recipe's `extra_locks` (rustup: the cargo instance's). Both plans
    /// use it, so neither can forget the second lock.
    fn locks(&self, inst: &ManagerInstance, detected: &Detected) -> Vec<ResourceLock> {
        let mut locks = vec![ResourceLock(inst.id.clone())];
        locks.extend((self.recipe.extra_locks)(detected));
        locks
    }
```

(d) In `plan`, replace the `OpKind::Upgrade => { … }` arm (C's, with `action: PlanAction::Command { … }` and `locks: vec![ResourceLock(inst.id.clone())]`) with:

```rust
            OpKind::Upgrade => {
                let detected = self.seated_detected_for(inst)?;
                let upgrade = &self.recipe.upgrade;
                Ok(Plan {
                    request: req.clone(),
                    action: PlanAction::Command {
                        // The launcher, exactly as previewed: never a
                        // program the recipe could name (spec 附录 B).
                        program: inst.exe_path.clone(),
                        args: upgrade.args.iter().map(|a| a.to_string()).collect(),
                        // Not the version read's environment: the tool's
                        // updater must not be told to stop updating
                        // (spec §3.4), and rustup's self update may
                        // install nothing anyway.
                        env: Vec::new(),
                    },
                    // Everything lives under $HOME (spec §五).
                    needs_password: false,
                    // The instance's own lock, plus rustup's cargo lock
                    // (spec §2.4): a self update replaces the binary the
                    // cargo instance's `cargo` proxy runs.
                    locks: self.locks(inst, &detected),
                    cancel_policy: upgrade.cancel,
                    warnings: Vec::new(),
                    affected: Vec::new(),
                    timeout_secs: upgrade.timeout_secs,
                })
            }
```

(`PlanAction` and `ResourceLock` are in the file's `use crate::model::{…}` line: add whichever is missing.) If C's `Paths` arm reads the seat through an accessor of its own, replace that read with `self.seated_detected_for(inst)?` (C checklist row 16) so one binding rule holds for both arms.

(e) In `#[cfg(test)] pub(super) mod testing`, after `claude_layout`, append:

```rust

    /// rustup's native layout: `<cargo_home>/bin/rustup`, an executable
    /// regular file, and the thirteen proxies rustup installs beside it as
    /// relative links to it (`TOOLS` + `DUP_TOOLS` in rustup's
    /// `src/lib.rs`; `ls -la ~/.cargo/bin` on this Mac, unknown-scan.md
    /// §2).
    pub struct RustupLayout {
        pub cargo_home: PathBuf,
        pub launcher: PathBuf,
    }

    /// Here until Task 5 moves it to `rustup.rs`, where the uninstall
    /// preview reads it in production.
    pub const RUSTUP_PROXIES: [&str; 13] = [
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

    pub fn rustup_layout(cargo_home: &Path) -> RustupLayout {
        use std::os::unix::fs::PermissionsExt;
        let bin = cargo_home.join("bin");
        std::fs::create_dir_all(&bin).expect("create cargo bin");
        let launcher = bin.join("rustup");
        std::fs::write(&launcher, b"#!/bin/sh\n").expect("write rustup");
        // Executable, as the installer leaves it and as
        // `TempHome::executable` makes claude's target: B's `shadow_note`
        // accepts only a `PATH` hit with an execute bit, so a 0644
        // launcher would give every detect over `<cargo_home>/bin` a
        // `NotOnPath` note.
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755))
            .expect("executable rustup");
        for proxy in RUSTUP_PROXIES {
            std::os::unix::fs::symlink("rustup", bin.join(proxy)).expect("proxy link");
        }
        RustupLayout {
            cargo_home: cargo_home.to_path_buf(),
            launcher,
        }
    }

    /// A `Detected` seat as `detect` writes it for `home` with this Cargo
    /// home, the default rustup home and no `ZDOTDIR`, for the recipe
    /// functions that take one.
    pub fn detected(home: &Path, cargo_home: &Path) -> super::Detected {
        super::Detected {
            home: home.to_path_buf(),
            euid: 501,
            cargo_home: Some(cargo_home.to_path_buf()),
            rustup_home: Some(home.join(".rustup")),
            zdotdir: None,
        }
    }
```

(`euid: 501,` is C's field per spec §3.2; drop that line if C did not add the field.)

In `crates/banager-core/src/adapters/standalone/removal.rs` (C's), in its test module's helper `fn detected(home: &Path) -> Detected { Detected { home: …, euid: … } }`, add the three fields `cargo_home: Some(home.join(".cargo")), rustup_home: Some(home.join(".rustup")), zdotdir: None,` (C checklist row 2). Every other `Detected {` literal the grep finds outside `mod.rs` gets the same three lines; a struct-update literal (`Detected { euid: …, ..detected(home.path()) }`) needs nothing.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib adapters::standalone`
Expected: PASS — in `route`, two new `expand_route` tests, four new `FlatFile` tests (present, a link or a directory, missing, dangling), B's three `expand` tests untouched; 3 in `recipes`; 5 in `mod.rs`; every earlier one, C's `removal` tests among them (B's `test_expand_refuses_a_path_that_is_not_under_home` still panics with `must start with ~/`; B's dangling-link tests still answer `LauncherOnly` for `SymlinkIntoRoot`).

- [ ] **Step 5: Run the gates**

Run `cargo fmt --all`, then all five from Global Constraints. Expected: all clean (`RUSTUP_PROXIES` is read by `rustup_layout`; `detected` and `rustup_layout` are read by the tests above and by Tasks 5, 6 and 10's).

- [ ] **Step 6: Commit**

If the `Recipe { … }` literal C wrote for its `uninstall: None` test (Step 3) lives in a file other than these five, append that file's exact path to the `git add` below.

```bash
git add crates/banager-core/src/adapters/standalone/recipe.rs crates/banager-core/src/adapters/standalone/recipes.rs crates/banager-core/src/adapters/standalone/route.rs crates/banager-core/src/adapters/standalone/mod.rs crates/banager-core/src/adapters/standalone/removal.rs
git commit -m "$(cat <<'EOF'
Recognise a flat-file launcher under the Cargo home, bind the seat to its instance, and let a recipe hold extra locks

rustup's launcher is a regular file at $CARGO_HOME/bin/rustup, so a route
can now be a flat file -- one with no launcher-only state, since a
dangling link at that path is not the program half removed -- and a
recipe path can name the Cargo home, which detect expands with the same
rule cargo's own detect uses. Detect seats the Cargo home, the rustup
home and ZDOTDIR for the plans that need them, and a plan now checks the
seat describes the instance it was given, so a later detect under
another home cannot lend its locks and warnings to the wrong launcher. A
recipe can name locks its plans hold besides its own -- the cargo
instance's, for rustup -- and the upgrade plan holds them; claude holds
none.

Co-Authored-By: <the executing session's attribution line>
EOF
)"
```

---

### Task 5: `rustup.rs` — the roots gate, toolchains, cargo-installed programs, the Homebrew signal, the startup-file visit model, the warnings, the lock

**Files:**
- Create: `crates/banager-core/src/adapters/standalone/rustup.rs`
- Modify: `crates/banager-core/src/adapters/standalone/mod.rs` — the `pub mod` list (add `pub mod rustup;`); `testing` (`RUSTUP_PROXIES` moves out of it)
- Modify: `crates/banager-core/src/scan/mod.rs` — `fn display_path` → `pub(crate) fn display_path`, only if C did not already (C checklist row 8)  [F's file]
- Test: `rustup.rs`'s `mod tests`.

**Interfaces:**
- Consumes: `cargo::{instance_id_for, parse_crates2_bins}` (Task 1); `Warning`'s six new variants (Task 2); `Detected { home, euid, cargo_home, rustup_home, zdotdir }` (Task 4); `testing::{TempHome, detected, rustup_layout}`; `ResourceLock`, `UninstallBlocked`; `scan::display_path` (F's, `pub(crate)` from C).
- Produces (verbatim): `pub const SHELL_RC_CANDIDATES: [&str; 8]`; `pub const RUSTUP_PROXIES: [&str; 13]` (moved from `testing`; readers: `bin_programs_rustup_removes` and `testing::rustup_layout`); `pub const HOMEBREW_PREFIXES: [&str; 2]` (reader: `uninstall_warnings`); `pub struct StandardRoots { pub cargo_home, pub rustup_home }` and `pub fn standard_roots(d: &Detected) -> Option<StandardRoots>` (readers: `uninstall_blocked`, `warnings_with`); `pub fn uninstall_blocked(d: &Detected) -> Option<UninstallBlocked>` (reader: `RUSTUP`'s `CommandUninstall.blocked`, Task 6, called by `inventory` and `plan`); `pub fn toolchain_names(rustup_home: &Path) -> Vec<String>`; `pub fn bin_programs_rustup_removes(cargo_home: &Path) -> Vec<String>`; `pub fn homebrew_rustup_present(prefixes: &[PathBuf]) -> bool`; `pub fn cargo_home_str(home: &Path, cargo_home: &Path) -> String`; `pub struct RcVisit { pub file: PathBuf, pub line: String }` and `pub fn rustup_rc_visits(home: &Path, zdotdir: Option<&Path>, cargo_home_str: &str) -> Vec<RcVisit>`; `pub fn remove_first_exact_line(contents: &mut String, line: &str) -> bool`; `pub struct LeftoverPatterns { pub sourcing: Vec<String>, pub needles: Vec<String> }`, `pub fn leftover_patterns(home: &Path, cargo_home: &Path) -> LeftoverPatterns`, `pub enum Leftover { Sources, Mentions }`, `pub fn classify_leftover(contents: &str, patterns: &LeftoverPatterns) -> Option<Leftover>`; `pub fn shell_config_leftovers(home: &Path, zdotdir: Option<&Path>, cargo_home: &Path) -> Vec<Warning>` (each read by the one after it, down to `warnings_with`); `pub fn warnings_with(d: &Detected, homebrew_prefixes: &[PathBuf]) -> Vec<Warning>` and `pub fn uninstall_warnings(d: &Detected) -> Vec<Warning>` (reader: `RUSTUP`'s `CommandUninstall.warnings`, Task 6, called by `plan`); `pub fn extra_locks(d: &Detected) -> Vec<ResourceLock>` (reader: `RUSTUP.extra_locks`, Task 6, called by `StandaloneAdapter::locks`).
- **Declared deferral:** the whole module's production caller is the `RUSTUP` recipe, which Task 6 writes and `plan`/`inventory` then call; in this commit only the tests below call it. Everything is `pub` in a `pub mod`, so `-D warnings` raises no `dead_code` in between; the step (spec §十's unit) ships producer and reader together.

Every fact this file encodes is Rulings 1, 2, 4, 15, 16, 18, 21 and 22's, with rustup 1.29.1's and `home` 0.5.12's line numbers in the doc comments. Nothing here runs a command. The warnings come in spec §6.6's order: toolchains (with the rustup home's path), the Cargo home (by path), the programs in its `bin/` (when any), the Homebrew line (when its Cellar directory is there), the shell edit, then one line per startup file left behind, in `SHELL_RC_CANDIDATES` order.

- [ ] **Step 1: Write the failing tests**

Create `crates/banager-core/src/adapters/standalone/rustup.rs` with only the test module for now (Step 3 adds the functions above it):

```rust
#[cfg(test)]
mod tests {
    use super::super::testing::{detected, rustup_layout, TempHome};
    use super::*;
    use crate::model::UninstallBlocked;
    use std::path::PathBuf;

    fn rc_line() -> String {
        ". \"$HOME/.cargo/env\"".to_string()
    }

    #[test]
    fn test_standard_roots_accepts_only_the_default_layout_of_real_directories() {
        // Ruling 18: both roots as rustup computes them (`home` 0.5.12),
        // both exactly `<home>/.cargo` and `<home>/.rustup`, the Cargo
        // home a real directory, the rustup home a real directory or not
        // there yet. Anything else is a layout Banager will not offer to
        // delete: rustup's `uninstall()` removes `$RUSTUP_HOME` and
        // `$CARGO_HOME` whole (self_update.rs:960-966, :1029), wherever
        // they point.
        let home = TempHome::new("roots-default");
        let cargo_home = home.dir(".cargo");
        let rustup_home = home.dir(".rustup");
        let roots = standard_roots(&detected(home.path(), &cargo_home)).expect("the default layout");
        assert_eq!(roots.cargo_home, cargo_home);
        assert_eq!(roots.rustup_home, rustup_home);

        // No `~/.rustup` yet (rustup itself creates it on first run):
        // still the standard layout.
        let home = TempHome::new("roots-no-rustup-home");
        let cargo_home = home.dir(".cargo");
        assert!(standard_roots(&detected(home.path(), &cargo_home)).is_some());

        // Custom, absolute: not offered.
        let home = TempHome::new("roots-custom-cargo");
        let custom = home.dir("elsewhere/cargo");
        assert!(standard_roots(&detected(home.path(), &custom)).is_none());
        let home = TempHome::new("roots-custom-rustup");
        let cargo_home = home.dir(".cargo");
        let d = Detected {
            rustup_home: Some(home.dir("elsewhere/rustup")),
            ..detected(home.path(), &cargo_home)
        };
        assert!(standard_roots(&d).is_none());

        // Relative (unsupported, seated as `None`): not offered.
        let home = TempHome::new("roots-relative");
        let cargo_home = home.dir(".cargo");
        let d = Detected {
            cargo_home: None,
            ..detected(home.path(), &cargo_home)
        };
        assert!(standard_roots(&d).is_none());
        let d = Detected {
            rustup_home: None,
            ..detected(home.path(), &cargo_home)
        };
        assert!(standard_roots(&d).is_none());

        // A root that is a link: the path Banager would list is not the
        // directory that would go.
        let home = TempHome::new("roots-linked-cargo");
        let elsewhere = home.dir("Volumes/Data/cargo");
        home.link(".cargo", &elsewhere);
        assert!(standard_roots(&detected(home.path(), &home.path().join(".cargo"))).is_none());
        let home = TempHome::new("roots-linked-rustup");
        let cargo_home = home.dir(".cargo");
        let elsewhere = home.dir("Volumes/Data/rustup");
        home.link(".rustup", &elsewhere);
        assert!(standard_roots(&detected(home.path(), &cargo_home)).is_none());

        // No `~/.cargo` at all: nothing to offer.
        let home = TempHome::new("roots-no-cargo-home");
        assert!(standard_roots(&detected(home.path(), &home.path().join(".cargo"))).is_none());
    }

    #[test]
    fn test_uninstall_blocked_is_no_safe_method_for_anything_but_the_standard_layout() {
        let home = TempHome::new("blocked");
        let cargo_home = home.dir(".cargo");
        assert_eq!(uninstall_blocked(&detected(home.path(), &cargo_home)), None);
        let custom = home.dir("elsewhere/cargo");
        assert_eq!(
            uninstall_blocked(&detected(home.path(), &custom)),
            Some(UninstallBlocked::NoSafeMethod)
        );
    }

    #[test]
    fn test_toolchain_names_is_empty_for_no_directory_and_lists_entries_sorted() {
        // `uninstall()` removes each entry of `<rustup_home>/toolchains`
        // (`cfg.list_toolchains()`, self_update.rs:955-958) and then the
        // whole home, so the entry names are the toolchains that go. A
        // linked toolchain (`rustup toolchain link`) is an entry too;
        // `.DS_Store` is not a toolchain. None, or an unreadable
        // directory: no names, and the dialog says "every toolchain"
        // (ruling 15).
        let home = TempHome::new("toolchains-none");
        assert_eq!(toolchain_names(&home.path().join(".rustup")), Vec::<String>::new());
        home.dir(".rustup");
        assert_eq!(toolchain_names(&home.path().join(".rustup")), Vec::<String>::new());

        let home = TempHome::new("toolchains-some");
        let rustup_home = home.dir(".rustup");
        home.dir(".rustup/toolchains/stable-aarch64-apple-darwin");
        home.dir(".rustup/toolchains/nightly-2026-09-01-aarch64-apple-darwin");
        home.dir(".rustup/toolchains/1.90.0-aarch64-apple-darwin");
        home.file(".rustup/toolchains/.DS_Store");
        let linked = home.dir("src/my-toolchain");
        home.link(".rustup/toolchains/custom", &linked);
        assert_eq!(
            toolchain_names(&rustup_home),
            vec![
                "1.90.0-aarch64-apple-darwin".to_string(),
                "custom".to_string(),
                "nightly-2026-09-01-aarch64-apple-darwin".to_string(),
                "stable-aarch64-apple-darwin".to_string(),
            ]
        );
    }

    #[test]
    fn test_rustup_proxies_are_the_thirteen_names_rustup_keeps() {
        // `TOOLS` (10) + `DUP_TOOLS` (3), rustup 1.29.1 `src/lib.rs:16-32`:
        // the names `uninstall()` spares in `bin/` besides `rustup`
        // itself (self_update.rs:996-1022). Sorted and unique, so a
        // missing or doubled name shows here, not as a program the
        // preview wrongly names.
        assert_eq!(RUSTUP_PROXIES.len(), 13);
        let mut sorted = RUSTUP_PROXIES.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted, RUSTUP_PROXIES.to_vec());
        for name in ["cargo", "rustc", "rustdoc", "rustfmt", "cargo-fmt", "rust-analyzer"] {
            assert!(RUSTUP_PROXIES.contains(&name), "{name}");
        }
        assert!(!RUSTUP_PROXIES.contains(&"rustup"));
    }

    #[test]
    fn test_bin_programs_rustup_removes_lists_the_recorded_programs_and_not_rustups_own() {
        // This Mac's layout: rustup, its thirteen proxies, and `hexyl`,
        // which the recorded `.crates2.json` lists.
        let home = TempHome::new("rustup-bins");
        let cargo_home = home.dir(".cargo");
        rustup_layout(&cargo_home);
        std::fs::write(cargo_home.join("bin/hexyl"), b"x").expect("write hexyl");
        std::fs::copy(
            "../../adapters/fixtures/cargo/1.98.1/crates2.json",
            cargo_home.join(".crates2.json"),
        )
        .expect("copy the recorded record");
        assert_eq!(bin_programs_rustup_removes(&cargo_home), vec!["hexyl".to_string()]);
    }

    #[test]
    fn test_bin_programs_rustup_removes_names_a_program_no_record_lists() {
        // rustup 1.29.1 deletes every entry of `bin/` whose *name* is not
        // one of its fourteen (self_update.rs:996-1022): a program copied
        // there by hand goes too, recorded or not, and with no record or
        // a broken one the listing still names it. `.DS_Store` is deleted
        // with the folder but is no program to name.
        let home = TempHome::new("rustup-bins-unrecorded");
        let cargo_home = home.dir(".cargo");
        rustup_layout(&cargo_home);
        std::fs::write(cargo_home.join("bin/mytool"), b"x").expect("write mytool");
        std::fs::write(cargo_home.join("bin/.DS_Store"), b"x").expect("write .DS_Store");
        assert_eq!(bin_programs_rustup_removes(&cargo_home), vec!["mytool".to_string()]);
        std::fs::write(cargo_home.join(".crates2.json"), "{ not json").expect("write");
        assert_eq!(bin_programs_rustup_removes(&cargo_home), vec!["mytool".to_string()]);
    }

    #[test]
    fn test_bin_programs_rustup_removes_flattens_sorts_and_dedups_the_records_binaries() {
        // A crate's binaries by their file names (`rg`, not `ripgrep`),
        // several per crate, united with the listing and each named once.
        let home = TempHome::new("rustup-bins-many");
        let cargo_home = home.dir(".cargo");
        rustup_layout(&cargo_home);
        for bin in ["hexyl", "rg"] {
            std::fs::write(cargo_home.join("bin").join(bin), b"x").expect("write bin");
        }
        std::fs::write(
            cargo_home.join(".crates2.json"),
            r#"{"installs":{
                "ripgrep 15.1.0 (registry+https://github.com/rust-lang/crates.io-index)":{"bins":["rg"]},
                "cargo-binstall 1.16.0 (registry+https://github.com/rust-lang/crates.io-index)":{"bins":["detect-targets","cargo-binstall"]},
                "hexyl 0.17.0 (registry+https://github.com/rust-lang/crates.io-index)":{"bins":["hexyl"]}
            }}"#,
        )
        .expect("write record");
        assert_eq!(
            bin_programs_rustup_removes(&cargo_home),
            vec![
                "cargo-binstall".to_string(),
                "detect-targets".to_string(),
                "hexyl".to_string(),
                "rg".to_string(),
            ]
        );
    }

    #[test]
    fn test_bin_programs_rustup_removes_is_empty_for_no_directory_and_no_record() {
        // Nothing to list and nothing recorded (or a broken record): no
        // names, so no `RemovesCargoInstalled` line -- never a refused
        // preview; the Cargo-folder sentence is always there.
        let home = TempHome::new("rustup-bins-none");
        let cargo_home = home.dir(".cargo");
        assert_eq!(bin_programs_rustup_removes(&cargo_home), Vec::<String>::new());
        std::fs::write(cargo_home.join(".crates2.json"), "{ not json").expect("write");
        assert_eq!(bin_programs_rustup_removes(&cargo_home), Vec::<String>::new());
        // rustup and its proxies alone: nothing else to name.
        rustup_layout(&cargo_home);
        assert_eq!(bin_programs_rustup_removes(&cargo_home), Vec::<String>::new());
    }

    #[test]
    fn test_homebrew_rustup_present_looks_for_the_formulas_cellar_directory() {
        // Homebrew's keg-only `rustup` formula lives under
        // `<prefix>/Cellar/rustup/<version>/`; that directory existing is
        // the one read-only, local sign of it (ruling 21). The real
        // prefixes are `HOMEBREW_PREFIXES`; the tests hand in their own.
        let home = TempHome::new("brew-present");
        let prefix = home.dir("opt/homebrew");
        assert!(!homebrew_rustup_present(&[prefix.clone()]));
        home.dir("opt/homebrew/Cellar/rustup/1.29.1/bin");
        assert!(homebrew_rustup_present(&[prefix.clone()]));
        assert!(homebrew_rustup_present(&[home.path().join("usr/local"), prefix]));
        assert!(!homebrew_rustup_present(&[]));
        assert_eq!(HOMEBREW_PREFIXES, ["/opt/homebrew", "/usr/local"]);
    }

    #[test]
    fn test_cargo_home_str_spells_the_default_home_as_rustup_writes_it() {
        // rustup 1.29.1 `cargo_home_str_with_home`
        // (src/cli/self_update/shell.rs:43-58): `$HOME/.cargo` when the
        // Cargo home is the default, so that is the text in the line it
        // wrote and the line it looks for.
        assert_eq!(
            cargo_home_str(Path::new("/Users/someone"), Path::new("/Users/someone/.cargo")),
            "$HOME/.cargo"
        );
    }

    #[test]
    fn test_cargo_home_str_spells_a_custom_home_absolutely() {
        assert_eq!(
            cargo_home_str(Path::new("/Users/someone"), Path::new("/Volumes/Data/cargo")),
            "/Volumes/Data/cargo"
        );
    }

    #[test]
    fn test_rustup_rc_visits_follow_do_remove_from_path_then_remove_legacy_paths() {
        // Ruling 2: `do_remove_from_path` (unix.rs:55-77) over the shells
        // in `enumerate_shells` order (shell.rs:63-74) -- Posix `.profile`
        // (:163-168), Bash `.bash_profile`/`.bash_login`/`.bashrc`
        // (:188-195), Zsh `~/.zshenv` (:240-245; `$ZDOTDIR/.zshenv` first
        // when there is one) -- each visit removing the current line; then
        // `remove_legacy_paths` (unix.rs:174-194): the pre-1.23 PATH line
        // and then the `source` line, each over `legacy_paths`
        // (shell.rs:564-574: `.bash_profile`, `.profile`,
        // `$ZDOTDIR/.zprofile`, `~/.zprofile`). Fish, Nu, Tcsh, Pwsh and
        // Xonsh edit files Banager does not read, so they have no visit
        // here. `.zshrc` and fish's `config.fish` are visited by nothing.
        let home = Path::new("/Users/someone");
        let current = rc_line();
        let legacy_path = "export PATH=\"$HOME/.cargo/bin:$PATH\"".to_string();
        let legacy_source = "source \"$HOME/.cargo/env\"".to_string();
        let visits = rustup_rc_visits(home, None, "$HOME/.cargo");
        let as_pairs: Vec<(String, String)> = visits
            .iter()
            .map(|v| (v.file.strip_prefix(home).unwrap().display().to_string(), v.line.clone()))
            .collect();
        assert_eq!(
            as_pairs,
            vec![
                (".profile".to_string(), current.clone()),
                (".bash_profile".to_string(), current.clone()),
                (".bash_login".to_string(), current.clone()),
                (".bashrc".to_string(), current.clone()),
                (".zshenv".to_string(), current.clone()),
                (".bash_profile".to_string(), legacy_path.clone()),
                (".profile".to_string(), legacy_path.clone()),
                (".zprofile".to_string(), legacy_path.clone()),
                (".bash_profile".to_string(), legacy_source.clone()),
                (".profile".to_string(), legacy_source.clone()),
                (".zprofile".to_string(), legacy_source.clone()),
            ]
        );
        assert!(visits.iter().all(|v| !v.file.ends_with(".zshrc")));
        // A custom Cargo home is spelled absolutely in every line.
        let visits = rustup_rc_visits(home, None, "/Volumes/Data/cargo");
        assert_eq!(visits[0].line, ". \"/Volumes/Data/cargo/env\"");
        assert_eq!(visits[5].line, "export PATH=\"/Volumes/Data/cargo/bin:$PATH\"");
        assert_eq!(visits[8].line, "source \"/Volumes/Data/cargo/env\"");
    }

    #[test]
    fn test_rustup_rc_visits_visit_zshenv_twice_when_zdotdir_is_home() {
        // Zsh's `rcfiles()` is `[$ZDOTDIR/.zshenv, ~/.zshenv]` with no
        // deduplication (shell.rs:240-245), and `legacy_paths` chains
        // `$ZDOTDIR/.zprofile` before `~/.zprofile` (shell.rs:564-574):
        // with `ZDOTDIR=$HOME` the same file is visited twice per line,
        // and each visit removes one exact copy. Another ZDOTDIR is a
        // file Banager does not read: the visit is there, and
        // `shell_config_leftovers` has no contents for it. An empty
        // ZDOTDIR is no ZDOTDIR (shell.rs:213).
        let home = Path::new("/Users/someone");
        let visits = rustup_rc_visits(home, Some(home), "$HOME/.cargo");
        let zshenv: Vec<_> = visits.iter().filter(|v| v.file == home.join(".zshenv")).collect();
        assert_eq!(zshenv.len(), 2);
        let zprofile: Vec<_> = visits.iter().filter(|v| v.file == home.join(".zprofile")).collect();
        assert_eq!(zprofile.len(), 4, "two legacy lines, two visits each");

        let elsewhere = Path::new("/Users/someone/.config/zsh");
        let visits = rustup_rc_visits(home, Some(elsewhere), "$HOME/.cargo");
        assert_eq!(visits.iter().filter(|v| v.file == home.join(".zshenv")).count(), 1);
        assert_eq!(visits.iter().filter(|v| v.file == elsewhere.join(".zshenv")).count(), 1);
        assert_eq!(visits.iter().filter(|v| v.file == elsewhere.join(".zprofile")).count(), 2);

        let visits = rustup_rc_visits(home, Some(Path::new("")), "$HOME/.cargo");
        assert_eq!(visits.iter().filter(|v| v.file == home.join(".zshenv")).count(), 1);
    }

    #[test]
    fn test_remove_first_exact_line_is_find_exact_line() {
        // `find_exact_line` (unix.rs:164-172): the line *with* its
        // newline, at a line start, byte for byte, first match only.
        let line = rc_line();
        let mut s = format!("{line}\n");
        assert!(remove_first_exact_line(&mut s, &line));
        assert_eq!(s, "");
        // Two copies: one goes per call.
        let mut s = format!("{line}\n{line}\n");
        assert!(remove_first_exact_line(&mut s, &line));
        assert_eq!(s, format!("{line}\n"));
        // Trailing whitespace is not the exact line.
        let mut s = format!("{line} \n");
        assert!(!remove_first_exact_line(&mut s, &line));
        // The same text as the last line with no newline after it stays.
        let mut s = format!("export A=1\n{line}");
        assert!(!remove_first_exact_line(&mut s, &line));
        // Not at a line start: stays.
        let mut s = format!("x {line}\n");
        assert!(!remove_first_exact_line(&mut s, &line));
        // At the start of a later line: goes, the rest intact.
        let mut s = format!("export A=1\n{line}\nexport B=2\n");
        assert!(remove_first_exact_line(&mut s, &line));
        assert_eq!(s, "export A=1\nexport B=2\n");
    }

    #[test]
    fn test_leftover_patterns_name_rustups_sourcing_forms_and_the_env_files_spellings() {
        // The certain tier is only a form rustup itself writes whose target
        // is this Cargo home (ruling 22); the needles are how the env file
        // may be mentioned. With a custom home, `$HOME/.cargo/env` is
        // neither: that file survives this uninstall.
        let home = Path::new("/Users/someone");
        let p = leftover_patterns(home, Path::new("/Users/someone/.cargo"));
        for form in [
            ". \"$HOME/.cargo/env\"",
            "source \"$HOME/.cargo/env\"",
            "source \"$HOME/.cargo/env.fish\"",
            ". \"/Users/someone/.cargo/env\"",
            "source \"/Users/someone/.cargo/env\"",
        ] {
            assert!(p.sourcing.iter().any(|s| s == form), "{form}");
        }
        assert!(p.needles.contains(&".cargo/env".to_string()));
        assert!(p.needles.contains(&"$CARGO_HOME/env".to_string()));
        assert!(p.needles.contains(&"${CARGO_HOME}/env".to_string()));

        let p = leftover_patterns(home, Path::new("/Volumes/Data/cargo"));
        assert!(p.sourcing.contains(&". \"/Volumes/Data/cargo/env\"".to_string()));
        assert!(!p.sourcing.iter().any(|s| s.contains("$HOME")));
        assert!(p.needles.contains(&"/Volumes/Data/cargo/env".to_string()));
        assert!(!p.needles.contains(&".cargo/env".to_string()));
    }

    #[test]
    fn test_classify_leftover_puts_rustups_own_forms_in_the_certain_tier_and_the_rest_in_the_qualified_one() {
        // Astra's counterexamples (finding 7), each decided on its own.
        let home = Path::new("/Users/someone");
        let p = leftover_patterns(home, Path::new("/Users/someone/.cargo"));
        // rustup's own line, left behind: will error.
        assert_eq!(classify_leftover(". \"$HOME/.cargo/env\"\n", &p), Some(Leftover::Sources));
        assert_eq!(classify_leftover("  source \"$HOME/.cargo/env\"  \n", &p), Some(Leftover::Sources));
        assert_eq!(
            classify_leftover("export A=1\n. \"$HOME/.cargo/env\"", &p),
            Some(Leftover::Sources),
            "last line, no newline: rustup leaves it, the shell runs it"
        );
        // A comment: nothing.
        assert_eq!(classify_leftover("# . \"$HOME/.cargo/env\"\n", &p), None);
        // An echo, a guard, another spelling, a variable: may.
        assert_eq!(
            classify_leftover("echo \"run . $HOME/.cargo/env\"\n", &p),
            Some(Leftover::Mentions)
        );
        assert_eq!(
            classify_leftover("[ -f \"$HOME/.cargo/env\" ] && . \"$HOME/.cargo/env\"\n", &p),
            Some(Leftover::Mentions)
        );
        assert_eq!(classify_leftover("source ~/.cargo/env\n", &p), Some(Leftover::Mentions));
        assert_eq!(classify_leftover(". \"$CARGO_HOME/env\"\n", &p), Some(Leftover::Mentions));
        // Certain beats qualified within one file.
        assert_eq!(
            classify_leftover("source ~/.cargo/env\n. \"$HOME/.cargo/env\"\n", &p),
            Some(Leftover::Sources)
        );
        // Lines about something else: nothing.
        assert_eq!(classify_leftover("export PATH=\"$HOME/.local/bin:$PATH\"\n", &p), None);
        assert_eq!(classify_leftover("export PATH=\"$HOME/.cargo/bin:$PATH\"\n", &p), None);
        assert_eq!(classify_leftover("", &p), None);

        // A custom Cargo home: the default's env file is not this
        // uninstall's business.
        let p = leftover_patterns(home, Path::new("/Volumes/Data/cargo"));
        assert_eq!(classify_leftover(". \"$HOME/.cargo/env\"\n", &p), None);
        assert_eq!(classify_leftover("source ~/.cargo/env\n", &p), None);
        assert_eq!(classify_leftover(". \"/Volumes/Data/cargo/env\"\n", &p), Some(Leftover::Sources));
        assert_eq!(
            classify_leftover("[ -f /Volumes/Data/cargo/env ] && . /Volumes/Data/cargo/env\n", &p),
            Some(Leftover::Mentions)
        );
    }

    #[test]
    fn test_shell_config_leftovers_reports_the_file_rustup_does_not_edit() {
        // This Mac (spec §6.4, re-checked 2026-09-25): `~/.zshenv:1` and
        // `~/.profile:1` hold rustup's line, `~/.zshrc:17` holds the same
        // line but rustup never edits `.zshrc` -- after the uninstall
        // every new zsh prints `no such file or directory: …/.cargo/env`.
        let home = TempHome::new("rustup-rc-zshrc");
        for rc in [".zshenv", ".profile", ".zshrc"] {
            std::fs::write(home.path().join(rc), format!("{}\n", rc_line())).expect("write rc");
        }
        assert_eq!(
            shell_config_leftovers(home.path(), None, &home.path().join(".cargo")),
            vec![Warning::LeavesShellConfigLine {
                path: "~/.zshrc".to_string(),
                certain: true
            }]
        );
    }

    #[test]
    fn test_shell_config_leftovers_is_silent_when_only_rustups_own_lines_exist() {
        let home = TempHome::new("rustup-rc-clean");
        std::fs::write(home.path().join(".zshenv"), format!("{}\n", rc_line())).expect("write");
        std::fs::write(
            home.path().join(".zprofile"),
            "source \"$HOME/.cargo/env\"\nexport PATH=\"$HOME/.cargo/bin:$PATH\"\n",
        )
        .expect("write the two pre-1.23 lines, which rustup also removes");
        std::fs::write(home.path().join(".bash_profile"), format!("{}\n", rc_line())).expect("write");
        assert!(shell_config_leftovers(home.path(), None, &home.path().join(".cargo")).is_empty());
        // A comment about the env file is not a line that loads it.
        std::fs::write(home.path().join(".zshrc"), "# added by rustup: . \"$HOME/.cargo/env\"\n")
            .expect("write");
        assert!(shell_config_leftovers(home.path(), None, &home.path().join(".cargo")).is_empty());
        // And with no startup files at all.
        let home = TempHome::new("rustup-rc-none");
        assert!(shell_config_leftovers(home.path(), None, &home.path().join(".cargo")).is_empty());
    }

    #[test]
    fn test_shell_config_leftovers_qualifies_a_mention_rustup_will_not_remove() {
        // A hand-written spelling beside rustup's line in a file it edits:
        // rustup removes its own, the other stays, and whether it errors
        // depends on what it is -- so "may", not "will" (ruling 22). fish's
        // own config with a `source` of `env.fish`: rustup's fish file is
        // `conf.d/rustup.fish`, never `config.fish`, and that form is one
        // rustup writes, so "will".
        let home = TempHome::new("rustup-rc-handwritten");
        std::fs::write(
            home.path().join(".zshenv"),
            format!("{}\nsource ~/.cargo/env\n", rc_line()),
        )
        .expect("write");
        std::fs::write(
            home.path().join(".bashrc"),
            "[ -f \"$HOME/.cargo/env\" ] && . \"$HOME/.cargo/env\"\n",
        )
        .expect("write");
        home.file(".config/fish/config.fish");
        std::fs::write(
            home.path().join(".config/fish/config.fish"),
            "source \"$HOME/.cargo/env.fish\"\n",
        )
        .expect("write");
        assert_eq!(
            shell_config_leftovers(home.path(), None, &home.path().join(".cargo")),
            vec![
                Warning::LeavesShellConfigLine {
                    path: "~/.zshenv".to_string(),
                    certain: false
                },
                Warning::LeavesShellConfigLine {
                    path: "~/.bashrc".to_string(),
                    certain: false
                },
                Warning::LeavesShellConfigLine {
                    path: "~/.config/fish/config.fish".to_string(),
                    certain: true
                },
            ]
        );
    }

    #[test]
    fn test_shell_config_leftovers_removes_two_copies_when_zdotdir_is_home_and_one_otherwise() {
        // Review Focus #2. Two copies of rustup's line in `~/.zshenv`:
        // with `ZDOTDIR=$HOME` rustup visits the file twice and both go;
        // with no ZDOTDIR one stays, and it is rustup's own form, so it
        // *will* error. The same for the legacy `source` line in
        // `~/.zprofile`, which `legacy_paths` visits under `$ZDOTDIR` and
        // under `~`. Its line last with no newline stays either way.
        let home = TempHome::new("rustup-rc-zdotdir");
        let two = format!("{}\n{}\n", rc_line(), rc_line());
        std::fs::write(home.path().join(".zshenv"), &two).expect("write");
        std::fs::write(
            home.path().join(".zprofile"),
            "source \"$HOME/.cargo/env\"\nsource \"$HOME/.cargo/env\"\n",
        )
        .expect("write");
        assert!(
            shell_config_leftovers(home.path(), Some(home.path()), &home.path().join(".cargo")).is_empty()
        );
        assert_eq!(
            shell_config_leftovers(home.path(), None, &home.path().join(".cargo")),
            vec![
                Warning::LeavesShellConfigLine {
                    path: "~/.zshenv".to_string(),
                    certain: true
                },
                Warning::LeavesShellConfigLine {
                    path: "~/.zprofile".to_string(),
                    certain: true
                },
            ]
        );
        std::fs::write(home.path().join(".zshenv"), format!("export A=1\n{}", rc_line())).expect("write");
        std::fs::remove_file(home.path().join(".zprofile")).expect("remove");
        assert_eq!(
            shell_config_leftovers(home.path(), Some(home.path()), &home.path().join(".cargo")),
            vec![Warning::LeavesShellConfigLine {
                path: "~/.zshenv".to_string(),
                certain: true
            }]
        );
    }

    #[test]
    fn test_shell_config_leftovers_follows_a_custom_cargo_home() {
        // With CARGO_HOME set, rustup wrote and looks for the absolute
        // path; a line spelled through $HOME/.cargo from an earlier
        // default install is about a file this uninstall does not touch,
        // so it is not reported at all (Astra's counterexample). The
        // gate keeps a custom home from ever reaching a preview; the
        // function is right on its own regardless.
        let home = TempHome::new("rustup-rc-custom");
        let cargo_home = home.dir("elsewhere/cargo");
        let line = format!(". \"{}/env\"\n", cargo_home.display());
        std::fs::write(home.path().join(".zshenv"), &line).expect("write");
        assert!(shell_config_leftovers(home.path(), None, &cargo_home).is_empty());
        std::fs::write(home.path().join(".profile"), format!("{}\n", rc_line())).expect("write");
        assert!(shell_config_leftovers(home.path(), None, &cargo_home).is_empty());
        std::fs::write(home.path().join(".zshrc"), &line).expect("write");
        assert_eq!(
            shell_config_leftovers(home.path(), None, &cargo_home),
            vec![Warning::LeavesShellConfigLine {
                path: "~/.zshrc".to_string(),
                certain: true
            }]
        );
    }

    #[test]
    fn test_warnings_come_in_the_dialogs_order_with_the_conditional_ones_only_when_true() {
        // Spec §6.6's rustup dialog, on this Mac's layout: toolchains (with
        // the rustup home's path), the Cargo home, hexyl, the shell edit,
        // ~/.zshrc -- and, when Homebrew's Cellar has a rustup, its line
        // before the shell edit.
        let home = TempHome::new("rustup-warnings-full");
        let cargo_home = home.dir(".cargo");
        rustup_layout(&cargo_home);
        std::fs::write(cargo_home.join("bin/hexyl"), b"x").expect("write hexyl");
        std::fs::copy(
            "../../adapters/fixtures/cargo/1.98.1/crates2.json",
            cargo_home.join(".crates2.json"),
        )
        .expect("copy");
        home.dir(".rustup/toolchains/stable-aarch64-apple-darwin");
        std::fs::write(home.path().join(".zshrc"), format!("{}\n", rc_line())).expect("write");
        let d = detected(home.path(), &cargo_home);
        let expected = vec![
            Warning::RemovesToolchains {
                path: "~/.rustup".to_string(),
                names: vec!["stable-aarch64-apple-darwin".to_string()],
            },
            Warning::DeletesCargoHome {
                path: "~/.cargo".to_string(),
            },
            Warning::RemovesCargoInstalled {
                names: vec!["hexyl".to_string()],
            },
            Warning::EditsShellConfig,
            Warning::LeavesShellConfigLine {
                path: "~/.zshrc".to_string(),
                certain: true,
            },
        ];
        assert_eq!(warnings_with(&d, &[]), expected);
        let brew = home.dir("opt/homebrew");
        home.dir("opt/homebrew/Cellar/rustup/1.29.1");
        let mut with_brew = expected.clone();
        with_brew.insert(3, Warning::HomebrewRustupLosesToolchains);
        assert_eq!(warnings_with(&d, &[brew]), with_brew);

        // No toolchains directory, nothing cargo-installed, no startup
        // files: the three unconditional lines, the toolchain one without
        // names.
        let home = TempHome::new("rustup-warnings-bare");
        let cargo_home = home.dir(".cargo");
        let d = detected(home.path(), &cargo_home);
        assert_eq!(
            warnings_with(&d, &[]),
            vec![
                Warning::RemovesToolchains {
                    path: "~/.rustup".to_string(),
                    names: Vec::new()
                },
                Warning::DeletesCargoHome {
                    path: "~/.cargo".to_string()
                },
                Warning::EditsShellConfig,
            ]
        );
        // `uninstall_warnings` is the same function over the real
        // prefixes; it is exercised through `plan` in Task 6 (with the
        // Homebrew line filtered, since that depends on the test Mac).
        assert_eq!(uninstall_warnings(&d).len(), warnings_with(&d, &[]).len().max(3));
    }

    #[test]
    fn test_warnings_are_empty_for_a_layout_the_gate_refuses() {
        // `plan` refuses before it asks; this is the function's own
        // answer for a layout it will not describe.
        let home = TempHome::new("rustup-warnings-refused");
        let custom = home.dir("elsewhere/cargo");
        assert!(warnings_with(&detected(home.path(), &custom), &[]).is_empty());
    }

    #[test]
    fn test_extra_locks_is_the_cargo_instances_lock_spelled_by_its_one_producer() {
        let d = detected(Path::new("/Users/someone"), Path::new("/Users/someone/.cargo"));
        assert_eq!(
            extra_locks(&d),
            vec![ResourceLock(crate::adapters::cargo::instance_id_for(Path::new(
                "/Users/someone/.cargo"
            )))]
        );
        assert_eq!(extra_locks(&d), vec![ResourceLock("cargo:/Users/someone/.cargo".to_string())]);
        // No usable Cargo home (a relative CARGO_HOME): no cargo instance
        // exists to lock -- and no rustup instance either, since its
        // launcher is under that home; the function still answers.
        let d = Detected {
            cargo_home: None,
            ..d
        };
        assert!(extra_locks(&d).is_empty());
    }
}
```

In `crates/banager-core/src/adapters/standalone/mod.rs`, in the `pub mod` list, add `pub mod rustup;` (alphabetically, after `pub mod route;`; C's `pub mod removal;` sits before it).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib adapters::standalone::rustup`
Expected: FAIL to compile — `cannot find function \`standard_roots\`` (and the other functions), `cannot find value \`SHELL_RC_CANDIDATES\``, `\`RUSTUP_PROXIES\`` (the one in `testing` is not `rustup.rs`'s) and `\`HOMEBREW_PREFIXES\``; `cannot find type \`Leftover\``, `\`Detected\``; `Warning`, `ResourceLock`, `Path` unresolved until Step 3's imports exist.

- [ ] **Step 3: Write the module**

Prepend to `crates/banager-core/src/adapters/standalone/rustup.rs` (above `#[cfg(test)]`):

```rust
//! What `rustup self uninstall` does, when Banager may offer it, and what
//! to tell the user before it runs (phase 4 spec §6.4): the facts here
//! were read from rustup's source at the tag the installed binary was
//! built from -- `1.29.1`, commit d95a37b6, the `d95a37b6a` in `rustup
//! --version` on the recording Mac -- and every line number below is that
//! tag's (`src/cli/self_update.rs`, `src/cli/self_update/unix.rs`,
//! `src/cli/self_update/shell.rs`), or `home` 0.5.12's
//! (`crates/home/src/env.rs`), the crate rustup reads its homes through.
//! Newer rustup keeps the programs `cargo install` installed
//! (`clean_cargo_home` on master); 1.29.1 does not, and this recipe is
//! verified against 1.29.1.
//!
//! `uninstall()` (self_update.rs:924-1032): removes every toolchain
//! (:955-958, the entries of `$RUSTUP_HOME/toolchains`), then
//! `$RUSTUP_HOME` (:960-966), then -- unless `--no-modify-path`, which
//! Banager does not pass -- the line it added to the shell startup files
//! (:971-973, `do_remove_from_path`), then everything in `$CARGO_HOME`
//! except `bin/` (:977-993), then everything in `bin/` that is not one of
//! its own proxies or `rustup` itself (:996-1022), and finally the whole
//! `$CARGO_HOME` directory (`delete_rustup_and_cargo_home`, :1029;
//! unix.rs:50-53). Both homes come from `RUSTUP_HOME`/`CARGO_HOME` or
//! default under `HOME` (env.rs:67-79, :101-113) -- wherever they point,
//! and permanently: nothing here goes to the Trash. So Banager offers the
//! command only for the standard layout (`standard_roots`, ruling 18) and
//! the preview names both folders by path.
//!
//! Nothing here runs a command or writes a file: `plan` hands in what
//! `detect` seated, and this module lists `<rustup_home>/toolchains` and
//! `<cargo_home>/bin` by name, reads `.crates2.json`, looks for
//! Homebrew's `Cellar/rustup`, reads eight startup files under the home,
//! replays rustup's own cleanup on copies of them in memory, and answers
//! with `Warning`s.

use super::Detected;
use crate::adapters::cargo::{instance_id_for, parse_crates2_bins};
use crate::model::{ResourceLock, UninstallBlocked, Warning};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The startup files Banager reads (never writes) for a line about the
/// Cargo env file, home-relative, in the order they are reported (spec
/// §6.4). `$ZDOTDIR/.zshenv` and `$ZDOTDIR/.zprofile` are read only when
/// `ZDOTDIR` is the home (then they are these files); a zsh whose files
/// live elsewhere is not checked, and the trust file says so.
pub const SHELL_RC_CANDIDATES: [&str; 8] = [
    ".zshenv",
    ".zprofile",
    ".zshrc",
    ".bash_profile",
    ".bash_login",
    ".bashrc",
    ".profile",
    ".config/fish/config.fish",
];

/// The names rustup 1.29.1's `uninstall()` spares in `<cargo_home>/bin`
/// besides `rustup` itself: its proxies, `TOOLS` + `DUP_TOOLS`
/// (`src/lib.rs:16-32`), which it compares by name
/// (self_update.rs:996-1022). Sorted. On this Mac all thirteen are
/// relative links to `rustup` (unknown-scan.md §2). Read by
/// `bin_programs_rustup_removes`, and by `testing::rustup_layout`, which
/// builds the same layout for the tests.
pub const RUSTUP_PROXIES: [&str; 13] = [
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

/// Homebrew's two default prefixes (Apple Silicon, Intel), where a
/// `Cellar/rustup` directory means the `rustup` formula is installed. A
/// custom prefix is unsupported by Homebrew itself on Apple Silicon and
/// is not looked for. Read by `uninstall_warnings`.
pub const HOMEBREW_PREFIXES: [&str; 2] = ["/opt/homebrew", "/usr/local"];

/// The two folders `rustup self uninstall` deletes, when they are the
/// standard ones (`standard_roots`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandardRoots {
    pub cargo_home: PathBuf,
    pub rustup_home: PathBuf,
}

fn is_real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_dir())
        .unwrap_or(false)
}

/// The gate (ruling 18): `Some` only when the Rust this instance belongs
/// to lives where a fresh `rustup-init` puts it. Both homes as rustup
/// computes them (`home` 0.5.12 over `RUSTUP_HOME`/`CARGO_HOME`/`HOME`,
/// seated by `detect`; `None` is a relative value, unsupported); each
/// exactly `<home>/.cargo` and `<home>/.rustup` -- compared lexically,
/// as rustup itself compares when it decides how to spell the Cargo home
/// in the shell line (`cargo_home_str_with_home`, shell.rs:43-58), over
/// the same `HOME`; the Cargo home a directory that is not a link; the
/// rustup home a directory that is not a link, or not there yet (rustup
/// creates it on its first run). Anything else -- a custom home, a link
/// to somewhere else, a relative variable -- and `uninstall()` would
/// `remove_dir` a place this preview did not name: not offered.
pub fn standard_roots(d: &Detected) -> Option<StandardRoots> {
    let cargo_home = d.cargo_home.as_deref()?;
    let rustup_home = d.rustup_home.as_deref()?;
    if !d.home.is_absolute()
        || cargo_home != d.home.join(".cargo")
        || rustup_home != d.home.join(".rustup")
    {
        return None;
    }
    if !is_real_dir(cargo_home) {
        return None;
    }
    match std::fs::symlink_metadata(rustup_home) {
        Ok(meta) if meta.file_type().is_dir() => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => return None,
    }
    Some(StandardRoots {
        cargo_home: cargo_home.to_path_buf(),
        rustup_home: rustup_home.to_path_buf(),
    })
}

/// `CommandUninstall.blocked` of the `RUSTUP` recipe: `NoSafeMethod` for
/// any layout but the standard one. The variant is the one the gate
/// (`session/plans.rs`) and the Installed page already refuse and hide
/// the button for; rustup's row says why in its own sentence
/// (`installed.blocked.NoSafeMethod.standalone-rustup`, src/lib/sources.ts).
pub fn uninstall_blocked(d: &Detected) -> Option<UninstallBlocked> {
    standard_roots(d)
        .is_none()
        .then_some(UninstallBlocked::NoSafeMethod)
}

/// Every installed toolchain, by name: the entries of
/// `<rustup_home>/toolchains`, sorted, hidden names skipped. That
/// directory is what `uninstall()` removes toolchain by toolchain
/// (`cfg.list_toolchains()`, self_update.rs:955-958) before it deletes
/// the home whole, so its names are the toolchains that go; a linked
/// toolchain (`rustup toolchain link`) is an entry like any other. No
/// directory, or an unreadable one: no names, and the dialog says "every
/// toolchain" (ruling 15). Nothing is run.
pub fn toolchain_names(rustup_home: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(rustup_home.join("toolchains"))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
                .filter(|name| !name.starts_with('.'))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The programs in `<cargo_home>/bin` that rustup 1.29.1's `self
/// uninstall` deletes, sorted and named once: every entry whose name is
/// not `rustup` or one of `RUSTUP_PROXIES` (self_update.rs:996-1022
/// compares names only, so a program copied there by hand goes too),
/// read with `read_dir` -- nothing is opened or run -- united with the
/// binaries `.crates2.json` records (every crate's `bins`, through
/// `parse_crates2_bins`, the parser cargo's own inventory uses). The
/// listing is what rustup acts on; the record still names what cargo
/// installed when the directory cannot be listed, and a record entry
/// whose file is already gone is named although nothing is left to
/// delete -- the safe direction. Not named: an entry starting with `.`
/// (`.DS_Store`: deleted with the folder, but no program), and a name
/// that is not UTF-8 -- deleted with the folder too (`remove_dir`,
/// :1029, takes everything), but not spellable in a sentence. No
/// directory, an unreadable one, no record or a broken one each add
/// nothing: a name is better missing than invented, and
/// `DeletesCargoHome` always says the whole folder goes.
pub fn bin_programs_rustup_removes(cargo_home: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(cargo_home.join("bin"))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if let Ok(json) = std::fs::read_to_string(cargo_home.join(".crates2.json")) {
        names.extend(
            parse_crates2_bins(&json)
                .unwrap_or_default()
                .into_iter()
                .flat_map(|(_, bins)| bins),
        );
    }
    names.retain(|name| {
        !name.starts_with('.') && name != "rustup" && !RUSTUP_PROXIES.contains(&name.as_str())
    });
    names.sort();
    names.dedup();
    names
}

/// Whether Homebrew's `rustup` formula is installed: `<prefix>/Cellar/rustup`
/// exists under one of `prefixes` (`HOMEBREW_PREFIXES` in production).
/// rustup's homes depend only on `RUSTUP_HOME`/`CARGO_HOME`/`HOME`, never
/// on where the binary sits (`home::rustup_home_with_cwd_env`,
/// env.rs:101-113), so that rustup shares `~/.rustup` with the native
/// one and loses its toolchains when it goes (ruling 21). Read-only.
pub fn homebrew_rustup_present(prefixes: &[PathBuf]) -> bool {
    prefixes
        .iter()
        .any(|prefix| prefix.join("Cellar/rustup").is_dir())
}

/// How rustup 1.29.1 spells the Cargo home in the line it writes and
/// looks for (`cargo_home_str_with_home`, shell.rs:43-58): `$HOME/.cargo`
/// when the Cargo home is `<home>/.cargo`, else the absolute path.
pub fn cargo_home_str(home: &Path, cargo_home: &Path) -> String {
    if cargo_home == home.join(".cargo") {
        "$HOME/.cargo".to_string()
    } else {
        cargo_home.display().to_string()
    }
}

/// One visit rustup 1.29.1's cleanup makes: remove the first exact copy
/// of `line` (followed by a newline, at a line start) from `file`, if the
/// file exists. The visits in order are `rustup_rc_visits`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RcVisit {
    pub file: PathBuf,
    pub line: String,
}

/// rustup 1.29.1's cleanup as a sequence of visits (ruling 2).
/// `do_remove_from_path` (unix.rs:55-77) takes the shells of
/// `enumerate_shells` in order (shell.rs:63-74) and, for each `rcfiles()`
/// that is a file, removes the first exact current line
/// (`. "<cargo_home_str>/env"`, `source_string`, shell.rs:138-140):
/// Posix's `~/.profile` (:163-168), Bash's `~/.bash_profile`,
/// `~/.bash_login`, `~/.bashrc` (:188-195), Zsh's `$ZDOTDIR/.zshenv`
/// when there is a ZDOTDIR and `~/.zshenv` (:240-245, no deduplication
/// -- `ZDOTDIR=$HOME` visits the same file twice); Fish, Nu, Tcsh, Pwsh
/// and Xonsh visit files Banager does not read. Then
/// `remove_legacy_paths` (unix.rs:174-194) removes the pre-1.23 line
/// `export PATH="<S>/bin:$PATH"` and then `source "<S>/env"`, each from
/// `legacy_paths` (shell.rs:564-574): `~/.bash_profile`, `~/.profile`,
/// `$ZDOTDIR/.zprofile` when there is a ZDOTDIR, `~/.zprofile`. Bash's
/// and Zsh's availability checks are folded in: a Bash file that is not
/// there is a no-op visit, and on a Mac zsh is at `/bin/zsh`. `zdotdir`
/// is `HostEnv.zdotdir` -- rustup itself asks `zsh -c 'echo -n $ZDOTDIR'`
/// when `SHELL` is not zsh (shell.rs:207-225), which Banager does not
/// (it runs nothing), so a ZDOTDIR set only inside a zsh startup file is
/// not modelled; an empty one is none (shell.rs:213). `~/.zshrc` and
/// fish's `config.fish` are visited by nothing.
pub fn rustup_rc_visits(home: &Path, zdotdir: Option<&Path>, cargo_home_str: &str) -> Vec<RcVisit> {
    let zdotdir = zdotdir.filter(|dir| !dir.as_os_str().is_empty());
    let current = format!(". \"{cargo_home_str}/env\"");
    let legacy_path = format!("export PATH=\"{cargo_home_str}/bin:$PATH\"");
    let legacy_source = format!("source \"{cargo_home_str}/env\"");
    let visit = |file: PathBuf, line: &String| RcVisit {
        file,
        line: line.clone(),
    };
    let mut visits = Vec::new();
    for rc in [".profile", ".bash_profile", ".bash_login", ".bashrc"] {
        visits.push(visit(home.join(rc), &current));
    }
    if let Some(zdotdir) = zdotdir {
        visits.push(visit(zdotdir.join(".zshenv"), &current));
    }
    visits.push(visit(home.join(".zshenv"), &current));
    for line in [&legacy_path, &legacy_source] {
        for rc in [".bash_profile", ".profile"] {
            visits.push(visit(home.join(rc), line));
        }
        if let Some(zdotdir) = zdotdir {
            visits.push(visit(zdotdir.join(".zprofile"), line));
        }
        visits.push(visit(home.join(".zprofile"), line));
    }
    visits
}

/// rustup's `find_exact_line` (unix.rs:164-172) and the splice around it
/// (unix.rs:62-70, :150-158): `line` followed by a newline, at a line
/// start, byte for byte, first match only, removed. `false` when there is
/// no such line -- trailing space, another spelling, or the line last in
/// the file with no newline after it are not it.
pub fn remove_first_exact_line(contents: &mut String, line: &str) -> bool {
    let needle = format!("{line}\n");
    let at = {
        let bytes = contents.as_bytes();
        bytes
            .windows(needle.len())
            .enumerate()
            .find_map(|(at, window)| {
                (window == needle.as_bytes() && (at == 0 || bytes[at - 1] == b'\n')).then_some(at)
            })
    };
    match at {
        Some(at) => {
            contents.replace_range(at..at + needle.len(), "");
            true
        }
        None => false,
    }
}

/// How a startup file may speak of the Cargo env file (ruling 22).
/// `sourcing`: whole lines (trimmed) that *will* fail once the file is
/// gone -- the sourcing forms rustup itself writes (`. "<X>/env"`,
/// `source "<X>/env"`, fish's `source "<X>/env.fish"`) with `<X>` a
/// spelling whose target is this Cargo home. `needles`: substrings that
/// mention the env file at all, for the qualified tier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeftoverPatterns {
    pub sourcing: Vec<String>,
    pub needles: Vec<String>,
}

/// The patterns for `cargo_home` under `home`: for the default home the
/// spellings are `$HOME/.cargo` (what rustup writes) and the absolute
/// path (what rustup writes for a custom home that happens to be this
/// one), and the needle is `.cargo/env`; for a custom home only its
/// absolute path -- `$HOME/.cargo/env` then names a file this uninstall
/// leaves alone. `$CARGO_HOME/env` and `${CARGO_HOME}/env` are needles
/// always: what they load depends on the shell's own environment.
pub fn leftover_patterns(home: &Path, cargo_home: &Path) -> LeftoverPatterns {
    let absolute = cargo_home.display().to_string();
    let default = cargo_home == home.join(".cargo");
    let mut spellings = vec![absolute.clone()];
    if default {
        spellings.push("$HOME/.cargo".to_string());
    }
    let sourcing = spellings
        .iter()
        .flat_map(|spelling| {
            [
                format!(". \"{spelling}/env\""),
                format!("source \"{spelling}/env\""),
                format!("source \"{spelling}/env.fish\""),
            ]
        })
        .collect();
    let mut needles = vec![format!("{absolute}/env")];
    if default {
        needles.push(".cargo/env".to_string());
    }
    needles.push("$CARGO_HOME/env".to_string());
    needles.push("${CARGO_HOME}/env".to_string());
    LeftoverPatterns { sourcing, needles }
}

/// What a startup file will do about the Cargo env file after rustup's
/// cleanup: `Sources` -- a line rustup's own form spells, which *will*
/// print an error in every new terminal; `Mentions` -- some other
/// non-comment line naming the file, which *may*.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leftover {
    Sources,
    Mentions,
}

/// The tier of a file's remaining contents, or `None` for a file that
/// says nothing about the env file outside comments. A comment is a
/// trimmed line starting with `#`; the certain tier wins over the
/// qualified one within a file.
pub fn classify_leftover(contents: &str, patterns: &LeftoverPatterns) -> Option<Leftover> {
    let mut mentions = false;
    for raw in contents.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if patterns.sourcing.iter().any(|form| form == line) {
            return Some(Leftover::Sources);
        }
        if patterns.needles.iter().any(|needle| line.contains(needle.as_str())) {
            mentions = true;
        }
    }
    mentions.then_some(Leftover::Mentions)
}

/// One `LeavesShellConfigLine` per startup file under `home` that will
/// still speak of the Cargo env file after `rustup self uninstall`, in
/// `SHELL_RC_CANDIDATES` order, `path` spelled `~/<file>`, `certain` by
/// tier. The eight files are read once (read-only: Banager never edits
/// a startup file, spec §6.8), rustup's visits (`rustup_rc_visits`) are
/// replayed on the copies -- so a second visit to the same file sees
/// the first's result, as rustup's does -- and what is left is
/// classified (`classify_leftover`). A visit to a file outside the eight
/// (a `$ZDOTDIR` that is not the home) has no copy to act on.
pub fn shell_config_leftovers(home: &Path, zdotdir: Option<&Path>, cargo_home: &Path) -> Vec<Warning> {
    let spelled = cargo_home_str(home, cargo_home);
    let mut files: BTreeMap<PathBuf, String> = SHELL_RC_CANDIDATES
        .iter()
        .filter_map(|rc| {
            let path = home.join(rc);
            std::fs::read_to_string(&path)
                .ok()
                .map(|contents| (path, contents))
        })
        .collect();
    for visit in rustup_rc_visits(home, zdotdir, &spelled) {
        if let Some(contents) = files.get_mut(&visit.file) {
            remove_first_exact_line(contents, &visit.line);
        }
    }
    let patterns = leftover_patterns(home, cargo_home);
    SHELL_RC_CANDIDATES
        .iter()
        .filter_map(|rc| {
            let contents = files.get(&home.join(rc))?;
            let tier = classify_leftover(contents, &patterns)?;
            Some(Warning::LeavesShellConfigLine {
                path: format!("~/{rc}"),
                certain: tier == Leftover::Sources,
            })
        })
        .collect()
}

/// The uninstall preview's list for `rustup self uninstall -y`, in the
/// order the dialog shows it (spec §6.6), over a caller-given list of
/// Homebrew prefixes so the tests are hermetic: the rustup home by path
/// with every toolchain in it, the Cargo home by path, the programs in
/// its `bin/` when there are any (rulings 1 and 16: 1.29.1 removes the
/// whole Cargo home), the Homebrew line when its Cellar has a rustup
/// (ruling 21), the shell edit, and each startup file left speaking of
/// Cargo's env file. Empty for a layout the gate refuses (`plan`
/// refuses first). Paths are spelled with `~` by the crate's one rule
/// (`scan::display_path`).
pub fn warnings_with(d: &Detected, homebrew_prefixes: &[PathBuf]) -> Vec<Warning> {
    let Some(roots) = standard_roots(d) else {
        return Vec::new();
    };
    let tilde = |path: &Path| crate::scan::display_path(path, &d.home).display().to_string();
    let mut warnings = vec![
        Warning::RemovesToolchains {
            path: tilde(&roots.rustup_home),
            names: toolchain_names(&roots.rustup_home),
        },
        Warning::DeletesCargoHome {
            path: tilde(&roots.cargo_home),
        },
    ];
    let bins = bin_programs_rustup_removes(&roots.cargo_home);
    if !bins.is_empty() {
        warnings.push(Warning::RemovesCargoInstalled { names: bins });
    }
    if homebrew_rustup_present(homebrew_prefixes) {
        warnings.push(Warning::HomebrewRustupLosesToolchains);
    }
    warnings.push(Warning::EditsShellConfig);
    warnings.extend(shell_config_leftovers(
        &d.home,
        d.zdotdir.as_deref(),
        &roots.cargo_home,
    ));
    warnings
}

/// `CommandUninstall.warnings` of the `RUSTUP` recipe: `warnings_with`
/// over Homebrew's real prefixes.
pub fn uninstall_warnings(d: &Detected) -> Vec<Warning> {
    let prefixes: Vec<PathBuf> = HOMEBREW_PREFIXES.iter().map(PathBuf::from).collect();
    warnings_with(d, &prefixes)
}

/// The cargo instance's lock, spelled by the one function that spells
/// its id (`cargo::instance_id_for`, spec §2.4), for both of rustup's
/// plans: `self update` unlinks and re-copies `$CARGO_HOME/bin/rustup`
/// (`install_bins`, self_update.rs:771-785), the file the cargo
/// instance's `cargo` proxy runs, so a cargo read in that window would
/// find a missing or half-written binary; `self uninstall` deletes the
/// `.crates2.json` cargo's inventory reads. None when there is no usable
/// Cargo home (then there is no rustup instance either).
/// `RUSTUP.extra_locks`.
pub fn extra_locks(d: &Detected) -> Vec<ResourceLock> {
    d.cargo_home
        .as_deref()
        .map(|cargo_home| ResourceLock(instance_id_for(cargo_home)))
        .into_iter()
        .collect()
}
```

If C did not make `scan::display_path` `pub(crate)` (C checklist row 8), change `fn display_path(path: &Path, home: &Path) -> PathBuf {` in `crates/banager-core/src/scan/mod.rs` to `pub(crate) fn display_path(…)`, with a doc sentence: `/// Also the one \`~\` rule for the rustup recipe's preview (\`adapters::standalone::rustup\`).`

In `crates/banager-core/src/adapters/standalone/mod.rs`, in `#[cfg(test)] pub(super) mod testing`, delete Task 4's `pub const RUSTUP_PROXIES: [&str; 13] = [ … ];` together with the two doc lines above it (`/// Here until Task 5 moves it to \`rustup.rs\`, …`), and in `rustup_layout` change `for proxy in RUSTUP_PROXIES {` to

```rust
        for proxy in super::rustup::RUSTUP_PROXIES {
```

— one list of the thirteen names, the one the uninstall preview reads.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib adapters::standalone`
Expected: PASS — 24 tests in `rustup` (two gate, one toolchains, one `RUSTUP_PROXIES`, four `bin_programs_rustup_removes`, one Homebrew, two `cargo_home_str`, two `rustup_rc_visits`, one `remove_first_exact_line`, one `leftover_patterns`, one `classify_leftover`, five `shell_config_leftovers`, two `warnings_with`, one `extra_locks`), and every earlier `standalone` test, whose layouts `rustup_layout` now builds from `rustup::RUSTUP_PROXIES`.

- [ ] **Step 5: Run the gates**

Run `cargo fmt --all`, then all five from Global Constraints. Expected: all clean. (Everything in `rustup.rs` is `pub` in a `pub mod`, so `-D warnings` raises no `dead_code` before Task 6 wires `uninstall_blocked`, `uninstall_warnings` and `extra_locks` into the recipe — the deferral the Interfaces block declares.)

- [ ] **Step 6: Commit**

If Step 3 changed `display_path`'s visibility, append `crates/banager-core/src/scan/mod.rs` to the `git add` below.

```bash
git add crates/banager-core/src/adapters/standalone/rustup.rs crates/banager-core/src/adapters/standalone/mod.rs
git commit -m "$(cat <<'EOF'
Say what rustup 1.29.1's self uninstall removes, and when Banager may offer it, from its source

Before rustup self uninstall runs, the preview names ~/.rustup with every
toolchain in it (the entries of its toolchains directory -- no command is
run), says ~/.cargo goes too, permanently and not to the Trash, with its
caches, settings and saved login, names every program in its bin folder
that goes with it -- by rustup's own rule, which spares only rustup and
its thirteen proxies, so a program copied there by hand is named too --
says a Homebrew rustup loses its toolchains when its Cellar shows one,
says rustup edits the shell startup files it wrote to, and names each
startup file that will still speak of Cargo's env file once it is gone:
rustup's own cleanup replayed visit by visit on copies of the files,
ZDOTDIR included, with "will" only for a line rustup's own form spells
and "may" for any other mention. The command is offered only when both
folders are the standard ones and real directories; anything else is
refused before a preview exists. The cargo instance's lock is spelled by
cargo's own id function so the two can never differ.

Co-Authored-By: <the executing session's attribution line>
EOF
)"
```

---

### Task 6: `Uninstall::Command`, the `RUSTUP` recipe and meta, the Uninstall plan and the gate in `inventory`, the host on the allowlist

**Files:**
- Create: `adapters/meta/standalone-rustup.toml`
- Modify: `crates/banager-core/src/adapters/standalone/recipe.rs` — `enum Uninstall` (C's; one arm), new `CommandUninstall`
- Modify: `crates/banager-core/src/adapters/standalone/recipes.rs` — `pub static RUSTUP` (not yet in `RECIPES`: Task 10 registers it with its recording); tests
- Modify: `crates/banager-core/src/adapters/standalone/mod.rs` — `inventory`'s `uninstall_blocked`, `plan`'s `OpKind::Uninstall` arm (one arm in C's match), new `command_uninstall_plan`; C's other branches on `recipe.uninstall` only where Step 3's list says so (`execute`'s dispatch); tests
- Modify: `crates/banager-core/src/http/real.rs` — `ALLOWED_HTTPS_HOSTS`, its doc comment, one test  [A's file: anchor by the constant and the test names]
- Modify: `docs/what-we-run.md` — the table under `## Network: Banager only connects to these hosts`  [A's file: anchor by the heading and B's `downloads.claude.ai` row, `:652` at `ea30cfb`]
- Test: `recipes.rs`'s, `mod.rs`'s and `real.rs`'s test modules; A's `what_we_run_test` keeps passing.

**Interfaces:**
- Consumes: `rustup::{uninstall_blocked, uninstall_warnings, extra_locks, HOMEBREW_PREFIXES}` (Task 5); `Latest::HttpTomlVersion`, `VersionParse::SecondToken` (Task 3); `RouteKind::FlatFile`, `Recipe.extra_locks`, `seated_detected_for`, `locks`, `testing::rustup_layout` (Task 4); `cargo::{instance_id_for, cargo_home_of, RUSTUP_AUTO_INSTALL_OFF}` and `CargoAdapter::new`/`detect` (Task 1, `cargo.rs`); C's `Uninstall`, `Recipe.uninstall`, `PlanAction::Command`, `StandaloneAdapter::new`'s `trasher` argument and `crate::trash::MockTrasher`; A's `host_allowed`; B's `RecordingRunner`.
- Produces (verbatim): `pub struct CommandUninstall { pub args, pub timeout_secs, pub cancel: CancelPolicy, pub blocked: fn(&Detected) -> Option<UninstallBlocked>, pub warnings: fn(&Detected) -> Vec<Warning> }`; `Uninstall::Command(CommandUninstall)` (readers: `plan`'s Uninstall arm, `inventory`; producer: `RUSTUP`); `pub static RUSTUP: Recipe` (readers: `RECIPES` in Task 10, the tests here, Tasks 7 and 8's tests); `fn command_uninstall_plan(&self, inst, req, detected: &Detected, cmd: &CommandUninstall) -> Result<Plan, AdapterError>` (reader: `plan`); `"static.rust-lang.org"` in `ALLOWED_HTTPS_HOSTS` (readers: A's `host_allowed` in `send`, the doc table, A's `what_we_run_test`, `test_every_recipe_latest_url_is_an_allowed_https_host` once `RUSTUP` is in `RECIPES`); `adapters/meta/standalone-rustup.toml` (readers: `RUSTUP.meta_toml`; A's `what_we_run_test` once registered).

The recipe's every value and its source (rustup.md, VERIFIED unless said, and this Mac re-checked read-only on 2026-09-25): launcher `$CARGO_HOME/bin/rustup`, a regular Mach-O file of 11,319,056 bytes (§2, `ls -la`, `file`); root `$CARGO_HOME` (spec §2.2: the proxies live in its `bin/`, Banager reads nothing under `RUSTUP_HOME` but its `toolchains/` names during the preview); `rustup --version` → stdout `rustup 1.29.1 (d95a37b6a 2026-08-13)`, two `info:` lines on stderr (§3; re-run 2026-09-25 with `2>/dev/null` and `2>&1 >/dev/null` to split them), **with `RUSTUP_AUTO_INSTALL=0`** (ruling 20: `display_version` resolves the active toolchain and, with none and the switch on, installs one); newest version `https://static.rust-lang.org/rustup/release-stable.toml` → `schema-version = '1'` / `version = '1.29.1'` (§6, curl, and rustup's `DEFAULT_UPDATE_ROOT`); `self_updates: false` (spec §3.5; rustup updates itself only inside `rustup update`/`rustup toolchain install`, which Banager never runs — `rustup_mode.rs:1042-1090`, `SelfUpdateMode::update`); upgrade `self update`, 600 s, **`NoCancel`** (Ruling 11: `install_bins` unlinks then copies, self_update.rs:779-782); **never `rustup update`** (touches toolchains; rust-lang/rustup#4724, §7); uninstall `self uninstall -y` (§8: `-y` skips the confirmation the `/dev/null` stdin would otherwise answer with EOF), 600 s, `NoCancel`, **no probe** (ruling 4), `blocked: rustup::uninstall_blocked` (ruling 18), `warnings: rustup::uninstall_warnings`; `--no-modify-path` is *not* passed (spec Q6: rustup removing its own line beats leaving one that errors on every terminal); `extra_locks: rustup::extra_locks` (spec §2.4).

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` in `crates/banager-core/src/adapters/standalone/recipes.rs`:

```rust

    #[test]
    fn test_rustup_is_the_flat_file_route_under_cargo_home_read_as_the_second_token_with_auto_install_off() {
        assert_eq!(RUSTUP.id, "rustup");
        assert_eq!(RUSTUP.route.kind, RouteKind::FlatFile);
        assert_eq!(RUSTUP.route.launcher, "$CARGO_HOME/bin/rustup");
        assert_eq!(RUSTUP.route.root, "$CARGO_HOME");
        assert_eq!(RUSTUP.version.args, &["--version"]);
        // Ruling 20: `rustup --version` resolves the active toolchain and,
        // with none active and auto-install on (the default), installs
        // one -- a download during a refresh. The switch that stops it
        // is the same constant cargo's detect uses for the proxy.
        assert_eq!(RUSTUP.version.env, &[("RUSTUP_AUTO_INSTALL", "0")]);
        assert_eq!(RUSTUP.version.env, &[crate::adapters::cargo::RUSTUP_AUTO_INSTALL_OFF]);
        assert_eq!(RUSTUP.version.parse, VersionParse::SecondToken);
        assert!(!RUSTUP.self_updates);
        assert_eq!(
            RUSTUP.latest,
            Latest::HttpTomlVersion {
                url: "https://static.rust-lang.org/rustup/release-stable.toml"
            }
        );
    }

    #[test]
    fn test_rustup_updates_and_uninstalls_itself_with_no_cancel_the_gate_and_the_cargo_lock() {
        // `self update`, never `update` (spec §五, D6): the latter touches
        // the toolchains and an interruption leaves them half installed.
        assert_eq!(RUSTUP.upgrade.args, &["self", "update"]);
        assert_eq!(RUSTUP.upgrade.timeout_secs, 600);
        assert_eq!(RUSTUP.upgrade.cancel, CancelPolicy::NoCancel);
        let Some(Uninstall::Command(cmd)) = &RUSTUP.uninstall else {
            panic!("rustup uninstalls with its own command");
        };
        assert_eq!(cmd.args, &["self", "uninstall", "-y"]);
        assert_eq!(cmd.timeout_secs, 600);
        assert_eq!(cmd.cancel, CancelPolicy::NoCancel);
        assert!(
            !cmd.args.contains(&"--no-modify-path"),
            "spec Q6: rustup removes its own startup line"
        );
        // The functions, by identity: the recipe is data, and these three
        // are the data's only behaviour.
        assert!(std::ptr::fn_addr_eq(
            cmd.blocked,
            super::super::rustup::uninstall_blocked
                as fn(&crate::adapters::standalone::Detected) -> Option<crate::model::UninstallBlocked>
        ));
        assert!(std::ptr::fn_addr_eq(
            cmd.warnings,
            super::super::rustup::uninstall_warnings
                as fn(&crate::adapters::standalone::Detected) -> Vec<crate::model::Warning>
        ));
        assert!(std::ptr::fn_addr_eq(
            RUSTUP.extra_locks,
            super::super::rustup::extra_locks
                as fn(&crate::adapters::standalone::Detected) -> Vec<crate::model::ResourceLock>
        ));
    }

    #[test]
    fn test_the_rustup_recipe_never_builds_rustup_update() {
        // Belt and braces over the assertion above, for every argv the
        // RUSTUP recipe holds -- upgrade, version read, uninstall:
        // `rustup update` is the one subcommand this recipe must never
        // build (spec D6, 附录 B). Scoped to rustup on purpose: `update`
        // is dangerous only as *rustup's* first argument, and it is
        // claude's documented upgrade (`claude update`, spec §五; B's
        // `CLAUDE.upgrade.args`), so a ban over every recipe would fail
        // on the one recipe that is right to use it.
        let Some(Uninstall::Command(cmd)) = &RUSTUP.uninstall else {
            panic!("rustup uninstalls with its own command");
        };
        for argv in [RUSTUP.upgrade.args, RUSTUP.version.args, cmd.args] {
            assert_ne!(argv.first(), Some(&"update"), "rustup: {argv:?}");
        }
    }
```

(`std::ptr::fn_addr_eq` is stable since Rust 1.85; the toolchain here is 1.98. If clippy objects to the `as fn(…)` casts, the intent is a comparison of the two function pointers' addresses, and `cmd.warnings as usize == uninstall_warnings as usize` is the fallback.) `Uninstall` must already be imported by the recipe file for `CLAUDE`'s `Paths`.

Append inside `mod tests` in `crates/banager-core/src/adapters/standalone/mod.rs`; add `use super::recipes::RUSTUP; use super::testing::rustup_layout; use crate::adapters::cargo::CargoAdapter; use crate::trash::MockTrasher;` to its `use` lines, skipping any the module already imports — C's tests build adapters with a trasher and likely import `MockTrasher` already, and a second identical `use` in one module is error E0252. No test here names `RUSTUP_PROXIES`, which `rustup_layout` reads for itself:

```rust

    const RELEASE_URL: &str = "https://static.rust-lang.org/rustup/release-stable.toml";
    const RUSTUP_VERSION_LINE: &str = "rustup 1.29.1 (d95a37b6a 2026-08-13)\n";

    /// `StandaloneAdapter::new` with C's fourth argument: nothing of
    /// rustup's goes through the Trash (its uninstall is a command), so a
    /// fresh `MockTrasher` stands in and is never called.
    fn rustup_adapter(runner: Arc<dyn CommandRunner>, http: Arc<MockHttpClient>) -> StandaloneAdapter {
        StandaloneAdapter::new(&RUSTUP, runner, http, Arc::new(MockTrasher::new()))
    }

    /// A rustup install under `cargo_home`, whose runner answers
    /// `--version`, detected under `env`.
    async fn detected_rustup(
        env: &HostEnv,
        cargo_home: &Path,
        http: Arc<MockHttpClient>,
    ) -> (StandaloneAdapter, ManagerInstance, Arc<MockRunner>) {
        let layout = rustup_layout(cargo_home);
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0(RUSTUP_VERSION_LINE),
        );
        let adapter = rustup_adapter(runner.clone(), http);
        let inst = adapter.detect(env).await.remove(0);
        (adapter, inst, runner)
    }

    /// The preview's Homebrew line depends on the Mac running the tests
    /// (`rustup::HOMEBREW_PREFIXES` are real paths): filtered out where a
    /// test asserts the whole list. `rustup::tests` proves the line
    /// itself over a temp prefix.
    fn without_homebrew_line(warnings: Vec<Warning>) -> Vec<Warning> {
        warnings
            .into_iter()
            .filter(|w| !matches!(w, Warning::HomebrewRustupLosesToolchains))
            .collect()
    }

    #[tokio::test]
    async fn test_detect_lists_rustup_under_the_cargo_home_with_the_second_token_version() {
        let home = TempHome::new("rustup-detect");
        let cargo_home = home.path().join(".cargo");
        let (adapter, inst, _runner) =
            detected_rustup(&home.env(vec![cargo_home.join("bin")]), &cargo_home, Arc::new(MockHttpClient::new())).await;
        assert_eq!(inst.id, "standalone-rustup");
        assert_eq!(inst.adapter_id, "standalone-rustup");
        // The launcher is the file itself; the root is the Cargo home.
        assert_eq!(inst.exe_path, cargo_home.join("bin/rustup"));
        assert_eq!(inst.prefix, cargo_home);
        assert_eq!(inst.version, Some("1.29.1".to_string()));
        assert_eq!(inst.unverified_version, None, "1.29.1 is the verified version");
        assert_eq!(inst.status.unavailable, None);
        assert!(inst.status.notes.is_empty(), "PATH finds this very file");
        assert_eq!(
            adapter.seated_detected_for(&inst).expect("seated").cargo_home,
            Some(cargo_home.clone())
        );

        let artifacts = adapter.inventory(&inst).await.expect("inventory");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].display_name, "rustup");
        assert_eq!(artifacts[0].version, "1.29.1");
        assert_eq!(artifacts[0].path, Some(cargo_home.join("bin/rustup")));
        assert!(!artifacts[0].auto_updates);
        // The standard layout (`~/.cargo`, no `~/.rustup` yet): the
        // official uninstall is offered (Q5) -- `Uninstall::Command`'s
        // `blocked` answered `None` through the seat.
        assert_eq!(artifacts[0].uninstall_blocked, None);
    }

    #[tokio::test]
    async fn test_detect_reads_rustups_version_with_auto_install_off_and_a_thirty_second_timeout() {
        // Ruling 20, end to end: the `CommandSpec` the version read hands
        // the runner carries `RUSTUP_AUTO_INSTALL=0` and nothing else,
        // the 30 s every adapter gives `--version`, and no cwd.
        let home = TempHome::new("rustup-detect-env");
        let layout = rustup_layout(&home.path().join(".cargo"));
        let runner = Arc::new(RecordingRunner {
            specs: StdMutex::new(Vec::new()),
            output: exited_0(RUSTUP_VERSION_LINE),
        });
        let adapter = rustup_adapter(runner.clone(), Arc::new(MockHttpClient::new()));
        adapter.detect(&home.env(vec![])).await;
        let specs = runner.specs.lock().unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].program, layout.launcher);
        assert_eq!(specs[0].args, vec!["--version".to_string()]);
        assert_eq!(
            specs[0].env,
            vec![("RUSTUP_AUTO_INSTALL".to_string(), "0".to_string())]
        );
        assert_eq!(specs[0].timeout, Duration::from_secs(30));
        assert_eq!(specs[0].output_use, OutputUse::Parsed);
        assert_eq!(specs[0].cwd, None);
    }

    #[tokio::test]
    async fn test_detect_reads_the_version_of_a_rustup_with_no_active_toolchain_without_running_anything_else() {
        // With `RUSTUP_AUTO_INSTALL=0` and no toolchain active, 1.29.1's
        // `display_version` (rustup_mode.rs:1819-1837) takes the
        // `active_toolchain()` path, prints its version line on stdout as
        // ever, says `info: no rustc is currently active` on stderr and
        // exits 0 -- quoted from the source, not recorded: recording it
        // would need a Mac with no toolchain, and installing or removing
        // one is out of bounds. Mocked, so nothing real runs: the version
        // is read, the row is not "not responding", and no second command
        // was spawned.
        let home = TempHome::new("rustup-detect-no-toolchain");
        let layout = rustup_layout(&home.path().join(".cargo"));
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            CommandOutput {
                exit_code: Some(0),
                stdout: RUSTUP_VERSION_LINE.to_string(),
                stderr: "info: This is the version for the rustup toolchain manager, not the rustc compiler.\ninfo: no `rustc` is currently active\n".to_string(),
                timed_out: false,
                cancelled: false,
            },
        );
        let adapter = rustup_adapter(runner.clone(), Arc::new(MockHttpClient::new()));
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        assert_eq!(inst.version, Some("1.29.1".to_string()));
        assert_eq!(inst.status.unavailable, None);
        assert_eq!(runner.calls().len(), 1, "one command, the version read");
    }

    #[tokio::test]
    async fn test_detect_follows_cargo_home_for_rustup() {
        // `CARGO_HOME=/elsewhere/cargo`: the launcher is looked for there,
        // never under ~/.cargo.
        let home = TempHome::new("rustup-detect-custom");
        let custom = home.path().join("elsewhere/cargo");
        let env = HostEnv {
            cargo_home: Some(custom.clone()),
            ..home.env(vec![custom.join("bin")])
        };
        let (_adapter, inst, _runner) = detected_rustup(&env, &custom, Arc::new(MockHttpClient::new())).await;
        assert_eq!(inst.exe_path, custom.join("bin/rustup"));
        assert_eq!(inst.prefix, custom);
        // With rustup under ~/.cargo but CARGO_HOME pointing elsewhere: no
        // instance -- that rustup is not where rustup itself would look.
        let home = TempHome::new("rustup-detect-mismatch");
        rustup_layout(&home.path().join(".cargo"));
        let env = HostEnv {
            cargo_home: Some(home.path().join("elsewhere/cargo")),
            ..home.env(vec![])
        };
        assert!(rustup_adapter(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()))
            .detect(&env)
            .await
            .is_empty());
        // An empty CARGO_HOME is the default (the `home` crate's rule);
        // a relative one is unsupported and finds nothing, running
        // nothing.
        let home = TempHome::new("rustup-detect-empty-and-relative");
        let layout = rustup_layout(&home.path().join(".cargo"));
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0(RUSTUP_VERSION_LINE),
        );
        let adapter = rustup_adapter(runner.clone(), Arc::new(MockHttpClient::new()));
        let env = HostEnv {
            cargo_home: Some(PathBuf::from("")),
            ..home.env(vec![])
        };
        assert_eq!(adapter.detect(&env).await.len(), 1);
        let env = HostEnv {
            cargo_home: Some(PathBuf::from(".cargo")),
            ..home.env(vec![])
        };
        let before = runner.calls().len();
        assert!(adapter.detect(&env).await.is_empty());
        assert_eq!(runner.calls().len(), before, "nothing run for a home Banager cannot name");
    }

    #[tokio::test]
    async fn test_check_updates_reads_the_release_file_and_lists_only_a_newer_rustup() {
        for (body, expected) in [
            ("schema-version = '1'\nversion = '1.30.0'\n", 1),
            ("schema-version = '1'\nversion = '1.29.1'\n", 0),
            ("schema-version = '1'\nversion = '1.28.2'\n", 0),
        ] {
            let home = TempHome::new("rustup-check");
            let cargo_home = home.path().join(".cargo");
            let http = Arc::new(MockHttpClient::new());
            http.respond(RELEASE_URL, answer(body));
            let (adapter, inst, _runner) = detected_rustup(&home.env(vec![]), &cargo_home, http.clone()).await;
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("check_updates");
            assert_eq!(out.candidates.len(), expected, "{body:?}");
            assert_eq!(http.calls(), vec![RELEASE_URL.to_string()]);
            if expected == 1 {
                let c = &out.candidates[0];
                assert_eq!(c.key.name, "rustup");
                assert_eq!(c.current, "1.29.1");
                assert_eq!(c.target, "1.30.0");
                assert_eq!(c.channel, UpdateChannel::Registry);
                assert!(c.checkable);
                assert!(c.warnings.is_empty());
                assert_eq!(c.blocked, None);
                let request = &http.requests()[0];
                assert_eq!(request.method, "GET");
                assert!(request.headers.is_empty());
                assert_eq!(request.timeout, Duration::from_secs(30));
            }
        }
    }

    #[tokio::test]
    async fn test_check_updates_marks_a_bad_release_file_uncheckable() {
        // A failed request, a non-200, HTML with status 200, a file with
        // no version: one could-not-check row each, never an `Err`.
        let cases: Vec<(Option<HttpResponse>, &str)> = vec![
            (None, "request to"),
            (
                Some(HttpResponse {
                    status: 503,
                    body: String::new(),
                }),
                "returned status 503",
            ),
            (Some(answer("<html>Sign in</html>")), "release file"),
            (Some(answer("schema-version = '1'\n")), "release file"),
        ];
        for (response, reason) in cases {
            let home = TempHome::new("rustup-check-bad");
            let cargo_home = home.path().join(".cargo");
            let http = Arc::new(MockHttpClient::new());
            match response {
                Some(r) => http.respond(RELEASE_URL, r),
                None => http.fail(RELEASE_URL, "connection refused"),
            }
            let (adapter, inst, _runner) = detected_rustup(&home.env(vec![]), &cargo_home, http).await;
            let out = adapter
                .check_updates(&inst, &CheckOptions::default())
                .await
                .expect("a failed lookup is not a source failure");
            assert_eq!(out.candidates.len(), 1, "{reason}");
            let c = &out.candidates[0];
            assert!(!c.checkable);
            assert_eq!(c.current, "1.29.1");
            assert_eq!(c.target, "1.29.1");
            assert!(
                matches!(&c.warnings[..], [Warning::Message(m)] if m.contains(reason) && m.len() < 200),
                "{reason}: {:?}",
                c.warnings
            );
        }
    }

    #[tokio::test]
    async fn test_plan_upgrade_for_rustup_is_self_update_no_cancel_with_the_cargo_lock() {
        let home = TempHome::new("rustup-plan-upgrade");
        let cargo_home = home.path().join(".cargo");
        let (adapter, inst, _runner) =
            detected_rustup(&home.env(vec![]), &cargo_home, Arc::new(MockHttpClient::new())).await;
        let plan = adapter
            .plan(&inst, &request_for("standalone-rustup", OpKind::Upgrade, "rustup"))
            .await
            .expect("plan");
        assert_eq!(
            plan.action,
            PlanAction::Command {
                program: cargo_home.join("bin/rustup"),
                args: vec!["self".to_string(), "update".to_string()],
                env: Vec::new(),
            }
        );
        assert!(!plan.needs_password);
        assert_eq!(
            plan.locks,
            vec![
                ResourceLock("standalone-rustup".to_string()),
                ResourceLock(crate::adapters::cargo::instance_id_for(&cargo_home)),
            ]
        );
        assert_eq!(plan.cancel_policy, CancelPolicy::NoCancel);
        assert!(plan.warnings.is_empty());
        assert!(plan.affected.is_empty());
        assert_eq!(plan.timeout_secs, 600);
    }

    #[tokio::test]
    async fn test_plan_uninstall_for_rustup_runs_nothing_and_lists_the_warnings() {
        // This Mac's layout (spec §6.6): one toolchain, hexyl installed
        // with cargo, rustup's line in ~/.zshenv and ~/.profile, the same
        // line by hand in ~/.zshrc. The preview reads the disk and runs
        // no command (ruling 4).
        let home = TempHome::new("rustup-plan-uninstall");
        let cargo_home = home.path().join(".cargo");
        let (adapter, inst, runner) =
            detected_rustup(&home.env(vec![]), &cargo_home, Arc::new(MockHttpClient::new())).await;
        std::fs::write(cargo_home.join("bin/hexyl"), b"x").expect("write hexyl");
        std::fs::copy(
            "../../adapters/fixtures/cargo/1.98.1/crates2.json",
            cargo_home.join(".crates2.json"),
        )
        .expect("copy the recorded record");
        home.dir(".rustup/toolchains/stable-aarch64-apple-darwin");
        for rc in [".zshenv", ".profile", ".zshrc"] {
            std::fs::write(home.path().join(rc), ". \"$HOME/.cargo/env\"\n").expect("write rc");
        }
        let calls_before = runner.calls().len();

        let plan = adapter
            .plan(&inst, &request_for("standalone-rustup", OpKind::Uninstall, "rustup"))
            .await
            .expect("plan");

        assert_eq!(
            plan.action,
            PlanAction::Command {
                program: cargo_home.join("bin/rustup"),
                args: vec!["self".to_string(), "uninstall".to_string(), "-y".to_string()],
                env: Vec::new(),
            }
        );
        assert_eq!(plan.cancel_policy, CancelPolicy::NoCancel);
        assert_eq!(plan.timeout_secs, 600);
        assert!(!plan.needs_password);
        assert!(plan.affected.is_empty(), "a non-empty list disables Confirm; hexyl does not break");
        assert_eq!(
            plan.locks,
            vec![
                ResourceLock("standalone-rustup".to_string()),
                ResourceLock(crate::adapters::cargo::instance_id_for(&cargo_home)),
            ]
        );
        assert_eq!(
            without_homebrew_line(plan.warnings),
            vec![
                Warning::RemovesToolchains {
                    path: "~/.rustup".to_string(),
                    names: vec!["stable-aarch64-apple-darwin".to_string()]
                },
                Warning::DeletesCargoHome {
                    path: "~/.cargo".to_string()
                },
                Warning::RemovesCargoInstalled {
                    names: vec!["hexyl".to_string()]
                },
                Warning::EditsShellConfig,
                Warning::LeavesShellConfigLine {
                    path: "~/.zshrc".to_string(),
                    certain: true
                },
            ]
        );
        assert_eq!(runner.calls().len(), calls_before, "the preview ran no command");
    }

    #[tokio::test]
    async fn test_plan_uninstall_for_rustup_survives_an_empty_cargo_home_and_no_toolchains() {
        // No `.crates2.json`, no `~/.rustup`, no startup files: the plan
        // still builds, with the toolchain sentence unnamed and nothing
        // invented.
        let home = TempHome::new("rustup-plan-uninstall-bare");
        let cargo_home = home.path().join(".cargo");
        let (adapter, inst, _runner) =
            detected_rustup(&home.env(vec![]), &cargo_home, Arc::new(MockHttpClient::new())).await;
        let plan = adapter
            .plan(&inst, &request_for("standalone-rustup", OpKind::Uninstall, "rustup"))
            .await
            .expect("plan");
        assert_eq!(
            without_homebrew_line(plan.warnings),
            vec![
                Warning::RemovesToolchains {
                    path: "~/.rustup".to_string(),
                    names: Vec::new()
                },
                Warning::DeletesCargoHome {
                    path: "~/.cargo".to_string()
                },
                Warning::EditsShellConfig,
            ]
        );
    }

    #[tokio::test]
    async fn test_inventory_and_plan_refuse_the_uninstall_for_a_non_standard_layout() {
        // Ruling 18, through the adapter: a custom CARGO_HOME, a custom
        // RUSTUP_HOME, and a linked root each make the artifact carry
        // `NoSafeMethod` (so the gate in session/plans.rs refuses and the
        // page hides the button) and make `plan(Uninstall)` refuse with
        // the same reason; the upgrade is not gated, and its cargo lock
        // names the custom home.
        let home = TempHome::new("rustup-gate-custom-cargo");
        let custom = home.path().join("elsewhere/cargo");
        let env = HostEnv {
            cargo_home: Some(custom.clone()),
            ..home.env(vec![])
        };
        let (adapter, inst, _runner) = detected_rustup(&env, &custom, Arc::new(MockHttpClient::new())).await;
        let artifacts = adapter.inventory(&inst).await.expect("inventory");
        assert_eq!(artifacts[0].uninstall_blocked, Some(UninstallBlocked::NoSafeMethod));
        assert!(matches!(
            adapter
                .plan(&inst, &request_for("standalone-rustup", OpKind::Uninstall, "rustup"))
                .await,
            Err(AdapterError::UninstallBlocked {
                reason: UninstallBlocked::NoSafeMethod
            })
        ));
        let upgrade = adapter
            .plan(&inst, &request_for("standalone-rustup", OpKind::Upgrade, "rustup"))
            .await
            .expect("the upgrade is not gated");
        assert!(upgrade.locks.contains(&ResourceLock(crate::adapters::cargo::instance_id_for(&custom))));

        let home = TempHome::new("rustup-gate-custom-rustup");
        let cargo_home = home.path().join(".cargo");
        let env = HostEnv {
            rustup_home: Some(home.path().join("elsewhere/rustup")),
            ..home.env(vec![])
        };
        let (adapter, inst, _runner) = detected_rustup(&env, &cargo_home, Arc::new(MockHttpClient::new())).await;
        assert_eq!(
            adapter.inventory(&inst).await.expect("inventory")[0].uninstall_blocked,
            Some(UninstallBlocked::NoSafeMethod)
        );

        let home = TempHome::new("rustup-gate-linked-rustup");
        let cargo_home = home.path().join(".cargo");
        let elsewhere = home.dir("Volumes/Data/rustup");
        home.link(".rustup", &elsewhere);
        let (adapter, inst, _runner) =
            detected_rustup(&home.env(vec![]), &cargo_home, Arc::new(MockHttpClient::new())).await;
        assert_eq!(
            adapter.inventory(&inst).await.expect("inventory")[0].uninstall_blocked,
            Some(UninstallBlocked::NoSafeMethod)
        );
        assert!(matches!(
            adapter
                .plan(&inst, &request_for("standalone-rustup", OpKind::Uninstall, "rustup"))
                .await,
            Err(AdapterError::UninstallBlocked { .. })
        ));
    }

    #[tokio::test]
    async fn test_rustup_locks_name_the_cargo_instance_detect_produces_with_and_without_cargo_home() {
        // Spec §2.4, §十三 #42: `acquire_resource_lock` compares names byte
        // for byte and reports nothing for two that merely look alike, so
        // the lock rustup's plans hold must equal the id `CargoAdapter::
        // detect` gives its instance on the same host -- with CARGO_HOME
        // unset and set. Both go through `cargo::instance_id_for` and
        // `cargo::cargo_home_of`; this proves it end to end. With it set,
        // only the upgrade has a plan (the uninstall is gated).
        for custom in [false, true] {
            let home = TempHome::new("rustup-cross-lock");
            let cargo_home = if custom {
                home.path().join("elsewhere/cargo")
            } else {
                home.path().join(".cargo")
            };
            let env = HostEnv {
                cargo_home: custom.then(|| cargo_home.clone()),
                ..home.env(vec![cargo_home.join("bin")])
            };
            let (rustup, rustup_inst, runner) =
                detected_rustup(&env, &cargo_home, Arc::new(MockHttpClient::new())).await;
            // The cargo instance, from the real cargo adapter over the same
            // env: its `cargo` is the proxy link `rustup_layout` wrote.
            runner.respond(
                vec![cargo_home.join("bin/cargo").to_str().unwrap(), "--version"],
                exited_0("cargo 1.98.1 (797e8a9bc 2026-08-05)\n"),
            );
            let cargo = CargoAdapter::new(runner.clone(), Arc::new(MockHttpClient::new()));
            let cargo_inst = cargo.detect(&env).await.remove(0);
            assert_eq!(cargo_inst.prefix, cargo_home, "custom={custom}");

            let kinds: &[OpKind] = if custom {
                &[OpKind::Upgrade]
            } else {
                &[OpKind::Upgrade, OpKind::Uninstall]
            };
            for kind in kinds {
                let plan = rustup
                    .plan(&rustup_inst, &request_for("standalone-rustup", *kind, "rustup"))
                    .await
                    .expect("plan");
                assert!(
                    plan.locks.contains(&ResourceLock(cargo_inst.id.clone())),
                    "custom={custom} {kind:?}: {:?} lacks {}",
                    plan.locks,
                    cargo_inst.id
                );
                assert_eq!(plan.locks.len(), 2, "custom={custom} {kind:?}");
            }
        }
    }

    #[tokio::test]
    async fn test_execute_runs_rustups_uninstall_through_run_plan() {
        let home = TempHome::new("rustup-execute");
        let cargo_home = home.path().join(".cargo");
        let (adapter, inst, runner) =
            detected_rustup(&home.env(vec![]), &cargo_home, Arc::new(MockHttpClient::new())).await;
        runner.respond(
            vec![cargo_home.join("bin/rustup").to_str().unwrap(), "self", "uninstall", "-y"],
            exited_0("info: removing toolchains\ninfo: rustup is uninstalled\n"),
        );
        let plan = adapter
            .plan(&inst, &request_for("standalone-rustup", OpKind::Uninstall, "rustup"))
            .await
            .expect("plan");
        let sink = Arc::new(VecSink::new());
        let outcome = adapter
            .execute(&plan, sink.clone(), 1, CancellationToken::new())
            .await
            .expect("execute");
        // `execute` reports the command's own exit; whether rustup is
        // gone is `run_operation`'s reading afterwards (Task 8).
        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(sink.snapshot().len(), 2, "two log lines, streamed");
    }

    /// B's `request` is fixed to `standalone-claude`; rustup's requests
    /// name its own instance.
    fn request_for(instance_id: &str, kind: OpKind, name: &str) -> OpRequest {
        OpRequest {
            kind,
            instance_id: instance_id.to_string(),
            artifact_kind: ArtifactKind::Binary,
            name: name.to_string(),
        }
    }
```

In `crates/banager-core/src/http/real.rs`, append inside `mod tests` after B's `test_host_allowed_accepts_claude_codes_channel_pointers`:

```rust

    #[test]
    fn test_host_allowed_accepts_rustups_release_file() {
        // The exact URL `StandaloneAdapter::check_updates` builds for the
        // `RUSTUP` recipe (adapters/standalone/recipes.rs), phase 4 step E.
        host_allowed("https://static.rust-lang.org/rustup/release-stable.toml")
            .expect("static.rust-lang.org");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib adapters::standalone` and `cargo test -p banager-core --lib http::real`
Expected: FAIL to compile — `cannot find value \`RUSTUP\``, `no variant named \`Command\` found for enum \`Uninstall\``, `cannot find type \`CommandUninstall\``; `real.rs`'s new test FAILS with `host not allowed: "static.rust-lang.org" is not one of […]`.

- [ ] **Step 3: Write the types, the recipe, the plan arm, the gate, the host, the doc row**

Create `adapters/meta/standalone-rustup.toml`:

```toml
schema_version = 1
id = "standalone-rustup"
name = "rustup"
kind = "standalone"
platforms = ["macos"]
homepage = "https://rust-lang.github.io/rustup/"
verified_versions = ["1.29.1"]
```

(`kind = "standalone"` is documentary, as for claude. `verified_versions` is what Task 10 records; if the recording day's `rustup --version` differs, Task 10 changes this line to match.)

In `crates/banager-core/src/adapters/standalone/recipe.rs`, make the model import `use crate::model::{CancelPolicy, ResourceLock, UninstallBlocked, Warning};` (keeping whatever C imports), and after C's `enum Uninstall`'s `Paths { … }` arm add:

```rust
    /// The tool's own official uninstall command (rustup: `self uninstall
    /// -y`), run against the launcher through `run_plan` unchanged; when
    /// it may be offered is said by `blocked`, and what it removes by
    /// `warnings`, since the argv alone cannot (spec §6.4). Read by
    /// `plan(Uninstall)` and `inventory`.
    Command(CommandUninstall),
```

and, after the `Uninstall` enum, the struct:

```rust

/// `Uninstall::Command`'s data. `cancel` is `NoCancel` for rustup: its
/// uninstall removes directories one after another and a kill partway
/// leaves a broken Rust. The preview runs no command: everything it says
/// comes from what `detect` seated and the disk.
#[derive(Debug)]
pub struct CommandUninstall {
    pub args: &'static [&'static str],
    pub timeout_secs: u64,
    pub cancel: CancelPolicy,
    /// Whether this install may be offered the command at all, from the
    /// seat: `Some(reason)` puts `uninstall_blocked` on the artifact
    /// (`inventory`), which the gate refuses and the page hides the
    /// button for, and makes `plan(Uninstall)` refuse with the same
    /// reason. rustup's is `rustup::uninstall_blocked`: `NoSafeMethod`
    /// unless Rust lives in its standard folders (plan ruling 18).
    pub blocked: fn(&Detected) -> Option<UninstallBlocked>,
    /// The preview's warnings, from what `detect` seated and the disk.
    /// Read by `plan(Uninstall)`; rustup's is `rustup::uninstall_warnings`.
    pub warnings: fn(&Detected) -> Vec<Warning>,
}
```

In `crates/banager-core/src/adapters/standalone/recipes.rs`, add `CommandUninstall`, `Uninstall` (if not yet), and `super::rustup` to the imports (`use super::recipe::{…}; use super::rustup;`), and `use crate::adapters::cargo::RUSTUP_AUTO_INSTALL_OFF;`, and after `CLAUDE` (before `RECIPES`) add:

```rust

/// rustup, the Rust toolchain installer, installed by its own script
/// (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`,
/// run by the user; Banager never runs it).
///
/// Every value here is from `.superpowers/phase4/rustup.md` (VERIFIED on
/// this Mac or in rustup's own source at tag 1.29.1, 2026-09-24/25,
/// unless noted) and from the recording in
/// `adapters/fixtures/standalone-rustup/<version>/`:
/// - the launcher `$CARGO_HOME/bin/rustup` is a regular Mach-O file (11 MB
///   on this Mac); the thirteen proxies beside it (`cargo`, `rustc`,
///   `rustfmt`, …) are relative symlinks to it (§2; unknown-scan.md §2),
///   which the Unknown page's rule 1 attributes. The root is the Cargo
///   home: Banager reads nothing under `RUSTUP_HOME` except, during the
///   uninstall preview, the names in its `toolchains/` (spec §2.2, §3.2);
/// - `rustup --version` prints `rustup <version> (<hash> <date>)` on
///   stdout, and two `info:` lines on stderr that are never read (§3;
///   `version-stderr.txt` in the recording). It runs with
///   `RUSTUP_AUTO_INSTALL=0`: 1.29.1's `display_version`
///   (rustup_mode.rs:1819-1837) resolves the active toolchain and, with
///   none active and auto-install on (the default, config.rs:435-441),
///   installs one -- a download during a refresh. With the switch it
///   says `info: no rustc is currently active` and exits 0. Two side
///   effects of any rustup invocation remain and the trust file says so:
///   `Cfg::from_env` creates `$RUSTUP_HOME` when it is missing
///   (config.rs:321-323), and `cleanup_self_updater` deletes a leftover
///   `$CARGO_HOME/bin/rustup-init` (self_update.rs:1314-1323) -- which is
///   why a refresh never runs this read while rustup's own update holds
///   its locks (`Session::refresh_round`, plan ruling 19);
/// - the newest published version is `version = '…'` in
///   `static.rust-lang.org/rustup/release-stable.toml`, the file `rustup
///   self update` itself reads (`DEFAULT_UPDATE_ROOT`, §6);
/// - it does not update itself on its own (spec §3.5): rustup updates
///   itself only as part of `rustup update` and `rustup toolchain
///   install` (`SelfUpdateMode::update`, rustup_mode.rs:1042-1090), which
///   Banager never runs;
/// - `rustup self update` (never `rustup update`, which updates the
///   toolchains and, interrupted, leaves them half installed:
///   rust-lang/rustup#4724, §7) is `NoCancel` with the cargo instance's
///   lock: `install_bins` (1.29.1 `src/cli/self_update.rs:771-785`)
///   unlinks the running `rustup` and then copies the new one in, and in
///   between all thirteen proxies -- the cargo instance's `cargo` among
///   them -- are dangling. 600 s: one 11 MB download;
/// - `rustup self uninstall -y` is the official uninstall (§8; `-y` skips
///   the confirmation an EOF on stdin would otherwise decline), `NoCancel`
///   for the same reason, with the same lock (it deletes the
///   `.crates2.json` cargo's inventory reads). It is offered only when
///   `CARGO_HOME` and `RUSTUP_HOME` resolve to `~/.cargo` and `~/.rustup`
///   and both are real directories (`rustup::uninstall_blocked`, plan
///   ruling 18): 1.29.1's `uninstall()` removes both homes whole,
///   wherever they point, and never to the Trash. The preview runs no
///   command; its warnings (`rustup::uninstall_warnings`) come from the
///   `toolchains/` listing, a listing of `$CARGO_HOME/bin`,
///   `.crates2.json`, Homebrew's Cellar and eight startup files -- read
///   from 1.29.1's source, which removes the whole Cargo home, every
///   program in its `bin/` included (`rustup.rs`'s module doc has the
///   lines). `--no-modify-path` is not passed (spec Q6): rustup removing
///   its own startup line beats leaving one that errors on every new
///   terminal;
/// - both commands run with Banager's own environment: `fix_path_env`
///   restores only `PATH` from the login shell, and the runner passes the
///   rest as inherited. A `RUSTUP_HOME` or `CARGO_HOME` exported only in
///   a shell startup file is not seen by Banager or by the rustup it
///   runs -- the two agree, which is what the gate relies on -- so the
///   preview and the uninstall act on the default folders, and a Rust
///   kept only where the shell says is left alone, not deleted (plan
///   ruling 17).
pub static RUSTUP: Recipe = Recipe {
    id: "rustup",
    meta_toml: include_str!("../../../../../adapters/meta/standalone-rustup.toml"),
    route: Route {
        kind: RouteKind::FlatFile,
        launcher: "$CARGO_HOME/bin/rustup",
        root: "$CARGO_HOME",
    },
    version: VersionCmd {
        args: &["--version"],
        env: &[RUSTUP_AUTO_INSTALL_OFF],
        parse: VersionParse::SecondToken,
    },
    latest: Latest::HttpTomlVersion {
        url: "https://static.rust-lang.org/rustup/release-stable.toml",
    },
    self_updates: false,
    upgrade: UpgradeCmd {
        args: &["self", "update"],
        timeout_secs: 600,
        cancel: CancelPolicy::NoCancel,
    },
    uninstall: Some(Uninstall::Command(CommandUninstall {
        args: &["self", "uninstall", "-y"],
        timeout_secs: 600,
        cancel: CancelPolicy::NoCancel,
        blocked: rustup::uninstall_blocked,
        warnings: rustup::uninstall_warnings,
    })),
    extra_locks: rustup::extra_locks,
};
```

(`RECIPES` stays `&[&CLAUDE]` until Task 10: adding `RUSTUP` registers it through `all()`, and `fixtures_layout_test`/`what_we_run_test` then demand the recording and the trust-file section, which Task 10 brings in the same commit. If step D landed first, `RUSTUP` also needs D's `backup_globs: &[],`.)

In `crates/banager-core/src/adapters/standalone/mod.rs`, add `CommandUninstall` and `Uninstall` to the `use self::recipe::{…}` line, and after `locks` (before `inventory`) insert:

```rust

    /// `Uninstall::Command` (spec §6.4): the tool's own uninstall argv
    /// against the launcher, refused with the recipe's reason when its
    /// gate says this layout is not offered, otherwise with the warnings
    /// the recipe builds from the seat and the disk. Nothing is run here
    /// (the preview must not run rustup: plan ruling 4). Through
    /// `run_plan` like every command; `affected` stays empty because a
    /// non-empty list disables Confirm and nothing here breaks another
    /// package.
    fn command_uninstall_plan(
        &self,
        inst: &ManagerInstance,
        req: &OpRequest,
        detected: &Detected,
        cmd: &CommandUninstall,
    ) -> Result<Plan, AdapterError> {
        if let Some(reason) = (cmd.blocked)(detected) {
            return Err(AdapterError::UninstallBlocked { reason });
        }
        Ok(Plan {
            request: req.clone(),
            action: PlanAction::Command {
                program: inst.exe_path.clone(),
                args: cmd.args.iter().map(|a| a.to_string()).collect(),
                env: Vec::new(),
            },
            needs_password: false,
            locks: self.locks(inst, detected),
            cancel_policy: cmd.cancel,
            warnings: (cmd.warnings)(detected),
            affected: Vec::new(),
            timeout_secs: cmd.timeout_secs,
        })
    }
```

and in `plan`'s `OpKind::Uninstall` arm, where C matches `self.recipe.uninstall` (its `None => Err(AdapterError::UninstallBlocked { reason: UninstallBlocked::NoSafeMethod })` and `Some(Uninstall::Paths { .. }) => …` arms), add the arm:

```rust
                Some(Uninstall::Command(cmd)) => {
                    let detected = self.seated_detected_for(inst)?;
                    self.command_uninstall_plan(inst, req, &detected, cmd)
                }
```

In `inventory`, replace the `uninstall_blocked:` field of the one artifact (C's rule; B's was `Some(UninstallBlocked::NoSafeMethod)`) with:

```rust
            // No uninstall method at all: `NoSafeMethod` (C's ruling 2). A
            // path list: offered. A command: the recipe's gate decides
            // from the seat -- rustup offers its own uninstall only for
            // the standard layout (plan ruling 18) -- and a seat that is
            // missing or describes another home reads as blocked, never
            // as offered.
            uninstall_blocked: match &self.recipe.uninstall {
                None => Some(UninstallBlocked::NoSafeMethod),
                Some(Uninstall::Paths { .. }) => None,
                Some(Uninstall::Command(cmd)) => self
                    .seated_detected_for(inst)
                    .map_or(Some(UninstallBlocked::NoSafeMethod), |seat| (cmd.blocked)(&seat)),
            },
```

Then settle every other place that branches on the recipe's uninstall — the ones the C checklist's row 5 names. Run `grep -rn "Uninstall::Paths\|recipe.uninstall\|\.uninstall\b" crates/banager-core/src/adapters/standalone/` and, for each hit outside the two arms just written:

- **`execute`'s dispatch.** A `Command` uninstall plan must reach `run_plan` unchanged (spec §6.1; `test_execute_runs_rustups_uninstall_through_run_plan`). If C's `execute` dispatches on `plan.action` (`PlanAction::Command { .. }` → `run_plan`, `PlanAction::TrashPaths { .. }` → `removal::execute_removal`), nothing changes. If it matches `self.recipe.uninstall` to find the `remove`/`keep` lists, keep that lookup inside the `TrashPaths` arm and let the `Command` action go to `run_plan` whatever the recipe says.
- **`reconcile_after_uninstall`** (C's override): it answers presence of the launcher through `probe_strict`, which Task 4 taught `FlatFile`; nothing to change, and Task 8 proves it.
- **Any other `match` / `if let` / `let … else` over `Uninstall` in non-test code**: a `Command` arm that does what the surrounding function does for "the recipe has its own command", or, where the function is about paths only, `Some(Uninstall::Command(_)) => <the same as None>` with a one-line comment saying why.
- **RECIPES-wide tests that destructure `Uninstall::Paths { .. }`** (launcher last, every `remove` path under home, `keep` copy, …): nothing to do here — `RUSTUP` is not in `RECIPES` until Task 10, which gives each of them its skip (Task 10, Step 4). `test_a_paths_recipe_names_only_home_paths` (Task 4) already skips a `Command` recipe.

In `crates/banager-core/src/http/real.rs`, change B's constant to

```rust
pub const ALLOWED_HTTPS_HOSTS: &[&str] = &[
    "crates.io",
    "pypi.org",
    "registry.ollama.ai",
    "downloads.claude.ai",
    "static.rust-lang.org",
];
```

and in its doc comment, change B's clause `and downloads.claude.ai (\`StandaloneAdapter::check_updates\`, Claude Code's channel pointer).` to `, downloads.claude.ai (\`StandaloneAdapter::check_updates\`, Claude Code's channel pointer) and static.rust-lang.org (the same, rustup's release file).`

In `docs/what-we-run.md`, under `## Network: Banager only connects to these hosts`, after B's row that begins `| \`downloads.claude.ai\` |`, add:

```markdown
| `static.rust-lang.org` | `GET /rustup/release-stable.toml` — the newest published rustup version, a two-line TOML file (`version = '…'`) | rustup's `check_updates` (`StandaloneAdapter`) |
```

- [ ] **Step 4: Run to verify it passes**

Run, as three commands (`cargo test` takes one test-name filter; a second positional is `unexpected argument`): `cargo test -p banager-core --lib adapters::standalone`, `cargo test -p banager-core --lib http::real` and `cargo test -p banager-core --test what_we_run_test`
Expected: PASS — 3 new `recipes` tests, 12 new `mod.rs` tests, the `real.rs` test; A's document tests still green (the host is now named in the doc). `fixtures_layout_test` and `test_new_registers_all_eight_adapters` are untouched: `RUSTUP` is not in `RECIPES` yet.

- [ ] **Step 5: Run the gates**

Run `cargo fmt --all`, then all five from Global Constraints. Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add adapters/meta/standalone-rustup.toml crates/banager-core/src/adapters/standalone/recipe.rs crates/banager-core/src/adapters/standalone/recipes.rs crates/banager-core/src/adapters/standalone/mod.rs crates/banager-core/src/http/real.rs docs/what-we-run.md
git commit -m "$(cat <<'EOF'
Describe rustup as a recipe, with its own uninstall command, its gate and its cargo lock

A flat file under the Cargo home, read as the second token with rustup's
auto-install switched off, checked against the release file rustup's own
updater reads, updated with `rustup self update` and removed with `rustup
self uninstall -y`: both NoCancel, both holding the cargo instance's lock,
because the first unlinks the binary every Rust proxy runs and the second
deletes the record cargo's inventory reads. The uninstall is offered only
when Rust lives in its standard folders -- rustup deletes both homes
whole, wherever they point -- and its preview runs nothing: it lists what
1.29.1 removes from the disk alone. The release file's host joins the
https allowlist with the request that contacts it.

Co-Authored-By: <the executing session's attribution line>
EOF
)"
```

---

### Task 7: A refresh leaves an adapter alone while an operation holds its instance

**Files:**
- Modify: `crates/banager-core/src/ops/mod.rs` — one method on `OperationManager`, beside `acquire_resource_lock` (`:876` at `ea30cfb`)
- Modify: `crates/banager-core/src/session/refresh.rs:156-235` (the detection fan-out and join) and `:236-262` (the per-instance carry-forward branch); its `mod tests` (`:1395-1465` renamed and re-asserted; two new tests)
- Test: `session/refresh.rs`'s `mod tests`; `tests/ops_lock_test.rs` and `tests/ops_cancel_test.rs` keep passing (nothing about acquisition changes).

**Interfaces:**
- Consumes: `OperationManager.held: Arc<Mutex<HashSet<ResourceLock>>>` (`ops/mod.rs:174`, shared with `ResourceLockGuard`); `refresh_round`'s `previous` snapshot, `AbortOnDropHandle`, `dedupe_instance_ids`; `RUSTUP` and `StandaloneAdapter::new(…, trasher)` (Task 6, C), `CargoAdapter::new`, `MockRunner::{respond, delay, calls}`, `MockHttpClient::respond`, `Session::with_adapters`, `session.ops` (private to `session`, visible to its child test module), `OperationManager::{submit, wait}`.
- Produces (verbatim): `pub fn locks_held(&self) -> HashSet<ResourceLock>` on `OperationManager` (reader: `refresh_round`); the skip and the carry-forward in `refresh_round` (readers: every refresh); the renamed test `test_refresh_carries_an_instance_under_an_operation_forward_and_still_refreshes_the_others`.

Ruling 19 in full. Why here and not in the adapter: `refresh_round` spawns every adapter's `detect` before any lock is taken (`refresh.rs:156-167`), and on a rustup Mac two adapters' detects run the rustup binary — `StandaloneAdapter::detect`'s `rustup --version` and `CargoAdapter::detect`'s `cargo --version` (the `cargo` proxy *is* rustup, `proxy_mode::main`), both of which begin with `cleanup_self_updater` (`rustup_mode.rs:669`, `proxy_mode.rs:15`), which deletes the `rustup-init` a running `self update` has downloaded and is about to run (ruling 4, 11). The operation holds `standalone-rustup` and `cargo:<home>` (Task 6); a refresh that consults those locks before detecting skips both adapters. **What changes for every adapter:** a refresh used to wait, in its per-instance fetch, for the instance's lock until the operation ended (`test_refresh_is_mutually_exclusive_with_an_operation_on_the_same_instance_but_not_others`) — a `brew install` held it for minutes; it now carries that instance forward unchanged and returns. The instance's own inventory after the operation is the operation's `reconcile` reading, under its locks, as before; the next refresh reads it again. **Residual, stated:** the held set is read once, at the start of the round; an operation submitted after that read may acquire its locks while a detect it would have skipped is already running its one `--version` (that window is the command's duration, 30 s at most); an operation still Queued behind another holds nothing and skips nothing; an operation holding only the cargo lock (a `cargo install`) does not skip rustup's detect, whose `rustup --version` runs under `RUSTUP_AUTO_INSTALL=0` and deletes a `rustup-init` only if one exists, which it does only during a self update, whose operation holds rustup's lock too.

- [ ] **Step 1: Write the failing tests**

In `crates/banager-core/src/session/refresh.rs`, inside `mod tests`, replace `test_refresh_is_mutually_exclusive_with_an_operation_on_the_same_instance_but_not_others` (`:1395-1465`: it seeds two `fake` instances, sets `block_execute`, refreshes once, submits an Install on `fake:1`, waits for it to be Running, spawns a second refresh, sleeps 200 ms, asserts `fake:2` was inventoried and `fake:1` was not, cancels the op, and awaits the refresh with a 2 s timeout) with:

```rust
    #[tokio::test]
    async fn test_refresh_carries_an_instance_under_an_operation_forward_and_still_refreshes_the_others(
    ) {
        // Phase 4 step E (plan ruling 19): an instance an operation is
        // holding is neither detected nor inventoried this round -- its
        // rows are last round's, unchanged, and the refresh does not wait
        // for the operation to end -- while every other instance, even of
        // the same adapter, is refreshed as usual. Before this the refresh
        // waited on the instance's lock, for as long as the operation took.
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
        let first = session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        state.lock().unwrap().inventory_calls.clear();
        assert_eq!(state.lock().unwrap().detect_calls, 1);

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

        // The refresh returns while the operation is still running: it
        // neither detects `fake` (one of its instances is held) nor
        // inventories `fake:1`; `fake:2` is inventoried as ever.
        let snapshot = tokio::time::timeout(
            Duration::from_secs(2),
            session.refresh(&non_root_env(), &CheckOptions::default()),
        )
        .await
        .expect("a refresh must not wait for an operation on one instance");
        {
            let s = state.lock().unwrap();
            assert!(
                s.inventory_calls.contains(&"fake:2".to_string()),
                "a different instance's refresh must proceed while fake:1 is held"
            );
            assert!(
                !s.inventory_calls.contains(&"fake:1".to_string()),
                "fake:1's refresh must not run while fake:1's operation holds its lock"
            );
            assert_eq!(s.detect_calls, 1, "the adapter's detect is not run this round");
        }
        // Carried forward *unchanged*: the same instance, no notice, no
        // stale flag, its rows as they were.
        let fake_1 = snapshot.instances.iter().find(|i| i.id == "fake:1").expect("fake:1");
        assert_eq!(
            fake_1,
            first.instances.iter().find(|i| i.id == "fake:1").unwrap()
        );
        assert!(snapshot.artifacts.iter().any(|a| a.key.name == "jq"));
        assert!(snapshot.artifacts.iter().any(|a| a.key.name == "wget"));
        assert!(!snapshot.stale);
        assert!(snapshot.errors.is_empty(), "{:?}", snapshot.errors);

        // And once the operation is over, the next refresh reads again.
        session.cancel(op_id).expect("cancel a Running op");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !session
            .operations()
            .iter()
            .any(|o| o.id == op_id && o.status == OpStatus::Done)
        {
            assert!(Instant::now() < deadline, "the cancelled operation never finished");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        state.lock().unwrap().inventory_calls.clear();
        session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        let s = state.lock().unwrap();
        assert_eq!(s.detect_calls, 2);
        assert!(s.inventory_calls.contains(&"fake:1".to_string()));
    }

    #[tokio::test]
    async fn test_an_operation_on_another_adapters_instance_does_not_skip_this_adapters_detect() {
        // The skip is by the held lock's name against the adapter's own
        // previous instances: an operation on `a:1` leaves adapter `b`
        // entirely alone.
        let (a, a_state) = FakeAdapter::new("a");
        let (b, b_state) = FakeAdapter::new("b");
        {
            let mut s = a_state.lock().unwrap();
            s.instances = vec![make_instance("a", "a:1")];
            s.artifacts
                .insert("a:1".to_string(), vec![make_artifact("a:1", "jq")]);
            s.block_execute = true;
        }
        {
            let mut s = b_state.lock().unwrap();
            s.instances = vec![make_instance("b", "b:1")];
            s.artifacts
                .insert("b:1".to_string(), vec![make_artifact("b:1", "wget")]);
        }
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![a, b], None);
        session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        b_state.lock().unwrap().inventory_calls.clear();

        let req = OpRequest {
            kind: OpKind::Install,
            instance_id: "a:1".to_string(),
            artifact_kind: ArtifactKind::Formula,
            name: "jq".to_string(),
        };
        let issued = session.issue_plan(&req).await.expect("issue_plan");
        let op_id = session.submit(issued.id).expect("submit");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !session
            .operations()
            .iter()
            .any(|o| o.id == op_id && o.status == OpStatus::Running)
        {
            assert!(Instant::now() < deadline, "operation never reached Running");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        session
            .refresh(&non_root_env(), &CheckOptions::default())
            .await;
        assert_eq!(a_state.lock().unwrap().detect_calls, 1, "a is skipped");
        assert_eq!(b_state.lock().unwrap().detect_calls, 2, "b is detected");
        assert!(b_state.lock().unwrap().inventory_calls.contains(&"b:1".to_string()));
        session.cancel(op_id).expect("cancel");
    }

    /// rustup's native layout in a temp home, built by hand
    /// (`adapters::standalone::testing` is not visible from here): the
    /// launcher, its `cargo` proxy link, and a `HostEnv` whose `PATH`
    /// finds the proxy so the cargo adapter detects too.
    fn rustup_home() -> (PathBuf, HostEnv) {
        use std::os::unix::fs::PermissionsExt;
        let raw = std::env::temp_dir().join(format!(
            "banager-refresh-rustup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&raw).expect("temp home");
        let home = std::fs::canonicalize(&raw).expect("canonical temp home");
        let bin = home.join(".cargo/bin");
        std::fs::create_dir_all(&bin).expect("cargo bin");
        let launcher = bin.join("rustup");
        std::fs::write(&launcher, b"#!/bin/sh\n").expect("rustup");
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755))
            .expect("executable rustup");
        std::os::unix::fs::symlink("rustup", bin.join("cargo")).expect("cargo proxy");
        let env = HostEnv {
            path_dirs: vec![bin],
            home: home.clone(),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        (home, env)
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

    #[tokio::test]
    async fn test_a_refresh_during_rustups_self_update_runs_neither_rustup_nor_cargo() {
        // Review Focus #6, with the real adapters: `rustup self update`
        // holds `standalone-rustup` and `cargo:<home>` (Task 6), and while
        // it runs a refresh must not run the rustup binary at all -- not
        // as `rustup --version`, not as `cargo --version` (the proxy is
        // the same binary, and both begin by deleting the updater the
        // operation is about to run: plan ruling 4). Both rows are carried
        // forward unchanged; the next refresh after the operation reads
        // again.
        use crate::adapters::cargo::CargoAdapter;
        use crate::adapters::standalone::recipes::RUSTUP;
        use crate::adapters::standalone::StandaloneAdapter;
        use crate::http::{HttpResponse, MockHttpClient};
        use crate::model::{Attention, ResourceLock};
        use crate::trash::MockTrasher;

        let (home, env) = rustup_home();
        let launcher = home.join(".cargo/bin/rustup").to_string_lossy().to_string();
        let cargo = home.join(".cargo/bin/cargo").to_string_lossy().to_string();
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![launcher.as_str(), "--version"],
            exited_0("rustup 1.29.1 (d95a37b6a 2026-08-13)\n"),
        );
        runner.respond(
            vec![cargo.as_str(), "--version"],
            exited_0("cargo 1.98.1 (797e8a9bc 2026-08-05)\n"),
        );
        // Illustrative log text (never recorded: the recording rules
        // forbid running it); the outcome rests on the exit code and the
        // two version readings alone. Slow enough for a refresh to land
        // while it runs.
        runner.respond(
            vec![launcher.as_str(), "self", "update"],
            exited_0("  rustup unchanged - 1.29.1\n"),
        );
        runner.delay(
            vec![launcher.as_str(), "self", "update"],
            Duration::from_millis(400),
        );
        let http = Arc::new(MockHttpClient::new());
        http.respond(
            "https://static.rust-lang.org/rustup/release-stable.toml",
            HttpResponse {
                status: 200,
                body: "schema-version = '1'\nversion = '1.29.1'\n".to_string(),
            },
        );
        let rustup: Arc<dyn Adapter> = Arc::new(StandaloneAdapter::new(
            &RUSTUP,
            runner.clone(),
            http.clone(),
            Arc::new(MockTrasher::new()),
        ));
        let cargo_adapter: Arc<dyn Adapter> =
            Arc::new(CargoAdapter::new(runner.clone(), http.clone()));
        let sink = Arc::new(VecSink::new());
        let session = Session::with_adapters(sink, vec![rustup.clone(), cargo_adapter], None);

        let first = session.refresh(&env, &CheckOptions::default()).await;
        let mut ids: Vec<&str> = first.instances.iter().map(|i| i.id.as_str()).collect();
        ids.sort();
        let cargo_id = format!("cargo:{}", home.join(".cargo").display());
        assert_eq!(ids, vec![cargo_id.as_str(), "standalone-rustup"]);
        assert!(first.errors.is_empty(), "{:?}", first.errors);

        let inst = first
            .instances
            .iter()
            .find(|i| i.id == "standalone-rustup")
            .expect("rustup's instance")
            .clone();
        let req = OpRequest {
            kind: OpKind::Upgrade,
            instance_id: "standalone-rustup".to_string(),
            artifact_kind: ArtifactKind::Binary,
            name: "rustup".to_string(),
        };
        let plan = rustup.plan(&inst, &req).await.expect("plan");
        assert_eq!(
            plan.locks,
            vec![
                ResourceLock("standalone-rustup".to_string()),
                ResourceLock(cargo_id.clone())
            ]
        );
        let op_id = session.ops.submit(plan);
        // Until the operation's own before-reading is done and `self
        // update` is under way: from here every call is the refresh's.
        let deadline = Instant::now() + Duration::from_secs(2);
        while !runner
            .calls()
            .iter()
            .any(|c| c.len() == 3 && c[1] == "self" && c[2] == "update")
        {
            assert!(Instant::now() < deadline, "self update never started");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(session.ops.locks_held().contains(&ResourceLock(cargo_id.clone())));

        let before = runner.calls().len();
        let during = session.refresh(&env, &CheckOptions::default()).await;
        let new_calls = runner.calls()[before..].to_vec();
        assert!(
            new_calls.is_empty(),
            "a refresh during rustup's self update ran {new_calls:?}"
        );
        assert_eq!(during.instances, first.instances);
        assert_eq!(during.artifacts, first.artifacts);
        assert!(!during.stale);
        assert!(during.errors.is_empty(), "{:?}", during.errors);

        assert_eq!(
            session.ops.wait(op_id).await,
            Some(Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade))
        );
        assert!(session.ops.locks_held().is_empty());
        let before = runner.calls().len();
        session.refresh(&env, &CheckOptions::default()).await;
        let after: Vec<Vec<String>> = runner.calls()[before..].to_vec();
        assert!(
            after.iter().any(|c| c[0] == launcher && c[1] == "--version"),
            "after the operation, rustup is read again: {after:?}"
        );
        assert!(
            after.iter().any(|c| c[0] == cargo && c[1] == "--version"),
            "and so is cargo: {after:?}"
        );
        let _ = std::fs::remove_dir_all(&home);
    }
```

and add to the test module's `use` lines whatever of `std::path::PathBuf`, `crate::runner::CommandOutput` it does not already import (it imports `CommandOutput`, `HostEnv`, `MockRunner` from `crate::runner` at `:612`; add `PathBuf` to `use std::path::Path;` → `use std::path::{Path, PathBuf};`).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib session::refresh::tests`
Expected: FAIL — the rustup test does not compile (`no method named \`locks_held\` found`); the renamed test fails at `"a refresh must not wait for an operation on one instance"` (the refresh waits on `fake:1`'s lock until the cancel, which the test no longer sends first); the two-adapter test fails at `"a is skipped"` (`detect_calls` is 2).

- [ ] **Step 3: The held-locks snapshot, the skip, the carry-forward**

In `crates/banager-core/src/ops/mod.rs`, inside `impl OperationManager` (the one holding `acquire_resource_lock`, at the end of the file), before `pub async fn acquire_resource_lock`, insert:

```rust
    /// The resource locks held this instant: by operations from the
    /// moment `run_operation` acquires theirs until `finish` releases
    /// them, and by a refresh's per-instance fetches
    /// (`acquire_resource_lock`). A snapshot of the same `held` set both
    /// of those use, read by `Session::refresh_round` before its
    /// detection fan-out so that an adapter whose instance an operation is
    /// working on is neither detected nor inventoried that round (phase 4
    /// step E: `rustup self update` replaces the binary that both
    /// `rustup --version` and, through the `cargo` proxy, `cargo
    /// --version` would run). It decides nothing about acquisition, which
    /// stays in `run_operation`'s own loop.
    pub fn locks_held(&self) -> HashSet<ResourceLock> {
        self.held.lock().unwrap().clone()
    }

```

In `crates/banager-core/src/session/refresh.rs`, add `InstanceId` to the `use crate::model::{…}` line and `use std::collections::HashSet;` to the imports. After the `use` lines and before `impl Session`, insert:

```rust

/// One adapter's part in a round's detection: spawned, or skipped with
/// last round's instances because an operation holds one of them
/// (`refresh_round`).
enum Detection {
    Spawned(AbortOnDropHandle<Vec<ManagerInstance>>),
    Skipped(Vec<ManagerInstance>),
}
```

Then, in `refresh_round`, replace the lines (the comment block above them, from `// Fanned out in adapter-id order` through `// cancellation, and nothing but dropping this future cancels.`, stays)

```rust
        let mut adapters: Vec<_> = self.adapters.values().collect();
        adapters.sort_by(|a, b| a.meta().id.cmp(&b.meta().id));
        let mut detect_handles = Vec::with_capacity(adapters.len());
        for adapter in adapters {
            // Cloned into the task because `tokio::spawn` needs a 'static
            // future: iterating `values()` by reference would tie it to
            // `&self`. (Written as an explicit clone rather than
            // `.values().cloned()` only because clippy's
            // `unnecessary_to_owned` misreads the latter here.)
            let adapter = adapter.clone();
            let env = env.clone();
            detect_handles.push((
                adapter.meta().id.clone(),
                AbortOnDropHandle::new(tokio::spawn(async move { adapter.detect(&env).await })),
            ));
        }
        let mut instances = Vec::new();
        let mut detect_errors = Vec::new();
        for (adapter_id, handle) in detect_handles {
            match handle.await {
                Ok(found) => instances.extend(found),
```

with

```rust
        // An adapter one of whose instances an operation is holding right
        // now is not asked anything this round (phase 4 step E). Its
        // `detect` runs the tool's own binary -- and on a Mac with rustup,
        // `rustup self update` replaces that very binary while it holds
        // both `standalone-rustup` and the cargo instance's lock, and the
        // `cargo` the cargo adapter would run is the same binary in proxy
        // mode, which begins by deleting the updater the operation is
        // about to run. So: the held set is read once here; every adapter
        // with a previous-round instance whose id is a held lock keeps
        // last round's instances unchanged (no notice, no `stale`: nothing
        // failed), and the per-instance loop below carries the held
        // instances' rows forward instead of waiting on their lock -- a
        // refresh used to wait out a `brew install` for minutes. Instances
        // of a skipped adapter that are not themselves held are still
        // inventoried under their own lock. The operation's own reading
        // afterwards (`run_operation`'s reconcile, under its locks) and the
        // refresh the front end runs when it finishes replace these rows.
        // Read once, so an operation submitted after this line may start
        // while a detect it would have skipped is running its one
        // `--version`: that window is the command's duration.
        let held = self.ops.locks_held();
        let mut under_operation: HashSet<InstanceId> = HashSet::new();
        let mut adapters: Vec<_> = self.adapters.values().collect();
        adapters.sort_by(|a, b| a.meta().id.cmp(&b.meta().id));
        let mut detections = Vec::with_capacity(adapters.len());
        for adapter in adapters {
            let adapter_id = adapter.meta().id.clone();
            let carried: Vec<ManagerInstance> = previous
                .instances
                .iter()
                .filter(|i| i.adapter_id == adapter_id)
                .cloned()
                .collect();
            let held_here: Vec<InstanceId> = carried
                .iter()
                .filter(|i| held.contains(&ResourceLock(i.id.clone())))
                .map(|i| i.id.clone())
                .collect();
            if !held_here.is_empty() {
                under_operation.extend(held_here);
                detections.push((adapter_id, Detection::Skipped(carried)));
                continue;
            }
            // Cloned into the task because `tokio::spawn` needs a 'static
            // future: iterating `values()` by reference would tie it to
            // `&self`. (Written as an explicit clone rather than
            // `.values().cloned()` only because clippy's
            // `unnecessary_to_owned` misreads the latter here.)
            let adapter = adapter.clone();
            let env = env.clone();
            detections.push((
                adapter_id,
                Detection::Spawned(AbortOnDropHandle::new(tokio::spawn(async move {
                    adapter.detect(&env).await
                }))),
            ));
        }
        let mut instances = Vec::new();
        let mut detect_errors = Vec::new();
        for (adapter_id, detection) in detections {
            let handle = match detection {
                Detection::Skipped(carried) => {
                    instances.extend(carried);
                    continue;
                }
                Detection::Spawned(handle) => handle,
            };
            match handle.await {
                Ok(found) => instances.extend(found),
```

(the `Err(_join_err) => { … }` arm and everything after it stay as they are). Then, in the per-instance loop, replace

```rust
            if inst.status.unavailable.is_some() {
```

with

```rust
            // And an instance an operation is holding (`under_operation`,
            // above): its rows are last round's, and this round does not
            // wait for the operation's lock to take them again.
            if inst.status.unavailable.is_some() || under_operation.contains(&inst.id) {
```

(the comment block above that `if`, about a source that already said it is not answering, stays; the branch's body is unchanged).

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib session` and `cargo test -p banager-core --test ops_lock_test --test ops_cancel_test --test ops_outcome_test`
Expected: PASS — the renamed test, the two new ones, every other `session` test (`test_dropping_a_refresh_cancels_its_workers_and_releases_their_locks` submits its operation after the dropped refresh's lock is free, and `test_a_refresh_behind_a_running_brew_update_reads_nothing_and_keeps_the_previous_rows` concerns brew's own in-adapter state, not this set), and every ops test (acquisition is untouched).

- [ ] **Step 5: Run the gates**

Run `cargo fmt --all`, then all five from Global Constraints. Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/ops/mod.rs crates/banager-core/src/session/refresh.rs
git commit -m "$(cat <<'EOF'
Leave an adapter alone in a refresh while an operation holds its instance

A refresh detects every adapter before it takes any lock, and detect runs
the tool's own binary. On a Mac with rustup, rustup self update replaces
that binary while it holds rustup's lock and cargo's, and the cargo the
cargo adapter would run is the same binary in proxy mode, which begins
by deleting the updater the operation is about to run. So a refresh now
reads the held locks once, skips the detection of every adapter one of
whose instances is held, keeps that adapter's instances from last round
unchanged, and carries the held instances' rows forward instead of
waiting on their lock -- which for a brew install it used to do for
minutes. Every other instance, of any adapter, is refreshed as before.

Co-Authored-By: <the executing session's attribution line>
EOF
)"
```

---

### Task 8: End to end through `OperationManager` — the upgrade's two outcomes, the uninstall's four, the lock cases

**Files:**
- Create: `crates/banager-core/tests/ops_rustup_uninstall_test.rs`
- Modify: `crates/banager-core/tests/ops_upgrade_version_test.rs` — imports (`:33-49` at `ea30cfb`; B added `banager_core::adapters::standalone::…` lines), one section appended at the end  [B's file]
- Test: both files.

**Interfaces:**
- Consumes: `RUSTUP` (Task 6), `StandaloneAdapter::new(…, trasher)` and `MockTrasher` (C), `Adapter::reconcile_after_uninstall` and `probe_strict`'s `FlatFile` arms (C, Task 4); `OperationManager::{new, register_adapter, register_instance, submit, wait, locks_held}` (Task 7); `HostEnv` with its two new fields (Task 1); `ScriptedRunner::script`, `exited_0(stdout, stderr)`, `upgrade(&runner, adapter, inst, kind, name) -> Outcome` (`ops_upgrade_version_test.rs:63-166`); `MockRunner`.
- Produces: tests only. They pin `run_operation`'s arms (`ops/mod.rs`): exit 0 + the launcher gone → `Succeeded`; exit 0 + the launcher there → `NeedsAttention(StillInstalledAfterUninstall)`; a timeout + the launcher there → `Unconfirmed`; a timeout + the launcher gone → `Succeeded` (the `Ok(Outcome::Unconfirmed)` arm's `OpKind::Uninstall` branch, `:796-800`: presence decides, and the user cannot have cancelled a `NoCancel` op); an upgrade that exits 0 with the version unchanged → `NeedsAttention(UnchangedAfterUpgrade)`, one that moved it → `Succeeded`; an operation whose plan names a cargo lock no registered cargo instance backs still acquires it by name and runs; a plan for an instance the seat no longer describes is refused before anything is submitted.

Why a separate file for the uninstall: `ops_upgrade_version_test.rs` is about upgrades and its `upgrade` helper submits an `OpKind::Upgrade`; the uninstall's harness mutates the disk as the command "returns" (the way `ClaudeMutationRunner` does at `:570-592`) and reads presence, which is C's `reconcile_after_uninstall` through the `Adapter` trait.

- [ ] **Step 1: Write the tests (they pass against the engine as it stands, and are kept as regressions)**

Append to `crates/banager-core/tests/ops_upgrade_version_test.rs` (at the end of the file), and add `use banager_core::adapters::standalone::recipes::RUSTUP;` beside B's `use banager_core::adapters::standalone::recipes::CLAUDE;` (or merge into one `use banager_core::adapters::standalone::recipes::{CLAUDE, RUSTUP};`), `use banager_core::runner::HostEnv;` and `use banager_core::trash::MockTrasher;` (C's; skip it if C already imports it for its claude helpers) to the imports:

```rust

// --- rustup (standalone, phase 4 step E) -----------------------------------

/// A native rustup layout in a temp home: `~/.cargo/bin/rustup`, an
/// executable regular file, and its `cargo` proxy link. The adapter's
/// inventory probes the disk, so the file has to be real; the runner
/// scripts only the two commands. `detect` is run rather than an instance
/// built by hand because rustup's upgrade plan reads what detect seated
/// (the Cargo home, for the cargo lock) and checks it describes the
/// instance.
async fn rustup_home_and_instance(
    runner: &Arc<ScriptedRunner>,
    versions: &[&str],
) -> (PathBuf, Arc<StandaloneAdapter>, ManagerInstance) {
    use std::os::unix::fs::PermissionsExt;
    let home = std::env::temp_dir().join(format!(
        "banager-ops-rustup-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let bin = home.join(".cargo/bin");
    std::fs::create_dir_all(&bin).expect("cargo bin");
    let launcher = bin.join("rustup");
    std::fs::write(&launcher, b"#!/bin/sh\n").expect("rustup binary");
    // Executable, as the installer leaves it and as the unit tests'
    // `rustup_layout` builds it (a 0644 file would only add a harmless
    // `NotOnPath` note here, but the two synthetic layouts agree).
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755))
        .expect("executable rustup");
    std::os::unix::fs::symlink("rustup", bin.join("cargo")).expect("cargo proxy");
    let launcher_str = launcher.to_string_lossy().to_string();
    runner.script(
        &[launcher_str.as_str(), "--version"],
        versions
            .iter()
            .map(|v| exited_0(&format!("rustup {v} (d95a37b6a 2026-08-13)\n"), ""))
            .collect(),
    );
    let adapter = Arc::new(StandaloneAdapter::new(
        &RUSTUP,
        runner.clone(),
        Arc::new(MockHttpClient::new()),
        // C's fourth argument; nothing here goes to the Trash.
        Arc::new(MockTrasher::new()),
    ));
    let env = HostEnv {
        path_dirs: vec![bin],
        home: home.clone(),
        euid: 501,
        cargo_home: None,
        rustup_home: None,
        zdotdir: None,
        ollama_host: None,
    };
    let inst = adapter.detect(&env).await.remove(0);
    (home, adapter, inst)
}

#[tokio::test]
async fn test_a_rustup_self_update_that_changed_nothing_is_not_reported_as_updated() {
    // `rustup self update` was never run to record this (the recording
    // rules forbid it), so the log text below is illustrative; as this
    // file's doc says, the outcome depends only on the exit code and the
    // two `--version` readings, which here are the same. The first
    // scripted reading is detect's, then the reading before, then the
    // one after (the last repeats).
    let runner = Arc::new(ScriptedRunner::default());
    let (home, adapter, inst) = rustup_home_and_instance(&runner, &["1.29.1"]).await;
    let launcher = inst.exe_path.to_string_lossy().to_string();
    runner.script(
        &[launcher.as_str(), "self", "update"],
        vec![exited_0(
            "  rustup unchanged - 1.29.1\n",
            "info: checking for self-update\n",
        )],
    );
    let outcome = upgrade(&runner, adapter, inst, ArtifactKind::Binary, "rustup").await;
    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(
        outcome,
        Outcome::NeedsAttention(Attention::UnchangedAfterUpgrade)
    );
}

#[tokio::test]
async fn test_a_rustup_self_update_that_moved_the_version_succeeded() {
    let runner = Arc::new(ScriptedRunner::default());
    let (home, adapter, inst) = rustup_home_and_instance(&runner, &["1.29.1", "1.29.1", "1.30.0"]).await;
    let launcher = inst.exe_path.to_string_lossy().to_string();
    runner.script(
        &[launcher.as_str(), "self", "update"],
        vec![exited_0("  rustup updated - 1.30.0 (from 1.29.1)\n", "")],
    );
    let outcome = upgrade(&runner, adapter, inst, ArtifactKind::Binary, "rustup").await;
    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(outcome, Outcome::Succeeded);
}

#[tokio::test]
async fn test_a_rustup_self_update_stopped_by_the_timeout_is_unconfirmed_whatever_the_readings_say() {
    // `rustup self update` is NoCancel, so the timeout is its only stop,
    // and a stopped upgrade is `Unconfirmed` unconditionally (the
    // `Ok(Outcome::Unconfirmed)` arm of `run_operation`, ops/mod.rs): an
    // unlinked-then-copied binary may read either version partway.
    for versions in [&["1.29.1", "1.29.1", "1.29.1"][..], &["1.29.1", "1.29.1", "1.30.0"][..]] {
        let runner = Arc::new(ScriptedRunner::default());
        let (home, adapter, inst) = rustup_home_and_instance(&runner, versions).await;
        let launcher = inst.exe_path.to_string_lossy().to_string();
        runner.script(&[launcher.as_str(), "self", "update"], vec![Stop::Timeout.output()]);
        let outcome = upgrade(&runner, adapter, inst, ArtifactKind::Binary, "rustup").await;
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(outcome, Outcome::Unconfirmed, "{versions:?}");
    }
}
```

Create `crates/banager-core/tests/ops_rustup_uninstall_test.rs`:

```rust
//! `rustup self uninstall -y`'s outcome, end to end through
//! `OperationManager` with the real `StandaloneAdapter` over the `RUSTUP`
//! recipe: its own `plan`, its own `execute` (`run_plan`), and C's
//! `reconcile_after_uninstall` reading presence of the launcher. The
//! command is scripted, and the runner removes files as the command
//! "returns", the way rustup's own uninstall leaves the disk in each
//! case (phase 4 step E, plan ruling 6's outcome list):
//!
//! - exit 0, the launcher gone, other `bin/` files left (rustup could not
//!   remove the folder): `Succeeded` -- presence of the launcher is what
//!   is read;
//! - exit 0, the launcher still there: `NeedsAttention(StillInstalledAfterUninstall)`;
//! - stopped by the timeout, the launcher still there: `Unconfirmed`;
//! - stopped by the timeout, the launcher gone: `Succeeded` (the
//!   `Ok(Outcome::Unconfirmed)` arm's `Uninstall` branch, ops/mod.rs:
//!   presence decides, and a NoCancel op cannot have been cancelled).
//!
//! And the lock cases: rustup's plan names the cargo instance's lock
//! whether or not a cargo instance is registered (the name is what the
//! engine compares), and a plan for an instance the adapter's seat no
//! longer describes is refused before anything is submitted.

use async_trait::async_trait;
use banager_core::adapters::standalone::recipes::RUSTUP;
use banager_core::adapters::standalone::StandaloneAdapter;
use banager_core::adapters::{Adapter, AdapterError};
use banager_core::events::VecSink;
use banager_core::http::MockHttpClient;
use banager_core::model::{
    ArtifactKind, Attention, ManagerInstance, OpKind, OpRequest, Outcome, ResourceLock,
};
use banager_core::ops::OperationManager;
use banager_core::runner::{
    CommandOutput, CommandRunner, CommandSpec, HostEnv, LineCallback, MockRunner, RunnerError,
};
use banager_core::trash::MockTrasher;
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

const VERSION_LINE: &str = "rustup 1.29.1 (d95a37b6a 2026-08-13)\n";

fn exited_0(stdout: &str) -> CommandOutput {
    CommandOutput {
        exit_code: Some(0),
        stdout: stdout.to_string(),
        stderr: String::new(),
        timed_out: false,
        cancelled: false,
    }
}

fn timed_out() -> CommandOutput {
    CommandOutput {
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
        timed_out: true,
        cancelled: false,
    }
}

/// What the scripted uninstall leaves on the disk as it returns.
#[derive(Clone, Copy, Debug)]
enum Leaves {
    /// rustup got as far as deleting its own binary but not the folder:
    /// `bin/rustup` gone, `bin/hexyl` and the proxies still there.
    LauncherGoneOthersLeft,
    /// Everything: the whole Cargo home is gone.
    CargoHomeGone,
    /// Nothing changed.
    Everything,
}

/// A rustup layout in a temp home: the launcher (an executable regular
/// file), its `cargo` proxy link, a `cargo install`ed `hexyl`, and one
/// toolchain directory under `~/.rustup`.
struct RustupHome {
    home: PathBuf,
    env: HostEnv,
}

impl RustupHome {
    fn new(tag: &str) -> RustupHome {
        use std::os::unix::fs::PermissionsExt;
        let raw = std::env::temp_dir().join(format!(
            "banager-ops-rustup-uninstall-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&raw).expect("temp home");
        let home = std::fs::canonicalize(&raw).expect("canonical temp home");
        let bin = home.join(".cargo/bin");
        std::fs::create_dir_all(&bin).expect("cargo bin");
        let launcher = bin.join("rustup");
        std::fs::write(&launcher, b"#!/bin/sh\n").expect("rustup");
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755))
            .expect("executable rustup");
        std::os::unix::fs::symlink("rustup", bin.join("cargo")).expect("cargo proxy");
        std::fs::write(bin.join("hexyl"), b"#!/bin/sh\n").expect("hexyl");
        std::fs::create_dir_all(home.join(".rustup/toolchains/stable-aarch64-apple-darwin"))
            .expect("toolchain");
        let env = HostEnv {
            path_dirs: vec![bin],
            home: home.clone(),
            euid: 501,
            cargo_home: None,
            rustup_home: None,
            zdotdir: None,
            ollama_host: None,
        };
        RustupHome { home, env }
    }

    fn launcher(&self) -> PathBuf {
        self.home.join(".cargo/bin/rustup")
    }

    fn cargo_id(&self) -> String {
        format!("cargo:{}", self.home.join(".cargo").display())
    }
}

impl Drop for RustupHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

/// Answers from the inner mock, then changes the disk the way the
/// scripted uninstall would have as it returns.
struct UninstallingRunner {
    inner: Arc<MockRunner>,
    home: PathBuf,
    leaves: Leaves,
}

#[async_trait]
impl CommandRunner for UninstallingRunner {
    async fn run(
        &self,
        spec: CommandSpec,
        on_line: Option<LineCallback>,
        cancel: CancellationToken,
    ) -> Result<CommandOutput, RunnerError> {
        let is_uninstall = spec.args == ["self", "uninstall", "-y"];
        let output = self.inner.run(spec, on_line, cancel).await?;
        if is_uninstall {
            match self.leaves {
                Leaves::LauncherGoneOthersLeft => {
                    std::fs::remove_file(self.home.join(".cargo/bin/rustup")).expect("unlink rustup");
                }
                Leaves::CargoHomeGone => {
                    std::fs::remove_dir_all(self.home.join(".cargo")).expect("remove cargo home");
                    std::fs::remove_dir_all(self.home.join(".rustup")).expect("remove rustup home");
                }
                Leaves::Everything => {}
            }
        }
        Ok(output)
    }
}

/// Detects rustup in `home` over a mock that answers `--version` and the
/// uninstall with `uninstall_output`, leaving `leaves` behind.
async fn rustup_adapter(
    home: &RustupHome,
    uninstall_output: CommandOutput,
    leaves: Leaves,
) -> (Arc<StandaloneAdapter>, ManagerInstance, Arc<MockRunner>) {
    let mock = Arc::new(MockRunner::new());
    let launcher = home.launcher().to_string_lossy().to_string();
    mock.respond(vec![launcher.as_str(), "--version"], exited_0(VERSION_LINE));
    mock.respond(
        vec![launcher.as_str(), "self", "uninstall", "-y"],
        uninstall_output,
    );
    let runner = Arc::new(UninstallingRunner {
        inner: mock.clone(),
        home: home.home.clone(),
        leaves,
    });
    let adapter = Arc::new(StandaloneAdapter::new(
        &RUSTUP,
        runner,
        Arc::new(MockHttpClient::new()),
        Arc::new(MockTrasher::new()),
    ));
    let inst = adapter.detect(&home.env).await.remove(0);
    (adapter, inst, mock)
}

fn uninstall_request() -> OpRequest {
    OpRequest {
        kind: OpKind::Uninstall,
        instance_id: "standalone-rustup".to_string(),
        artifact_kind: ArtifactKind::Binary,
        name: "rustup".to_string(),
    }
}

/// Plans and submits the uninstall through a fresh `OperationManager`
/// holding only the rustup adapter and instance, and returns the
/// outcome and the manager (for its held locks).
async fn uninstall(
    adapter: Arc<StandaloneAdapter>,
    inst: ManagerInstance,
) -> (Outcome, Arc<OperationManager>) {
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    manager.register_instance(inst.clone());
    let plan = adapter.plan(&inst, &uninstall_request()).await.expect("plan");
    let op_id = manager.submit(plan);
    let outcome = manager.wait(op_id).await.expect("an outcome");
    (outcome, manager)
}

#[tokio::test]
async fn test_an_uninstall_exiting_zero_with_the_launcher_gone_and_other_bin_files_left_succeeded() {
    let home = RustupHome::new("launcher-gone");
    let (adapter, inst, _mock) = rustup_adapter(
        &home,
        exited_0("info: rustup is uninstalled\n"),
        Leaves::LauncherGoneOthersLeft,
    )
    .await;
    let (outcome, manager) = uninstall(adapter, inst).await;
    assert_eq!(outcome, Outcome::Succeeded);
    assert!(home.home.join(".cargo/bin/hexyl").exists(), "presence is the launcher's");
    assert!(manager.locks_held().is_empty());
}

#[tokio::test]
async fn test_an_uninstall_exiting_zero_with_the_launcher_still_there_needs_attention() {
    let home = RustupHome::new("launcher-left");
    let (adapter, inst, _mock) = rustup_adapter(
        &home,
        exited_0("info: rustup is uninstalled\n"),
        Leaves::Everything,
    )
    .await;
    let (outcome, _manager) = uninstall(adapter, inst).await;
    assert_eq!(
        outcome,
        Outcome::NeedsAttention(Attention::StillInstalledAfterUninstall)
    );
}

#[tokio::test]
async fn test_an_uninstall_stopped_by_the_timeout_before_the_launcher_went_is_unconfirmed() {
    // NoCancel: the timeout is the only stop. The launcher is still
    // there, nobody cancelled, so nothing can be said.
    let home = RustupHome::new("timeout-before");
    let (adapter, inst, _mock) = rustup_adapter(&home, timed_out(), Leaves::Everything).await;
    let (outcome, _manager) = uninstall(adapter, inst).await;
    assert_eq!(outcome, Outcome::Unconfirmed);
}

#[tokio::test]
async fn test_an_uninstall_stopped_by_the_timeout_after_the_launcher_went_succeeded() {
    // The reading after tells the artifact's current state: gone is
    // gone, whatever stopped the command.
    let home = RustupHome::new("timeout-after");
    let (adapter, inst, _mock) = rustup_adapter(&home, timed_out(), Leaves::CargoHomeGone).await;
    let (outcome, _manager) = uninstall(adapter, inst).await;
    assert_eq!(outcome, Outcome::Succeeded);
}

#[tokio::test]
async fn test_an_uninstall_with_no_cargo_instance_registered_still_holds_the_cargo_lock_by_name() {
    // Spec §2.4: the second lock is a name, `cargo:<cargo_home>`, and the
    // engine compares names; a cargo instance need not exist for the op
    // to take it and release it.
    let home = RustupHome::new("cargo-absent");
    let (adapter, inst, _mock) = rustup_adapter(
        &home,
        exited_0("info: rustup is uninstalled\n"),
        Leaves::CargoHomeGone,
    )
    .await;
    let plan = adapter.plan(&inst, &uninstall_request()).await.expect("plan");
    assert_eq!(
        plan.locks,
        vec![
            ResourceLock("standalone-rustup".to_string()),
            ResourceLock(home.cargo_id()),
        ]
    );
    let (outcome, manager) = uninstall(adapter, inst).await;
    assert_eq!(outcome, Outcome::Succeeded);
    assert!(manager.locks_held().is_empty(), "both names released");
}

#[tokio::test]
async fn test_a_plan_for_an_instance_from_another_home_is_refused_and_a_redetect_restores_it() {
    // Plan ruling 9: the adapter's seat is one slot. Detect A, detect B,
    // plan for A: refused, so nothing with B's cargo lock and B's
    // warnings ever reaches the manager for A's launcher. Detect A again,
    // and the plan goes through with A's lock.
    let home_a = RustupHome::new("seat-a");
    let home_b = RustupHome::new("seat-b");
    let mock = Arc::new(MockRunner::new());
    for home in [&home_a, &home_b] {
        mock.respond(
            vec![home.launcher().to_str().unwrap(), "--version"],
            exited_0(VERSION_LINE),
        );
    }
    let adapter = StandaloneAdapter::new(
        &RUSTUP,
        mock.clone(),
        Arc::new(MockHttpClient::new()),
        Arc::new(MockTrasher::new()),
    );
    let inst_a = adapter.detect(&home_a.env).await.remove(0);
    let _inst_b = adapter.detect(&home_b.env).await.remove(0);
    assert!(matches!(
        adapter.plan(&inst_a, &uninstall_request()).await,
        Err(AdapterError::Refused(_))
    ));
    adapter.detect(&home_a.env).await;
    let plan = adapter.plan(&inst_a, &uninstall_request()).await.expect("plan for A again");
    assert_eq!(plan.locks[1], ResourceLock(home_a.cargo_id()));
}
```

- [ ] **Step 2: Run to verify it passes**

Run: `cargo test -p banager-core --test ops_upgrade_version_test` and `cargo test -p banager-core --test ops_rustup_uninstall_test`
Expected: PASS — the three upgrade tests and the six uninstall tests. These pass against the engine as it stands: the recipe (Task 6), C's `reconcile_after_uninstall` and Task 4's `FlatFile` arms in `probe_strict` are what they exercise, and they are here because this is where each of the outcomes the trust file describes becomes a test rather than a sentence. If `test_an_uninstall_exiting_zero_with_the_launcher_gone_and_other_bin_files_left_succeeded` fails with `StillInstalledAfterUninstall`, C's `reconcile_after_uninstall` is answering presence of something other than the launcher: read it (C checklist row 6) and write down what it reads before touching anything.

- [ ] **Step 3: Run the gates**

Run `cargo fmt --all`, then all five from Global Constraints. Expected: all clean.

- [ ] **Step 4: Commit**

```bash
git add crates/banager-core/tests/ops_upgrade_version_test.rs crates/banager-core/tests/ops_rustup_uninstall_test.rs
git commit -m "$(cat <<'EOF'
Prove rustup's outcomes end to end: unchanged, updated, stopped, gone, still there

Through the operation manager with the real adapter over the rustup
recipe: a self update that exits 0 with the version unchanged is
reported as unchanged, one that moved it as done, one the timeout
stopped as unconfirmed whatever the readings say; an uninstall is judged
by whether the launcher is still there -- done when it is gone even if
other files stayed or the timeout struck, needing attention when it is
there after exit 0, unconfirmed when the timeout struck with it there.
The cargo lock is taken by name whether or not a cargo instance exists,
and a plan for an instance the seat no longer describes is refused.

Co-Authored-By: <the executing session's attribution line>
EOF
)"
```

---

### Task 9: `NoCancel`'s first producer and `operations.noCancelHint`'s two readers

**Files:**
- Modify: `crates/banager-core/src/model.rs` — the doc comment on `NoCancel` inside `pub enum CancelPolicy` (the seven `///` lines directly above `NoCancel,`; C's `PlanAction` lands above the enum and moves it)  [B's/C's file: anchor by the variant]
- Modify: `crates/banager-core/tests/ops_cancel_test.rs` — the policy-matrix comment, the three lines reading `none produced yet; a standalone` … (`:742-744` at `ea30cfb`)
- Modify: `src-tauri/src/ipc.rs:629-630` (the fake adapter's `cancel_policy` field doc), `:1733` and `:1778` (the two test names)
- Modify: `src/lib/types.ts` — the doc comment above `export type CancelPolicy` (`:168-176` at `ea30cfb`)  [A's/C's file: anchor by the type]
- Modify: `src/components/OperationBar.tsx:32-37` (the comment above `cancellable`)
- Modify: `src/components/OperationBar.test.tsx:170-234` (the two `NoCancel` cases)
- Modify: `src/components/UninstallDialog.tsx` — after the `<CommandPreview … />` element  [C's file]
- Modify: `src/components/UninstallDialog.test.tsx` — one test appended inside `describe("UninstallDialog", …)`  [C's file]
- Modify: `src/pages/UpdatesPage.tsx` — after the `<CommandPreview … />` element inside `{item.issued !== null ? (…) : null}`  [B's/C's file]
- Modify: `src/pages/UpdatesPage.test.tsx` — the harness (`let needsPassword`, `issuedPlanFor`'s `cancel_policy` line, `beforeEach`) and one test appended inside `describe("UpdatesPage", …)`  [B's/C's file]
- Modify: `src/i18n/en.json`, `src/i18n/zh-CN.json` — `operations.noCancelHint`
- Test: `ops_cancel_test`, `ipc.rs`'s tests, `OperationBar.test.tsx`, `UninstallDialog.test.tsx`, `UpdatesPage.test.tsx`, `completeness.test.ts`.

**Interfaces:**
- Consumes: `RUSTUP` (Task 6) as the producer the sentences name; `Plan.cancel_policy` on the wire (`types.ts`); C's `CommandPreview action={…}` element in both readers (C checklist row 10); C's `issuedPlanFor` harnesses (row 11). The end-to-end proof that the plans are `NoCancel` and honest is Task 8's.
- Produces (verbatim): the key `operations.noCancelHint` (readers: `UninstallDialog.tsx` and `UpdatesPage.tsx`, both under the condition `plan.cancel_policy === "NoCancel"`); the four sentences now naming rustup; the renamed tests `test_cancel_operation_impl_refuses_a_running_no_cancel_op_such_as_rustup_self_update` and `test_cancel_operation_impl_cancels_a_queued_no_cancel_op_such_as_rustup_self_update`.

The hint is spec §9.2's: "Don't close Banager or your Mac while this runs. Stopping it partway leaves a broken installation, so this can't be cancelled once it starts." It is true of both rustup plans (Ruling 11 for `self update`; a directory-by-directory removal for `self uninstall`), and consistent with the Running-only rule: while the op is still Queued nothing has started, and `OperationBar` keeps its Cancel button.

- [ ] **Step 1: Write the failing tests**

In `src/components/OperationBar.test.tsx`, replace the two `NoCancel` cases (`:170-234`) with:

```tsx
  it("offers no Cancel button for a running rustup self update, whose plan says NoCancel", async () => {
    // `OperationManager::cancel` (ops/mod.rs) refuses such an op once it
    // is Running, so a button here would promise something the backend
    // will not do. rustup's `self update` and `self uninstall` are the
    // plans that say NoCancel (crates/banager-core/src/adapters/
    // standalone/recipes.rs): the first unlinks and re-copies the binary
    // every Rust proxy runs, the second removes Rust directory by
    // directory.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "list_operations") {
        return Promise.resolve([
          {
            id: 7,
            kind: "Upgrade",
            instance_id: "standalone-rustup",
            artifact_kind: "Binary",
            name: "rustup",
            status: "Running",
            outcome: null,
            argv_preview: ["/Users/me/.cargo/bin/rustup", "self", "update"],
            cancel_policy: "NoCancel",
          },
        ]);
      }
      return Promise.resolve(undefined);
    });

    const { findByText, queryByRole } = renderWithProviders(<OperationBar />);

    await findByText("Updating rustup — running");
    expect(queryByRole("button", { name: "Cancel" })).not.toBeInTheDocument();
  });

  it("offers Cancel for a queued rustup self update, whose plan says NoCancel, and it reaches cancel_operation", async () => {
    // A Queued op has started nothing, so `OperationManager::cancel`
    // (ops/mod.rs) accepts its cancel whatever the plan says and the
    // command never runs; NoCancel only bites once the op is Running.
    // Without the button the user could not drop a NoCancel op stuck
    // behind another op's lock -- rustup's plans also hold the cargo
    // instance's lock, so one queued behind a cargo install is real.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "list_operations") {
        return Promise.resolve([
          {
            id: 8,
            kind: "Upgrade",
            instance_id: "standalone-rustup",
            artifact_kind: "Binary",
            name: "rustup",
            status: "Queued",
            outcome: null,
            argv_preview: ["/Users/me/.cargo/bin/rustup", "self", "update"],
            cancel_policy: "NoCancel",
          },
        ]);
      }
      return Promise.resolve(undefined);
    });

    const { findByRole, findByText } = renderWithProviders(<OperationBar />);

    await findByText("Updating rustup — queued");
    const cancel = await findByRole("button", { name: "Cancel" });
    expect(cancel).toBeEnabled();
    fireEvent.click(cancel);
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("cancel_operation", { opId: 8 }),
    );
  });
```

In `src/components/UninstallDialog.test.tsx`, append inside `describe("UninstallDialog", …)` (before its closing `});`):

```tsx

  it("says a NoCancel plan cannot be stopped once it starts, and says nothing of the kind for a cancellable one", async () => {
    // rustup's `self uninstall` (crates/banager-core/src/adapters/
    // standalone/recipes.rs): `OperationBar` will offer no Cancel once it
    // is Running, so the preview says so before the click (spec §五,
    // §6.6's last line).
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ cancel_policy: "NoCancel" }));

    const { unmount } = renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="rustup" />,
    );

    expect(
      await screen.findByText(
        "Don't close Banager or your Mac while this runs. Stopping it partway leaves a broken installation, so this can't be cancelled once it starts.",
      ),
    ).toBeInTheDocument();

    unmount();
    vi.mocked(invoke).mockResolvedValue(issuedPlanFor({ cancel_policy: "KillThenReconcile" }));

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="jq" />,
    );

    // Wait for the plan to land, so absence means "not rendered", not "not yet".
    await screen.findByText("/opt/homebrew/bin/brew uninstall --formula jq");
    expect(screen.queryByText(/can't be cancelled once it starts/)).not.toBeInTheDocument();
  });
```

(The second half's "plan landed" line is what C's `issuedPlanFor` previews for the brew request; if C changed that preview text, use the text C's own tests wait for.)

In `src/pages/UpdatesPage.test.tsx`: after `let needsPassword: Set<string>;` add

```ts
// Names whose plan comes back `NoCancel`, mirroring the rustup recipe's
// `self update` (crates/banager-core/src/adapters/standalone/recipes.rs).
let noCancel: Set<string>;
```

in `issuedPlanFor`, change the line `cancel_policy: "KillThenReconcile",` to

```ts
      cancel_policy: noCancel.has(request.name) ? "NoCancel" : "KillThenReconcile",
```

in `beforeEach`, after `needsPassword = new Set();` add `noCancel = new Set();`, and append inside `describe("UpdatesPage", …)` (before its closing `});`):

```tsx

  it("says per item, under its command, that a NoCancel update cannot be stopped once it starts", async () => {
    // A batch can mix a rustup self update (NoCancel) with a Homebrew
    // upgrade (cancellable); the sentence belongs next to the command it
    // is true of (spec §五's second reader of operations.noCancelHint).
    noCancel.add("onyx");
    const { findAllByRole, getByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    const checkboxes = await findAllByRole("checkbox");
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    fireEvent.click(getByRole("button", { name: "Update selected" }));
    const dialog = await findByRole("dialog");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");

    const hints = within(dialog).getAllByText(
      "Don't close Banager or your Mac while this runs. Stopping it partway leaves a broken installation, so this can't be cancelled once it starts.",
    );
    expect(hints).toHaveLength(1);
    expect(hints[0].closest("div")?.textContent).toContain("onyx");
    expect(hints[0].closest("div")?.textContent).not.toContain("glib");
  });
```

(This follows `warns per item, before the sudo prompt, about the one update that needs a password` at `:372-393` exactly; the two preview strings are the harness's, and C may have changed them with `PlanAction` — use whatever that test waits for.)

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm test`
Expected: `pnpm exec vitest run src/components/OperationBar.test.tsx` PASSES already (the component reads the wire shape only; the renamed cases pin the rustup shape — keep them). FAIL: `UninstallDialog.test.tsx` and `UpdatesPage.test.tsx` cannot find the hint text; `completeness.test.ts` is not yet involved (no key was added).

- [ ] **Step 3: Point the four sentences at rustup, rename the two tests, add the key and its two readers**

In `crates/banager-core/src/model.rs`, in `pub enum CancelPolicy`, replace the seven-line doc comment directly above `NoCancel,` — at `ea30cfb` it begins `/// Cancel is refused once the op is Running` and ends with the lines `/// \`KillThenReconcile\` and the command never starts. No adapter` / `/// produces this yet: a standalone self-updating installer (\`rustup` / `/// self update\`) is the expected first.` — with:

```rust
    /// Cancel is refused once the op is Running, and its command then ends
    /// on its own or at `Plan::timeout_secs`, which the runner counts from
    /// spawn. While the op is still Queued nothing has started and no
    /// timeout is counting, so Cancel is accepted as under
    /// `KillThenReconcile` and the command never starts. Produced by the
    /// rustup recipe (`adapters/standalone/recipes.rs`) for `rustup self
    /// update`, which unlinks `$CARGO_HOME/bin/rustup` -- the one binary
    /// its thirteen proxies run -- and copies the new one in, not
    /// atomically (rustup 1.29.1 `install_bins`), and for `rustup self
    /// uninstall`, which removes Rust directory by directory; a kill
    /// partway leaves no working Rust.
```

In `crates/banager-core/tests/ops_cancel_test.rs`, replace the comment lines (`:742-744` at `ea30cfb`)

```rust
// reconciled. A `NoCancel` plan -- none produced yet; a standalone
// self-updating installer is the expected first -- refuses the Cancel once
// the op is Running and runs to its end; while the op is still Queued,
```

with

```rust
// reconciled. A `NoCancel` plan -- rustup's `self update` and `self
// uninstall` (adapters/standalone/recipes.rs), which replace or remove
// the one binary every Rust proxy runs -- refuses the Cancel once
// the op is Running and runs to its end; while the op is still Queued,
```

In `src-tauri/src/ipc.rs`, rename `test_cancel_operation_impl_refuses_a_running_no_cancel_op` (`:1733` at `ea30cfb`) to `test_cancel_operation_impl_refuses_a_running_no_cancel_op_such_as_rustup_self_update` and `test_cancel_operation_impl_cancels_a_queued_no_cancel_op` (`:1778`) to `test_cancel_operation_impl_cancels_a_queued_no_cancel_op_such_as_rustup_self_update`; in the fake adapter's field doc (`:629-630`) replace the two old names with the new ones (the sentence `Only \`…refuses_a_running_no_cancel_op\` and \`…cancels_a_queued_no_cancel_op\` set \`NoCancel\`` keeps its shape).

In `src/lib/types.ts`, replace the doc comment directly above `export type CancelPolicy = "KillThenReconcile" | "NoCancel";` — the `/** … */` block that begins ` * What the user's Cancel does to an operation.` and ends ` * adapter produces \`NoCancel\` yet.` (`:168-176` at `ea30cfb`) — with:

```ts
/**
 * What the user's Cancel does to an operation. Mirrors `CancelPolicy` in
 * crates/banager-core/src/model.rs: bare-string unit variants.
 * `OperationBar.tsx` reads the copy `OpSummary` carries, with its
 * `status`, and offers no Cancel button for a Running `NoCancel` op,
 * which `OperationManager::cancel` would refuse; a Queued one keeps the
 * button, since nothing has started and the cancel is accepted. rustup's
 * `self update` and `self uninstall` produce `NoCancel` (the recipe in
 * crates/banager-core/src/adapters/standalone/recipes.rs); the update
 * confirmation and the uninstall dialog say so under the command
 * (`operations.noCancelHint`) before the click.
 */
```

In `src/components/OperationBar.tsx`, replace the comment line `// No adapter produces \`NoCancel\` yet.` (`:37`) with:

```ts
  // rustup's `self update` and `self uninstall` produce `NoCancel`
  // (crates/banager-core/src/adapters/standalone/recipes.rs); the preview
  // said so under the command, before the click.
```

In `src/components/UninstallDialog.tsx`, after the `<CommandPreview … />` element (and before `{plan.needs_password && (`), insert:

```tsx

            {plan.cancel_policy === "NoCancel" && (
              // The one policy the operation bar will offer no Cancel for
              // once the command is Running (`OperationManager::cancel`,
              // crates/banager-core/src/ops/mod.rs): rustup's own uninstall,
              // which removes Rust directory by directory. Said here, before
              // the click, as the preview's password notice is.
              <p className="text-sm font-medium text-[var(--color-foreground)]">
                {t("operations.noCancelHint")}
              </p>
            )}
```

In `src/pages/UpdatesPage.tsx`, after the `{item.issued !== null ? (<CommandPreview … />) : null}` element (and before `{itemWarnings.length > 0 ? (`), insert:

```tsx
                {item.issued?.plan.cancel_policy === "NoCancel" ? (
                  // Per item, next to the command it is true of (a batch
                  // can mix a rustup self update with Homebrew upgrades):
                  // once Running, `OperationBar` offers no Cancel for it.
                  <p className="text-sm font-medium text-[var(--color-foreground)]">
                    {t("operations.noCancelHint")}
                  </p>
                ) : null}
```

In `src/i18n/en.json`, in the `"operations"` object, after `"cancel": "Cancel",` add:

```json
    "noCancelHint": "Don't close Banager or your Mac while this runs. Stopping it partway leaves a broken installation, so this can't be cancelled once it starts.",
```

In `src/i18n/zh-CN.json`, after `"cancel": "取消",` in `"operations"`:

```json
    "noCancelHint": "运行期间请不要关闭 Banager 或 Mac。中途停止会留下损坏的安装，所以这个操作一旦开始就不能取消。",
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --test ops_cancel_test`, `cargo test -p banager --lib` (the Tauri crate's tests, renamed), `pnpm typecheck && pnpm test`
Expected: PASS — including `completeness.test.ts` (`operations.noCancelHint` is a literal in two components) and `no-literal-strings.test.ts` (the new JSX carries `t()` only).

- [ ] **Step 5: Run the gates**

Run `cargo fmt --all`, then all five from Global Constraints. Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/model.rs crates/banager-core/tests/ops_cancel_test.rs src-tauri/src/ipc.rs src/lib/types.ts src/components/OperationBar.tsx src/components/OperationBar.test.tsx src/components/UninstallDialog.tsx src/components/UninstallDialog.test.tsx src/pages/UpdatesPage.tsx src/pages/UpdatesPage.test.tsx src/i18n/en.json src/i18n/zh-CN.json
git commit -m "$(cat <<'EOF'
Name rustup as the producer of NoCancel, and warn in the preview that it cannot be stopped

The cancel policy, its refusal in the operation manager and its missing
Cancel button all existed with no plan producing NoCancel; rustup's
self update and self uninstall now do, so the four places that said no
adapter does point at the recipe, and the two IPC and operation-bar
cases are named after it. Both confirmation screens say, under the
command, that once it starts it cannot be cancelled.

Co-Authored-By: <the executing session's attribution line>
EOF
)"
```


---

### Task 10: Recording, registration, the trust file

**Files:**
- Create: `adapters/fixtures/standalone-rustup/1.29.1/{README.md, version.txt, version-stderr.txt, release-stable.toml, toolchains.txt, layout.txt}` — recorded, never typed (the directory is named after the recorded version; see Step 1)
- Modify: `adapters/meta/standalone-rustup.toml` only if the recorded version is not `1.29.1`
- Modify: `crates/banager-core/src/adapters/standalone/recipes.rs` — `RECIPES`, `test_recipes_lists_claude_once`; and C's RECIPES-wide `Uninstall::Paths` tests, a skip each, in whichever file C put them (the C checklist's row 5 grep names it)
- Modify: `crates/banager-core/src/adapters/standalone/mod.rs` — three fixture-backed tests in `mod tests`
- Modify: `crates/banager-core/src/session/mod.rs` — `test_new_registers_all_eight_adapters` (`:508` at `ea30cfb`) → nine  [B's file]
- Modify: `crates/banager-core/src/scan/mod.rs` — the comment on B's `("standalone-rustup", …)` tuple in `test_owned_roots_table` (`:810-812`), and the `standalone-rustup` clause of `owned_roots`'s doc comment (`:257-258`); the `_` arm's comment about it (`:291-292`, "`standalone-rustup` never joins: everything of rustup's resolves to its launcher") stays true — `hexyl` is cargo's, not rustup's — and is not touched  [F's/B's file]
- Modify: `crates/banager-core/src/lib.rs` — the crate doc's list of sources  [F's/B's file]
- Modify: `docs/what-we-run.md` — intro (`:4`), "Where the program comes from", a `## rustup` section after `## Claude Code` (`:460`), "Files Banager reads" (`:602`), "What Banager never does" (`:693`)  [A's/B's file]
- Test: `fixtures_layout_test.rs`, `what_we_run_test.rs` (both existing), the session test, the three fixture tests, `recipes` tests.

**Interfaces:**
- Consumes: `RUSTUP` (Task 6), `standalone::all` (B, C), `Session::new`'s `adapters.extend(standalone::all(…))`, F's `owned_roots`, A's document structure with B's `## Claude Code` section; `rustup::toolchain_names` (Task 5).
- Produces: the registered id `standalone-rustup` (readers: `Session::refresh`'s fan-out, `fixtures_layout_test`, `what_we_run_test`, `ADAPTER_LABEL_KEYS` in Task 11); `RECIPES = &[&CLAUDE, &RUSTUP]` (reader: `all()`); the `## rustup` section (reader: the person spec §12 wrote the file for; `what_we_run_test`).

Why one task: `tests/fixtures_layout_test.rs` asserts the fixture directory set equals the registered id set, and `tests/what_we_run_test.rs` asserts a `## <meta.name>` section per registered id — so the recording, the registration (`RECIPES`) and the section cannot be green separately (B's ruling 6).

- [ ] **Step 1: Record the fixture on this Mac (read-only commands only)**

On the author's Mac, in the repo root. **Never run `rustup update`, `rustup self update`, `rustup self uninstall`, `rustup toolchain install/uninstall` or `rustup check` here** — the recording must not change what it records. The one rustup invocation below is `--version`, under `RUSTUP_AUTO_INSTALL=0` (ruling 20: without the switch a `--version` with no active toolchain installs one); everything else is `curl` and `ls`. (rustup itself still does two small things on any invocation, which the README records as expected: it creates `~/.rustup` if missing — not the case on a Mac with a toolchain — and deletes a leftover `~/.cargo/bin/rustup-init`, which is present only during a self update.)

```bash
# 1. The installed version. stdout is the version line; stderr is two
#    `info:` lines, recorded separately to prove the parser never reads
#    them. Expected stdout shape: `rustup 1.29.1 (d95a37b6a 2026-08-13)`
#    (rustup.md §3). If the number is not 1.29.1, use the number it printed
#    for VERSION below, in adapters/meta/standalone-rustup.toml's
#    verified_versions, and in the "Verified against rustup <VERSION>"
#    sentence of the `## rustup` section written in Step 4 (c) -- the three
#    must agree. (If it is not 1.29.1, the facts in rustup.rs's module doc
#    were read from a different tag than the one installed: re-read
#    `uninstall()`, `shell.rs`, `unix.rs`, `display_version` and
#    `should_auto_install` at the installed tag before going on, and say
#    in the README which tag was read.)
RUSTUP_AUTO_INSTALL=0 ~/.cargo/bin/rustup --version
VERSION=1.29.1   # <- the number the line above printed
mkdir -p "adapters/fixtures/standalone-rustup/$VERSION"
RUSTUP_AUTO_INSTALL=0 ~/.cargo/bin/rustup --version > "adapters/fixtures/standalone-rustup/$VERSION/version.txt" 2> "adapters/fixtures/standalone-rustup/$VERSION/version-stderr.txt"

# 2. The release file, byte for byte (rustup.md §6: two TOML lines).
curl -sS https://static.rust-lang.org/rustup/release-stable.toml > "adapters/fixtures/standalone-rustup/$VERSION/release-stable.toml"

# 3. The toolchains, as the uninstall preview lists them: the entry names
#    of ~/.rustup/toolchains, one per line (no rustup subcommand is run).
ls -1 ~/.rustup/toolchains > "adapters/fixtures/standalone-rustup/$VERSION/toolchains.txt"

# 4. The layout, as corroboration (not a parser input): rustup is a regular
#    file, the thirteen proxies are relative links to it, and whatever
#    `cargo install` put beside them. No absolute paths are printed.
ls -la ~/.cargo/bin > "adapters/fixtures/standalone-rustup/$VERSION/layout.txt"

# 5. For the README's provenance line, and the facts it states.
date +%F; hostname
grep -c . "adapters/fixtures/standalone-rustup/$VERSION/toolchains.txt"            # -> [TOOLCHAIN_COUNT]
grep -c ' -> rustup$' "adapters/fixtures/standalone-rustup/$VERSION/layout.txt"     # -> [PROXY_COUNT] (13 expected)
grep -v ' -> rustup$' "adapters/fixtures/standalone-rustup/$VERSION/layout.txt" | awk 'NR>3 && $NF!="rustup" {print $NF}'   # -> [OTHER_BINS]: the non-proxy, non-rustup entries
ls -d /opt/homebrew/Cellar/rustup /usr/local/Cellar/rustup 2>&1 | head -2          # -> [BREW]: "No such file or directory" twice = no Homebrew rustup (the same read-only signal rustup::homebrew_rustup_present uses)
env | grep -cE '^(CARGO_HOME|RUSTUP_HOME|ZDOTDIR)='                                 # -> [ENV_COUNT]: how many of the three are set in this shell (0 expected; the values are not recorded)
ls -ld ~/.cargo ~/.rustup | awk '{print $1, $NF}' | sed "s|$HOME|~|g"               # -> [ROOTS]: both real directories (`d` first), the standard layout
grep -n 'cargo/env' ~/.zshenv ~/.zprofile ~/.zshrc ~/.bash_profile ~/.bash_login ~/.bashrc ~/.profile ~/.config/fish/config.fish 2>/dev/null | sed "s|$HOME|~|g"   # -> [RC_LINES]: file:line:text, for the README only
```

Then write `adapters/fixtures/standalone-rustup/$VERSION/README.md`, filling the bracketed values from the commands above and nothing else:

```markdown
# rustup [VERSION] fixtures (native installer route, `standalone-rustup`)

Recorded [DATE] on [HOSTNAME] (macOS 27, Apple Silicon) by running the
commands below and saving their output byte for byte. Nothing here is
hand-written or edited — if a parser disagrees with one of these files, the
parser is wrong.

Commands (all read-only; `rustup update`, `rustup self update`, `rustup self
uninstall`, `rustup toolchain list` and `rustup check` were **not** run — the
only rustup invocation is `--version`, under `RUSTUP_AUTO_INSTALL=0`, the
switch every version read Banager makes carries):
- `RUSTUP_AUTO_INSTALL=0 ~/.cargo/bin/rustup --version` -> `version.txt`
  (stdout: `rustup <version> (<hash> <date>)`; the version is the second
  token) and `version-stderr.txt` (stderr: the two `info:` lines rustup
  prints after it, which the version read never looks at — a parser fed
  this file finds no version)
- `curl -sS https://static.rust-lang.org/rustup/release-stable.toml` ->
  `release-stable.toml` (the file `rustup self update` itself reads: two TOML
  lines, `schema-version` and `version`)
- `ls -1 ~/.rustup/toolchains` -> `toolchains.txt` (one entry name per line:
  what the uninstall preview lists as the toolchains that go, read from the
  directory, never from a rustup command; [TOOLCHAIN_COUNT] on the
  recording day)
- `ls -la ~/.cargo/bin` -> `layout.txt` (corroboration only, not a parser
  input: `rustup` is a regular file; [PROXY_COUNT] entries are relative
  symbolic links to it — rustup's proxies, `TOOLS` + `DUP_TOOLS` in its
  `src/lib.rs`, which the Unknown page's rule 1 attributes; the other entries
  ([OTHER_BINS]) are the rest of the directory: what the uninstall preview
  names as deleted with it (`rustup::bin_programs_rustup_removes`) and,
  where `.crates2.json` records them, what cargo's own inventory places)

Layout on this Mac: [ROOTS] — the standard one, which is the only one
Banager offers the uninstall for (`rustup::standard_roots`); [ENV_COUNT] of
`CARGO_HOME`, `RUSTUP_HOME` and `ZDOTDIR` were set in the recording shell.
Homebrew's rustup formula (`Cellar/rustup` under `/opt/homebrew` or
`/usr/local`, the signal `rustup::homebrew_rustup_present` reads): [BREW —
"No such file or directory" for both = not installed, so the preview's
Homebrew line was not exercised on this Mac; otherwise, which prefix has it].

Shell startup files on the recording day (`grep -n 'cargo/env'` over the
eight files Banager reads, home spelled `~`): [RC_LINES, one per line]. The
files themselves are personal and are not recorded; the startup-file rule is
tested on synthetic files (`adapters/standalone/rustup.rs`).

The `.crates2.json` parse this recipe's warnings depend on is not recorded
here: it is `adapters/fixtures/cargo/1.98.1/crates2.json`, recorded 2026-09-20
on the same Mac.

What `rustup self uninstall` removes was read from rustup's source at tag
`[VERSION]` (GitHub `rust-lang/rustup`), the tag whose commit hash
`version.txt` carries; the line numbers are in `adapters/standalone/rustup.rs`.
```

(Every statement in that README is an output of a step above — the plan author's 2026-09-25 observations (one toolchain, thirteen proxies, `hexyl`, no Homebrew rustup, `CARGO_HOME`/`RUSTUP_HOME`/`ZDOTDIR` unset, real `~/.cargo` and `~/.rustup`, rustup's line in `~/.zshenv:1` and `~/.profile:1` and the same line in `~/.zshrc:17`) are expectations, not recordings, and the executor's Mac may differ.)

Check what was recorded: `cat "adapters/fixtures/standalone-rustup/$VERSION/version.txt"` is one line; `version-stderr.txt` is two `info:` lines (the second names the active toolchain's `rustc`); `release-stable.toml` is two lines and its `version` equals the recorded one if this Mac is current; `toolchains.txt` has at least one line; `layout.txt` lists `rustup` and the proxies.

- [ ] **Step 2: Write the failing tests**

Append inside `mod tests` in `crates/banager-core/src/adapters/standalone/mod.rs`:

```rust

    /// The recorded fixture directory for the version the rustup meta
    /// file verifies: `adapters/fixtures/standalone-rustup/<verified>/`.
    fn rustup_fixture(name: &str) -> String {
        let adapter = rustup_adapter(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));
        let version = adapter
            .meta
            .verified_versions
            .first()
            .expect("meta lists the recorded version")
            .clone();
        let path = format!("../../adapters/fixtures/standalone-rustup/{version}/{name}");
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
    }

    #[test]
    fn test_the_recorded_rustup_version_line_parses_and_its_stderr_does_not() {
        let verified = rustup_adapter(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()))
            .meta
            .verified_versions[0]
            .clone();
        assert_eq!(
            latest::parse_version(&rustup_fixture("version.txt"), RUSTUP.version.parse),
            Some(verified)
        );
        // The two `info:` lines: fed to the parser by mistake they would
        // yield no version, which is why the version read takes stdout
        // only.
        let stderr = rustup_fixture("version-stderr.txt");
        assert!(stderr.starts_with("info:"), "{stderr:?}");
        assert_eq!(latest::parse_version(&stderr, RUSTUP.version.parse), None);
    }

    #[test]
    fn test_the_recorded_toolchain_names_list_as_the_preview_lists_them() {
        // The recording is `ls -1 ~/.rustup/toolchains`; the preview
        // reads the same directory (`rustup::toolchain_names`). A temp
        // `toolchains/` with the recorded names lists them back sorted.
        let recorded: Vec<String> = rustup_fixture("toolchains.txt")
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
        assert!(!recorded.is_empty());
        assert!(
            recorded.iter().all(|n| n.contains("apple-darwin")),
            "recorded on a Mac: {recorded:?}"
        );
        let home = TempHome::new("rustup-toolchains-recorded");
        for name in &recorded {
            home.dir(&format!(".rustup/toolchains/{name}"));
        }
        let mut expected = recorded.clone();
        expected.sort();
        assert_eq!(rustup::toolchain_names(&home.path().join(".rustup")), expected);
    }

    #[tokio::test]
    async fn test_check_updates_over_the_recorded_release_file_lists_only_a_real_update() {
        // Fed the recorded release file as the endpoint's body: a
        // candidate exactly when the published version is greater than
        // the installed one -- derived from the recording, so a
        // re-recording on a later day stays honest.
        let installed = rustup_adapter(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()))
            .meta
            .verified_versions[0]
            .clone();
        let body = rustup_fixture("release-stable.toml");
        let published = latest::parse_release_stable_toml(&body).expect("release file");
        let home = TempHome::new("rustup-check-recorded");
        let cargo_home = home.path().join(".cargo");
        let layout = rustup_layout(&cargo_home);
        let http = Arc::new(MockHttpClient::new());
        http.respond(RELEASE_URL, answer(&body));
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0(&format!("rustup {installed} (d95a37b6a 2026-08-13)\n")),
        );
        let adapter = rustup_adapter(runner, http);
        let inst = adapter.detect(&home.env(vec![])).await.remove(0);
        let out = adapter
            .check_updates(&inst, &CheckOptions::default())
            .await
            .expect("check_updates");
        // `Ordering` reaches this module through `use super::*`: B's
        // mod.rs imports `std::cmp::Ordering` for `check_updates`, and
        // B's own `test_the_recorded_channel_pointers_are_bare_versions`
        // in this module names it the same way. No import to add.
        let expected = match latest::compare_dotted(&installed, &published) {
            Some(Ordering::Less) => 1,
            _ => 0,
        };
        assert_eq!(out.candidates.len(), expected, "{installed} vs {published}");
        if expected == 1 {
            assert_eq!(out.candidates[0].target, published);
            assert!(out.candidates[0].checkable);
        }
    }
```

In `crates/banager-core/src/adapters/standalone/recipes.rs`, replace `test_recipes_lists_claude_once` with:

```rust
    #[test]
    fn test_recipes_lists_each_registered_tool_once_in_reading_order() {
        assert_eq!(RECIPES.len(), 2);
        assert!(std::ptr::eq(RECIPES[0], &CLAUDE));
        assert!(std::ptr::eq(RECIPES[1], &RUSTUP));
        let mut ids: Vec<&str> = RECIPES.iter().map(|r| r.id).collect();
        ids.dedup();
        assert_eq!(ids.len(), RECIPES.len(), "one recipe per tool");
    }
```

In `crates/banager-core/src/session/mod.rs`, replace `test_new_registers_all_eight_adapters` (B's) with:

```rust
    #[test]
    fn test_new_registers_all_nine_adapters() {
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
                "standalone-rustup".to_string(),
                "uv".to_string(),
            ]
        );
    }
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p banager-core`
Expected: FAIL — `test_recipes_lists_each_registered_tool_once_in_reading_order` (`RECIPES.len()` is 1); `test_new_registers_all_nine_adapters` (no `standalone-rustup`); `fixtures_layout_test::test_every_registered_adapter_has_a_documented_fixture_directory` (`adapters/fixtures/*` now has a `standalone-rustup` directory no registered adapter matches). The three fixture tests PASS already (they read the recording through the recipe, which exists since Task 6) — they are here because the recording is.

- [ ] **Step 4: Register, and write the section**

In `crates/banager-core/src/adapters/standalone/recipes.rs`, change `pub static RECIPES: &[&Recipe] = &[&CLAUDE];` to:

```rust
pub static RECIPES: &[&Recipe] = &[&CLAUDE, &RUSTUP];
```

(`Session::new` already extends its list with `standalone::all(…)`, one adapter per recipe — B's Task 9, C's trasher; nothing else registers.)

In `crates/banager-core/src/scan/mod.rs`, in `test_owned_roots_table`, replace the comment B put above the `("standalone-rustup", "standalone-rustup", "/Users/someone/.cargo"),` tuple

```rust
            // No adapter with this id exists yet (rustup is step E); the
            // default arm answers for it as for any unknown id, and
            // everything of rustup's is rule 1's anyway.
```

with

```rust
            // rustup's root is the Cargo home, whose `bin/` is the very
            // directory being scanned: nothing of rustup's is placed by
            // its prefix. rustup itself and its thirteen proxies resolve
            // to the launcher (rule 1); `cargo install`ed programs carry
            // their path on cargo's artifacts (rule 2).
```

and in `owned_roots`'s doc comment, the paragraph listing the standalone roots: keep it, and change its clause `\`standalone-rustup\` nothing, since everything of rustup's resolves to its launcher and rule 1 has it` — hard-wrapped in the file across two lines, which at `ea30cfb` (`:257-258`) read `/// instance's \`prefix\`; \`standalone-rustup\` nothing, since everything of` and `/// rustup's resolves to its launcher and rule 1 has it -- in the same`; match by those words, not by one line — to `\`standalone-rustup\` nothing (its root is the Cargo home, whose \`bin/\` is scanned; rule 1 has the launcher and its proxies, rule 2 the \`cargo install\`ed programs)`, and re-wrap the paragraph at the file's width — same fact, both rules named now that both apply.

In every RECIPES-wide test C wrote that destructures `Uninstall::Paths { .. }` (the C checklist's row 5 grep lists them; typical shapes are "the launcher is removed last", "every `remove` path is under home", "every `keep` has copy"), `RUSTUP` now reaches the destructuring with `Uninstall::Command`: give each a skip for it, in whichever form matches the test — `let Some(Uninstall::Paths { remove, .. }) = &recipe.uninstall else { continue };` (a recipe with no path list has nothing to check; bind only the fields the test reads, `..` for the rest, or `-D warnings` stops at an unused binding) or, in a `match`, `Some(Uninstall::Command(_)) | None => continue,`. A test that is about *every* recipe having a verified way out keeps passing without a change: `RUSTUP.uninstall` is `Some`.

In `crates/banager-core/src/lib.rs`, in the crate doc, change B's clause `(Claude Code). This crate is the part that does the work: the` to `(Claude Code, rustup). This crate is the part that does the work: the` (keep the surrounding wording as it stands in the tree).

In `docs/what-we-run.md` (both sentences below are hard-wrapped in the file; match by words and keep the wrapping style):

(a) In the opening paragraph (`:4-5`), change B's `for the eight sources it manages today: Homebrew, npm, pipx, uv, pip (read-only), Cargo, Ollama, and Claude Code (a tool with its own installer).` to `for the nine sources it manages today: Homebrew, npm, pipx, uv, pip (read-only), Cargo, Ollama, and two tools with their own installer, Claude Code and rustup.`

(b) Under `## How Banager runs anything`, in `**Where the program comes from.**`, change B's `and so is a tool with its own installer: Claude Code at the one path its installer writes (its section).` to `and so is a tool with its own installer: Claude Code at the one path its installer writes, rustup at \`$CARGO_HOME/bin/rustup\` (their sections).`

(c) After B's `## Claude Code` section (its last paragraph begins `\`claude update\` runs *without* \`DISABLE_AUTOUPDATER\``; C may have added paragraphs after it — insert after the whole section, before the next `## ` heading) insert:

```markdown

## rustup

Adapter: `StandaloneAdapter` over the `RUSTUP` recipe in
`crates/banager-core/src/adapters/standalone/` (`recipes.rs` is the data,
`rustup.rs` what its uninstall does, when Banager may offer it, and what to
say about it). Verified against rustup 1.29.1 (the version in
`adapters/meta/standalone-rustup.toml` and the name of the recorded fixture
directory; write the recording day's number here). The Rust toolchain
installer, installed by its own script (`curl … https://sh.rustup.rs | sh`,
run by the user — Banager never runs it); the one item under it is rustup
itself. The toolchains it manages, and the programs `cargo install`
installs, are not rows of this source: the first are outside phase 4, the
second are Cargo's.

**Detect.** Banager looks at the fixed path the installer writes,
`$CARGO_HOME/bin/rustup` — `CARGO_HOME` from the environment Banager was
started with (see "Which Rust" below), read the way rustup and cargo read
it: an empty value means the default `~/.cargo`, a relative value names a
folder relative to the tool's own working directory, which Banager cannot
know, so it then lists no rustup rather than guess — never a `rustup`
found through `PATH` — and checks with `lstat` and `realpath` that it is a
regular file, not a link: the installer's copy is an executable of its own,
and the thirteen commands beside it (`cargo`, `rustc`, `rustfmt`, …) are
links *to* it. A link at that path (Homebrew's keg-only `rustup` formula
linked there by hand) is not this route and is not listed. Then
`<rustup> --version` (30 s) with `RUSTUP_AUTO_INSTALL=0` in its
environment: rustup's `--version` looks up the active toolchain, and with
none active it would otherwise *install* one — a download during a
refresh. With the switch it prints `info: no rustc is currently active` and
exits 0. The version is the second token of the first line of standard
output (`rustup 1.29.1 (d95a37b6a 2026-08-13)`); the two `info:` lines
rustup prints on standard error are not read. Two things rustup itself
does on *any* invocation, this read included: it creates `~/.rustup` if it
is missing, and it deletes a leftover `~/.cargo/bin/rustup-init` from an
earlier self update, if there is one. Banager also asks where `rustup`
would run from if typed in Terminal and says so under the source when it
is not this copy (as for Claude Code); that is a notice, not a command.

**Read-only commands and requests** (background checks; never need a
password):

| Purpose | Argv or request | Timeout |
|---|---|---|
| Detect, inventory, and the reading before and after an update | `<rustup> --version`, with `RUSTUP_AUTO_INSTALL=0` | 30 s |
| Newest published version (`check_updates`) | `GET https://static.rust-lang.org/rustup/release-stable.toml` — the two-line TOML file `rustup self update` itself reads | 30 s |

The uninstall preview runs no command at all (below). An update is listed
only when the published `version` is greater than the installed one,
comparing the dot-separated integers; a request that fails, answers
anything but 200, or answers something that is not a versioned TOML file is
listed as "could not check", never as an error for the source. rustup does
not update itself on its own: it updates itself only as part of `rustup
update` and `rustup toolchain install`, which Banager never runs.

**While rustup is being updated or uninstalled, Banager does not run it.**
Both write commands hold rustup's own lock and the Cargo source's (the
`cargo` command is rustup's binary under another name), and a refresh that
arrives while an operation holds a source's lock skips that source
entirely — neither `rustup --version` nor `cargo --version` runs — and
keeps the rows it has until the operation ends. The check is made once, at
the start of a refresh; an operation that starts in the seconds after it
may overlap one version read that was already under way.

**Write commands** (only run after the user reviews and confirms a plan
preview):

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Upgrade | `<rustup> self update` | 600 s | No |
| Uninstall | `<rustup> self uninstall -y` | 600 s | No |

**Never `rustup update`**: that updates the toolchains, and an interrupted
run leaves a toolchain half installed (rust-lang/rustup#4724). `self update`
replaces only rustup's own binary — by unlinking the running one and copying
the new one in (rustup 1.29.1's `install_bins`, `src/cli/self_update.rs`),
during which the thirteen linked commands, `cargo` among them, point at
nothing. So the plan is **not cancellable once it is running** (the preview
says so; the operation bar offers no Cancel; while it is still queued it
can be cancelled, since nothing has started), and it holds the Cargo
source's lock as well as its own. If it exits 0 and the version did not
move, the operation is reported as needing attention, as for every source.
A run stopped by the timeout is reported as unconfirmed, whatever the
version reads before and after say: an upgrade stopped partway is never
called done on the strength of a version number.

`rustup self uninstall -y` is rustup's official uninstall (`-y` skips its
own confirmation prompt, which would otherwise read end-of-file from the
`/dev/null` standard input and stop). **Banager offers it only when Rust
lives in its standard folders**: `CARGO_HOME` and `RUSTUP_HOME` (from the
environment Banager was started with, read as rustup reads them) resolve to
`~/.cargo` and `~/.rustup`, `~/.cargo` is a real folder and not a link, and
`~/.rustup` is a real folder, not a link, or not there yet. Any other
layout — a custom folder, a relative variable, a linked folder — gets no
Uninstall button, and the row says why: rustup's uninstall deletes both
folders whole, wherever they point, and Banager will not ask it to delete a
folder the preview did not name. Read from rustup 1.29.1's source
(`uninstall()` in `src/cli/self_update.rs`, lines 924–1032 at tag
`1.29.1`), it removes, **permanently — nothing goes to the Trash**: every
installed toolchain; `~/.rustup` entirely; the line it added to your shell
startup files (below); everything in `~/.cargo` except `bin/` — the
registry and git caches, `.crates2.json`, and also Cargo's own `config.toml`
and `credentials.toml` (the crates.io login) and `env`; everything in
`bin/` whose name is not rustup's or one of its thirteen links' — that is,
**every program `cargo install` installed, and anything copied there by
hand**; and then the `~/.cargo` folder itself. (Newer rustup keeps the
`cargo install`ed programs; the version this source is verified against
does not, and the preview says what this version does.) The preview lists,
before the button, and without running anything: `~/.rustup` by path, with
every toolchain in it by name (the entries of `~/.rustup/toolchains`) and
the fact that any other rustup using that folder — Homebrew's, when its
`Cellar/rustup` folder is there — loses its toolchains too; `~/.cargo` by
path, with its downloads, its record of what `cargo install` installed,
Cargo's own settings and saved login, and anything else kept there; the
programs in its `bin/` by name where known (a listing of `~/.cargo/bin`
minus rustup and its thirteen links, together with the binaries
`~/.cargo/.crates2.json` records — the same file the Cargo source reads);
that rustup will edit your shell startup files; and each startup file that
will still speak of Cargo's env file afterwards. It is not cancellable once
running, for the same reason as the update, and holds the same two locks
(it deletes the record the Cargo source's inventory reads). A run stopped
by the timeout is judged by whether `~/.cargo/bin/rustup` is still there:
gone is reported as done, still there as unconfirmed. `--no-modify-path`
is not passed: rustup removing its own line beats leaving one that prints
an error in every new terminal.

**Which Rust.** rustup runs with the environment Banager itself was
started with: at launch Banager restores only `PATH` from your login shell,
and every command it runs inherits the rest. Banager reads `CARGO_HOME`,
`RUSTUP_HOME` and `ZDOTDIR` from that same environment — the one the
rustup it runs will see, so the two always agree about which folders are
meant. A `RUSTUP_HOME` or `CARGO_HOME` exported only in a shell startup
file is therefore not seen by either: the preview and the uninstall act on
the default folders, and a Rust kept only where the shell says is left
alone, not deleted; a `CARGO_HOME` exported only there also means Banager
looks for rustup under `~/.cargo` and does not list one installed elsewhere.

**Shell startup files.** Banager never edits one. rustup's uninstall removes
exactly the line it wrote, `. "$HOME/.cargo/env"` (the absolute path when
`CARGO_HOME` is set), from `~/.profile`, `~/.bash_profile`, `~/.bash_login`,
`~/.bashrc`, `$ZDOTDIR/.zshenv` and `~/.zshenv`, in that order, and then the
two lines rustup wrote before version 1.23 from `~/.bash_profile`,
`~/.profile`, `$ZDOTDIR/.zprofile` and `~/.zprofile` (`shell.rs` and
`unix.rs` under `src/cli/self_update/`, tag `1.29.1`). Each visit removes
the first line that matches byte for byte, newline included; when
`ZDOTDIR` is your home folder the same file is visited twice and two copies
go. It never edits `~/.zshrc` or fish's `config.fish`. So before the
uninstall Banager reads those eight files — `~/.zshenv`, `~/.zprofile`,
`~/.zshrc`, `~/.bash_profile`, `~/.bash_login`, `~/.bashrc`, `~/.profile`,
`~/.config/fish/config.fish` — replays rustup's removals on copies in
memory, and names each file that still speaks of Cargo's env file: "will
print an error" when what is left is a line in the exact form rustup itself
writes (a file rustup does not edit, such as `~/.zshrc`; a second copy of
its line; its line last in the file with no newline after it), "may" for
any other mention rustup will not remove (a guarded line such as
`[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"`, an `echo`, another
spelling such as `source ~/.cargo/env`, a `$CARGO_HOME/env`); a comment
counts for nothing. rustup learns `ZDOTDIR` by asking `zsh` when your
login shell is not zsh; Banager runs nothing and reads only the variable
it was started with, so a `ZDOTDIR` set only inside a zsh startup file is
not modelled, and a zsh whose files live under such a `ZDOTDIR` is not
read.
```

(d) Under `## Files Banager reads`, after B's `- Claude Code: …` bullet, add:

```markdown
- rustup: whether `$CARGO_HOME/bin/rustup` exists and is a regular file
  (`lstat`, `realpath`); whether `~/.cargo` and `~/.rustup` are real folders
  and not links (`lstat`, to decide whether the uninstall is offered);
  during the uninstall preview only, the names in `~/.rustup/toolchains`
  and in `~/.cargo/bin` (directory listings — nothing in them is opened),
  `~/.cargo/.crates2.json`, whether `/opt/homebrew/Cellar/rustup` or
  `/usr/local/Cellar/rustup` exists, and the eight shell startup files named
  in its section, each read whole and only searched for a line about Cargo's
  env file; nothing else under `RUSTUP_HOME` is ever read.
```

(e) Under `## What Banager never does`, after the bullet `- Never runs an installer script, and never reruns one to update a tool.`, add:

```markdown
- Never runs `rustup update`: rustup's own update of its toolchains, which
  an interruption leaves half installed. Only `rustup self update`, which
  replaces rustup alone. Never runs `rustup` at all while an update or
  uninstall of it is under way, and never lets a version read of rustup or
  cargo install a toolchain (`RUSTUP_AUTO_INSTALL=0`).
- Never asks rustup to uninstall from anywhere but its standard folders,
  `~/.cargo` and `~/.rustup`: rustup deletes both whole, permanently, and
  Banager offers that only when the preview can name exactly those two.
```

and in the bullet `- Never writes, moves or deletes a file on the Mac itself, other than its own \`settings.json\`; never edits a shell startup file.` (or C's rewording of it, which adds the move-to-Trash exception), append the clause ` — rustup's own uninstall edits its startup line and deletes its two folders permanently, and the preview says so.`

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p banager-core`
Expected: PASS — `test_new_registers_all_nine_adapters`, `fixtures_layout_test` (nine directories, nine ids, one README each), `what_we_run_test` (a `## rustup` section; every host named), `test_recipes_lists_each_registered_tool_once_in_reading_order`, `test_every_recipe_latest_url_is_an_allowed_https_host`, `test_every_recipe_path_is_under_home_or_the_cargo_home` and `test_a_paths_recipe_names_only_home_paths` (now over both recipes), B's `test_every_recipe_launcher_is_named_after_its_id` and `test_every_recipe_meta_parses_and_names_the_standalone_id` (`rustup` / `standalone-rustup`), each of C's RECIPES-wide `Uninstall::Paths` tests with its new skip, `test_owned_roots_table`, the three fixture tests, and everything before.

- [ ] **Step 6: Run the gates**

Run `cargo fmt --all`, then all five from Global Constraints. Expected: all clean.

- [ ] **Step 7: Commit**

If one of C's RECIPES-wide tests that got a skip in Step 4 lives outside `recipes.rs` and `mod.rs` (C keeps its removal logic in `crates/banager-core/src/adapters/standalone/removal.rs`), append that file's exact path to the `git add` below; nothing else joins it.

```bash
git add adapters/fixtures/standalone-rustup adapters/meta/standalone-rustup.toml crates/banager-core/src/adapters/standalone/recipes.rs crates/banager-core/src/adapters/standalone/mod.rs crates/banager-core/src/session/mod.rs crates/banager-core/src/scan/mod.rs crates/banager-core/src/lib.rs docs/what-we-run.md
git commit -m "$(cat <<'EOF'
Register rustup as a source, with its recording and its trust-file section

The rustup recipe joins the registration list, so a native rustup is a
group on the Installed page. The fixture directory the layout test
demands is a real recording from this Mac: the version line (read with
rustup's auto-install switched off) and the stderr the parser must not
read, the release file, the toolchain directory's names the uninstall
preview lists, and the bin directory's layout. The trust file gains the
section that says every command, request and file this source involves
-- when the uninstall is offered and when it is not, that it deletes two
folders permanently and never to the Trash, which startup files rustup's
own cleanup visits and in what order, and that nothing of Banager's runs
rustup while rustup is being replaced.

Co-Authored-By: <the executing session's attribution line>
EOF
)"
```

---

### Task 11: Front end — label, summary, empty states, the standard-folders sentence

**Files:**
- Modify: `src/lib/sources.ts` — `ADAPTER_LABEL_KEYS`, `StandaloneAdapterId`, `STANDALONE_SUMMARY_KEYS` (all B's, `:20-60` at `ea30cfb`); new `UNINSTALL_BLOCKED_OVERRIDES` and `uninstallBlockedCopy` beside `UNINSTALL_BLOCKED_KEYS` (`:554-591`)  [B's file]
- Modify: `src/lib/sources.test.ts` — B's `describe("STANDALONE_SUMMARY_KEYS", …)` (one test appended) and a new `describe("uninstallBlockedCopy", …)`  [B's file]
- Modify: `src/pages/InstalledPage.tsx` — the two `UNINSTALL_BLOCKED_KEYS[artifact.uninstall_blocked]` reads (`:147`, `:170`)  [B's file]
- Modify: `src/components/UninstallDialog.tsx` — the `UNINSTALL_BLOCKED_KEYS[blocked]` read (`:106`)  [C's file]
- Modify: `src/components/SnapshotStatus.test.tsx` — the two empty-state sentences (B's text, `:57`, `:242`)  [B's file]
- Modify: `src/i18n/en.json`, `src/i18n/zh-CN.json` — `adapters`, `standalone.summary`, `emptyStates`, `installed.blocked.NoSafeMethod.standalone-rustup`  [B's files]
- Test: `sources.test.ts`, `SnapshotStatus.test.tsx`, `completeness.test.ts`, `InstalledPage.test.tsx` and `UninstallDialog.test.tsx` (unchanged, still green).

**Interfaces:**
- Consumes: B's `ADAPTER_LABEL_KEYS: Record<string, string>` (read at `InstalledPage.tsx`, `UpdatesPage.tsx`, `UninstallDialog.tsx`), `StandaloneAdapterId`, `STANDALONE_SUMMARY_KEYS: Record<StandaloneAdapterId, string>` (read by `InstalledPage`'s `installedDescription`), `standaloneSummaryKey`; B's `UninstallBlockedCopy` and `UNINSTALL_BLOCKED_KEYS` (`sources.ts:523-591`), read by `InstalledPage`'s `installedBadge` and `installedDescription` and by `UninstallDialog`'s refused path (which has `instance?.adapter_id` from the snapshot at `:49`); the `emptyStates` sentences (read by `SnapshotStatus.tsx`); the artifact's `NoSafeMethod` from Task 6's gate.
- Produces (verbatim): `ADAPTER_LABEL_KEYS["standalone-rustup"] = "adapters.standalone-rustup"`; `StandaloneAdapterId = "standalone-claude" | "standalone-rustup"`; `STANDALONE_SUMMARY_KEYS["standalone-rustup"] = "standalone.summary.standalone-rustup"`; `export function uninstallBlockedCopy(reason: UninstallBlocked, adapterId: string | undefined): UninstallBlockedCopy` (readers: `InstalledPage.tsx`'s two sites, `UninstallDialog.tsx`'s one); the keys `adapters.standalone-rustup`, `standalone.summary.standalone-rustup`, `installed.blocked.NoSafeMethod.standalone-rustup.description`, `installed.blocked.NoSafeMethod.standalone-rustup.refused`; the two `emptyStates` sentences naming rustup (Ruling 14).

Copy is spec §9.2's: label `rustup` in both locales; summary "Rust's toolchain manager: it installs and updates the Rust compiler and Cargo." / "Rust 的工具链管理器：负责安装和更新 Rust 编译器与 Cargo。" The two empty-state sentences are B's, extended to name rustup beside Claude Code (Ruling 14): B's wording, not the spec's, because B's plan rewrote both after the spec and both of B's name Claude Code. The standard-folders sentence is ruling 18's: `NoSafeMethod` is the variant the gate and the page already act on, but B's sentence for it ("{{source}} has no uninstall command…") would be false for rustup, which has one; the copy is keyed by adapter id for that one reason, and every other row keeps B's sentences.

- [ ] **Step 1: Write the failing tests**

In `src/lib/sources.test.ts`, inside B's `describe("STANDALONE_SUMMARY_KEYS", …)` (before its closing `});`), append:

```ts

  it("gives rustup its sentence and its label, in both locales", () => {
    expect(standaloneSummaryKey("standalone-rustup")).toBe("standalone.summary.standalone-rustup");
    expect(ADAPTER_LABEL_KEYS["standalone-rustup"]).toBe("adapters.standalone-rustup");
    expect(en.standalone.summary["standalone-rustup"]).toBe(
      "Rust's toolchain manager: it installs and updates the Rust compiler and Cargo.",
    );
    expect(zhCN.standalone.summary["standalone-rustup"]).toBe(
      "Rust 的工具链管理器：负责安装和更新 Rust 编译器与 Cargo。",
    );
    expect(en.adapters["standalone-rustup"]).toBe("rustup");
    expect(zhCN.adapters["standalone-rustup"]).toBe("rustup");
  });
```

and, after that `describe`'s closing `});`, append:

```ts

describe("uninstallBlockedCopy", () => {
  it("gives rustup's row its own reason for NoSafeMethod, and every other row B's", () => {
    // The rustup recipe's gate puts `NoSafeMethod` on the artifact when
    // Rust is not in its standard folders (crates/banager-core/src/
    // adapters/standalone/rustup.rs, `uninstall_blocked`); B's sentence
    // for that variant says the tool has no uninstall command, which is
    // false for rustup. The badge stays; the two sentences are rustup's.
    const rustup = uninstallBlockedCopy("NoSafeMethod", "standalone-rustup");
    expect(rustup.badge).toBe(UNINSTALL_BLOCKED_KEYS.NoSafeMethod.badge);
    expect(rustup.description).toBe("installed.blocked.NoSafeMethod.standalone-rustup.description");
    expect(rustup.descriptionSourceUnavailable).toBe(
      "installed.blocked.NoSafeMethod.standalone-rustup.description",
    );
    expect(rustup.refused).toBe("installed.blocked.NoSafeMethod.standalone-rustup.refused");
    expect(rustup.command({ instance_id: "standalone-rustup", kind: "Binary", name: "rustup" }, undefined)).toBe("");
    expect(en.installed.blocked.NoSafeMethod["standalone-rustup"].description).toBe(
      "Banager only removes Rust from its standard folders, ~/.cargo and ~/.rustup, and this Mac keeps them somewhere else (CARGO_HOME or RUSTUP_HOME is set, or one of the folders is a link), so it doesn't offer to. rustup's official documentation explains rustup self uninstall.",
    );
    expect(zhCN.installed.blocked.NoSafeMethod["standalone-rustup"].description).toBe(
      "Banager 只会从标准位置（~/.cargo 和 ~/.rustup）删除 Rust，而这台 Mac 把它们放在了别处（设置了 CARGO_HOME 或 RUSTUP_HOME，或者其中一个文件夹是链接），所以这里不提供卸载。rustup 的官方文档说明了怎么用 rustup self uninstall 卸载。",
    );
    // Everyone else: B's copy, whatever the adapter.
    expect(uninstallBlockedCopy("NoSafeMethod", "standalone-claude")).toBe(
      UNINSTALL_BLOCKED_KEYS.NoSafeMethod,
    );
    expect(uninstallBlockedCopy("NoSafeMethod", undefined)).toBe(UNINSTALL_BLOCKED_KEYS.NoSafeMethod);
    expect(uninstallBlockedCopy("Pinned", "standalone-rustup")).toBe(UNINSTALL_BLOCKED_KEYS.Pinned);
    expect(uninstallBlockedCopy("Pinned", "brew")).toBe(UNINSTALL_BLOCKED_KEYS.Pinned);
  });
});
```

(`ADAPTER_LABEL_KEYS`, `en` and `zhCN` are already imported in that file for B's tests; add `ADAPTER_LABEL_KEYS`, `UNINSTALL_BLOCKED_KEYS` and `uninstallBlockedCopy` to the `import { … } from "./sources"` list where missing.)

In `src/components/SnapshotStatus.test.tsx`, replace B's two expected sentences (B wrote them at the lines that were `:57` and `:242` before B; match by text). The one that reads

```ts
        "Banager works with Homebrew, npm, pipx, uv, pip, Cargo, Ollama, and Claude Code at its native installer's default location. None of them are set up on this Mac yet — Homebrew is the easiest place to start.",
```

becomes

```ts
        "Banager works with Homebrew, npm, pipx, uv, pip, Cargo and Ollama, and with Claude Code and rustup at their own installers' default locations. None of them are set up on this Mac yet — Homebrew is the easiest place to start.",
```

and the one that reads

```ts
        "Items installed with Homebrew, npm, pipx, uv, pip, Cargo or Ollama appear here, along with Claude Code installed at its native installer's default location.",
```

becomes

```ts
        "Items installed with Homebrew, npm, pipx, uv, pip, Cargo or Ollama appear here, along with Claude Code and rustup installed at their own installers' default locations.",
```

(Both of B's sentences name Claude Code, so both must name rustup: an empty state that lists one tool with its own installer and not the other would tell a rustup user their tool is not supported.)

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm typecheck`
Expected: FAIL — `Argument of type '"standalone-rustup"' is not assignable to parameter of type 'StandaloneAdapterId'` is *not* raised (the function takes `string`), but `Property 'standalone-rustup' does not exist on type '{ "standalone-claude": string; }'` is, at `en.standalone.summary["standalone-rustup"]` and `en.adapters["standalone-rustup"]`, and `Module '"./sources"' has no exported member 'uninstallBlockedCopy'`; `pnpm exec vitest run src/components/SnapshotStatus.test.tsx` fails on both sentences.

- [ ] **Step 3: Implement**

In `src/lib/sources.ts`: in `ADAPTER_LABEL_KEYS`, after B's `"standalone-claude": "adapters.standalone-claude",` add

```ts
  "standalone-rustup": "adapters.standalone-rustup",
```

change B's `export type StandaloneAdapterId = "standalone-claude";` to

```ts
export type StandaloneAdapterId = "standalone-claude" | "standalone-rustup";
```

and in `STANDALONE_SUMMARY_KEYS`, after `"standalone-claude": "standalone.summary.standalone-claude",` add

```ts
  "standalone-rustup": "standalone.summary.standalone-rustup",
```

(the `Record` over the union makes `tsc` demand this line the moment the union grows).

Still in `src/lib/sources.ts`, directly after the closing `};` of `export const UNINSTALL_BLOCKED_KEYS: Record<UninstallBlocked, UninstallBlockedCopy> = { … }` (and before `parseUninstallBlocked`'s doc comment), insert:

```ts

/**
 * One source's own words for a reason, where B's sentence would be false
 * of it. rustup's row carries `NoSafeMethod` when Rust is not in its
 * standard folders (`rustup::uninstall_blocked` in
 * crates/banager-core/src/adapters/standalone/rustup.rs) -- not because it
 * has no uninstall command, which is what `UNINSTALL_BLOCKED_KEYS`'s
 * sentence says. Keyed by adapter id, then reason; a missing entry means
 * B's copy. Literal keys, so `completeness.test.ts` finds each one.
 */
const UNINSTALL_BLOCKED_OVERRIDES: Partial<
  Record<StandaloneAdapterId, Partial<Record<UninstallBlocked, UninstallBlockedCopy>>>
> = {
  "standalone-rustup": {
    NoSafeMethod: {
      badge: "installed.blocked.NoSafeMethod.badge",
      description: "installed.blocked.NoSafeMethod.standalone-rustup.description",
      descriptionSourceUnavailable: "installed.blocked.NoSafeMethod.standalone-rustup.description",
      command: () => "",
      refused: "installed.blocked.NoSafeMethod.standalone-rustup.refused",
    },
  },
};

/**
 * The copy for `reason` on a row of `adapterId`: the source's own words
 * where it has them (`UNINSTALL_BLOCKED_OVERRIDES`), else
 * `UNINSTALL_BLOCKED_KEYS`. `adapterId` is the instance's `adapter_id`
 * (the Installed page has it; the uninstall dialog finds the instance in
 * the snapshot, and passes `undefined` when it cannot).
 */
export function uninstallBlockedCopy(
  reason: UninstallBlocked,
  adapterId: string | undefined,
): UninstallBlockedCopy {
  const overrides =
    adapterId !== undefined && Object.prototype.hasOwnProperty.call(UNINSTALL_BLOCKED_OVERRIDES, adapterId)
      ? UNINSTALL_BLOCKED_OVERRIDES[adapterId as StandaloneAdapterId]
      : undefined;
  return overrides?.[reason] ?? UNINSTALL_BLOCKED_KEYS[reason];
}
```

In `src/pages/InstalledPage.tsx`, replace the two reads of `UNINSTALL_BLOCKED_KEYS[artifact.uninstall_blocked]` — in `installedBadge` (`return { text: t(UNINSTALL_BLOCKED_KEYS[artifact.uninstall_blocked].badge), variant: "neutral" };`, `:147`) and in `installedDescription` (`const copy = UNINSTALL_BLOCKED_KEYS[artifact.uninstall_blocked];`, `:170`) — with `uninstallBlockedCopy(artifact.uninstall_blocked, instance.adapter_id)` (both functions take `instance: ManagerInstance`), and change the `import { …, UNINSTALL_BLOCKED_KEYS, … } from "../lib/sources"` list to import `uninstallBlockedCopy` instead if `UNINSTALL_BLOCKED_KEYS` has no other reader left in the file (`tsc`/eslint's unused-import rule says).

In `src/components/UninstallDialog.tsx`, replace `const copy = UNINSTALL_BLOCKED_KEYS[blocked];` (`:106`) with `const copy = uninstallBlockedCopy(blocked, instance?.adapter_id);` (`instance` is the snapshot lookup at `:49`, already in scope), and adjust the import from `../lib/sources` the same way.

In `src/i18n/en.json`: in `"adapters"`, after `"standalone-claude": "Claude Code"` add `,` and

```json
    "standalone-rustup": "rustup"
```

in `"standalone"` → `"summary"`, after `"standalone-claude": "…"` add `,` and

```json
      "standalone-rustup": "Rust's toolchain manager: it installs and updates the Rust compiler and Cargo."
```

and in `"emptyStates"`, replace B's two descriptions: `"noSources"` → `"description"` (B's `"Banager works with …, and Claude Code at its native installer's default location. None of them …"`) with

```json
      "description": "Banager works with Homebrew, npm, pipx, uv, pip, Cargo and Ollama, and with Claude Code and rustup at their own installers' default locations. None of them are set up on this Mac yet — Homebrew is the easiest place to start."
```

and `"nothingInstalled"` → `"description"` (B's `"Items installed with … appear here, along with Claude Code installed at its native installer's default location."`) with

```json
      "description": "Items installed with Homebrew, npm, pipx, uv, pip, Cargo or Ollama appear here, along with Claude Code and rustup installed at their own installers' default locations."
```

and in `"installed"` → `"blocked"` → `"NoSafeMethod"` (B's object with `badge`, `description`, `refused`), after `"refused": "…"` add `,` and:

```json
        "standalone-rustup": {
          "description": "Banager only removes Rust from its standard folders, ~/.cargo and ~/.rustup, and this Mac keeps them somewhere else (CARGO_HOME or RUSTUP_HOME is set, or one of the folders is a link), so it doesn't offer to. rustup's official documentation explains rustup self uninstall.",
          "refused": "Banager only removes Rust from its standard folders, ~/.cargo and ~/.rustup, and this Mac keeps them somewhere else, so it didn't. Nothing has been changed."
        }
```

In `src/i18n/zh-CN.json`, the same five places:

```json
    "standalone-rustup": "rustup"
```

```json
      "standalone-rustup": "Rust 的工具链管理器：负责安装和更新 Rust 编译器与 Cargo。"
```

`"noSources"` → `"description"` (B's `"Banager 支持 Homebrew、npm、pipx、uv、pip、Cargo、Ollama，以及用 Claude Code 原生安装器装在默认位置的 Claude Code。…"`):

```json
      "description": "Banager 支持 Homebrew、npm、pipx、uv、pip、Cargo、Ollama，以及用各自的原生安装器装在默认位置的 Claude Code 和 rustup。这台 Mac 上一个都还没装，建议先从 Homebrew 开始。"
```

`"nothingInstalled"` → `"description"` (B's `"用 Homebrew、npm、pipx、uv、pip、Cargo、Ollama 装的东西，以及用 Claude Code 原生安装器装在默认位置的 Claude Code，会出现在这里。"`):

```json
      "description": "用 Homebrew、npm、pipx、uv、pip、Cargo、Ollama 装的东西，以及用各自的原生安装器装在默认位置的 Claude Code 和 rustup，会出现在这里。"
```

and under `"installed"` → `"blocked"` → `"NoSafeMethod"`:

```json
        "standalone-rustup": {
          "description": "Banager 只会从标准位置（~/.cargo 和 ~/.rustup）删除 Rust，而这台 Mac 把它们放在了别处（设置了 CARGO_HOME 或 RUSTUP_HOME，或者其中一个文件夹是链接），所以这里不提供卸载。rustup 的官方文档说明了怎么用 rustup self uninstall 卸载。",
          "refused": "Banager 只会从标准位置（~/.cargo 和 ~/.rustup）删除 Rust，而这台 Mac 把它们放在了别处，所以没有卸载。什么都没有改动。"
        }
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm typecheck && pnpm test`
Expected: PASS — including `completeness.test.ts` (`standalone.summary.standalone-rustup`, `adapters.standalone-rustup` and the two `installed.blocked.NoSafeMethod.standalone-rustup.*` keys are literals in `sources.ts`; if `completeness.test.ts` treats `installed.blocked` as a subtree looked up by assembled key — check its `INTERPOLATED_SUBTREES`/lookup list — the two literals in `UNINSTALL_BLOCKED_OVERRIDES` satisfy it either way), and `InstalledPage.test.tsx`/`UninstallDialog.test.tsx` unchanged: every row they render keeps B's copy.

- [ ] **Step 5: Run the gates**

Run all five from Global Constraints. Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add src/lib/sources.ts src/lib/sources.test.ts src/pages/InstalledPage.tsx src/components/UninstallDialog.tsx src/components/SnapshotStatus.test.tsx src/i18n/en.json src/i18n/zh-CN.json
git commit -m "$(cat <<'EOF'
Name rustup on the pages, and say why a non-standard layout gets no Uninstall

Its group label, the one-sentence summary the Installed page shows in
place of a description, and both empty states, which named Claude Code
as the one tool Banager finds at its own installer's location and now
name rustup beside it, in both locales. A rustup whose folders are not
the standard ones carries the same refusal claude used to -- no safe
method -- but the sentence for it said the tool has no uninstall
command, which rustup has; rustup's row now says Banager only removes
Rust from its standard folders, and every other row keeps its words.

Co-Authored-By: <the executing session's attribution line>
EOF
)"
```


---

### Task 12: README — the source row and the test counts

**Files:**
- Modify: `README.md` — the "What it manages" table (after B's `| Claude Code — …` row, `:29` at `ea30cfb`) and the two test-count sentences (the `> **Status: pre-release.**` block near the top, `:12-13`, and its Chinese counterpart under `## 中文`, `:176-177`)  [B's file: anchor by text]
- Test: none new (prose); the five gates.

**Interfaces:** none — prose. The row's claims are Task 6 (reads, both plans, the gate), Tasks 5/10 (what the uninstall preview says), Task 7 (nothing runs rustup while it is being replaced) and `docs/what-we-run.md`'s rustup section.

- [ ] **Step 1: Add the row**

Directly after B's table row that begins `| Claude Code — the native install, via its own installer |`, insert:

```markdown
| rustup — the Rust toolchain manager, via its own installer | yes | updates yes (`rustup self update`); install no (the installer is rust-lang's, and Banager never runs it); uninstall yes (`rustup self uninstall -y`), offered only when Rust is in its standard folders (`~/.cargo`, `~/.rustup`) and previewed with everything it removes — permanently, not to the Trash: every toolchain by name, the whole Cargo folder with its settings and saved login, and the programs in its `bin` folder, named where known. Neither can be cancelled once it is running, and the preview says so |
```

- [ ] **Step 2: Update the two test counts**

Get the numbers from the suites, never by hand:

```bash
cargo test --workspace 2>&1 | grep -E '^test result' | awk '{ passed += $4 } END { print passed }'
pnpm test 2>&1 | grep -E '^\s*Tests\s'
```

Put the Rust total where the status block says `covered by <N> Rust tests` and the Chinese block says `有 <N> 个 Rust 测试`, and the front-end total where both say `<M> front-end tests` / `<M> 个前端测试` (B's Task 11 and C's plan changed the same four numbers; whatever they read now, replace them with today's).

- [ ] **Step 3: Run the gates**

Run all five from Global Constraints. Expected: all clean.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "$(cat <<'EOF'
List rustup among the sources, with the new test counts

Reads, an update and an official uninstall that is offered only for the
standard folders and says what it removes, neither cancellable once
running -- which the row says rather than implies.

Co-Authored-By: <the executing session's attribution line>
EOF
)"
```

- [ ] **Step 5: Delivery note (goes in the branch's PR description / handover; not a file)**

> **Step E: rustup.** A native rustup (`$CARGO_HOME/bin/rustup`) is a *rustup* group on the Installed page with one row; its update badge compares the launcher's live version with `static.rust-lang.org/rustup/release-stable.toml` and lists an update only when the published version is greater; *Update* runs `rustup self update`; *Uninstall* runs `rustup self uninstall -y` — offered only when `CARGO_HOME` and `RUSTUP_HOME` resolve to `~/.cargo` and `~/.rustup` and both are real folders (any other layout gets no button and a sentence saying why) — after a preview that runs nothing and names, from rustup 1.29.1's source, what goes permanently and not to the Trash: `~/.rustup` with every toolchain in it by name (and any other rustup's, Homebrew's when its Cellar shows one), `~/.cargo` with its caches, its record, its settings and saved login, the programs in its `bin` folder named where known — `cargo install`ed or copied there by hand (1.29.1 removes the whole Cargo home — read from its source, not from the research, which quoted a newer version) — that rustup edits its startup line, and each startup file that will still speak of Cargo's env file: "will" for a line in rustup's own form, "may" for any other mention. A `rustup` launcher that is a dangling link is not listed. Both plans are `NoCancel` (the first any adapter produces) and hold the cargo instance's lock; both confirmation screens say so before the click. Every version read of the rustup binary — rustup's and the cargo proxy's — carries `RUSTUP_AUTO_INSTALL=0`, and a refresh that lands while rustup's update or uninstall is running skips rustup and cargo and keeps their rows. `HostEnv` now carries `RUSTUP_HOME` and `ZDOTDIR`. Every cargo artifact carries the path of the program its crate installed — the one named after the crate, or the first listed — so those `cargo install`ed programs leave the Unknown page; the other binaries of a multi-binary crate (`cargo-binstall`'s `detect-targets`) stay there until `InstalledArtifact.path` can hold several.
>
> **What changed for every source** — a refresh no longer waits on an instance's lock while an operation holds it; it carries that instance's rows forward unchanged and returns (Task 7, ruling 19). The front end's refresh after an operation finishes replaces them.
>
> **What later steps change** — honest, not bugs: until **D**, `agy` and `grok` are not sources and the empty-state copy names Claude Code and rustup only. D's recipes need `extra_locks: no_extra_locks` (Task 4's field) and, if D lands after this, `RUSTUP` needs D's `backup_globs: &[]`; a route kind D adds must take an arm in `probe_strict`'s dangling-branch `match` (Task 4).
>
> **Rulings taken** (see "Rulings this plan makes"): `RemovesCargoInstalled` replaces the spec's `LeavesUnmanaged` (ruling 1, the one user-visible fact the spec had wrong); the verified startup-file visits, replayed in order, and the two-tier sentence (2, 22); `extra_locks` on `Recipe` (3); `CommandUninstall` with a gate and no probe (4); `expand_route` beside B's `expand` (5); the `home` crate's rule, one id producer, `RUSTUP_AUTO_INSTALL=0` on cargo's read (6); one binary per cargo artifact (7); `FlatFile`/`SecondToken` introduced here, and no launcher-only state for a flat file (8); the seat bound to its instance (9); read-only recording (10); `self update` is unlink-then-copy (11); nine adapters (13); B's empty-state wording, both sentences (14); toolchains from the directory (15); `DeletesCargoHome` by path (16); what Banager's own environment means for `RUSTUP_HOME`/`CARGO_HOME` (17); the standard-folders gate (18); the refresh skip (19); `HostEnv.{rustup_home, zdotdir}` (20); the Homebrew signal (21).
>
> **Recorded on this Mac**: `adapters/fixtures/standalone-rustup/<version>/` — the version line (under `RUSTUP_AUTO_INSTALL=0`) and its stderr, the release file, the toolchain directory's names, the bin layout; read-only commands only; `rustup update`, `self update`, `self uninstall`, `toolchain list` and `check` were never run.
>
> **UNVERIFIED, left as such**: the exact text `rustup self update` prints when nothing changes (the ops test's outcome depends on the exit code and the two readings only); a `ZDOTDIR` rustup would learn only by asking `zsh` (Banager reads the variable it was started with); rustup's behaviour with no active toolchain under `RUSTUP_AUTO_INSTALL=0` is read from `display_version`'s source and mocked, not recorded.

---

## Self-review against the spec

**1. Spec coverage** — every requirement of §十 row E and the sections it points at, with the task that carries it:

| Requirement | Where | Task |
|---|---|---|
| rustup recipe: `FlatFile`, `$CARGO_HOME/bin/rustup`, root `$CARGO_HOME`, `--version` with `RUSTUP_AUTO_INSTALL=0` (the spec said no env; ruling 20), `SecondToken`, `HttpTomlVersion` (release-stable.toml), `self_updates: false`, `["self","update"]` 600 s `NoCancel`, `Command` uninstall (§3.5, §五, §6.4) | `RUSTUP`; its three recipe tests | 6 |
| meta TOML `standalone-rustup`, `verified_versions = ["1.29.1"]` (§2.1, §3.5) | Task 6 (adjusted to the recording in 10) | 6, 10 |
| Fixture `adapters/fixtures/standalone-rustup/1.29.1/{README, version.txt, version-stderr.txt, release-stable.toml, toolchains.txt, layout.txt}`, real recording, read-only commands only (§9.3) | Task 10 Step 1; the forbidden commands listed; every rustup invocation under the switch | 10 |
| `Uninstall::Command`; `warnings(&Detected)`; `extra_locks(&Detected)` (§3.1, §十三 #3) — **no probe** (ruling 4), plus `blocked(&Detected)` (ruling 18) | `CommandUninstall`, `Recipe.extra_locks` (rulings 3, 4) | 4, 6 |
| The warnings with copy; `names` empty → the unlisted key; `{{path}}` spelled `~` (§6.4, §6.5, §9.2) — six variants, two of them keyed by payload | Task 2 (variants, mirror, keys, copy), Task 5 (producer) | 2, 5 |
| `RemovesToolchains` — from the `toolchains/` directory, not `toolchain list` (§6.4 said the probe; ruling 4, 15) | `toolchain_names`, `warnings_with` | 5 |
| `DeletesCargoCaches` (§6.4) → **`DeletesCargoHome { path }`** (ruling 16: the folder by path, permanently, settings and login included) | `warnings_with`; `warnings.deletesCargoHome` | 2, 5 |
| `LeavesUnmanaged { names }` from `.crates2.json`'s `bins` via a new `parse_crates2_bins`, not `parse_crates2_entries` (§6.4, §十三 #4) | `parse_crates2_bins`, read by `rustup::bin_programs_rustup_removes`, which unites it with a listing of `bin/` minus rustup's fourteen names; **the variant is `RemovesCargoInstalled`** (ruling 1: 1.29.1 deletes them) | 1, 2, 5 |
| `FlatFile` recognition (§3.3 step 3): a regular file, not a link; no launcher-only state | `probe_strict`'s `FlatFile` arm and its dangling-branch `match kind` (ruling 8) | 4 |
| `EditsShellConfig`; `LeavesShellConfigLine { path }` per rc file with a reference outside rustup's managed set; the managed set verified against `shell.rs` before merge (§6.4, §十 row E's 合并前) | `rustup_rc_visits`, `remove_first_exact_line`, `classify_leftover`, `shell_config_leftovers`; rulings 2 and 22 with line numbers; `certain` tiers the sentence | 5 |
| `cargo::parse_crates2_bins` also fills cargo `inventory`'s `InstalledArtifact.path = {cargo_home}/bin/{bin}` (§6.4, §8.3 rule 2, 附录 A) | `parse_crates2`; the scan companion test | 1 |
| `cargo::instance_id_for` single producer; `detect` and `extra_locks` both call it; the equality test on one `HostEnv` with and without `CARGO_HOME` for both `plan(Upgrade)` and `plan(Uninstall)` (§2.4, §十三 #42, §9.4) | `instance_id_for`, `cargo_home_of`; `test_rustup_locks_name_the_cargo_instance_detect_produces_with_and_without_cargo_home` (with `CARGO_HOME` set only the upgrade has a plan: the gate) | 1, 6 |
| Both rustup plans list two locks, `affected` empty, `needs_password: false` (§2.4, §五, §6.4) | `locks`, the Upgrade arm, `command_uninstall_plan`; tests | 4, 6 |
| `Detected.cargo_home` per `cargo.rs:133-136`'s rule; no `rustup_home` on `HostEnv` (§3.2, §十三 #16/#44) — **deviates**: `HostEnv.rustup_home` and `.zdotdir` are added, with readers (ruling 20) | `tool_home`, `detect`'s seat | 1, 4 |
| `route::expand` handles `$CARGO_HOME/` (§3.1) — **as a new function** beside B's (ruling 5) | `expand_route` | 4 |
| `plan()` refuses before any detect (§3.2) — and refuses a seat that describes another instance (ruling 9) | `seated_detected_for` | 4 |
| First `NoCancel` producer: the four "No adapter produces `NoCancel` yet" sentences → rustup; the two `ipc.rs` and two `OperationBar.test.tsx` cases named after rustup (§0.1, §五, §十 row E) | Task 9 | 9 |
| `operations.noCancelHint` with **two** readers, `UpdatesPage` and `UninstallDialog`, after the `CommandPreview`, on `plan.cancel_policy === "NoCancel"` (§五, §6.6, §9.2, §十三 #9) | Task 9 (C landed first, so both readers here) | 9 |
| `ALLOWED_HTTPS_HOSTS` += `static.rust-lang.org` with its producer; recipes host test; trust-file row (§4.2) | Task 6 | 6 |
| Only remote > local is a candidate; failure/non-200/garbage → uncheckable, never `Err` (§4.1, §4.3) | `test_check_updates_reads_the_release_file_and_lists_only_a_newer_rustup`, `…marks_a_bad_release_file_uncheckable` | 6 |
| Never `rustup update` (D6, §五, 附录 B) | `test_rustup_updates_and_uninstalls_itself_with_no_cancel_the_gate_and_the_cargo_lock`, `test_the_rustup_recipe_never_builds_rustup_update` (scoped to rustup: `update` is claude's own upgrade); the trust file's never-list | 6, 10 |
| `execute` = `run_plan` for the Command uninstall (§6.1) | `test_execute_runs_rustups_uninstall_through_run_plan` | 6 |
| Honest outcome for `self update` that changed nothing (§4.4 item 5, §9.4's pattern); the stopped upgrade `Unconfirmed`; the uninstall by presence (§五's `run_plan:500-502` → `ops/mod.rs:787-799`) | Task 8's nine tests | 8 |
| The version read must not trigger a write (§3.4's rule, applied to rustup: ruling 20) | `RUSTUP.version.env`, `CargoAdapter::detect`'s env, the refresh skip | 1, 6, 7 |
| Registration; the eight-adapter test → nine (§2.1) | Task 10 | 10 |
| `owned_roots`: `standalone-rustup` → empty, everything by rules 1 and 2 (§8.3) | B's tuple kept; comments updated | 10 |
| `docs/what-we-run.md`: a `## rustup` section (detect, env, update check, upgrade argv and cancel policy, `self uninstall -y` and what it removes, when it is offered, the refresh skip, rustup's own side effects), "files read", the host, the never-list's `rustup update` (§9.5) | Task 10 (c)–(e), Task 6's row | 6, 10 |
| `adapters.standalone-rustup`, `standalone.summary.standalone-rustup`, `emptyStates` naming rustup (§9.2); the standard-folders sentence for `NoSafeMethod` (ruling 18) | Task 11 | 11 |
| `StandaloneAdapterId` union grows (B's `Record` forces the summary) | Task 11 | 11 |
| README row (project convention) | Task 12 | 12 |
| Tests §9.4 lists for E: recipe paths under home or `$CARGO_HOME`; hosts allowed; fixture version parse incl. the stderr file; TOML parse; `plan(Upgrade)` argv/policy/timeout with the cargo lock; `plan(Uninstall)` two locks equal to `CargoAdapter::detect`'s id with/without `CARGO_HOME`; `LeavesShellConfigLine` for `~/.zshrc` with the line / only `~/.zshenv` with it; `parse_crates2_bins` over the cargo fixture → `[("hexyl", ["hexyl"])]`; `session/mod.rs` count; `fixtures_layout_test`; `types.test.ts` variants; `warnings.test.ts`; `UpdatesPage.test` and `UninstallDialog.test` `noCancelHint`; `OperationBar.test` renamed | named per task above | 1–11 |

Not in E, by the spec's own list: `UpdateBlocked::SelfUpdatesOnly`, `Glob`/`backup_globs`, `Latest::{HttpJsonField, Command}` (D); the rustup toolchain rows (§十一).

**2. Placeholder scan** — no "TBD", "TODO", "implement later", "similar to Task N", or code-less code steps. The bracketed values in Task 10's fixture README are the recording's own outputs (`[VERSION]`, `[DATE]`, `[HOSTNAME]`, `[TOOLCHAIN_COUNT]`, `[PROXY_COUNT]`, `[OTHER_BINS]`, `[BREW]`, `[ENV_COUNT]`, `[ROOTS]`, `[RC_LINES]`), by the "real recordings only" rule. The bounded conditionals are C's (the checklist): `Detected.euid`, the `probe`/`probe_strict` split, `display_path`'s visibility, `new`'s fourth argument — each named in exactly the places the checklist lists.

**3. Type consistency** — names used across tasks, checked against where they are defined: `HostEnv.{rustup_home, zdotdir}`, `tool_home`, `cargo_home_of` (→ `Option<PathBuf>`), `instance_id_for`, `parse_crates2_bins`, `RUSTUP_AUTO_INSTALL_OFF` (1; used 4, 5, 6, 7, 8); `Warning::{RemovesToolchains { path, names }, DeletesCargoHome { path }, RemovesCargoInstalled { names }, HomebrewRustupLosesToolchains, EditsShellConfig, LeavesShellConfigLine { path, certain }}` (2; used 5, 6); `VersionParse::SecondToken`, `Latest::HttpTomlVersion { url }`, `parse_release_stable_toml` (3; used 6, 10); `RouteKind::FlatFile`, `expand_route(home, cargo_home, spec)`, `Detected.{cargo_home, rustup_home, zdotdir}`, `Recipe.extra_locks`, `no_extra_locks`, `seated_detected_for`, `locks`, `testing::{RustupLayout, rustup_layout, RUSTUP_PROXIES, detected}` (4; used 5, 6, 7, 8, 10 — `RUSTUP_PROXIES` moves to `rustup.rs` in 5, and `rustup_layout` then reads `super::rustup::RUSTUP_PROXIES`); `SHELL_RC_CANDIDATES`, `RUSTUP_PROXIES`, `HOMEBREW_PREFIXES`, `StandardRoots`, `standard_roots`, `uninstall_blocked`, `toolchain_names`, `bin_programs_rustup_removes`, `homebrew_rustup_present`, `cargo_home_str`, `RcVisit`, `rustup_rc_visits`, `remove_first_exact_line`, `LeftoverPatterns`, `leftover_patterns`, `Leftover`, `classify_leftover`, `shell_config_leftovers(home, zdotdir, cargo_home)`, `warnings_with`, `uninstall_warnings`, `extra_locks` (5; used 6, 10); `CommandUninstall { args, timeout_secs, cancel, blocked, warnings }`, `Uninstall::Command`, `RUSTUP`, `command_uninstall_plan` (sync), `request_for`, `detected_rustup`, `rustup_adapter`, `without_homebrew_line`, `RELEASE_URL`, `RUSTUP_VERSION_LINE`, `test_the_rustup_recipe_never_builds_rustup_update` (6; used 7, 8, 10); `OperationManager::locks_held`, `Detection`, `under_operation`, `rustup_home()` in the refresh tests (7; used 8's tests read `locks_held` too); `RustupHome`, `UninstallingRunner`, `Leaves`, `uninstall`, `rustup_home_and_instance` (8); `operations.noCancelHint`, the renamed tests (9); `RECIPES = &[&CLAUDE, &RUSTUP]`, `test_new_registers_all_nine_adapters`, `rustup_fixture` (10); `ADAPTER_LABEL_KEYS["standalone-rustup"]`, `StandaloneAdapterId`, `STANDALONE_SUMMARY_KEYS["standalone-rustup"]`, `UNINSTALL_BLOCKED_OVERRIDES`, `uninstallBlockedCopy` (11). C's names used as the checklist spells them: `PlanAction`, `Plan.action`, `Uninstall::Paths`, `Detected.euid`, `StandaloneAdapter::new(…, trasher)`, `MockTrasher`, `probe_strict`, `reconcile_after_uninstall`, `display_path` (4, 5, 6, 7, 8). B's names used verbatim: `TempHome`, `claude_layout`, `exited_0`, `adapter`, `RecordingRunner`, `instance_for`, `request`, `answer`, `fixture`, `Probe`, `probe`, `parse_version`, `is_dotted_version`, `compare_dotted`, `CLAUDE`, `all`, `detect`, `inventory`, `check_updates`, `latest_version`, `plan`, `execute`, `VERSION_TIMEOUT`. A's: `ALLOWED_HTTPS_HOSTS`, `host_allowed`. F's: `owned_roots`, `test_owned_roots_table`, `scan_dirs`, `Home`, `exe`, `link`, `artifact`, `display_path`. The ops file's: `ScriptedRunner`, `exited_0(stdout, stderr)`, `Stop`, `upgrade`. The refresh file's: `FakeAdapter`, `FakeState.{block_execute, detect_calls, inventory_calls}`, `make_instance`, `make_artifact`, `non_root_env`.

**4. Review Focus** — each of the seven lines has its test in the named task: (1) Task 5 `test_toolchain_names_is_empty_for_no_directory_and_lists_entries_sorted`; (2) Task 5 `test_rustup_rc_visits_visit_zshenv_twice_when_zdotdir_is_home`, `test_shell_config_leftovers_removes_two_copies_when_zdotdir_is_home_and_one_otherwise`, `test_classify_leftover_puts_rustups_own_forms_in_the_certain_tier_and_the_rest_in_the_qualified_one`; (3) Task 1 `test_tool_home_follows_the_home_crates_rule`, Task 5 `test_standard_roots_accepts_only_the_default_layout_of_real_directories`, Task 6 `test_detect_follows_cargo_home_for_rustup`, `test_inventory_and_plan_refuse_the_uninstall_for_a_non_standard_layout`, `test_rustup_locks_name_the_cargo_instance_detect_produces_with_and_without_cargo_home`; (4) Task 5 `test_bin_programs_rustup_removes_names_a_program_no_record_lists`, `test_bin_programs_rustup_removes_is_empty_for_no_directory_and_no_record`, Task 6 `test_plan_uninstall_for_rustup_survives_an_empty_cargo_home_and_no_toolchains`; (5) Task 3 `test_parse_release_stable_toml_refuses_anything_that_is_not_a_versioned_release_file`, Task 6 `test_check_updates_marks_a_bad_release_file_uncheckable`; (6) Task 7 `test_a_refresh_during_rustups_self_update_runs_neither_rustup_nor_cargo`; (7) Task 4 `test_plan_refuses_an_instance_the_seat_no_longer_describes`, Task 8 `test_a_plan_for_an_instance_from_another_home_is_refused_and_a_redetect_restores_it`.

## Spec points this plan could not follow literally, and facts found while writing it

1. **§6.4's `LeavesUnmanaged { names }` ("hexyl … 会留在 Mac 上") is false for the installed rustup.** rustup.md §8's `clean_cargo_home` quotation is from `master`; the tag the installed binary was built from (`1.29.1` = `d95a37b6`, matched to `rustup --version`'s hash) removes every non-proxy entry of `$CARGO_HOME/bin` and then `$CARGO_HOME` itself (`self_update.rs:996-1022`, `:1029`; `unix.rs:50-53`). The variant is `RemovesCargoInstalled { names }` with copy that says so (ruling 1). The spec's §6.4, §6.5, §6.6 (the third bullet) and §9.2's `leavesUnmanaged` entries should be corrected; the `uninstall()` doc comment at `:915-922` still describes the newer behaviour, which is how the research was misled.
2. **§6.4's UNVERIFIED startup-file set is now verified**, and the author's memory was right: `$ZDOTDIR/.zshenv`/`~/.zshenv`, `~/.profile`, `~/.bash_profile`/`~/.bash_login`/`~/.bashrc`, fish's `conf.d/rustup.fish` — plus, for the two pre-1.23 line shapes only, `$ZDOTDIR/.zprofile`/`~/.zprofile` (ruling 2, with lines). The rule is rustup's own ordered sequence of visits replayed on copies of the files, because rustup removes one exact line per visit and visits `.zshenv` twice under `ZDOTDIR=$HOME`; and the sentence has two tiers, because a mention rustup will not remove is not necessarily an error (ruling 22).
3. **§3.1's `extra_locks` inside `Uninstall::Command` cannot serve §2.4's Upgrade lock**; it is a `Recipe` field (ruling 3). §3.1's `Probe { args, timeout_secs }` is gone: the preview must not run rustup (ruling 4); `CommandUninstall` carries `blocked` instead, the gate the spec did not have (ruling 18).
4. **§3.2's `Detected { …, launcher, real }`**: B cached neither (inventory re-probes), and rustup needs neither; this plan adds `cargo_home`, `rustup_home` and `zdotdir`, all `Option`. **§3.2's "`HostEnv` 不改" is not followed**: `rustup_home` and `zdotdir` are added, each with a production reader (the gate; the visit model) — the reason the spec gave for refusing `rustup_home` (no reader, §十三 #16/#44) no longer holds (ruling 20). 39 literal sites, one mechanical edit each.
5. **§4.2's allowlist comment says six hosts for phase 4**; after B and E it is five, with D's `antigravity-cli-auto-updater-…run.app` making six.
6. **§十 row E says "改四处 … + 两个测试名"**; the `OperationBar.test.tsx` cases are also renamed and their fixtures changed to rustup shapes (Task 9), since their comments said "No adapter produces `NoCancel` yet" too.
7. **`rustup self update`'s atomicity, UNVERIFIED in §五**, is now VERIFIED as *not* atomic (unlink, then copy: `install_bins`, `self_update.rs:779-782`; ruling 11). The spec's "原子性 UNVERIFIED" can become this fact.
8. **§6.4's "每行第一个 token" over `rustup toolchain list`** is replaced by the entry names of `~/.rustup/toolchains` (ruling 15): the probe is gone, and the directory is what `uninstall()` itself iterates.
9. **`InstalledArtifact.path` is one path**, so a multi-binary crate attributes one program (ruling 7); §8.3's "`hexyl` → cargo" holds, `ripgrep` → `rg` holds, `cargo-binstall`'s `detect-targets` stays on the Unknown page — a backlog item, noted in `parse_crates2`'s doc and the delivery note.
10. **The 2026-09-25 re-check of this Mac** (read-only): `rustup 1.29.1 (d95a37b6a 2026-08-13)`; the two `info:` lines are stderr; `release-stable.toml` still `1.29.1`; one toolchain; thirteen proxies as relative links, `rustup` and `hexyl` regular files; `. "$HOME/.cargo/env"` at `~/.zshenv:1`, `~/.zshrc:17`, `~/.profile:1`; `SHELL=/bin/zsh`, `ZDOTDIR` unset — the spec's §6.4 sample stands.
11. **Line numbers**: the spec pins `8ba6f52`; this plan was written at `f7e2917`, re-baselined at `3b5117a` and revised at `ea30cfb` (2026-09-25: A, F and all of B landed; C's plan committed, C's code not). Every `file:line` cited at `ea30cfb` was re-read there: `refresh.rs:156-167, 236-262, 274-277, 1395-1465`; `ops/mod.rs:174, 317-353, 787-799, 876`; `path_env.rs:5-24, 70-93, 95-103`; `cargo.rs:29-37, 39-79, 129-176, 186-206, 440-441, 536-538, 540, 552`; `model.rs:25-34, 314, 407-421, 424`; `adapters/mod.rs:270, 386, 415, 426, 470-514`; `standalone/mod.rs:56-58, 105-152, 157-188, 203-244, 384-423, 538-636, 657-690, 755-786`; `standalone/route.rs:21-26, 100-165`; `standalone/recipes.rs` whole; `sources.ts:20-60, 523-591`; `InstalledPage.tsx:147, 170`; `UninstallDialog.tsx:49, 106, 252`; `UpdatesPage.tsx:1086`; `types.ts:168-177`; `OperationBar.tsx:37`; `OperationBar.test.tsx:170-234`; `ops_cancel_test.rs:742-744`; `ops_upgrade_version_test.rs:33-166, 535-700`; `ipc.rs:629-630, 1733, 1778`; `scan/mod.rs:209, 257-258, 291-295, 330-331, 810-815`; `unknown_scan_test.rs:405-410`; `session/mod.rs:259-340, 508`; `what-we-run.md:4, 460, 602, 640, 652, 693, 697, 707`; `README.md:12-13, 29, 176-177`; `en.json:48-52, 310, 321`; `SnapshotStatus.test.tsx:57, 242`. In files C still changes, every edit is anchored by a symbol or quoted text as well, and the number is a hint only.
12. **§9.2's `deletesCargoCaches` sentence and §6.4's source for the program names understate what 1.29.1 deletes**, and neither says the deletion is permanent. The variant is renamed `DeletesCargoHome { path }`, the copy names the folder, says "permanently — not to the Trash" and lists the settings and the login (ruling 16); the names are the record's united with a listing of `bin/` minus rustup's fourteen.
13. **§9.2's `leavesShellConfigLine` sentence hard-codes `~/.cargo/env`**; with `CARGO_HOME` set the leftover line loads `<CARGO_HOME>/env`, which the Rust side matches. The sentence says "Cargo's env file" in both locales, and has a qualified twin (Task 2).
14. **§9.2's two `emptyStates` sentences were rewritten by B's plan** (both name Claude Code "at its native installer's default location"); this step extends B's two, not the spec's, to name rustup (ruling 14).
15. **§3.3's dangling-link rule (`LauncherOnly`) is the link-shaped route's only**: a flat-file launcher that is a dangling link is `Absent` (ruling 8), or rustup's Cargo-home root would make any dangling name under it a rustup row.
16. **§3.2's "`rustup self uninstall` 自己知道它在哪"** holds for the environment Banager was started with, not the shell's: `fix_path_env` restores `PATH` only, so a `RUSTUP_HOME` set in a startup file is seen by neither Banager nor the rustup it runs — the two agree, which the gate relies on (ruling 17, 18). `HostEnv.rustup_home` is added for the gate, not to override that.
17. **§6.4's "not in rustup's managed set" is per visit, first match, newline included**: rustup removes the first exact copy of its line (with its newline) at each visit, so a second copy or a last line without a newline stays and is reported — unless `ZDOTDIR=$HOME` gives `.zshenv` a second visit (ruling 2).
18. **§6.4 offers the uninstall for every native rustup; this plan offers it only for the standard layout** (ruling 18). §6.4 also says Banager reads nothing under `RUSTUP_HOME`; the preview now reads the *names* in `~/.rustup/toolchains` (ruling 15), and the trust file says so.
19. **§3.4's "版本读取不得触发自更新" reaches further for rustup than the spec knew**: `--version` can install a *toolchain* (ruling 20), and the cargo proxy's `--version` can too; both carry `RUSTUP_AUTO_INSTALL=0`. And two side effects of any rustup invocation cannot be switched off (`~/.rustup` created if missing; a leftover `rustup-init` deleted) — disclosed, and the second kept away from a running self update by the refresh skip (ruling 19).
20. **§2.4's locking model assumed detection is safe to run concurrently with an operation**; for rustup it is not, and the refresh now skips an adapter whose instance an operation holds (ruling 19) — a change for every source: a refresh no longer waits out an operation, it carries the instance forward.
21. **§十一 left the Homebrew twin UNVERIFIED and unwarned**; that rustup's homes are independent of the binary is now VERIFIED from `home` 0.5.12's source, and the preview warns conditionally on a local read-only signal (ruling 21).

## Review log

Adversarial review of this plan, 2026-09-25, 13 points. Each was checked against the source it cites before anything changed: B's plan (`plan-step-b-skeleton-claude.md`, byte-identical to the copy committed at `2a360c8`), the worktree read with `git show 3b5117a:<file>` (never the working tree, which held B's Task 3 in progress), C's draft plan (`plan-step-c-trash-uninstall.md`, Tasks 1–3), rustup's source at tag `1.29.1` fetched read-only (`src/cli/self_update.rs`, `self_update/{unix,shell}.rs`, `src/lib.rs`, `src/utils/raw.rs`), and `fix-path-env` at the pinned rev `c4c45d5`. Rust claims were compiled in a scratch crate outside the worktree (rustc/cargo 1.98.1): Task 5's module with its 17 tests, and B's `route.rs` with Task 4's two `probe` changes under B's 19 route tests plus Task 4's 8 (all pass; `cargo clippy --all-targets -- -D warnings` clean). **Result: 12 accepted, 1 partly accepted (one sub-claim rejected), 0 rejected outright; 4 more defects found while verifying, all fixed.** (Several of the shapes this log records were superseded by the Astra review below — the probe, `DeletesCargoCaches`, `leftover_env_lines`, `expand`'s signature — and the rows say what they were at the time.)

| # | Verdict | Evidence | What changed |
|---|---|---|---|
| 1 | **Accepted** | B's `probe` (B plan `:2341-2370`): the `Err(NotFound)` branch returns `Absent` only when the launcher is *not* a link, then answers `LauncherOnly` for any link text landing under `root`, with no look at `kind`. rustup's root is `$CARGO_HOME`, so a dangling `$CARGO_HOME/bin/rustup -> rustup.old` would have been a version-less rustup row whose plans run a dangling link. The plan's sentence claimed the opposite. | Task 4: the dangling branch opens with an exhaustive `match kind` (`FlatFile` → `Absent`, `SymlinkIntoRoot` → B's rule unchanged); new `test_probe_is_absent_for_a_dangling_link_where_a_flat_file_is_expected`, which also shows the same link is still `LauncherOnly` for the link route; rules paragraph corrected; ruling 8 extended; step counts, commit body, delivery note (D must give a new route kind an arm), self-review row. Compiled and run against B's code: B's 19 route tests are unchanged and green. |
| 2 | **Accepted** | B's plan `:4893`, `:4905` (tests), `:5040`, `:5044` (en) and the zh-CN pair: `noSources` reads "…Cargo, Ollama, and Claude Code at its native installer's default location…"; `nothingInstalled` reads "…along with Claude Code installed at its native installer's default location." Both name Claude Code. Task 9 quoted text B never writes and kept the second sentence unchanged, contradicting ruling 14. | Task 11 (then 9): both anchors are B's real text, located by key (`emptyStates.noSources.description`, `emptyStates.nothingInstalled.description`) and by the two `SnapshotStatus.test.tsx` expectations (`:57`/`:242`). Both sentences name Claude Code and rustup in en and zh-CN. Step 2 now expects both to fail; commit body updated. `git grep` found no other test that asserts these sentences. |
| 3 | **Accepted** | Adding `Command` to C's `enum Uninstall` breaks any exhaustive match outside `plan`, and registering `RUSTUP` breaks any RECIPES-wide test that destructures `Paths`. The table row existed, but Task 6 Step 3 and the registration task (now Task 10) carried no instruction. C's draft ruling 2 confirms that the `None` arm is the only producer of `NoSafeMethod`. | Task 6 Step 3: a grep plus a case list (`inventory`'s `uninstall_blocked`, `execute`'s dispatch, any other branch, RECIPES-wide tests deferred to the registration task). Task 10 Step 4: the skip for each such test. Files lists updated; a `git add` note covers a test that lives in `removal.rs`. |
| 4 | **Accepted** (to a newer HEAD) | The worktree moved past the reviewer's `2a360c8`: B's Tasks 1–2 landed (`31d756b`, `3b5117a`). Re-reading at `3b5117a` found the stale numbers: `model.rs` path 201→233, `ResourceLock` 373→419, `NoCancel` doc 362-368→408-414; `types.ts` `Warning` doc 66-78→69-81, `CancelPolicy` doc 151-159→168-176; plus the reworded `scan/mod.rs` doc sentence. | Baseline pinned to `3b5117a` (since re-pinned to `ea30cfb`), with an order to re-run `git log` and the confirm-grep before Task 1. Task 1 matches the three-line wrapped sentence by its words. Task 9's two doc edits are anchored by symbol and quoted first and last lines, with the number as a hint. Self-review item 11 re-baselined with the full list, each entry checked with `git show`. |
| 5 | **Accepted** | No test in Tasks 6–8 names `RUSTUP_PROXIES`. After point 9 it no longer lives in `testing` at all, so the old import would not even resolve. | Task 6: `use super::testing::rustup_layout;` only; the hedge sentence is deleted; Interfaces updated. |
| 6 | **Accepted** | Checked with cargo 1.98.1: `cargo test --lib foo bar` → `error: unexpected argument 'bar' found` (usage `cargo test [OPTIONS] [TESTNAME]`). | Task 6 Step 4 is three commands (Step 2 was already split). Every other `cargo test` line in the plan checked: one filter each; multiple `--test` flags are options and legal. |
| 7 | **Accepted** | `CLAUDE.upgrade.args` is `&["update"]` (B plan `:1745`; B's `test_claude_updates_with_its_own_updater` asserts it). The blanket test iterated `RECIPES`, which includes `CLAUDE`, so it could never pass. | Replaced by `test_the_rustup_recipe_never_builds_rustup_update`, scoped to `RUSTUP`'s argvs, with a comment on why the scope is right. Self-review rows and type list renamed. |
| 8 | **Accepted** | B's `shadow_note` (B plan `:2408-2414`) accepts only a `PATH` hit that is a regular file with an execute bit, else returns `NotOnPath`. `rustup_layout` wrote the launcher with mode 0644, so `test_detect_lists_rustup_…`'s `notes.is_empty()` failed. B's `claude_layout` uses `TempHome::executable` (0o755). `resolve_exe` checks only `is_file`, so the cargo detect in the cross-lock test was not affected. | `rustup_layout` sets 0o755, with a comment explaining why. Task 8's `rustup_home_and_instance` sets it too, so the two synthetic layouts agree. |
| 9 | **Accepted** | Tag `1.29.1` `uninstall()`: `:977-993` removes everything in `$CARGO_HOME` except `bin/`, which includes `config.toml`, `credentials.toml` and `env`. `:996-1022` removes every `bin/` entry whose UTF-8 name is not `rustup` or one of `TOOLS`+`DUP_TOOLS` (`src/lib.rs:16-32`, 13 names), recorded or not. `:1029` → `unix.rs:50-53` removes the directory. The unconditional sentence named only packages and the record, and the names came from `.crates2.json` alone. | The unconditional Cargo sentence says the whole Cargo folder goes, settings and login included (en and zh) — since renamed `DeletesCargoHome { path }` and made to say "permanently" by the Astra review. `removesCargoInstalled` says the programs in the folder's `bin` go, which is also true of a hand-placed one. `rustup::bin_programs_rustup_removes` replaces `cargo_installed_bins`: a read-only `read_dir` of `bin/` united with the record's `bins`, minus `rustup` and `RUSTUP_PROXIES`, skipping hidden names and non-UTF-8 names, sorted and deduplicated. `RUSTUP_PROXIES` moved into `rustup.rs`, where it has a production reader. Five tests. Also updated: Review Focus, Interfaces, recipe doc, trust-file section and files-read bullet, README row, Goal and Architecture, delivery note, commit bodies, self-review, ruling 16. Compiled: 17 tests pass, clippy clean. |
| 10 | **Partly accepted** | *Accepted:* re-pinning the baseline and re-running the confirm-grep (as point 4); the wrapped `scan/mod.rs` text in Task 1, and in the registration task, whose `standalone-rustup` clause spans `:257-258`; counts taken from the suites (unchanged). *Rejected:* "`Ordering::Less` needs `use std::cmp::Ordering;` in the test imports." `Ordering` reaches the test module through `use super::*`: B's `mod.rs` imports `std::cmp::Ordering` for `check_updates` (B plan `:3491`), and B's own `test_the_recorded_channel_pointers_are_bare_versions` in the same module uses `Ordering::Greater` with no import. A glob import brings a parent's private `use` into a child module (RFC 1560); this was checked with rustc 1.98.1 in a scratch crate. | The registration task matches the wrapped clause by its words. A comment in the fixture test says where `Ordering` comes from, so no executor adds a redundant import. No import was added. |
| 11 | **Accepted** (the "declare" option) | Both deferrals are real: `Detected.cargo_home` and all of `rustup.rs` have only test readers until Task 6. The spec's rule (§十) is per step and is met. | Task 4's and Task 5's Interfaces blocks each carry a **Declared deferral** naming the reader and the task that wires it. The Global Constraints bullet names both (and, since the Astra review, `HostEnv`'s two fields). Task 5 was not merged into Task 6: merging would make one very large commit that mixes the preview logic with the recipe and the plan arm, and it buys no check the declared deferral lacks. |
| 12 | **Accepted, refined** | `fix_path_env::fix()` is `fix_vars(&["PATH"])` at `c4c45d5` (`src/lib.rs:91-92`), called at `src-tauri/src/lib.rs:18`. `HostEnv::discover` reads `CARGO_HOME` from the process (`path_env.rs:75`), `RealRunner` inherits the environment (`real.rs:666`), and rustup reads `home::rustup_home()` (`:963`) and `process.cargo_home()` (`:932`). The reviewer's wording holds for `RUSTUP_HOME`. For `CARGO_HOME` set only in a shell file, Banager does not find a rustup installed there at all, so nothing is offered. It removes the default `~/.cargo` only if a rustup also sits there. | Ruling 17 rewritten with both cases. The `RUSTUP` recipe doc has a bullet on it. The trust file gets a **Which Rust** paragraph, and the Detect sentence points to it. Spec-deviation 16 added. |
| 13 | **Accepted** (the "reword" option) | With `CARGO_HOME` set, the leftover line loads `<CARGO_HOME>/env`; Task 5's needles already match that spelling, but the copy said `~/.cargo/env`. | `leavesShellConfigLine` says "Cargo's env file, which will be gone" in en and zh-CN. Updated to match: trust-file text, Goal, commit bodies, `uninstall_warnings` doc and spec-deviation 13. No payload was added then; the Astra review added `certain: bool` for a different reason (the two tiers). |

**Found while verifying (not raised by the review), fixed:**

- **A1 — `leftover_env_lines` under-warned.** It counted every exact copy of rustup's line as removed, and also a last line with no newline after it. rustup's `find_exact_line` (`unix.rs:164-172`) matches the line *with* its newline and returns the first match only, once per file (`:55-77`, `:147-162`). So a duplicated line, or one with no final newline, survives the uninstall and errors in every new terminal, and the preview said nothing. rustup's own writes end in `\n` (`raw.rs:144-155`, `writeln!`), so only a hand edit produces that case. The function was made to emulate rustup exactly, with two assertions added — and has since become `remove_first_exact_line` inside the visit model (Astra finding 8), which also handles the `ZDOTDIR=$HOME` double visit. Ruling 2, Review Focus #2, the trust-file paragraph and spec-deviation 17 were updated then and again now.
- **A2 — C's draft changes `StandaloneAdapter::new` to four arguments** (`…, trasher: Arc<dyn Trasher>`; `banager_core::trash::MockTrasher` is public). This plan's direct constructor calls had B's three and would not compile once C lands. Every one now passes `Arc::new(MockTrasher::new())`. Imports are added only where C has not added them, since a second identical `use` is E0252 (checked). There is a C-checklist row, and the Baseline records that C's plan is committed.
- **A3 — C's ruling 2 promises a test-only `Recipe` literal with `uninstall: None`.** Task 4's new required field `extra_locks` would stop the build there. Task 4 Step 3 adds `extra_locks: no_extra_locks` to every literal the baseline grep finds, and there is a `git add` note for one outside the four files.
- **A4 — two comment edits said something false.** Task 1's reworded `Known` doc sentence let "for those, rules 1 and 2 compare the same file" cover cargo. For cargo, rule 2 is the *only* rule that places `hexyl`. The sentence is reworded, and the paragraph is correctly attributed to `struct Known`, not `owned_roots`. The registration task's Files list also promised an edit to B's "`standalone-rustup` never joins" arm comment that no step made; that comment stays true, and the list now says it is untouched.

## Review log (Astra, 2026-09-25)

Independent review by GPT-6 Astra (`~/dev/Banager/.superpowers/phase4/astra/review-plan-e.md`), eleven findings and one "checked, clean" note; the controller ruled on each. Every ruling was applied against the sources named below, re-read read-only on 2026-09-25: rustup at tag `1.29.1` (`src/cli/rustup_mode.rs`, `src/cli/proxy_mode.rs`, `src/bin/rustup-init.rs`, `src/config.rs`, `src/cli/self_update.rs`, `src/cli/self_update/{shell,unix}.rs`, `src/lib.rs`), the `home` crate at `home-0.5.12` (`crates/home/src/{env,lib}.rs`), and the worktree `~/dev/Banager-phase4` at `ea30cfb` (never modified; no cargo or pnpm run there). Nothing in this revision was compiled: the shapes it depends on — C's — are not in the tree yet. The plan's first Task 5 module had been compiled in a scratch crate at the earlier review; the rewritten one has not, and Task 5's Step 4 is where that is found out.

| # | Finding | Ruling | Applied — and what the evidence decided where the ruling left it open |
|---|---|---|---|
| 1 | `rustup --version` with an empty environment can install a toolchain (`display_version` → `maybe_ensure_active_toolchain`; auto-install on by default). | `RUSTUP_AUTO_INSTALL=0` on every rustup read Banager makes and on the recording commands; replace the empty-env test; an isolated missing-toolchain test with a mocked runner. | **Applied.** Confirmed at `rustup_mode.rs:1819-1837`, `config.rs:435-441`, `:555-578`: `should_auto_install` is `true` unless the variable is `0` or the setting is `disable`. `RUSTUP.version.env = &[RUSTUP_AUTO_INSTALL_OFF]` (Task 6), asserted by `test_rustup_is_the_flat_file_route_…_with_auto_install_off` (which replaced the `env.is_empty()` assertion) and end to end by `test_detect_reads_rustups_version_with_auto_install_off_and_a_thirty_second_timeout`; `test_detect_reads_the_version_of_a_rustup_with_no_active_toolchain_without_running_anything_else` mocks the stderr 1.29.1 prints on that path (quoted from source; a recording would need a Mac with no toolchain). The recording commands in Task 10 carry the variable. **Extended, on evidence the ruling's own words cover:** `proxy_mode::main` (`proxy_mode.rs:14-59`) calls `Cfg::from_env(…, allow_auto_install = true, …)` and `local_toolchain(None)` → `maybe_ensure_active_toolchain`, so `cargo --version` on a rustup Mac is a rustup invocation that installs a toolchain under the same conditions; `CargoAdapter::detect` gets the same variable (Task 1, `test_detect_reads_cargos_version_with_rustups_auto_install_off`), from one constant `cargo::RUSTUP_AUTO_INSTALL_OFF`. Two side effects the variable does not stop are disclosed instead (ruling 20): `Cfg::from_env` creates `$RUSTUP_HOME` (`config.rs:321-323`) and `cleanup_self_updater` deletes a leftover `rustup-init`. |
| 2 | Custom deletion roots are never shown; `uninstall()` removes `$RUSTUP_HOME` whole; `RUSTUP_HOME=~/Documents` puts documents in scope; the command bypasses C's path safeguards. | Offer the uninstall only when both roots, computed as rustup computes them, are exactly `~/.cargo` and `~/.rustup` (a symlinked root refuses); otherwise `NoSafeMethod` with copy in both locales; the preview names both roots and says PERMANENTLY, not to the Trash, with settings/logins/packages/programs; conditional Homebrew wording on a local read-only signal, else a general clause. | **Applied.** The gate is `rustup::standard_roots` (Task 5, ruling 18): both `Detected` roots `Some`, lexically `<home>/.cargo` and `<home>/.rustup` (the comparison rustup itself makes in `cargo_home_str_with_home`, `shell.rs:43-58`), `~/.cargo` a directory and not a link, `~/.rustup` a directory and not a link or absent. `CommandUninstall.blocked` carries it; `inventory` puts `NoSafeMethod` on the artifact, `plan(Uninstall)` refuses with it (Task 6, `test_inventory_and_plan_refuse_the_uninstall_for_a_non_standard_layout`, `test_standard_roots_accepts_only_the_default_layout_of_real_directories`). The row's sentence is rustup's own for that variant (Task 11, `uninstallBlockedCopy`, both locales) — the controller said `NoSafeMethod`, and B's sentence for it ("has no uninstall command") is false of rustup, so the copy is keyed by adapter id for that one reason. The preview: `RemovesToolchains { path, names }` and `DeletesCargoHome { path }` name the roots (`~/.rustup`, `~/.cargo` through `scan::display_path`) and both sentences say "permanently — not to the Trash", the Cargo one listing downloads, the record, settings and saved login (`config.toml`, `credentials.toml`); `RemovesCargoInstalled { names }` names the programs where known. **Homebrew:** a local read-only signal exists — `<prefix>/Cellar/rustup` under `/opt/homebrew` or `/usr/local` (`rustup::homebrew_rustup_present`, `HOMEBREW_PREFIXES`) — so `HomebrewRustupLosesToolchains` is conditional on it, and the `removesToolchains` sentence always ends "Any other rustup that uses this folder loses its toolchains too" (ruling 21; that rustup's homes are independent of the binary is now VERIFIED from `env.rs:101-113`). **Decided on evidence:** to read `RUSTUP_HOME` at all, `HostEnv` gains `rustup_home` (and `zdotdir`, finding 8) — the spec refused it for want of a reader; the gate is one (ruling 20; 39 literal sites, mechanical). |
| 3 | The locks do not protect detection; `rustup --version` and the preview's `toolchain list` run unlocked; every rustup invocation runs `cleanup_self_updater`, which deletes `bin/rustup-init`. | The preview must not run rustup at all (read `~/.rustup/toolchains/`); for the version read choose (a) a generic refresh-level skip with carry-forward or (b) rustup's read under its lock — with a concurrency test showing a refresh during a running `self update` never invokes rustup, and the residual stated. | **Applied — (a).** The probe is gone (`CommandUninstall` has no `probe`; `rustup::toolchain_names` lists the directory; `test_plan_uninstall_for_rustup_runs_nothing_and_lists_the_warnings` asserts zero runner calls). **Why (a) and not (b):** `proxy_mode::main` begins with `self_update::cleanup_self_updater(process)?` (`proxy_mode.rs:15`) exactly as `rustup_mode::main` does (`rustup_mode.rs:669`), so the cargo adapter's `cargo --version` — `refresh.rs:167`, unlocked — deletes a running self update's `rustup-init` (`prepare_update` puts it at `$CARGO_HOME/bin/rustup-init`, `self_update.rs:1165-1225`; `run_update` runs it, `unix.rs:120-131`) just as `rustup --version` would; (b) would have left that. (a) is Task 7: `OperationManager::locks_held()`; `refresh_round` skips the detection of every adapter one of whose previous-round instance ids is a held lock, keeps that adapter's instances unchanged, and carries the held instances' rows forward instead of waiting on their lock — per instance in the fan-out, so the existing "but not others" guarantee holds; that test (`refresh.rs:1395-1465`) is renamed and re-asserted because the refresh no longer *waits*. Concurrency tests: `test_a_refresh_during_rustups_self_update_runs_neither_rustup_nor_cargo` (real adapters over one `MockRunner` with a delayed `self update`: zero new calls during, rows equal, reads resume after), `test_refresh_carries_an_instance_under_an_operation_forward_and_still_refreshes_the_others`, `test_an_operation_on_another_adapters_instance_does_not_skip_this_adapters_detect`. **Residual, stated in ruling 19, Task 7 and the trust file:** the held set is a snapshot at the round's start (an operation submitted after it can overlap one `--version`); a Queued operation skips nothing; a `cargo install` alone does not skip rustup's detect (harmless under the switch, and `rustup-init` exists only during a self update, which holds rustup's lock). |
| 4 | The Cargo-home rule differs from rustup's: empty and relative `CARGO_HOME` are taken as paths; `home` 0.5.12 ignores empty and resolves relative against cwd. | Match `home` 0.5.12: empty ignored, relative unsupported for Banager; use the normalized value for discovery, locks and warnings; bind commands to the previewed environment. | **Applied.** `path_env::tool_home` (Task 1) implements `env.rs:67-79`/`:101-113`: empty → default, absolute → itself, relative → `None`; `cargo::cargo_home_of` returns `Option<PathBuf>` and `CargoAdapter::detect` lists nothing for `None` (today it would name an instance whose prefix is relative to Banager's own cwd — a wrong answer, not a missing one); `StandaloneAdapter::detect` seats the same value and `expand_route` answers `None` for a `$CARGO_HOME` path without a Cargo home, so rustup is not listed either (tests in Tasks 1, 4, 6). Locks and warnings read `Detected.cargo_home`, the same normalized value. Binding to the previewed environment: `PlanAction::Command.env` is empty and the runner inherits Banager's process environment — the environment `detect` read `CARGO_HOME`/`RUSTUP_HOME` from (`HostEnv::discover`), so the rustup that runs computes the same roots the gate checked (ruling 17, 18); the seat binding (finding 6) refuses a plan whose seat was read under another home. |
| 5 | E changes `expand`'s signature and misses C's `removal.rs` callers and C's `Detected` literal. | Explicitly migrate every `expand` call and `Detected` literal after C lands; include `removal.rs`; a C dependency checklist. | **Applied, by not changing `expand`.** B's `expand(home, spec)` keeps its signature; `route::expand_route(home, cargo_home, spec)` is the new function `detect` and `seated_detected_for` use (ruling 5), so C's five `removal.rs` calls compile untouched, and `recipes::tests::test_a_paths_recipe_names_only_home_paths` pins that a `Paths` recipe never names `$CARGO_HOME`. `Detected` literals: `detect`'s write (Task 4), `testing::detected` (Task 4), C's `removal.rs` test helper `detected(home)` (Task 4 edits it; checklist row 2), with `grep -rn "Detected {"` as the net and `missing field` as the compiler's. The **C dependency checklist** (17 rows, before the Global Constraints) lists every touch point with what E does if C spelled it differently; the executor re-verifies it before Task 1 and before Task 4. |
| 6 | The seat is not tied to the planned instance: detect A, detect B, plan A uses A's launcher with B's warnings and lock. | `plan()` verifies the seat's launcher/roots match the instance (`exe_path`, `prefix`); mismatch → `Refused`; test the A/B/A sequence. | **Applied.** `seated_detected_for(inst)` (Task 4, ruling 9) expands the recipe's launcher and root against the seat and refuses unless both equal `inst.exe_path` and `inst.prefix`; `plan`'s Upgrade and Command arms and `inventory`'s gate read the seat through it (a mismatched seat reads as blocked in `inventory`); C's `Paths` arm is routed through it too (checklist row 16). Tests: `test_plan_refuses_an_instance_the_seat_no_longer_describes` (Task 4, claude, A/B/A with a re-detect of A) and `test_a_plan_for_an_instance_from_another_home_is_refused_and_a_redetect_restores_it` (Task 8, rustup, asserting the restored plan's cargo lock names A's home). |
| 7 | Substring matching does not establish terminal errors: comments, `echo`, guarded sourcing are flagged as errors; `. "$CARGO_HOME/env"` is missed; with a custom home the `.cargo/env` needle flags an unrelated default install. | Recognise only rustup's own sourcing forms, resolve their target against the real `CARGO_HOME`, skip comments; omit or qualify other mentions; test the counterexamples in both locales. | **Applied — qualify.** `rustup::leftover_patterns` builds the certain forms (`. "<X>/env"`, `source "<X>/env"`, fish's `source "<X>/env.fish"`, `<X>` a spelling whose target is this Cargo home: `$HOME/.cargo` and the absolute default for the default home, the absolute custom path otherwise) and the needles (`.cargo/env` only for the default home; `<custom>/env`; `$CARGO_HOME/env`, `${CARGO_HOME}/env`); `classify_leftover` skips comments, answers `Sources` for a trimmed line equal to a certain form and `Mentions` for any other line with a needle; `LeavesShellConfigLine { path, certain }` keys `warnings.leavesShellConfigLine` ("will print an error") or `warnings.leavesShellConfigLineMaybe` ("may") in both locales (Tasks 2, 5; ruling 22). Every counterexample is a test line: the comment (nothing), `echo` (may), the guard (may), `source ~/.cargo/env` (may), `. "$CARGO_HOME/env"` (may), and with a custom home `. "$HOME/.cargo/env"` (nothing — the file survives) and the absolute custom line (will). Omission was not chosen: a guarded line is harmless, but an unguarded `source ~/.cargo/env` is not, and the two are told apart only by running the shell, which Banager does not; "may" is what is known. |
| 8 | "First match once per file" is not rustup's behaviour: Zsh's `rcfiles()` returns `$ZDOTDIR/.zshenv` and `~/.zshenv` without deduplication, so `ZDOTDIR=$HOME` visits the file twice and removes two copies; legacy `.zprofile` likewise. | Model rustup's actual ordered file visits (including `ZDOTDIR == HOME` visiting `.zshenv` twice, and the legacy `.zprofile` handling) instead of a per-filename removal count. | **Applied.** `rustup::rustup_rc_visits(home, zdotdir, S)` (Task 5, ruling 2) lists the visits in rustup's order — `do_remove_from_path` over `enumerate_shells` (`shell.rs:63-74`): Posix, Bash's three, `$ZDOTDIR/.zshenv`, `~/.zshenv`; then `remove_legacy_paths`'s two lines over `legacy_paths` (`shell.rs:564-574`: `.bash_profile`, `.profile`, `$ZDOTDIR/.zprofile`, `~/.zprofile`) — and `shell_config_leftovers` replays them on in-memory copies keyed by path, so `ZDOTDIR=$HOME` visits the same copy twice (`test_rustup_rc_visits_visit_zshenv_twice_when_zdotdir_is_home`, `test_shell_config_leftovers_removes_two_copies_when_zdotdir_is_home_and_one_otherwise`). Shell availability: Bash's check is "any of its files exists", which the per-file `is_file` visit already encodes; Zsh's is `SHELL` contains `zsh` or `zsh` on `PATH` — on a Mac `/bin/zsh` is always there, and a zsh that were not available would also never run `.zshenv`, so no leftover in it could print; the doc says so. **Decided on evidence:** `ZDOTDIR` must be an input, so `HostEnv.zdotdir` is added (ruling 20); rustup itself asks `zsh -c 'echo -n $ZDOTDIR'` when `SHELL` is not zsh (`shell.rs:207-225`), which Banager does not run — a `ZDOTDIR` set only inside a zsh startup file is not modelled, and the trust file says so (an `rc` under such a `ZDOTDIR` is a file Banager does not read; a visit to it has no copy to act on). |
| 9 | The timeout sentence contradicts the engine: a timed-out upgrade stays `Unconfirmed` whatever the readings; uninstall is reconciled by presence. | Fix the sentence; distinguish the uninstall's absence-based reconciliation. | **Applied.** `ops/mod.rs:787-800`: `Ok(Outcome::Unconfirmed)` with `OpKind::Upgrade` → `Unconfirmed` unconditionally; `OpKind::Uninstall` → `Succeeded` when `!r.present`, `Cancelled` when the user cancelled (impossible for `NoCancel`), else `Unconfirmed`. The `## rustup` section's update paragraph now says a timed-out run is unconfirmed "whatever the version reads before and after say", and the uninstall paragraph says a timed-out run "is judged by whether `~/.cargo/bin/rustup` is still there: gone is reported as done, still there as unconfirmed". The Global Constraints' honest-outcomes bullet says the same with the line numbers; Task 8's tests pin each arm. |
| 10 | The uninstall lacks end-to-end coverage: the test asserts `execute` succeeds while the launcher remains, never exercising `OperationManager` or `reconcile_after_uninstall`. | End-to-end tests through `OperationManager`: launcher gone with other `bin/` files left → `Succeeded`; launcher present after exit 0 → `StillInstalledAfterUninstall`; timeout before/after removal; the cargo-absent and mismatched-home lock cases. | **Applied.** New `tests/ops_rustup_uninstall_test.rs` (Task 8) with `UninstallingRunner` mutating the disk as the command returns: `…launcher_gone_and_other_bin_files_left_succeeded` (hexyl stays, `Succeeded`), `…launcher_still_there_needs_attention` (`StillInstalledAfterUninstall`), `…timeout_before_the_launcher_went_is_unconfirmed`, `…timeout_after_the_launcher_went_succeeded`, `…no_cargo_instance_registered_still_holds_the_cargo_lock_by_name` (only the rustup adapter registered; both names acquired and released), `test_a_plan_for_an_instance_from_another_home_is_refused_and_a_redetect_restores_it`. The upgrade's two tests moved from the NoCancel task into Task 8, and a third (`…stopped_by_the_timeout_is_unconfirmed_whatever_the_readings_say`) pins finding 9. `test_execute_runs_rustups_uninstall_through_run_plan` (Task 6) keeps asserting `execute`'s own exit only, and says so. All rest on C's `reconcile_after_uninstall` reading the launcher's presence through `probe_strict` (checklist row 6). |
| 11 | Overreaching claims: non-UTF-8 names "survive" (the final `remove_dir` deletes them); README's "each named"; the handover's multi-binary claim; "rustup does not update itself". | Correct every sentence, in both locales and the trust file. | **Applied.** Non-UTF-8: ruling 16, `bin_programs_rustup_removes`'s doc and `RemovesCargoInstalled`'s doc now say such a name is deleted with the folder (`remove_dir`, `:1029`) and merely cannot be spelled in a sentence. "Each named" → "named where known" (README row, `RemovesCargoInstalled`'s doc, the trust file's "the programs in its `bin/` by name where known"). Multi-binary: `parse_crates2`'s doc, Task 1's test comment, the delivery note and spec-deviation 9 say the other binaries of a multi-binary crate stay on the Unknown page. "rustup does not update itself" → `rustup_mode.rs:1042-1090`: `update()` (the toolchain update) and `toolchain install` call `SelfUpdateMode::update`, so the recipe doc, the trust file and the README now say rustup updates itself only as part of `rustup update`/`rustup toolchain install`, which Banager never runs. Locale copy: none of the four sentences was locale copy; the locale sentences touched by findings 2 and 7 are in Task 2, both languages. |
| — | Checked, clean: the deletion correction, lock identity, cancellation semantics, test isolation, the already-passing Task 7 tests. | — | Kept. The already-passing tests are still labelled as regressions (Task 8 Step 1's heading says they pass against the engine as it stands). |

**Remaining risks (after this revision):**

1. **C is not landed, and its plan is being revised in parallel.** Every C shape this plan uses is in the checklist with a fallback, but two of them shape E's own code paths: `probe_strict` (where the `FlatFile` arms go) and `reconcile_after_uninstall` (what Task 8's outcomes rest on). If C's final shapes differ from its committed plan, the executor resolves them at the checklist, not in the tasks.
2. **Nothing in this revision was compiled.** The earlier Task 5 module was compiled in a scratch crate; the rewritten one (the visit model, the tiers, the gate) has not been, nor have Tasks 6–8. The plan's red → green steps are where a slip shows; the most likely one is a borrow in `remove_first_exact_line` (the index is computed in a block so the `as_bytes` borrow ends before `replace_range`) or a `HashSet`/`InstanceId` import in `refresh.rs`.
3. **`HostEnv` grows by two fields across 39 literals in 13 files**, one of them in `src-tauri`. Mechanical, and the compiler names every miss, but it is the widest edit in the plan and touches files no other task does.
4. **The refresh skip changes behaviour for every source** (ruling 19): a refresh no longer waits out an operation on an instance; it carries the instance forward. The renamed test pins the new behaviour; whether any front-end flow relied on the refresh *waiting* (and so returning post-operation data) was not checked here — the front end refreshes on the operation's `Finished` event, which is the path that replaces the carried rows.
5. **`ZDOTDIR` is modelled from the variable Banager was started with**; rustup asks `zsh` when `SHELL` is not zsh, and a `ZDOTDIR` set only inside a zsh startup file is invisible to Banager. Disclosed in the trust file; a zsh whose files live there is not read at all.
6. **The Homebrew signal is a directory under two hard-coded prefixes.** A Homebrew at a custom prefix (unsupported by Homebrew on Apple Silicon) would not be seen; the general "any other rustup" clause still covers the consequence.
7. **Only 1.29.1 is verified.** If the recording Mac's rustup is newer, Task 10 Step 1 requires re-reading `uninstall()`, `shell.rs`, `unix.rs`, `display_version` and `should_auto_install` at the installed tag. The Cargo-folder copy would then over-warn: newer rustup keeps programs and possibly config, and the plan has no second copy set for that.
8. **The per-adapter skip in the detect phase is coarser than per instance.** An npm with two instances, one under `npm install -g`, has both instances' *detection* carried forward for that round (their inventories still run); a stale version on the other instance for one round is the cost, stated in ruling 19.
